import { WasmTerrainEngine } from "olayer-wasm";
import { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";
import { retryTileRequest } from "./tile_cache";

export type RgbTerrainEncoding = "Mapbox" | "Terrarium";

export interface RgbTerrainSourceOptions {
  encoding?: RgbTerrainEncoding;
  tileSize?: number;
  maxTiles?: number;
  maxRetries?: number;
}

/**
 * Map data source for Mapbox Terrain-RGB and Mapzen/Nextzen Terrarium elevation tiles.
 * Connects directly to the WASM TerrainEngine, decoding RGB pixels into sub-grid bilinear elevation models.
 */
export class RgbTerrainSource implements MapDataSource {
  public readonly id: string;
  private terrainEngine: WasmTerrainEngine;
  private urlTemplate: string | ((z: number, x: number, y: number) => string);
  public readonly encoding: RgbTerrainEncoding;
  public readonly tileSize: number;
  private maxTiles: number;
  private maxRetries: number;
  private loadedTiles: Set<string> = new Set();
  private readonly pending = new Map<string, Promise<void>>();

  constructor(
    id: string,
    terrainEngine: WasmTerrainEngine,
    urlTemplate: string | ((z: number, x: number, y: number) => string),
    options: RgbTerrainSourceOptions = {}
  ) {
    this.id = id;
    this.terrainEngine = terrainEngine;
    this.urlTemplate = urlTemplate;
    this.encoding = options.encoding ?? "Mapbox";
    this.tileSize = options.tileSize ?? 256;
    this.maxTiles = options.maxTiles ?? 64;
    this.maxRetries = options.maxRetries ?? 2;
  }

  /**
   * Loads a Web Mercator RGB elevation tile at (x, y, z).
   */
  public async loadTile(x: number, y: number, z: number = 0, options: TileRequestOptions = {}): Promise<void> {
    const requestKey = `${z}/${x}/${y}`;

    if (this.loadedTiles.has(requestKey)) {
      return;
    }

    const existing = this.pending.get(requestKey);
    if (existing) return existing;

    const request = this.loadTileInternal(requestKey, z, x, y, options);
    this.pending.set(requestKey, request);
    try {
      await request;
    } finally {
      this.pending.delete(requestKey);
    }
  }

  private async loadTileInternal(
    requestKey: string,
    z: number,
    x: number,
    y: number,
    options: TileRequestOptions
  ): Promise<void> {
    if (!this.urlTemplate) return;

    let url = "";
    if (typeof this.urlTemplate === "function") {
      url = this.urlTemplate(z, x, y);
    } else {
      url = this.urlTemplate
        .replace("{z}", z.toString())
        .replace("{x}", x.toString())
        .replace("{y}", y.toString());
    }

    const response = await retryTileRequest(
      () => fetch(url, { signal: options.signal }),
      options.maxRetries ?? this.maxRetries,
      options.signal
    );

    if (!response.ok) {
      throw new Error(`Failed to fetch RGB terrain tile: HTTP ${response.status}`);
    }

    const arrayBuffer = await response.arrayBuffer();
    const rawBytes = new Uint8Array(arrayBuffer);

    // If tile is already raw RGBA or decoded in worker/browser
    this.injectRgbaTile(z, x, y, rawBytes, this.tileSize, this.tileSize);
    this.loadedTiles.add(requestKey);
  }

  /**
   * Directly injects decoded raw RGBA byte buffer into WASM terrain engine.
   */
  public injectRgbaTile(
    z: number,
    x: number,
    y: number,
    rgbaBytes: Uint8Array,
    width: number = this.tileSize,
    height: number = this.tileSize
  ): void {
    this.terrainEngine.load_rgb_tile(z, x, y, this.encoding, rgbaBytes, width, height);
    this.loadedTiles.add(`${z}/${x}/${y}`);
  }

  /**
   * Unloads an RGB elevation tile from the engine.
   */
  public unloadTile(x: number, y: number, z: number = 0): void {
    const key = `${z}/${x}/${y}`;
    this.terrainEngine.unload_rgb_tile(z, x, y);
    this.loadedTiles.delete(key);
  }

  /**
   * Clears all RGB elevation tiles loaded by this source.
   */
  public clearCache(): void {
    this.terrainEngine.clear_rgb_cache();
    this.loadedTiles.clear();
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
      items: this.loadedTiles.size,
      bytes: this.loadedTiles.size * this.tileSize * this.tileSize * 4,
      hits: 0,
      misses: 0,
      evictions: 0,
    };
  }
}
