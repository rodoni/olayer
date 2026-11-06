import { buildWcsGetCoverageUrl, buildWfsGetFeatureUrl, buildWmsGetMapUrl } from "./ogc";
import type { OgcBounds, WcsCoverageRequestOptions, WfsFeatureRequestOptions, WmsMapRequestOptions } from "./ogc";
import { OgcHttpClient } from "./ogc_client";
import type { WcsJsonCoverage, WfsFeatureCollection } from "./ogc_client";
import { throwIfAborted } from "./tile_cache";

export interface WmsMapImage {
  /** Owned by the caller, which must close it after upload. */
  readonly bitmap: ImageBitmap;
  readonly crs: string;
  readonly bounds: OgcBounds;
}

/** A bounded WMS image, independent of the display dimension. */
export class WmsMapSource {
  readonly #client: OgcHttpClient;
  readonly #options: WmsMapRequestOptions;

  public constructor(client: OgcHttpClient, options: WmsMapRequestOptions) {
    buildWmsGetMapUrl(options);
    this.#client = client;
    this.#options = { ...options, bounds: [...options.bounds] };
  }

  public async load(signal?: AbortSignal): Promise<WmsMapImage> {
    const bitmap = await this.#client.fetchImageBitmap(
      buildWmsGetMapUrl(this.#options), this.#options.format ?? "image/png", signal,
    );
    try {
      throwIfAborted(signal);
      if (bitmap.width !== this.#options.width || bitmap.height !== this.#options.height) {
        throw new RangeError("WMS image dimensions do not match the request");
      }
      return { bitmap, crs: this.#options.crs, bounds: [...this.#options.bounds] };
    } catch (error: unknown) {
      bitmap.close();
      throw error;
    }
  }
}

export class WfsVectorSource {
  readonly #client: OgcHttpClient;
  readonly #options: WfsFeatureRequestOptions;

  public constructor(client: OgcHttpClient, options: WfsFeatureRequestOptions) {
    buildWfsGetFeatureUrl(options);
    this.#client = client;
    this.#options = { ...options, ...(options.bbox ? { bbox: [...options.bbox] as OgcBounds } : {}) };
  }

  public get crs(): string { return this.#options.crs; }

  public async load(signal?: AbortSignal): Promise<WfsFeatureCollection> {
    const collection = await this.#client.fetchWfs(buildWfsGetFeatureUrl(this.#options), signal);
    throwIfAborted(signal);
    if (collection.crs !== undefined && collection.crs !== this.crs) {
      throw new RangeError("WFS response CRS conflicts with request");
    }
    return collection;
  }
}

export class WcsRasterSource {
  readonly #client: OgcHttpClient;
  readonly #options: WcsCoverageRequestOptions;

  public constructor(client: OgcHttpClient, options: WcsCoverageRequestOptions) {
    buildWcsGetCoverageUrl(options);
    this.#client = client;
    this.#options = { ...options, bounds: [...options.bounds] };
  }

  public get crs(): string { return this.#options.crs; }
  public get bounds(): OgcBounds { return [...this.#options.bounds]; }

  public async loadJson(signal?: AbortSignal): Promise<WcsJsonCoverage> {
    const coverage = await this.#client.fetchWcsJson(buildWcsGetCoverageUrl({
      ...this.#options, format: "application/json",
    }), signal);
    throwIfAborted(signal);
    if (coverage.width !== this.#options.width || coverage.height !== this.#options.height) {
      throw new RangeError("WCS dimensions differ from request");
    }
    return coverage;
  }

  public loadImageBitmap(signal?: AbortSignal): Promise<ImageBitmap> {
    return this.#client.fetchImageBitmap(
      buildWcsGetCoverageUrl(this.#options),
      this.#options.format ?? "image/tiff",
      signal,
    );
  }
}
