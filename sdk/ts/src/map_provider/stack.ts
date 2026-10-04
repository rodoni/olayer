import type { MapDataSource, TileCacheStats } from "./datasource";

interface CacheSizeSource extends MapDataSource {
  getCacheSize(): number;
}

function hasCacheSize(source: MapDataSource): source is CacheSizeSource {
  return "getCacheSize" in source && typeof source.getCacheSize === "function";
}

/**
 * Orchestrator for the Map Data Stack.
 * Registers multiple data sources (such as TerrainTileSource, RasterTileSource, VectorTileSource)
 * and provides a unified interface for cache clearing and retrieval.
 */
export class MapDataStack {
  private sources: Map<string, MapDataSource> = new Map();

  /**
   * Registers a data source in the stack.
   */
  public registerSource(source: MapDataSource): void {
    this.sources.set(source.id, source);
  }

  /**
   * Retrieves a registered data source by its identifier.
   */
  public getSource(id: string): MapDataSource | null {
    return this.sources.get(id) ?? null;
  }

  /**
   * Clears the caches of all registered data sources.
   */
  public clearCache(): void {
    for (const source of this.sources.values()) {
      source.clearCache();
    }
  }

  /**
   * Unloads all resources and removes references.
   */
  public destroy(): void {
    this.clearCache();
    this.sources.clear();
  }

  /**
   * Returns the aggregate size of all source caches.
   */
  public getCacheSize(): number {
    let size = 0;
    for (const source of this.sources.values()) {
      if (hasCacheSize(source)) size += source.getCacheSize();
    }
    return size;
  }

  public getCacheStats(): TileCacheStats {
    const total: TileCacheStats = { items: 0, bytes: 0, hits: 0, misses: 0, evictions: 0 };
    for (const source of this.sources.values()) {
      const stats = source.getCacheStats?.();
      if (!stats) continue;
      total.items += stats.items;
      total.bytes += stats.bytes;
      total.hits += stats.hits;
      total.misses += stats.misses;
      total.evictions += stats.evictions;
    }
    return total;
  }
}
export default MapDataStack;
