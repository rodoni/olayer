import { retryTileRequest, throwIfAborted } from "./tile_cache";

export interface OgcClientOptions {
  readonly maxRetries?: number;
  readonly timeoutMs?: number;
  readonly maxResponseBytes?: number;
}

export interface WfsFeature {
  readonly id?: string;
  readonly geometry: unknown;
  readonly properties: Readonly<Record<string, unknown>>;
}

export interface WfsFeatureCollection {
  readonly crs?: string;
  readonly features: readonly WfsFeature[];
}

export interface WcsJsonCoverage {
  readonly width: number;
  readonly height: number;
  readonly samples: readonly number[];
  readonly nodata: number | null;
}

export class OgcHttpError extends Error {
  public constructor(readonly status: number, url: string) {
    super(`OGC request failed with HTTP ${status}: ${url}`);
    this.name = "OgcHttpError";
  }
}

export class OgcResponseLimitError extends Error {
  public constructor(readonly maxBytes: number) {
    super(`OGC response exceeds the ${maxBytes}-byte limit`);
    this.name = "OgcResponseLimitError";
  }
}

export class OgcTimeoutError extends Error {
  public constructor(readonly timeoutMs: number) {
    super(`OGC request timed out after ${timeoutMs}ms`);
    this.name = "OgcTimeoutError";
  }
}

export class OgcImageDecodeError extends Error {
  public constructor(message: string) {
    super(message);
    this.name = "OgcImageDecodeError";
  }
}

const DEFAULT_MAX_RETRIES = 2;
const DEFAULT_MAX_RESPONSE_BYTES = 64 * 1024 * 1024;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isPositiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

function decodeWfsFeatureCollection(value: unknown): WfsFeatureCollection {
  if (!isRecord(value) || value["type"] !== "FeatureCollection" || !Array.isArray(value["features"])) {
    throw new TypeError("WFS response must be a GeoJSON FeatureCollection");
  }
  const features = value["features"].map((rawFeature): WfsFeature => {
    if (!isRecord(rawFeature) || rawFeature["type"] !== "Feature") {
      throw new TypeError("WFS feature must have type Feature");
    }
    const properties = rawFeature["properties"];
    if (properties !== null && properties !== undefined && !isRecord(properties)) {
      throw new TypeError("WFS feature properties must be an object or null");
    }
    const rawId = rawFeature["id"];
    const id = typeof rawId === "string" || typeof rawId === "number" ? String(rawId) : undefined;
    return {
      ...(id === undefined ? {} : { id }),
      geometry: rawFeature["geometry"] ?? null,
      properties: properties === null || properties === undefined ? {} : properties,
    };
  });
  const metadata = value["crs"];
  if (metadata !== undefined && metadata !== null) {
    if (!isRecord(metadata) || metadata["type"] !== "name" || !isRecord(metadata["properties"]) || typeof metadata["properties"]["name"] !== "string") {
      throw new TypeError("Unsupported WFS CRS metadata");
    }
    const name = metadata["properties"]["name"];
    const match = /^(?:EPSG:|urn:ogc:def:crs:EPSG::|https?:\/\/www\.opengis\.net\/def\/crs\/EPSG\/0\/)([1-9][0-9]*)$/.exec(name);
    if (!match) throw new TypeError("Unsupported WFS CRS identifier");
    return { features, crs: `EPSG:${match[1]}` };
  }
  return { features };
}

function decodeWcsJsonCoverage(value: unknown): WcsJsonCoverage {
  if (!isRecord(value)) throw new TypeError("WCS coverage must be an object");
  const rawWidth = value["width"];
  const rawHeight = value["height"];
  const rawSamples = value["samples"];
  if (!isPositiveInteger(rawWidth) || !isPositiveInteger(rawHeight) || !Array.isArray(rawSamples)) {
    throw new TypeError("WCS coverage dimensions or samples are invalid");
  }
  if (rawSamples.length !== rawWidth * rawHeight || !rawSamples.every(isFiniteNumber)) {
    throw new TypeError("WCS sample count or values are invalid");
  }
  const nodata = value["nodata"];
  if (nodata !== undefined && nodata !== null && (typeof nodata !== "number" || !Number.isFinite(nodata))) {
    throw new TypeError("WCS nodata must be a finite number or null");
  }
  return { width: rawWidth, height: rawHeight, samples: rawSamples, nodata: nodata ?? null };
}

/** Executes bounded, cancellable OGC HTTP requests and validates JSON responses. */
export class OgcHttpClient {
  readonly #maxRetries: number;
  readonly #timeoutMs: number | undefined;
  readonly #maxResponseBytes: number;

  public constructor(options: OgcClientOptions = {}) {
    const maxRetries = options.maxRetries ?? DEFAULT_MAX_RETRIES;
    const maxResponseBytes = options.maxResponseBytes ?? DEFAULT_MAX_RESPONSE_BYTES;
    if (!Number.isInteger(maxRetries) || maxRetries < 0) throw new RangeError("maxRetries must be non-negative");
    if (options.timeoutMs !== undefined && (!Number.isInteger(options.timeoutMs) || options.timeoutMs <= 0)) throw new RangeError("timeoutMs must be positive");
    if (!Number.isSafeInteger(maxResponseBytes) || maxResponseBytes <= 0) throw new RangeError("maxResponseBytes must be a positive safe integer");
    this.#maxRetries = maxRetries;
    this.#timeoutMs = options.timeoutMs;
    this.#maxResponseBytes = maxResponseBytes;
  }

  public async fetchBytes(url: string, signal?: AbortSignal): Promise<Uint8Array> {
    return retryTileRequest(() => this.#fetchOnce(url, signal), this.#maxRetries, signal);
  }

  public async fetchWfs(url: string, signal?: AbortSignal): Promise<WfsFeatureCollection> {
    const bytes = await this.fetchBytes(url, signal);
    const text = new TextDecoder().decode(bytes);
    return decodeWfsFeatureCollection(JSON.parse(text) as unknown);
  }

  public async fetchWcsJson(url: string, signal?: AbortSignal): Promise<WcsJsonCoverage> {
    const bytes = await this.fetchBytes(url, signal);
    return decodeWcsJsonCoverage(JSON.parse(new TextDecoder().decode(bytes)) as unknown);
  }

  public async fetchImageBitmap(url: string, mimeType: string, signal?: AbortSignal): Promise<ImageBitmap> {
    if (typeof createImageBitmap !== "function") {
      throw new OgcImageDecodeError("ImageBitmap decoding is unavailable in this runtime");
    }
    const bytes = await this.fetchBytes(url, signal);
    try {
      const imageBytes = new Uint8Array(bytes.byteLength);
      imageBytes.set(bytes);
      return await createImageBitmap(new Blob([imageBytes.buffer], { type: mimeType }));
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : "unknown image decoding error";
      throw new OgcImageDecodeError(message);
    }
  }

  async #fetchOnce(url: string, signal?: AbortSignal): Promise<Uint8Array> {
    throwIfAborted(signal);
    const controller = this.#timeoutMs === undefined ? undefined : new AbortController();
    const onAbort = (): void => controller?.abort();
    let timer: ReturnType<typeof setTimeout> | undefined;
    if (controller && signal) signal.addEventListener("abort", onAbort, { once: true });
    if (controller) timer = setTimeout(() => controller.abort(), this.#timeoutMs);
    try {
      const requestSignal = controller?.signal ?? signal;
      const requestInit: RequestInit = requestSignal === undefined ? {} : { signal: requestSignal };
      const response = await fetch(url, requestInit);
      if (!response.ok) throw new OgcHttpError(response.status, url);
      const contentLength = response.headers.get("content-length");
      if (contentLength !== null && Number(contentLength) > this.#maxResponseBytes) {
        void response.body?.cancel().catch(() => undefined);
        throw new OgcResponseLimitError(this.#maxResponseBytes);
      }
      if (response.body === null) return new Uint8Array();
      const reader = response.body.getReader();
      const chunks: Uint8Array[] = [];
      let total = 0;
      try {
        while (true) {
          throwIfAborted(requestSignal);
          const next = await reader.read();
          throwIfAborted(requestSignal);
          if (next.done) break;
          if (next.value.byteLength > this.#maxResponseBytes - total) {
            throw new OgcResponseLimitError(this.#maxResponseBytes);
          }
          total += next.value.byteLength;
          chunks.push(next.value);
        }
      } catch (error: unknown) {
        // Preserve the original error even when transport cleanup itself fails.
        void reader.cancel().catch(() => undefined);
        throw error;
      } finally {
        reader.releaseLock();
      }
      const result = new Uint8Array(total);
      let offset = 0;
      for (const chunk of chunks) {
        result.set(chunk, offset);
        offset += chunk.byteLength;
      }
      return result;
    } catch (error: unknown) {
      if (controller?.signal.aborted && !signal?.aborted) throw new OgcTimeoutError(this.#timeoutMs ?? 0);
      throw error;
    } finally {
      if (timer !== undefined) clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
    }
  }
}
