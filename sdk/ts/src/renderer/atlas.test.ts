import { describe, it, expect, vi } from "vitest";
import { TextureAtlasManager, SymbolUV } from "./atlas";

function createMockGL(): WebGL2RenderingContext {
  const texture = { id: Math.random() } as unknown as WebGLTexture;
  return {
    createTexture: vi.fn(() => texture),
    bindTexture: vi.fn(),
    texParameteri: vi.fn(),
    texImage2D: vi.fn(),
    texSubImage2D: vi.fn(),
    pixelStorei: vi.fn(),
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
    UNPACK_PREMULTIPLY_ALPHA_WEBGL: 0x9240,
  } as unknown as WebGL2RenderingContext;
}

describe("TextureAtlasManager", () => {
  it("should create an atlas with correct size", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 1024);
    expect(atlas.getTexture()).toBeTruthy();
    expect(gl.createTexture).toHaveBeenCalledOnce();
  });

  it("should register a symbol and return UVs", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 512);

    const uv = atlas.registerSymbol("test", 64, 64, (ctx) => {
      ctx.fillStyle = "red";
      ctx.fillRect(0, 0, 64, 64);
    });

    expect(uv.u0).toBe(0);
    expect(uv.v0).toBe(0);
    expect(uv.u1).toBe(64 / 512);
    expect(uv.v1).toBe(64 / 512);
    expect(atlas.getSymbolUV("test")).toEqual(uv);
  });

  it("should throw when atlas is full", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 64);

    atlas.registerSymbol("s1", 32, 32, () => {});
    // atlas is 64x64; s2 wraps to next shelf (y=34) and overflows (34+32+2=68>64)
    expect(() => atlas.registerSymbol("s2", 32, 32, () => {})).toThrow(/full/);
  });

  it("should reuse existing symbol UVs", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 512);

    const uv1 = atlas.registerSymbol("sym", 32, 32, () => {});
    const uv2 = atlas.registerSymbol("sym", 32, 32, () => {});
    expect(uv1).toBe(uv2);
  });

  it("should register a WASM symbol correctly", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 512);

    const mockRegistry = {
      resolve_symbol: vi.fn(() => ({
        bbox: [-10, -10, 10, 10],
        anchor: [0, 0],
        primitives: [
          {
            type: "Circle",
            cx: 0,
            cy: 0,
            r: 5,
            fill: { r: 255, g: 0, b: 0, a: 255 },
          },
          {
            type: "Path",
            commands: "M -10 -10 L 10 10",
            stroke: { color: { r: 0, g: 0, b: 0, a: 255 }, width: 2 },
          },
          {
            type: "Text",
            content: "X",
            offset_x: 0,
            offset_y: 0,
            font_size: 10,
            color: { r: 255, g: 255, b: 255, a: 255 },
          }
        ]
      }))
    };
    const mockStyle = {};

    const uv = atlas.registerWasmSymbol("test_wasm_sym", mockRegistry, mockStyle);
    expect(uv).toBeTruthy();
    expect(mockRegistry.resolve_symbol).toHaveBeenCalledWith("test_wasm_sym", mockStyle);
    expect(uv.width).toBe(24);
    expect(uv.height).toBe(24);
  });

  it("should destroy the WebGL texture", () => {
    const gl = createMockGL();
    const atlas = new TextureAtlasManager(gl, 512);
    const texture = atlas.getTexture();

    atlas.destroy();
    expect(gl.deleteTexture).toHaveBeenCalledWith(texture);
    expect(atlas.getTexture()).toBeNull();
  });
});
