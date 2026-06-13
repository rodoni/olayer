import { describe, it, expect, vi, beforeAll } from "vitest";
import { TerrainTileSource } from "./index";
import { WasmTerrainEngine, initSync } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";

/**
 * Builds a minimal mock DTED Level 0 tile (4x4) for tests.
 */
function createMockDted0(originLat: string, originLon: string, numCols: number, numRows: number): Uint8Array {
  let data = new Uint8Array(3428);
  const uhl = new TextEncoder().encode("UHL1");
  data.set(uhl, 0);
  const lonBytes = new TextEncoder().encode(originLon.padEnd(8, " "));
  const latBytes = new TextEncoder().encode(originLat.padEnd(8, " "));
  data.set(lonBytes, 4);
  data.set(latBytes, 12);
  const spacing = new TextEncoder().encode("0300");
  data.set(spacing, 20);
  data.set(spacing, 24);
  const cols = new TextEncoder().encode(numCols.toString().padStart(4, "0"));
  const rows = new TextEncoder().encode(numRows.toString().padStart(4, "0"));
  data.set(cols, 47);
  data.set(rows, 51);

  const colSize = 11 + numRows * 2;
  for (let c = 0; c < numCols; c++) {
    const col = new Uint8Array(colSize);
    col[0] = 0xAA;
    col[1] = 0;
    col[2] = 0;
    col[3] = c;
    col[4] = 0;
    col[5] = 0;
    col[6] = 0;
    for (let r = 0; r < numRows; r++) {
      const height = c * 10 + r;
      const be = new Int16Array([height]);
      const buf = new Uint8Array(be.buffer);
      // Big-endian swap
      const idx = 7 + r * 2;
      col[idx] = buf[1];
      col[idx + 1] = buf[0];
    }
    const newData = new Uint8Array(data.length + colSize);
    newData.set(data);
    newData.set(col, data.length);
    data = newData;
  }
  return data;
}

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync(wasmBuffer);
});

describe("TerrainTileSource", () => {
  it("should inject a tile and update cache", () => {
    const engine = new WasmTerrainEngine();
    const source = new TerrainTileSource(engine);

    const mock = createMockDted0("230000S", "0480000W", 4, 4);
    source.injectTile(-23, -48, mock);

    expect(source.getCacheSize()).toBe(1);
  });

  it("should unload a tile by request key", () => {
    const engine = new WasmTerrainEngine();
    const source = new TerrainTileSource(engine);

    const mock = createMockDted0("230000S", "0480000W", 4, 4);
    source.injectTile(-23, -48, mock);
    source.unloadTile(-23, -48);

    expect(source.getCacheSize()).toBe(0);
  });

  it("should evict oldest tile on LRU overflow", () => {
    const engine = new WasmTerrainEngine();
    const source = new TerrainTileSource(engine, "", 2);

    const mock1 = createMockDted0("230000S", "0480000W", 4, 4);
    const mock2 = createMockDted0("240000S", "0480000W", 4, 4);
    const mock3 = createMockDted0("250000S", "0480000W", 4, 4);

    source.injectTile(-23, -48, mock1);
    source.injectTile(-24, -48, mock2);
    source.injectTile(-25, -48, mock3);

    expect(source.getCacheSize()).toBe(2);
  });

  it("should clear all tiles", () => {
    const engine = new WasmTerrainEngine();
    const source = new TerrainTileSource(engine);

    const mock = createMockDted0("230000S", "0480000W", 4, 4);
    source.injectTile(-23, -48, mock);
    source.clearCache();

    expect(source.getCacheSize()).toBe(0);
  });
});
