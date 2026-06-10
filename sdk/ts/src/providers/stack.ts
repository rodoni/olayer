import { MapDataSource } from "./datasource";

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
   * Retrieves a registered data source by its identifier, casted to its specific type.
   */
  public getSource<T extends MapDataSource>(id: string): T | null {
    return (this.sources.get(id) as T) || null;
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
      if ("getCacheSize" in source && typeof (source as any).getCacheSize === "function") {
        size += (source as any).getCacheSize();
      } else if ("getCacheSize" in source) {
        // Fallback for custom objects
        size += (source as any).tileCache?.size || 0;
      }
    }
    return size;
  }
}
export default MapDataStack;
