import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { initSync, WasmTerrainEngine } from "olayer-wasm";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { WcsTerrainSource } from "./wcs_terrain";
import type { WcsElevationInterpretation } from "./wcs_terrain";
import { WcsRasterSource } from "./ogc_sources";
import { OgcHttpClient } from "./ogc_client";
import { buildWmsMesh } from "../layers/wms_mesh";

beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));
afterEach(() => vi.unstubAllGlobals());
const fixture = readFileSync(resolve(__dirname, "../../../../tests/fixtures/wcs/elevation.json"), "utf8");
const options = {endpoint:"https://fixture.test/wcs",coverageId:"terrain",crs:"EPSG:4326",bounds:[-2,-1,2,1] as const,width:3,height:3};
function source(engine: WasmTerrainEngine, interpretation: WcsElevationInterpretation = {}): WcsTerrainSource {
  return new WcsTerrainSource("wcs",engine,new WcsRasterSource(new OgcHttpClient({maxRetries:0}),options),interpretation);
}

describe("WCS terrain integration", () => {
  it("uses one request to drive all display modes through the real WASM engine", async () => {
    const fetch = vi.fn(async () => new Response(fixture));
    vi.stubGlobal("fetch",fetch);
    const engine = new WasmTerrainEngine();
    try {
      await source(engine).load();
      for (const viewMode of ["2D","2.5D","3D"] as const) {
        const mesh = buildWmsMesh(options.crs,options.bounds,{
          viewMode,centerLat:0,centerLon:0,zoom:1,project:(lat,lon)=>[lon*6378137,lat*6378137],
          elevation:(lat,lon)=>engine.get_elevation_status(lat,lon).elevation_meters,
        },2);
        expect(mesh.length).toBeGreaterThan(0);
        if (viewMode !== "3D") expect(mesh[2]).toBe(viewMode === "2D" ? 0 : 500);
      }
      expect(fetch).toHaveBeenCalledTimes(1);
      expect(String(fetch.mock.calls[0]?.[0])).toContain("FORMAT=application%2Fjson");
    } finally { engine.free(); }
  });

  it("preserves nodata, checks dimensions, and retains the previous grid on failure", async () => {
    vi.stubGlobal("fetch",vi.fn(async()=>new Response(JSON.stringify({width:3,height:3,samples:[100,200,300,400,-9999,600,700,800,900],nodata:-9999}))));
    const engine = new WasmTerrainEngine();
    try {
      const terrain = source(engine);
      await terrain.load();
      expect(engine.get_elevation_status(0,0).elevation_meters).toBeNull();
      expect(engine.get_elevation_status(Math.PI/180,-2*Math.PI/180).elevation_meters).toBe(100);
      vi.stubGlobal("fetch",vi.fn(async()=>new Response('{"width":1,"height":1,"samples":[0]}')));
      await expect(terrain.load()).rejects.toThrow("dimensions");
      expect(engine.get_elevation_status(Math.PI/180,-2*Math.PI/180).elevation_meters).toBe(100);
      terrain.unload();
      expect(()=>engine.get_elevation_status(0,0)).toThrow();
    } finally { engine.free(); }
  });

  it("ignores a pending load after unload and honors cancellation", async () => {
    let finish!: (response: Response) => void;
    vi.stubGlobal("fetch",vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
    const engine = new WasmTerrainEngine();
    try {
      const terrain = source(engine);
      const pending = terrain.load();
      terrain.unload();
      finish(new Response(fixture));
      await pending;
      expect(()=>engine.get_elevation_status(0,0)).toThrow();
      const abort = new AbortController();
      abort.abort();
      await expect(terrain.load(abort.signal)).rejects.toThrow();
    } finally { engine.free(); }
  });

  it("converts units and explicitly converts orthometric samples before atomic installation", async () => {
    vi.stubGlobal("fetch",vi.fn(async()=>new Response(JSON.stringify({width:3,height:3,samples:Array<number>(9).fill(100),nodata:-9999}))));
    const engine = new WasmTerrainEngine();
    try {
      const terrain = source(engine, {
        unit: "international-foot",
        verticalReference: "orthometric",
        geoid: { geoidUndulationMeters: () => 30 },
      });
      await terrain.load();
      expect(engine.get_elevation_status(0,0).elevation_meters).toBeCloseTo(60.48, 6);
      const previous = engine.get_elevation_status(0,0).elevation_meters;
      vi.stubGlobal("fetch",vi.fn(async()=>new Response(JSON.stringify({width:3,height:3,samples:Array<number>(9).fill(200)}))));
      const missingGeoid = source(engine, {
        unit: "metre", verticalReference: "orthometric",
        geoid: { geoidUndulationMeters: () => null },
      });
      await expect(missingGeoid.load()).rejects.toThrow("geoid correction");
      expect(engine.get_elevation_status(0,0).elevation_meters).toBe(previous);
    } finally { engine.free(); }
  });

  it("registers cell-centred coverage at pixel centres and preserves nodata sentinels", async () => {
    vi.stubGlobal("fetch",vi.fn(async()=>new Response(JSON.stringify({width:3,height:3,samples:[0,1,2,3,4,5,6,7,8]}))));
    const engine = new WasmTerrainEngine();
    try {
      await source(engine,{ registration: "cell-center" }).load();
      expect(engine.get_elevation_status(0,-Math.PI/180).elevation_meters).toBeCloseTo(3.25, 6);
      expect(engine.get_elevation_status(0,0).elevation_meters).toBeCloseTo(4, 6);
      expect(() => engine.get_elevation_status(0,-1.9*Math.PI/180)).toThrow();
    } finally { engine.free(); }
  });
});
