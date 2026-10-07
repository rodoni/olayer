import { afterEach, describe, expect, it, vi } from "vitest";
import { OgcHttpClient, OgcHttpError, OgcResponseLimitError } from "./ogc_client";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("OgcHttpClient", () => {
  it("fetches and validates WFS GeoJSON while preserving properties", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
      type: "FeatureCollection",
      features: [{ type: "Feature", id: 7, geometry: null, properties: { nested: { value: 2 } } }],
    }), { status: 200, headers: { "content-type": "application/geo+json" } })));
    const result = await new OgcHttpClient({ maxRetries: 0 }).fetchWfs("https://example.test/wfs");
    expect(result.features[0]?.id).toBe("7");
    expect(result.features[0]?.properties.nested).toEqual({ value: 2 });
  });

  it("decodes valid WCS JSON grids", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ width: 2, height: 1, samples: [1, 2], nodata: -9999 }))));
    const result = await new OgcHttpClient({ maxRetries: 0 }).fetchWcsJson("https://example.test/wcs");
    expect(result.samples).toEqual([1, 2]);
    expect(result.nodata).toBe(-9999);
  });

  it("enforces response limits and typed HTTP failures", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("123456", { status: 200 })));
    await expect(new OgcHttpClient({ maxRetries: 0, maxResponseBytes: 3 }).fetchBytes("https://example.test/large"))
      .rejects.toBeInstanceOf(OgcResponseLimitError);

    vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 503 })));
    await expect(new OgcHttpClient({ maxRetries: 0 }).fetchBytes("https://example.test/fail"))
      .rejects.toBeInstanceOf(OgcHttpError);
  });
});
