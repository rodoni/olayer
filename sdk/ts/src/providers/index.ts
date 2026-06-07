import { WasmTerrainEngine } from "olayer-wasm";

export class DataManager {
  private terrainEngine: WasmTerrainEngine;
  private terrainCache: Map<string, Uint8Array> = new Map(); // Key format: "lat,lon"
  private maxTiles: number;

  constructor(terrainEngine: WasmTerrainEngine, maxTiles: number = 9) {
    this.terrainEngine = terrainEngine;
    this.maxTiles = maxTiles;
  }

  /**
   * Fetches a DTED tile from a URL, registers it in the WASM TerrainEngine,
   * and manages the local LRU cache eviction if capacity is reached.
   */
  public async loadDtedTile(latDeg: number, lonDeg: number, url: string): Promise<void> {
    const key = `${latDeg},${lonDeg}`;

    // If already exists, mark as most recently used by re-inserting
    if (this.terrainCache.has(key)) {
      const bytes = this.terrainCache.get(key)!;
      this.terrainCache.delete(key);
      this.terrainCache.set(key, bytes);
      return;
    }

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Failed to fetch DTED tile: HTTP ${response.status}`);
      }

      const buffer = await response.arrayBuffer();
      const bytes = new Uint8Array(buffer);

      // Evict oldest tile if cache is full (First-In, First-Out on JS Map keys behaves like LRU)
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
      console.error(`Failed to load DTED tile for [${latDeg}, ${lonDeg}] from ${url}:`, error);
      throw error;
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
