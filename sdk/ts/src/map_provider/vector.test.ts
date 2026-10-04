import { describe, it, expect, vi, afterEach } from "vitest";
import { PbfWriter } from "pbf";
import { VectorTileSource } from "./vector";

function createPointMvt(layerName = "test"): ArrayBuffer {
  const tile = new PbfWriter();
  tile.writeMessage(3, (_value, writer) => {
    writer.writeVarintField(15, 2);
    writer.writeStringField(1, layerName);
    writer.writeMessage(2, (_feature, featureWriter) => {
      featureWriter.writeVarintField(3, 1);
      featureWriter.writePackedVarint(4, [9, 4096, 4096]);
    }, null);
    writer.writeVarintField(5, 4096);
  }, null);
  return tile.finish().buffer as ArrayBuffer;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("VectorTileSource", () => {
  it("loads GeoJSON features and converts Web Mercator coordinates", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.json");
    const geojson = {
      features: [
        {
          geometry: { type: "Point", coordinates: [0, 0] },
          properties: { name: "Test" },
        },
      ],
    };

    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(new TextEncoder().encode(JSON.stringify(geojson)).buffer),
    } as Response)));

    await source.loadTile(0, 0, 0);
    const features = source.getTileFeatures(0, 0, 0);
    expect(features).toHaveLength(1);
    expect(features[0].type).toBe("Point");
    expect(features[0].coordinates).toEqual([[0, 0]]);
    expect(features[0].properties.name).toBe("Test");
  });

  it("supports GeoJSON coordinates in EPSG:4326", async () => {
    const source = new VectorTileSource("https://example.com/tile.json", 100, {
      geoJsonCrs: "EPSG:4326",
    });
    const geojson = {
      features: [{ geometry: { type: "LineString", coordinates: [[10, 20], [30, 40]] } }],
    };

    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(new TextEncoder().encode(JSON.stringify(geojson)).buffer),
    } as Response)));

    await source.loadTile(0, 0, 0);
    expect(source.getTileFeatures(0, 0, 0)[0].coordinates).toEqual([
      [20 * Math.PI / 180, 10 * Math.PI / 180],
      [40 * Math.PI / 180, 30 * Math.PI / 180],
    ]);
  });

  it("decodes binary MVT features instead of using mock data", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf");
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(createPointMvt()),
    } as Response)));

    await source.loadTile(0, 0, 0);
    const features = source.getTileFeatures(0, 0, 0);
    expect(features).toHaveLength(1);
    expect(features[0].type).toBe("Point");
    expect(features[0].coordinates).toEqual([[0, 0]]);
  });

  it("decodes only the configured MVT layer", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf", 100, {
      mvtLayer: "test",
    });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(createPointMvt("test")),
    } as Response)));

    await source.loadTile(0, 0, 0);
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(1);
  });

  it("rejects when the configured MVT layer is missing", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf", 100, {
      mvtLayer: "missing",
    });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(createPointMvt("test")),
    } as Response)));

    await expect(source.loadTile(0, 0, 0)).rejects.toThrow("MVT layer not found");
  });

  it("rejects when no URL resolver is configured", async () => {
    const source = new VectorTileSource("");
    await expect(source.loadTile(0, 0, 0)).rejects.toThrow("URL resolver");
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(0);
  });

  it("rejects fetch and decode failures without inserting mock features", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf", 100, { maxRetries: 0 });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({ ok: false, status: 404 } as Response)));

    await expect(source.loadTile(0, 0, 0)).rejects.toThrow("HTTP 404");
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(0);
  });

  it("does not duplicate load requests", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf");
    let resolveFetch: ((response: Response) => void) | undefined;
    const fetchMock = vi.fn(() => new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    }));
    vi.stubGlobal("fetch", fetchMock);

    const first = source.loadTile(0, 0, 0);
    const second = source.loadTile(0, 0, 0);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    resolveFetch!({ ok: true, arrayBuffer: () => Promise.resolve(createPointMvt()) } as Response);
    await Promise.all([first, second]);
  });

  it("clears cache", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf");
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(createPointMvt()),
    } as Response)));

    await source.loadTile(0, 0, 0);
    source.clearCache();
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(0);
  });

  it("shares pending requests and evicts the least recently used tile", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf", 2);
    const fetchMock = vi.fn(() => Promise.resolve({
      ok: true,
      arrayBuffer: () => Promise.resolve(createPointMvt()),
    } as Response));
    vi.stubGlobal("fetch", fetchMock);

    await Promise.all([
      source.loadTile(0, 0, 0),
      source.loadTile(0, 0, 0),
    ]);
    await source.loadTile(1, 0, 0);
    source.getTileFeatures(0, 0, 0);
    await source.loadTile(2, 0, 0);

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(1);
    expect(source.getTileFeatures(1, 0, 0)).toHaveLength(0);
    expect(source.getCacheStats().bytes).toBeGreaterThan(0);
  });

  it("retries failed requests according to the configured policy", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf", 100, { maxRetries: 1 });
    const fetchMock = vi.fn()
      .mockRejectedValueOnce(new Error("temporary failure"))
      .mockResolvedValueOnce({
        ok: true,
        arrayBuffer: () => Promise.resolve(createPointMvt()),
      } as Response);
    vi.stubGlobal("fetch", fetchMock);

    await source.loadTile(0, 0, 0);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("honors an aborted request before fetching", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.pbf");
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();
    controller.abort();

    await expect(source.loadTile(0, 0, 0, { signal: controller.signal })).rejects.toMatchObject({
      name: "AbortError",
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
