import { beforeAll, describe, expect, it } from "vitest";
import { initSync, WasmTerrainEngine, lla_to_ecef } from "olayer-wasm";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { buildWmsMesh } from "./wms_mesh";
import type { WmsMeshContext } from "./wms_mesh";
import type { OgcBounds } from "../map_provider/ogc";

const scene = JSON.parse(readFileSync(resolve(__dirname, "../../../../tests/fixtures/wms/scene.json"), "utf8"));
const bounds: OgcBounds = scene.bounds;
const context: WmsMeshContext = {
  viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1,
  project: (lat, lon) => [lon * 6378137, lat * 6378137],
};
beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));

describe("WMS image geometry", () => {
  it("places north-west image pixels at the north-west geographic corner", () => {
    const vertices = buildWmsMesh("EPSG:4326", bounds, context, 1);
    expect(vertices.length).toBe(30);
    expect(vertices[0]).toBeCloseTo(-2 * Math.PI / 180 * 6378137, 1);
    expect(vertices[1]).toBeCloseTo(Math.PI / 180 * 6378137, 1);
    expect(Array.from(vertices.slice(3, 5))).toEqual([0, 0]);
    expect(Array.from(vertices.slice(8, 10))).toEqual([0, 1]);
  });

  it("samples actual terrain in 2.5D/3D while 2D stays flat", () => {
    const terrain = new WasmTerrainEngine();
    try {
      terrain.load_rgb_tile(0, 0, 0, "mapbox", new Uint8Array([...scene.rgbaTerrain, ...scene.rgbaTerrain, ...scene.rgbaTerrain, ...scene.rgbaTerrain]), 2, 2);
      const elevation = (lat: number, lon: number): number | null => terrain.get_elevation_status(lat, lon).elevation_meters;
      const flat = buildWmsMesh("EPSG:4326", bounds, { ...context, elevation }, 2);
      const draped = buildWmsMesh("EPSG:4326", bounds, { ...context, viewMode: "2.5D", elevation }, 2);
      expect(flat[2]).toBe(0);
      expect(draped[2]).toBeCloseTo(scene.elevation, 1);
      const globe = buildWmsMesh("EPSG:4326", bounds, { ...context, viewMode: "3D", elevation }, 2);
      const expected = lla_to_ecef(Math.PI / 180, -2 * Math.PI / 180, scene.elevation);
      for (let i = 0; i < 3; i++) expect(globe[i]).toBeCloseTo(expected[i]!, 0);
      expect(buildWmsMesh("EPSG:4326", bounds, { ...context, viewMode: "2.5D", elevation: () => null }, 2)).toHaveLength(0);
    } finally { terrain.free(); }
  });

  it("subdivides the globe and clips its far side, including camera attitude changes", () => {
    const globe = { ...context, viewMode: "3D" as const };
    expect(buildWmsMesh("EPSG:4326", bounds, globe, 16).length).toBe(16 * 16 * 6 * 5);
    expect(buildWmsMesh("EPSG:4326", [170, -1, 175, 1], globe, 4)).toHaveLength(0);
    expect(buildWmsMesh("EPSG:4326", bounds, { ...globe, pitch: Math.PI }, 4)).toHaveLength(0);
    const edge = buildWmsMesh("EPSG:4326", [65, -1, 85, 1], globe, 4);
    expect(edge.length).toBeGreaterThan(0);
    for (let i = 0; i < edge.length; i += 5) expect(edge[i]).toBeGreaterThanOrEqual(6378137 ** 2 / (6378137 + 15000000) - 1);
  });

  it("maps Mercator metres to the same geographic footprint", () => {
    const radius = 6378137;
    const mercator: OgcBounds = [
      bounds[0] * Math.PI / 180 * radius, Math.asinh(Math.tan(bounds[1] * Math.PI / 180)) * radius,
      bounds[2] * Math.PI / 180 * radius, Math.asinh(Math.tan(bounds[3] * Math.PI / 180)) * radius,
    ];
    expect(buildWmsMesh("EPSG:3857", mercator, context, 1)).toEqual(buildWmsMesh("EPSG:4326", bounds, context, 1));
  });

  it("rejects unsupported CRS, invalid bounds and nonfinite geometry", () => {
    expect(() => buildWmsMesh("EPSG:4978", bounds, context)).toThrow();
    expect(() => buildWmsMesh("EPSG:4326", [-181, -1, 1, 1], context)).toThrow();
    expect(() => buildWmsMesh("EPSG:4326", bounds, context, 65)).toThrow();
    expect(() => buildWmsMesh("EPSG:4326", bounds, { ...context, project: () => [NaN, 0] })).toThrow();
  });
});
