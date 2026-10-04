export interface WmtsTile {
  readonly x: number;
  readonly y: number;
  readonly z: number;
}

export interface WmtsTileRequestOptions {
  readonly endpoint: string;
  readonly layer: string;
  readonly style?: string;
  readonly matrixSet?: string;
  readonly format?: string;
}

const DEFAULT_STYLE = "default";
const DEFAULT_MATRIX_SET = "EPSG:900913";
const DEFAULT_FORMAT = "image/png";
const MAX_ZOOM = 31;

function encodeQueryValue(value: string): string {
  return encodeURIComponent(value);
}

function appendQuery(endpoint: string, query: string): string {
  const separator = endpoint.includes("?") ? (endpoint.endsWith("?") || endpoint.endsWith("&") ? "" : "&") : "?";
  return `${endpoint}${separator}${query}`;
}

/**
 * Builds a canonical WMTS GetTile URL without performing network I/O.
 *
 * @param options - WMTS endpoint and layer configuration.
 * @param tile - Non-negative XYZ tile coordinates.
 * @returns A deterministically ordered, escaped WMTS query URL.
 * @throws {RangeError} If tile coordinates are invalid or exceed the matrix range.
 * @throws {TypeError} If endpoint or layer is empty.
 */
export function buildWmtsTileUrl(options: WmtsTileRequestOptions, tile: WmtsTile): string {
  if (options.endpoint.length === 0 || options.layer.length === 0) {
    throw new TypeError("WMTS endpoint and layer are required");
  }
  if (!Number.isInteger(tile.x) || !Number.isInteger(tile.y) || !Number.isInteger(tile.z)) {
    throw new RangeError("WMTS tile coordinates must be integers");
  }
  if (tile.x < 0 || tile.y < 0 || tile.z < 0 || tile.z > MAX_ZOOM) {
    throw new RangeError("WMTS tile coordinates are outside the supported matrix range");
  }
  const matrixSize = 2 ** tile.z;
  if (tile.x >= matrixSize || tile.y >= matrixSize) {
    throw new RangeError("WMTS tile is outside its matrix");
  }

  const matrixSet = options.matrixSet ?? DEFAULT_MATRIX_SET;
  const matrix = `${matrixSet}:${tile.z}`;
  const format = options.format ?? DEFAULT_FORMAT;
  const queryEntries: ReadonlyArray<readonly [string, string]> = [
    ["SERVICE", "WMTS"],
    ["REQUEST", "GetTile"],
    ["VERSION", "1.0.0"],
    ["LAYER", options.layer],
    ["STYLE", options.style ?? DEFAULT_STYLE],
    ["TILEMATRIXSET", matrixSet],
    ["TILEMATRIX", matrix],
    ["TILEROW", String(tile.y)],
    ["TILECOL", String(tile.x)],
    ["FORMAT", format],
  ];
  const query = queryEntries
    .map(([key, value]) => `${key}=${encodeQueryValue(value)}`)
    .join("&");
  return appendQuery(options.endpoint, query);
}
