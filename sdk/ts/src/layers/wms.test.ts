import { afterEach, describe, expect, it, vi } from "vitest";
import { WmsLayer } from "./wms";
import { WmsMapSource } from "../map_provider/ogc_sources";
import { OgcHttpClient } from "../map_provider/ogc_client";
import { buildWmsGetMapUrl } from "../map_provider/ogc";

const options = { endpoint: "https://fixture.test/wms", layers: "quadrants", crs: "EPSG:4326", bounds: [-2, -1, 2, 1] as const, width: 2, height: 2 };
function bitmap(width = 2) {
  return { width, height: 2, close: vi.fn() } as unknown as ImageBitmap;
}
function source() { return new WmsMapSource(new OgcHttpClient({ maxRetries: 0 }), options); }
function context() {
  const gl = {
    createTexture: vi.fn(() => ({})), deleteTexture: vi.fn(), bindTexture: vi.fn(),
    texParameteri: vi.fn(), pixelStorei: vi.fn(), texImage2D: vi.fn(),
    getParameter: vi.fn(() => 4096),
  };
  return { gl: gl as unknown as WebGL2RenderingContext, spies: gl };
}
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

describe("WMS image loading and ownership", () => {
  it("serializes geographic axis order at the request boundary", () => {
    expect(new URL(buildWmsGetMapUrl(options)).searchParams.get("BBOX")).toBe("-1,-2,1,2");
    expect(new URL(buildWmsGetMapUrl({ ...options, crs: "EPSG:3857" })).searchParams.get("BBOX")).toBe("-2,-1,2,1");
  });

  it("fetches an image with immutable placement metadata and closes invalid dimensions", async () => {
    const image = bitmap();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(new Uint8Array([1]))));
    vi.stubGlobal("createImageBitmap", vi.fn(async () => image));
    const mutableBounds: [number, number, number, number] = [-2, -1, 2, 1];
    const provider = new WmsMapSource(new OgcHttpClient({ maxRetries: 0 }), { ...options, bounds: mutableBounds });
    mutableBounds[0] = 20;
    const result = await provider.load();
    expect(result.bounds).toEqual([-2, -1, 2, 1]);
    expect(image.close).not.toHaveBeenCalled();
    const invalid = bitmap(3);
    vi.stubGlobal("createImageBitmap", vi.fn(async () => invalid));
    await expect(provider.load()).rejects.toThrow("dimensions");
    expect(invalid.close).toHaveBeenCalledOnce();
    image.close();
  });

  it("closes decoded images if cancellation arrives while decoding", async () => {
    const abort = new AbortController();
    const image = bitmap();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(new Uint8Array([1]))));
    vi.stubGlobal("createImageBitmap", vi.fn(async () => { abort.abort(); return image; }));
    await expect(source().load(abort.signal)).rejects.toThrow();
    expect(image.close).toHaveBeenCalledOnce();
  });

  it("replaces textures and releases images immediately after upload", async () => {
    const provider = source();
    const first = bitmap(), second = bitmap();
    vi.spyOn(provider, "load").mockResolvedValueOnce({ bitmap: first, crs: options.crs, bounds: options.bounds })
      .mockResolvedValueOnce({ bitmap: second, crs: options.crs, bounds: options.bounds });
    const { gl, spies } = context();
    const layer = new WmsLayer("wms", provider);
    await layer.load(gl);
    await layer.load(gl);
    expect(first.close).toHaveBeenCalledOnce();
    expect(second.close).toHaveBeenCalledOnce();
    expect(spies.texImage2D).toHaveBeenCalledTimes(2);
    expect(spies.deleteTexture).toHaveBeenCalledTimes(1);
    layer.destroy(gl);
    layer.destroy(gl);
    expect(spies.deleteTexture).toHaveBeenCalledTimes(2);
  });

  it("does not recreate GPU resources when a pending load finishes after destruction", async () => {
    const provider = source();
    let finish!: (value: Awaited<ReturnType<WmsMapSource["load"]>>) => void;
    vi.spyOn(provider, "load").mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const { gl, spies } = context();
    const layer = new WmsLayer("wms", provider);
    const loading = layer.load(gl);
    layer.destroy(gl);
    const image = bitmap();
    finish({ bitmap: image, crs: options.crs, bounds: options.bounds });
    await loading;
    expect(image.close).toHaveBeenCalledOnce();
    expect(spies.createTexture).not.toHaveBeenCalled();
  });

  it("releases failed uploads while keeping the previous image", async () => {
    const provider = source();
    const first = bitmap(), failed = bitmap();
    vi.spyOn(provider, "load").mockResolvedValueOnce({ bitmap: first, crs: options.crs, bounds: options.bounds })
      .mockResolvedValueOnce({ bitmap: failed, crs: options.crs, bounds: options.bounds });
    const { gl, spies } = context();
    const layer = new WmsLayer("wms", provider);
    await layer.load(gl);
    spies.texImage2D.mockImplementationOnce(() => { throw new Error("upload failed"); });
    await expect(layer.load(gl)).rejects.toThrow("upload failed");
    expect(failed.close).toHaveBeenCalledOnce();
    expect(spies.deleteTexture).toHaveBeenCalledTimes(1);
    layer.destroy(gl);
    expect(spies.deleteTexture).toHaveBeenCalledTimes(2);
  });
});
