import { afterEach, describe, expect, it, vi } from "vitest";
import { OgcHttpClient, OgcHttpError, OgcResponseLimitError } from "./ogc_client";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("OgcHttpClient", () => {
  it("does not wait for pending stream cancellation when rejecting an oversized body", async () => {
    const stream = new ReadableStream<Uint8Array>({
      start(controller) { controller.enqueue(new Uint8Array([1, 2, 3, 4])); },
      cancel() { return new Promise<void>(() => undefined); },
    });
    vi.stubGlobal("fetch", async () => new Response(stream));
    await expect(new OgcHttpClient({ maxRetries: 0, maxResponseBytes: 3 }).fetchBytes("https://example.test"))
      .rejects.toBeInstanceOf(OgcResponseLimitError);
    expect(stream.locked).toBe(false);
  });

  it("preserves the limit error when header-based body cancellation fails", async () => {
    const stream = new ReadableStream<Uint8Array>({
      cancel() { return Promise.reject(new Error("cleanup failed")); },
    });
    vi.stubGlobal("fetch", async () => new Response(stream, { headers: { "content-length": "10" } }));
    await expect(new OgcHttpClient({ maxRetries: 0, maxResponseBytes: 3 }).fetchBytes("https://example.test"))
      .rejects.toBeInstanceOf(OgcResponseLimitError);
  });

  it("accepts chunked data exactly at the byte budget", async () => {
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2]));
        controller.enqueue(new Uint8Array([3]));
        controller.close();
      },
    });
    vi.stubGlobal("fetch", async () => new Response(stream));
    const bytes = await new OgcHttpClient({ maxRetries: 0, maxResponseBytes: 3 }).fetchBytes("https://example.test");
    expect(Array.from(bytes)).toEqual([1, 2, 3]);
  });

  it("cancels oversized streams without buffering the rest", async () => {
    let cancelled = false;
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2]));
        controller.enqueue(new Uint8Array([3, 4]));
      },
      cancel() { cancelled = true; },
    });
    vi.stubGlobal("fetch", async () => new Response(stream));
    await expect(new OgcHttpClient({ maxRetries: 0, maxResponseBytes: 3 }).fetchBytes("https://example.test"))
      .rejects.toBeInstanceOf(OgcResponseLimitError);
    expect(cancelled).toBe(true);
  });

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
