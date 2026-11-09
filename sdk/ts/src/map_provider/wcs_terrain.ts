import type { WasmTerrainEngine } from "olayer-wasm";
import { WcsRasterSource } from "./ogc_sources";
import { throwIfAborted } from "./tile_cache";

export type WcsGridRegistration = "node" | "cell-center";
export type WcsElevationUnit = "metre" | "international-foot" | "us-survey-foot";
export type WcsVerticalReference = "ellipsoidal" | "orthometric";

export interface GeoidUndulationProvider {
  /**
   * Returns geoid undulation N in metres, where ellipsoidal height = orthometric height + N.
   * x/y are in request CRS units: EPSG:4326 degrees or EPSG:3857/900913 metres.
   */
  geoidUndulationMeters(x: number, y: number, epsg: number): number | null;
}

export interface WcsElevationInterpretation {
  readonly registration?: WcsGridRegistration;
  readonly unit?: WcsElevationUnit;
  readonly verticalReference?: WcsVerticalReference;
  readonly geoid?: GeoidUndulationProvider;
}

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
    private readonly interpretation: WcsElevationInterpretation = {},
  ) {
    const codes: Readonly<Record<string, number>> = {
      "EPSG:4326": 4326, "EPSG:3857": 3857, "EPSG:900913": 900913,
    };
    const epsg = codes[source.crs.toUpperCase()];
    if (epsg === undefined) throw new RangeError(`Unsupported WCS terrain CRS: ${source.crs}`);
    this.epsg = epsg;
    if (interpretation.verticalReference === "orthometric" && !interpretation.geoid) {
      throw new TypeError("Orthometric WCS heights require an explicit geoid correction provider");
    }
  }

  /** Atomically replaces this source's terrain after a successful, current load. */
  public async load(signal?: AbortSignal): Promise<void> {
    const generation = ++this.generation;
    const coverage = await this.source.loadJson(signal);
    throwIfAborted(signal);
    if (generation !== this.generation) return;
    const bounds = this.registeredBounds(coverage.width, coverage.height);
    const samples = this.ellipsoidalSamples(coverage.width, coverage.height, coverage.samples, coverage.nodata);
    this.engine.load_elevation_grid(this.id, this.epsg, new Float64Array(bounds),
      coverage.width, coverage.height, samples, coverage.nodata ?? undefined);
  }

  /** Invalidates pending loads and removes only this source's grid. */
  public unload(): void {
    this.generation++;
    this.engine.unload_elevation_grid(this.id);
  }

  private registeredBounds(width: number, height: number): readonly [number, number, number, number] {
    const bounds = this.source.bounds;
    if ((this.interpretation.registration ?? "node") === "node") return bounds;
    const [west, south, east, north] = bounds;
    const cellWidth = (east - west) / width;
    const cellHeight = (north - south) / height;
    return [
      width > 1 ? west + cellWidth / 2 : west,
      height > 1 ? south + cellHeight / 2 : south,
      width > 1 ? east - cellWidth / 2 : east,
      height > 1 ? north - cellHeight / 2 : north,
    ];
  }

  private ellipsoidalSamples(width: number, height: number, samples: readonly number[], nodata: number | null): Float64Array {
    const bounds = this.source.bounds;
    const [west, south, east, north] = bounds;
    const registration = this.interpretation.registration ?? "node";
    const unit = this.interpretation.unit ?? "metre";
    const unitToMetres = unit === "metre" ? 1 : unit === "international-foot" ? 0.3048 : 1200 / 3937;
    const verticalReference = this.interpretation.verticalReference ?? "ellipsoidal";
    const converted = new Float64Array(samples.length);
    for (let row = 0; row < height; row++) {
      for (let column = 0; column < width; column++) {
        const index = row * width + column;
        const raw = samples[index];
        if (raw === undefined || !Number.isFinite(raw)) throw new RangeError("WCS elevation sample is not finite");
        if (nodata !== null && raw === nodata) {
          converted[index] = raw;
          continue;
        }
        let heightMeters = raw * unitToMetres;
        if (!Number.isFinite(heightMeters)) throw new RangeError("WCS elevation unit conversion overflowed");
        if (verticalReference === "orthometric") {
          const x = registration === "cell-center"
            ? west + (column + 0.5) * (east - west) / width
            : width === 1 ? (west + east) / 2 : west + column * (east - west) / (width - 1);
          const y = registration === "cell-center"
            ? north - (row + 0.5) * (north - south) / height
            : height === 1 ? (south + north) / 2 : north - row * (north - south) / (height - 1);
          const undulation = this.interpretation.geoid?.geoidUndulationMeters(x, y, this.epsg);
          if (undulation === null || undulation === undefined || !Number.isFinite(undulation)) {
            throw new RangeError("WCS orthometric sample has no valid geoid correction");
          }
          heightMeters += undulation;
          if (!Number.isFinite(heightMeters)) throw new RangeError("WCS geoid conversion overflowed");
        }
        converted[index] = heightMeters;
      }
    }
    return converted;
  }
}
