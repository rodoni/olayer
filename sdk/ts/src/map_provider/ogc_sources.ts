import { buildWcsGetCoverageUrl, buildWfsGetFeatureUrl } from "./ogc";
import type { WcsCoverageRequestOptions, WfsFeatureRequestOptions } from "./ogc";
import { OgcHttpClient } from "./ogc_client";
import type { WcsJsonCoverage, WfsFeatureCollection } from "./ogc_client";

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
