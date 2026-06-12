import { describe, it, expect, vi } from "vitest";
import { RasterTileSource } from "./raster";

function createMockGL(): WebGL2RenderingContext {
  const texture = { id: Math.random() } as unknown as WebGLTexture;
  return {
    createTexture: vi.fn(() => texture),
    bindTexture: vi.fn(),
    texParameteri: vi.fn(),
    texImage2D: vi.fn(),
    deleteTexture: vi.fn(),
    TEXTURE_2D: 0x0de1,
    TEXTURE_WRAP_S: 0x2802,
    TEXTURE_WRAP_T: 0x2803,
    TEXTURE_MIN_FILTER: 0x2801,
    TEXTURE_MAG_FILTER: 0x2800,
    CLAMP_TO_EDGE: 0x812f,
    LINEAR: 0x2601,
    RGBA: 0x1908,
    UNSIGNED_BYTE: 0x1401,
  } as unknown as WebGL2RenderingContext;
}

describe("RasterTileSource", () => {
  it("should load a tile and create a texture", async () => {
    const gl = createMockGL();
    const source = new RasterTileSource(gl, "https://example.com/{z}/{x}/{y}.png");

    // Mock fetch to return a 1x1 PNG-like blob
    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        blob: () => Promise.resolve(new Blob([new Uint8Array(1)], { type: "image/png" })),
      } as Response)
    );

    // Mock Image constructor
    const originalImage = global.Image;
    global.Image = vi.fn(function() {
      const img: any = {};
      Object.defineProperty(img, "crossOrigin", { set: () => {} });
      Object.defineProperty(img, "onload", {
        set: (fn: () => void) => { setTimeout(fn, 0); }
      });
      Object.defineProperty(img, "onerror", { set: () => {} });
      Object.defineProperty(img, "src", { set: () => {} });
      return img;
    }) as any;

    await source.loadTile(0, 0, 0);
    expect(source.getCacheSize()).toBe(1);
    expect(source.getTileTexture(0, 0, 0)).toBeTruthy();

    global.Image = originalImage;
  });

  it("should unload a tile and remove from cache", async () => {
    const gl = createMockGL();
    const source = new RasterTileSource(gl, "https://example.com/{z}/{x}/{y}.png");

    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        blob: () => Promise.resolve(new Blob([new Uint8Array(1)], { type: "image/png" })),
      } as Response)
    );

    const originalImage = global.Image;
    global.Image = vi.fn(function() {
      const img: any = {};
      Object.defineProperty(img, "crossOrigin", { set: () => {} });
      Object.defineProperty(img, "onload", {
        set: (fn: () => void) => { setTimeout(fn, 0); }
      });
      Object.defineProperty(img, "onerror", { set: () => {} });
      Object.defineProperty(img, "src", { set: () => {} });
      return img;
    }) as any;

    await source.loadTile(1, 1, 1);
    source.unloadTile(1, 1, 1);
    expect(source.getTileTexture(1, 1, 1)).toBeNull();
    expect(source.getCacheSize()).toBe(0);

    global.Image = originalImage;
  });

  it("should evict oldest tile on LRU overflow", async () => {
    const gl = createMockGL();
    const source = new RasterTileSource(gl, "https://example.com/{z}/{x}/{y}.png", 2);

    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        blob: () => Promise.resolve(new Blob([new Uint8Array(1)], { type: "image/png" })),
      } as Response)
    );

    const originalImage = global.Image;
    global.Image = vi.fn(function() {
      const img: any = {};
      Object.defineProperty(img, "crossOrigin", { set: () => {} });
      Object.defineProperty(img, "onload", {
        set: (fn: () => void) => { setTimeout(fn, 0); }
      });
      Object.defineProperty(img, "onerror", { set: () => {} });
      Object.defineProperty(img, "src", { set: () => {} });
      return img;
    }) as any;

    await source.loadTile(0, 0, 0);
    await source.loadTile(1, 1, 1);
    await source.loadTile(2, 2, 2);

    expect(source.getCacheSize()).toBe(2);
    expect(source.getTileTexture(0, 0, 0)).toBeNull();

    global.Image = originalImage;
  });

  it("should clear all tiles", async () => {
    const gl = createMockGL();
    const source = new RasterTileSource(gl, "https://example.com/{z}/{x}/{y}.png");

    global.fetch = vi.fn(() =>
      Promise.resolve({
        ok: true,
        blob: () => Promise.resolve(new Blob([new Uint8Array(1)], { type: "image/png" })),
      } as Response)
    );

    const originalImage = global.Image;
    global.Image = vi.fn(function() {
      const img: any = {};
      Object.defineProperty(img, "crossOrigin", { set: () => {} });
      Object.defineProperty(img, "onload", {
        set: (fn: () => void) => { setTimeout(fn, 0); }
      });
      Object.defineProperty(img, "onerror", { set: () => {} });
      Object.defineProperty(img, "src", { set: () => {} });
      return img;
    }) as any;

    await source.loadTile(0, 0, 0);
    source.clearCache();
    expect(source.getCacheSize()).toBe(0);

    global.Image = originalImage;
  });
});
