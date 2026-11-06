import { describe, expect, it, vi } from "vitest";
import { OgcHttpClient } from "./ogc_client";
import { WcsRasterSource, WfsVectorSource } from "./ogc_sources";

describe("OGC sources", () => {
  it("loads WFS features through the shared request builder", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ type: "FeatureCollection", features: [] }))));
    const source = new WfsVectorSource(new OgcHttpClient({ maxRetries: 0 }), {
      endpoint: "https://example.test/wfs",
      typeName: "roads",
      crs: "EPSG:4326",
    });
    expect((await source.load()).features).toHaveLength(0);
    vi.unstubAllGlobals();
  });

  it("loads WCS JSON coverage through the shared request builder", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ width: 1, height: 1, samples: [42] }))));
    const source = new WcsRasterSource(new OgcHttpClient({ maxRetries: 0 }), {
      endpoint: "https://example.test/wcs",
      coverageId: "elevation",
      crs: "EPSG:4326",
      bounds: [0, 0, 1, 1],
      width: 1,
      height: 1,
    });
    expect((await source.loadJson()).samples).toEqual([42]);
    vi.unstubAllGlobals();
  });
  it("rejects conflicting WFS CRS and malformed property arrays", async () => {
    const source = new WfsVectorSource(new OgcHttpClient({ maxRetries: 0 }), {
      endpoint: "https://example.test/wfs", typeName: "roads", crs: "EPSG:4326",
    });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ type: "FeatureCollection", crs: { type: "name", properties: { name: "EPSG:3857" } }, features: [] }))));
    await expect(source.load()).rejects.toThrow("CRS");
    for (const name of ["EPSG:4326", "urn:ogc:def:crs:EPSG::4326", "http://www.opengis.net/def/crs/EPSG/0/4326"]) {
      vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ type: "FeatureCollection", crs: { type: "name", properties: { name } }, features: [] }))));
      expect((await source.load()).crs).toBe("EPSG:4326");
    }
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ type: "FeatureCollection", features: [{ type: "Feature", geometry: null, properties: [] }] }))));
    await expect(source.load()).rejects.toThrow("properties");
    vi.unstubAllGlobals();
  });

});
