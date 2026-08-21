import { WasmTerrainEngine } from "olayer-wasm";
import { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";

/**
 * Map data source for Cloud-Optimized GeoTIFF (COG) and standard GeoTIFF elevation rasters.
 * Connects directly to the WASM TerrainEngine, supporting 32-bit floating-point elevation matrices.
 */
export class CogTerrainSource implements MapDataSource {
  public readonly id: string;
  private terrainEngine: WasmTerrainEngine;
  private url: string;
  private loaded: boolean = false;

  constructor(id: string, terrainEngine: WasmTerrainEngine, url: string = "") {
    this.id = id;
    this.terrainEngine = terrainEngine;
    this.url = url;
  }

  /**
   * Loads the GeoTIFF raster over network or from buffer.
   */
  public async loadTile(_x: number = 0, _y: number = 0, _z: number = 0, options: TileRequestOptions = {}): Promise<void> {
    if (this.loaded || !this.url) return;

    const response = await fetch(this.url, { signal: options.signal });
    if (!response.ok) {
      throw new Error(`Failed to fetch GeoTIFF raster: HTTP ${response.status}`);
    }

    const buffer = await response.arrayBuffer();
    this.injectGeoTiff(new Uint8Array(buffer));
  }

  /**
   * Directly injects raw GeoTIFF bytes into WASM terrain engine.
   * Returns geographic bounding box `[minLatDeg, minLonDeg, maxLatDeg, maxLonDeg]`.
   */
  public injectGeoTiff(data: Uint8Array): number[] {
    const bounds = this.terrainEngine.load_geotiff_tile(data) as number[];
    this.loaded = true;
    return bounds;
  }

  public unloadTile(_x?: number, _y?: number, _z?: number): void {
    // GeoTIFFs are cleared via clearCache
  }

  public clearCache(): void {
    this.terrainEngine.clear_geotiff_cache();
    this.loaded = false;
  }

  /**
   * Queries interpolated ground elevation in meters at geographic coordinates in degrees.
   */
  public getElevationAt(latDeg: number, lonDeg: number): number | null {
    try {
      return this.terrainEngine.get_elevation(latDeg, lonDeg);
    } catch {
      return null;
    }
  }

  public getCacheStats(): TileCacheStats {
    return {
      items: this.loaded ? 1 : 0,
      bytes: 0,
      hits: 0,
      misses: 0,
      evictions: 0,
    };
  }
}
