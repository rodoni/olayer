import { WasmTerrainEngine } from "olayer-wasm";
import { MapDataSource } from "./datasource";

/**
 * Dynamic provider for DTED (Digital Terrain Elevation Data) tiles.
 * Connects directly to the WASM TerrainEngine and manages WebAssembly heap deallocation.
 */
export class TerrainTileSource implements MapDataSource {
  public readonly id: string = "terrain_dted";
  private terrainEngine: WasmTerrainEngine;
  private terrainCache: Map<string, Uint8Array> = new Map(); // Key format: "lat,lon"
  private maxTiles: number;
  private urlResolver: string | ((lat: number, lon: number) => string);

  constructor(
    terrainEngine: WasmTerrainEngine,
    urlResolver: string | ((lat: number, lon: number) => string) = "",
    maxTiles: number = 9
  ) {
    this.terrainEngine = terrainEngine;
    this.urlResolver = urlResolver;
    this.maxTiles = maxTiles;
  }

  /**
   * Loads a DTED tile at geographical coordinates.
   * If a URL resolver is configured, it fetches the tile from the server,
   * otherwise it assumes mock loading or manual injection.
   */
  public async loadTile(lat: number, lon: number, _unused?: number): Promise<void> {
    const key = `${lat},${lon}`;

    // LRU hit: mark as most recently used by re-inserting
    if (this.terrainCache.has(key)) {
      const bytes = this.terrainCache.get(key)!;
      this.terrainCache.delete(key);
      this.terrainCache.set(key, bytes);
      return;
    }

    if (!this.urlResolver) {
      // If no URL resolver is present, we cannot fetch over the network
      // (Used when injecting tiles manually via mock generator)
      return;
    }

    // Resolve URL
    let url = "";
    if (typeof this.urlResolver === "function") {
      url = this.urlResolver(lat, lon);
    } else {
      const latChar = lat < 0 ? "S" : "N";
      const lonChar = lon < 0 ? "W" : "E";
      const latStr = `${Math.abs(Math.round(lat)).toString().padStart(2, "0")}0000${latChar}`;
      const lonStr = `${Math.abs(Math.round(lon)).toString().padStart(3, "0")}0000${lonChar}`;

      url = this.urlResolver
        .replace("{lat}", lat.toString())
        .replace("{lon}", lon.toString())
        .replace("{latStr}", latStr)
        .replace("{lonStr}", lonStr);
    }

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Failed to fetch DTED tile: HTTP ${response.status}`);
      }

      const buffer = await response.arrayBuffer();
      const bytes = new Uint8Array(buffer);

      // Evict oldest tile if cache is full (FIFO on Map keys behaves like LRU)
      if (this.terrainCache.size >= this.maxTiles) {
        const oldestKey = this.terrainCache.keys().next().value;
        if (oldestKey) {
          const [oldLat, oldLon] = oldestKey.split(",").map(Number);
          // Unload from WASM TerrainEngine
          this.terrainEngine.unload_tile(oldLat, oldLon);
          // Delete from local JS cache
          this.terrainCache.delete(oldestKey);
          console.log(`LRU Eviction: Unloaded tile [lat: ${oldLat}, lon: ${oldLon}] to free WASM memory.`);
        }
      }

      // Load tile in the WASM engine
      this.terrainEngine.load_tile(bytes);

      // Store in JS cache
      this.terrainCache.set(key, bytes);
    } catch (error) {
      console.error(`Failed to load DTED tile for [${lat}, ${lon}] from ${url}:`, error);
      throw error;
    }
  }

  /**
   * Manually injects a pre-downloaded or mock DTED tile into the cache and WASM engine.
   */
  public injectTile(lat: number, lon: number, bytes: Uint8Array): void {
    const key = `${lat},${lon}`;
    
    if (this.terrainCache.has(key)) {
      this.terrainCache.delete(key);
    }

    if (this.terrainCache.size >= this.maxTiles) {
      const oldestKey = this.terrainCache.keys().next().value;
      if (oldestKey) {
        const [oldLat, oldLon] = oldestKey.split(",").map(Number);
        this.terrainEngine.unload_tile(oldLat, oldLon);
        this.terrainCache.delete(oldestKey);
      }
    }

    this.terrainEngine.load_tile(bytes);
    this.terrainCache.set(key, bytes);
  }

  /**
   * Unloads the tile from cache and WASM heap.
   */
  public unloadTile(lat: number, lon: number, _unused?: number): void {
    const key = `${lat},${lon}`;
    if (this.terrainCache.has(key)) {
      this.terrainEngine.unload_tile(lat, lon);
      this.terrainCache.delete(key);
    }
  }

  /**
   * Unloads all tiles and clears the cache.
   */
  public clearCache(): void {
    for (const key of this.terrainCache.keys()) {
      const [lat, lon] = key.split(",").map(Number);
      this.terrainEngine.unload_tile(lat, lon);
    }
    this.terrainCache.clear();
  }

  /**
   * Returns the current number of tiles stored in the cache.
   */
  public getCacheSize(): number {
    return this.terrainCache.size;
  }
}

// Preserve DataManager alias for backward compatibility
export { TerrainTileSource as DataManager };

export type { MapDataSource } from "./datasource";
export { RasterTileSource } from "./raster";
export { VectorTileSource } from "./vector";
export { MapDataStack } from "./stack";

