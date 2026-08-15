import { WasmTerrainEngine, WasmTileKey } from "olayer-wasm";
import { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";
import { retryTileRequest, throwIfAborted } from "./tile_cache";

/**
 * Dynamic provider for DTED (Digital Terrain Elevation Data) tiles.
 * Connects directly to the WASM TerrainEngine and manages WebAssembly heap deallocation.
 */
export class TerrainTileSource implements MapDataSource {
  public readonly id: string = "terrain_dted";
  private terrainEngine: WasmTerrainEngine;
  private terrainCache: Map<string, Uint8Array> = new Map(); // requestKey -> bytes
  private tileKeyMap: Map<string, WasmTileKey> = new Map();  // requestKey -> WasmTileKey
  private maxTiles: number;
  private readonly pending = new Map<string, Promise<void>>();
  private readonly invalidated = new Set<string>();
  private generation = 0;
  private urlResolver: string | ((lat: number, lon: number) => string);
  private maxRetries: number;

  constructor(
    terrainEngine: WasmTerrainEngine,
    urlResolver: string | ((lat: number, lon: number) => string) = "",
    maxTiles: number = 9,
    maxRetries: number = 2,
  ) {
    this.terrainEngine = terrainEngine;
    this.urlResolver = urlResolver;
    this.maxTiles = maxTiles;
    if (!Number.isInteger(maxRetries) || maxRetries < 0) {
      throw new Error("Terrain tile maxRetries must be a non-negative integer");
    }
    this.maxRetries = maxRetries;
  }

  /**
   * Loads a DTED tile at geographical coordinates.
   * If a URL resolver is configured, it fetches the tile from the server,
   * otherwise it assumes mock loading or manual injection.
   */
  public async loadTile(lat: number, lon: number, _unused?: number, options: TileRequestOptions = {}): Promise<void> {
    const requestKey = `${lat},${lon}`;

    // LRU hit: mark as most recently used by re-inserting
    if (this.terrainCache.has(requestKey)) {
      const bytes = this.terrainCache.get(requestKey)!;
      this.terrainCache.delete(requestKey);
      this.terrainCache.set(requestKey, bytes);
      return;
    }

    const existing = this.pending.get(requestKey);
    if (existing) return existing;
    this.invalidated.delete(requestKey);
    const requestGeneration = this.generation;
    const request = this.loadTileInternal(requestKey, lat, lon, requestGeneration, options);
    this.pending.set(requestKey, request);
    try { await request; } finally { this.pending.delete(requestKey); }
  }

  private async loadTileInternal(
    requestKey: string,
    lat: number,
    lon: number,
    requestGeneration: number,
    options: TileRequestOptions,
  ): Promise<void> {

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
      const response = await retryTileRequest(
        () => fetch(url, { signal: options.signal }),
        options.maxRetries ?? this.maxRetries,
        options.signal,
      );
      if (!response.ok) {
        throw new Error(`Failed to fetch DTED tile: HTTP ${response.status}`);
      }

      const buffer = await response.arrayBuffer();
      throwIfAborted(options.signal);
      const bytes = new Uint8Array(buffer);

      // Evict oldest tile if cache is full (FIFO on Map keys behaves like LRU)
      if (requestGeneration !== this.generation || this.invalidated.has(requestKey)) return;
      if (this.terrainCache.size >= this.maxTiles) {
        const oldestRequestKey = this.terrainCache.keys().next().value;
        if (oldestRequestKey) {
          const oldestTileKey = this.tileKeyMap.get(oldestRequestKey);
          if (oldestTileKey) {
            this.terrainEngine.unload_tile(oldestTileKey.lat_deg, oldestTileKey.lon_deg);
          }
          this.terrainCache.delete(oldestRequestKey);
          this.tileKeyMap.delete(oldestRequestKey);
        }
      }

      // Load tile in the WASM engine and capture the actual tile key
      const wasmKey = this.terrainEngine.load_tile(bytes);
      this.tileKeyMap.set(requestKey, wasmKey);

      // Store in JS cache
      this.terrainCache.set(requestKey, bytes);
    } catch (error) {
      console.error(`Failed to load DTED tile for [${lat}, ${lon}] from ${url}:`, error);
      throw error;
    }
  }

  /**
   * Manually injects a pre-downloaded or mock DTED tile into the cache and WASM engine.
   */
  public injectTile(lat: number, lon: number, bytes: Uint8Array): void {
    const requestKey = `${lat},${lon}`;

    if (this.terrainCache.has(requestKey)) {
      this.terrainCache.delete(requestKey);
      this.tileKeyMap.delete(requestKey);
    }

    if (this.terrainCache.size >= this.maxTiles) {
      const oldestRequestKey = this.terrainCache.keys().next().value;
      if (oldestRequestKey) {
        const oldestTileKey = this.tileKeyMap.get(oldestRequestKey);
        if (oldestTileKey) {
          this.terrainEngine.unload_tile(oldestTileKey.lat_deg, oldestTileKey.lon_deg);
        }
        this.terrainCache.delete(oldestRequestKey);
        this.tileKeyMap.delete(oldestRequestKey);
      }
    }

    const wasmKey = this.terrainEngine.load_tile(bytes);
    this.tileKeyMap.set(requestKey, wasmKey);
    this.terrainCache.set(requestKey, bytes);
  }

  /**
   * Unloads the tile from cache and WASM heap.
   */
  public unloadTile(lat: number, lon: number, _unused?: number): void {
    const requestKey = `${lat},${lon}`;
    this.invalidated.add(requestKey);
    if (this.terrainCache.has(requestKey)) {
      const tileKey = this.tileKeyMap.get(requestKey);
      if (tileKey) {
        this.terrainEngine.unload_tile(tileKey.lat_deg, tileKey.lon_deg);
      }
      this.terrainCache.delete(requestKey);
      this.tileKeyMap.delete(requestKey);
    }
  }

  /**
   * Unloads all tiles and clears the cache.
   */
  public clearCache(): void {
    this.generation++;
    this.invalidated.clear();
    for (const requestKey of this.terrainCache.keys()) {
      const tileKey = this.tileKeyMap.get(requestKey);
      if (tileKey) {
        this.terrainEngine.unload_tile(tileKey.lat_deg, tileKey.lon_deg);
      }
    }
    this.terrainCache.clear();
    this.tileKeyMap.clear();
  }

  /**
   * Returns the current number of tiles stored in the cache.
   */
  public getCacheSize(): number {
    return this.terrainCache.size;
  }

  public getCacheStats(): TileCacheStats {
    return {
      items: this.terrainCache.size,
      bytes: Array.from(this.terrainCache.values()).reduce((total, bytes) => total + bytes.byteLength, 0),
      hits: 0,
      misses: 0,
      evictions: 0,
    };
  }
}

// Preserve DataManager alias for backward compatibility
export { TerrainTileSource as DataManager };

export type { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";
export { RasterTileSource } from "./raster";
export { VectorTileSource } from "./vector";
export type { VectorFeature, VectorGeometryType, VectorCoordinates, VectorTileSourceOptions } from "./vector";
export { MapDataStack } from "./stack";
