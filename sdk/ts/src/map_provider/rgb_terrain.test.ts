import { describe, it, expect, beforeAll } from "vitest";
import { initSync, WasmTerrainEngine, decode_mapbox_rgb, decode_terrarium_rgb } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";
import { RgbTerrainSource } from "./rgb_terrain";
import { CogTerrainSource } from "./cog_terrain";

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync({ module: wasmBuffer });
});

describe("Civil Terrain Providers (GIS-PROP-005)", () => {
  it("should decode Mapbox RGB and Terrarium values in WASM", () => {
    // Sea level
    expect(decode_mapbox_rgb(1, 134, 160)).toBeCloseTo(0.0, 1);
    expect(decode_terrarium_rgb(128, 0, 0)).toBeCloseTo(0.0, 5);

    // Everest
    expect(decode_mapbox_rgb(2, 224, 72)).toBeCloseTo(8848.8, 1);

    // Trench
    expect(decode_mapbox_rgb(0, 0, 0)).toBeCloseTo(-10000.0, 5);
  });

  it("should load Mapbox RGB tiles and sample elevation", () => {
    const engine = new WasmTerrainEngine();
    const source = new RgbTerrainSource("rgb-test", engine, "", {
      encoding: "Mapbox",
      tileSize: 2,
    });

    // 2x2 RGBA buffer with constant 500m elevation: [1, 154, 40, 255]
    const rgba = new Uint8Array([
      1, 154, 40, 255,   1, 154, 40, 255,
      1, 154, 40, 255,   1, 154, 40, 255,
    ]);

    // Inject tile at z=10, x=512, y=512 (lat in [-0.3515, 0], lon in [0, 0.3515])
    source.injectRgbaTile(10, 512, 512, rgba, 2, 2);

    const elev = source.getElevationAt(-0.05, 0.05);
    expect(elev).not.toBeNull();
    expect(elev!).toBeCloseTo(500.0, 0.5);

    expect(source.getElevationAt(50.0, 50.0)).toBeNull();

    source.unloadTile(512, 512, 10);
    expect(source.getElevationAt(-0.05, 0.05)).toBeNull();
  });

  it("should load Terrarium RGB tiles and sample elevation", () => {
    const engine = new WasmTerrainEngine();
    const source = new RgbTerrainSource("terrarium-test", engine, "", {
      encoding: "Terrarium",
      tileSize: 2,
    });

    // +1000m Terrarium: [131, 232, 0, 255]
    const rgba = new Uint8Array([
      131, 232, 0, 255,   131, 232, 0, 255,
      131, 232, 0, 255,   131, 232, 0, 255,
    ]);

    source.injectRgbaTile(10, 512, 512, rgba, 2, 2);
    const elev = source.getElevationAt(-0.05, 0.05);
    expect(elev).not.toBeNull();
    expect(elev!).toBeCloseTo(1000.0, 0.5);

    source.clearCache();
    expect(source.getElevationAt(-0.05, 0.05)).toBeNull();
  });

  it("should support CogTerrainSource for GeoTIFFs", () => {
    const engine = new WasmTerrainEngine();
    const source = new CogTerrainSource("cog-test", engine);

    expect(source.id).toBe("cog-test");
    expect(source.getElevationAt(0, 0)).toBeNull();
  });
});
