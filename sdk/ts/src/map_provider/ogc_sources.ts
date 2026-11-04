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
    this.#client = client;
    this.#options = options;
  }

  public load(signal?: AbortSignal): Promise<WfsFeatureCollection> {
    return this.#client.fetchWfs(buildWfsGetFeatureUrl(this.#options), signal);
  }
}

export class WcsRasterSource {
  readonly #client: OgcHttpClient;
  readonly #options: WcsCoverageRequestOptions;

  public constructor(client: OgcHttpClient, options: WcsCoverageRequestOptions) {
    this.#client = client;
    this.#options = options;
  }

  public loadJson(signal?: AbortSignal): Promise<WcsJsonCoverage> {
    return this.#client.fetchWcsJson(buildWcsGetCoverageUrl(this.#options), signal);
  }

  public loadImageBitmap(signal?: AbortSignal): Promise<ImageBitmap> {
    return this.#client.fetchImageBitmap(
      buildWcsGetCoverageUrl(this.#options),
      this.#options.format ?? "image/tiff",
      signal,
    );
  }
}
