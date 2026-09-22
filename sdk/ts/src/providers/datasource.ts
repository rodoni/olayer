/**
 * Common interface for all map data providers (raster, vector tiles, terrain elevation).
 */
export interface MapDataSource {
  id: string;
  
  /**
   * Loads a tile asynchronously.
   * For OSM/WMTS/MVT: x, y are tile grid coordinates, z is the zoom level.
   * For Terrain/DTED: x and y represent lat/lon degrees, and z is unused.
   */
  loadTile(x: number, y: number, z?: number, options?: TileRequestOptions): Promise<void>;

  /**
   * Unloads a tile from the cache and releases associated resources.
   */
  unloadTile(x: number, y: number, z?: number): void;

  /**
   * Clears the local provider cache.
   */
  clearCache(): void;

  getCacheStats?(): TileCacheStats;
}

/** Structured telemetry sink used by providers for recoverable load failures. */
export interface ProviderLogger {
  error(message: string, context: Readonly<Record<string, unknown>>): void;
}

/** Default logger which preserves library silence unless an application opts in. */
export const SILENT_PROVIDER_LOGGER: ProviderLogger = {
  error: () => undefined,
};

export interface TileRequestOptions {
  signal?: AbortSignal;
  maxRetries?: number;
}

export interface TileCacheStats {
  items: number;
  bytes: number;
  hits: number;
  misses: number;
  evictions: number;
}
