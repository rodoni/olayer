import { describe, it, expect, vi } from "vitest";
import { VectorTileSource } from "./vector";

describe("VectorTileSource", () => {
  it("should load GeoJSON features and convert coordinates", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.json");

    const geojson = {
      features: [
        {
          geometry: {
            type: "Point",
            coordinates: [0, 0],
          },
          properties: { name: "Test" },
        },
      ],
    };

    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        arrayBuffer: () => Promise.resolve(new TextEncoder().encode(JSON.stringify(geojson)).buffer),
      } as Response)
    );

    await source.loadTile(0, 0, 0);
    const features = source.getTileFeatures(0, 0, 0);
    expect(features).toHaveLength(1);
    expect(features[0].type).toBe("Point");
    expect(features[0].properties.name).toBe("Test");
  });

  it("should generate mock features when no URL resolver is set", async () => {
    const source = new VectorTileSource("");
    await source.loadTile(0, 0, 0);
    const features = source.getTileFeatures(0, 0, 0);
    expect(features.length).toBeGreaterThan(0);
  });

  it("should handle fetch failures gracefully", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.json");
    global.fetch = vi.fn(() => Promise.resolve({ ok: false, status: 404 } as Response));

    await source.loadTile(0, 0, 0);
    const features = source.getTileFeatures(0, 0, 0);
    expect(features.length).toBeGreaterThan(0); // falls back to mock
  });

  it("should not duplicate load requests", async () => {
    const source = new VectorTileSource("https://example.com/{z}/{x}/{y}.json");
    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
      } as Response)
    );

    await source.loadTile(0, 0, 0);
    await source.loadTile(0, 0, 0);
    expect(global.fetch).toHaveBeenCalledTimes(1);
  });

  it("should clear cache", async () => {
    const source = new VectorTileSource("");
    await source.loadTile(0, 0, 0);
    source.clearCache();
    expect(source.getTileFeatures(0, 0, 0)).toHaveLength(0);
  });
});
