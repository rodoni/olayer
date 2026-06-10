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
  loadTile(x: number, y: number, z?: number): Promise<void>;

  /**
   * Unloads a tile from the cache and releases associated resources.
   */
  unloadTile(x: number, y: number, z?: number): void;

  /**
   * Clears the local provider cache.
   */
  clearCache(): void;
}
