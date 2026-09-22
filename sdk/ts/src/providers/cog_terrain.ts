import { WasmTerrainEngine } from "olayer-wasm";
import { MapDataSource, ProviderLogger, SILENT_PROVIDER_LOGGER, TileCacheStats, TileRequestOptions } from "./datasource";

/**
 * Map data source for Cloud-Optimized GeoTIFF (COG) and standard GeoTIFF elevation rasters.
 * Connects directly to the WASM TerrainEngine, supporting 32-bit floating-point elevation matrices.
 */
export class CogTerrainSource implements MapDataSource {
  public readonly id: string;
  private terrainEngine: WasmTerrainEngine;
  private url: string;
  private loaded: boolean = false;
  private readonly logger: ProviderLogger;

  constructor(id: string, terrainEngine: WasmTerrainEngine, url: string = "", logger: ProviderLogger = SILENT_PROVIDER_LOGGER) {
    this.id = id;
    this.terrainEngine = terrainEngine;
    this.url = url;
    this.logger = logger;
  }

  /**
   * Loads the GeoTIFF raster over network or from buffer.
   */
  public async loadTile(_x: number = 0, _y: number = 0, _z: number = 0, options: TileRequestOptions = {}): Promise<void> {
    if (this.loaded || !this.url) return;
    try {
      const response = await fetch(this.url, options.signal ? { signal: options.signal } : {});
      if (!response.ok) {
        throw new Error(`Failed to fetch GeoTIFF raster: HTTP ${response.status}`);
      }
      const buffer = await response.arrayBuffer();
      this.injectGeoTiff(new Uint8Array(buffer));
    } catch (error) {
      this.logger.error("Failed to load GeoTIFF raster", { url: this.url, error });
      throw error;
    }
  }

  /**
   * Directly injects raw GeoTIFF bytes into WASM terrain engine.
   * Returns geographic bounding box `[minLatDeg, minLonDeg, maxLatDeg, maxLonDeg]`.
   */
  public injectGeoTiff(data: Uint8Array): number[] {
    const rawBounds: unknown = this.terrainEngine.load_geotiff_tile(data);
    if (!Array.isArray(rawBounds) || rawBounds.length !== 4 || !rawBounds.every((value): value is number => typeof value === "number" && Number.isFinite(value))) {
      throw new Error("GeoTIFF loader returned invalid geographic bounds");
    }
    const bounds = rawBounds;
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
