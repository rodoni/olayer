import { Layer } from "./layer";
import { WasmSigmetDataset } from "olayer-wasm";

export interface SigmetLayerOptions {
  strokeColor?: string;
  fillColor?: string;
  lineWidth?: number;
}

/**
 * Meteorological Warning Layer for SIGMET, AIRMET, and convective hazard polygons.
 * Supports point containment queries, altitude filtering, and GeoJSON ingestion.
 */
export class SigmetLayer extends Layer {
  public strokeColor: string;
  public fillColor: string;
  public lineWidth: number;

  private dataset: WasmSigmetDataset | null = null;
  private geoJsonString: string | null = null;

  constructor(id: string, options: SigmetLayerOptions = {}) {
    super(id);
    this.strokeColor = options.strokeColor ?? "#ff1744"; // Vivid red warning
    this.fillColor = options.fillColor ?? "rgba(255, 23, 68, 0.18)";
    this.lineWidth = options.lineWidth ?? 2.0;
  }

  /**
   * Loads SIGMET hazard polygons from a GeoJSON FeatureCollection string.
   */
  public loadGeoJson(jsonContent: string): void {
    if (this.dataset) {
      this.dataset.free();
    }
    this.dataset = WasmSigmetDataset.from_geojson(jsonContent);
    this.geoJsonString = this.dataset.to_geojson();
  }

  /**
   * Directly assigns a pre-parsed WASM SIGMET dataset.
   */
  public setDataset(dataset: WasmSigmetDataset): void {
    if (this.dataset && this.dataset !== dataset) {
      this.dataset.free();
    }
    this.dataset = dataset;
    this.geoJsonString = dataset.to_geojson();
  }

  /**
   * Returns the underlying WASM SIGMET dataset if loaded.
   */
  public getDataset(): WasmSigmetDataset | null {
    return this.dataset;
  }

  /**
   * Returns the dataset serialized as GeoJSON.
   */
  public getGeoJsonString(): string | null {
    return this.geoJsonString;
  }

  /**
   * Total number of loaded warning features.
   */
  public getWarningCount(): number {
    return this.dataset ? this.dataset.total_count() : 0;
  }

  /**
   * Finds all active hazard warnings containing the specified coordinate (lat/lon in degrees, optional altitude in meters).
   */
  public findHazardsAt(latDeg: number, lonDeg: number, altM?: number): any[] {
    if (!this.dataset) return [];
    try {
      return this.dataset.find_hazards_at_point(latDeg, lonDeg, altM) as any[];
    } catch {
      return [];
    }
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // GPU polygon boundary rendering hook
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    if (!this.visible || !this.dataset) return;
    // Dynamic warning labels and animations hook
  }

  public destroy(): void {
    if (this.dataset) {
      this.dataset.free();
      this.dataset = null;
    }
    this.geoJsonString = null;
  }
}
