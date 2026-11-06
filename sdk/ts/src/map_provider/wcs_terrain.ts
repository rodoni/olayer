import type { WasmTerrainEngine } from "olayer-wasm";
import { WcsRasterSource } from "./ogc_sources";
import { throwIfAborted } from "./tile_cache";

/** Explicit adapter for north-up elevation node grids in ellipsoidal metres.
 * Scalar scientific grids and orthometric heights need conversion before use.
 */
export class WcsTerrainSource {
  private generation = 0;
  private readonly epsg: number;

  public constructor(
    public readonly id: string,
    private readonly engine: WasmTerrainEngine,
    private readonly source: WcsRasterSource,
  ) {
    const codes: Readonly<Record<string, number>> = {
      "EPSG:4326": 4326, "EPSG:3857": 3857, "EPSG:900913": 900913,
    };
    const epsg = codes[source.crs];
    if (epsg === undefined) throw new RangeError(`Unsupported WCS terrain CRS: ${source.crs}`);
    this.epsg = epsg;
  }

  /** Atomically replaces this source's terrain after a successful, current load. */
  public async load(signal?: AbortSignal): Promise<void> {
    const generation = ++this.generation;
    const coverage = await this.source.loadJson(signal);
    throwIfAborted(signal);
    if (generation !== this.generation) return;
    this.engine.load_elevation_grid(this.id, this.epsg, new Float64Array(this.source.bounds),
      coverage.width, coverage.height, new Float64Array(coverage.samples), coverage.nodata ?? undefined);
  }

  /** Invalidates pending loads and removes only this source's grid. */
  public unload(): void {
    this.generation++;
    this.engine.unload_elevation_grid(this.id);
  }
}
