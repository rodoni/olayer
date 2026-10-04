export type OgcBounds = readonly [minX: number, minY: number, maxX: number, maxY: number];

export interface WmsMapRequestOptions {
  readonly endpoint: string;
  readonly layers: string;
  readonly crs: string;
  readonly bounds: OgcBounds;
  readonly width: number;
  readonly height: number;
  readonly styles?: string;
  readonly format?: string;
  readonly transparent?: boolean;
}

export interface WfsFeatureRequestOptions {
  readonly endpoint: string;
  readonly typeName: string;
  readonly crs: string;
  readonly bbox?: OgcBounds;
  readonly outputFormat?: string;
  readonly count?: number;
}

export interface WcsCoverageRequestOptions {
  readonly endpoint: string;
  readonly coverageId: string;
  readonly crs: string;
  readonly bounds: OgcBounds;
  readonly width: number;
  readonly height: number;
  readonly format?: string;
}

function encode(value: string): string {
  return encodeURIComponent(value);
}

function validateEndpoint(endpoint: string): void {
  if (endpoint.length === 0) throw new TypeError("OGC endpoint is required");
}

function validateBounds(bounds: OgcBounds): void {
  if (!bounds.every(Number.isFinite) || bounds[0] >= bounds[2] || bounds[1] >= bounds[3]) {
    throw new RangeError("OGC bounds must be finite and ordered");
  }
}

function validateDimensions(width: number, height: number): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new RangeError("OGC dimensions must be positive integers");
  }
}

function queryUrl(endpoint: string, entries: ReadonlyArray<readonly [string, string]>): string {
  validateEndpoint(endpoint);
  const query = entries.map(([key, value]) => `${key}=${encode(value)}`).join("&");
  const separator = endpoint.includes("?") ? (endpoint.endsWith("?") || endpoint.endsWith("&") ? "" : "&") : "?";
  return `${endpoint}${separator}${query}`;
}

/** Builds a deterministic WMS 1.3.0 GetMap URL. */
export function buildWmsGetMapUrl(options: WmsMapRequestOptions): string {
  validateBounds(options.bounds);
  validateDimensions(options.width, options.height);
  if (options.layers.length === 0 || options.crs.length === 0) throw new TypeError("WMS layers and CRS are required");
  return queryUrl(options.endpoint, [
    ["SERVICE", "WMS"],
    ["REQUEST", "GetMap"],
    ["VERSION", "1.3.0"],
    ["LAYERS", options.layers],
    ["STYLES", options.styles ?? ""],
    ["CRS", options.crs],
    ["BBOX", options.bounds.join(",")],
    ["WIDTH", String(options.width)],
    ["HEIGHT", String(options.height)],
    ["FORMAT", options.format ?? "image/png"],
    ["TRANSPARENT", String(options.transparent ?? true)],
  ]);
}

/** Builds a deterministic WFS 2.0.0 GetFeature URL. */
export function buildWfsGetFeatureUrl(options: WfsFeatureRequestOptions): string {
  if (options.typeName.length === 0 || options.crs.length === 0) throw new TypeError("WFS type name and CRS are required");
  if (options.bbox) validateBounds(options.bbox);
  if (options.count !== undefined && (!Number.isInteger(options.count) || options.count <= 0)) throw new RangeError("WFS count must be a positive integer");
  const entries: Array<readonly [string, string]> = [
    ["SERVICE", "WFS"],
    ["REQUEST", "GetFeature"],
    ["VERSION", "2.0.0"],
    ["TYPENAMES", options.typeName],
    ["SRSNAME", options.crs],
    ["OUTPUTFORMAT", options.outputFormat ?? "application/geo+json"],
  ];
  if (options.bbox) entries.push(["BBOX", options.bbox.join(",")]);
  if (options.count !== undefined) entries.push(["COUNT", String(options.count)]);
  return queryUrl(options.endpoint, entries);
}

/** Builds a deterministic WCS 2.0.1 GetCoverage URL. */
export function buildWcsGetCoverageUrl(options: WcsCoverageRequestOptions): string {
  validateBounds(options.bounds);
  validateDimensions(options.width, options.height);
  if (options.coverageId.length === 0 || options.crs.length === 0) throw new TypeError("WCS coverage ID and CRS are required");
  return queryUrl(options.endpoint, [
    ["SERVICE", "WCS"],
    ["REQUEST", "GetCoverage"],
    ["VERSION", "2.0.1"],
    ["COVERAGEID", options.coverageId],
    ["SUBSET", `x(${options.bounds[0]},${options.bounds[2]})`],
    ["SUBSET", `y(${options.bounds[1]},${options.bounds[3]})`],
    ["SUBSETTINGCRS", options.crs],
    ["FORMAT", options.format ?? "image/tiff"],
    ["SIZE", `x(${options.width})`],
    ["SIZE", `y(${options.height})`],
  ]);
}
