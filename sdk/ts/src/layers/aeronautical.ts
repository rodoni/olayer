import { Layer } from "./layer";
import { WasmAeronauticalDataset, parse_aixm_51, parse_geojson_aviation } from "olayer-wasm";

export interface AeronauticalLayerOptions {
  airspaceStrokeColor?: string;
  airspaceFillColor?: string;
  airspaceLineWidth?: number;
  showNavaids?: boolean;
  showAirways?: boolean;
  showAirspaces?: boolean;
}

/**
 * Visualization and query layer for aeronautical features (airspaces, navaids, airways, aerodromes).
 * Backed by high-performance WASM AIXM 5.1 and GeoJSON-Aviation parsers.
 */
export class AeronauticalLayer extends Layer {
  private dataset: WasmAeronauticalDataset | null = null;
  private geoJsonString: string | null = null;
  public options: Required<AeronauticalLayerOptions>;

  constructor(id: string, options: AeronauticalLayerOptions = {}) {
    super(id);
    this.options = {
      airspaceStrokeColor: options.airspaceStrokeColor ?? "#ff9100", // High-visibility amber/orange
      airspaceFillColor: options.airspaceFillColor ?? "rgba(255, 145, 0, 0.08)",
      airspaceLineWidth: options.airspaceLineWidth ?? 1.5,
      showNavaids: options.showNavaids ?? true,
      showAirways: options.showAirways ?? true,
      showAirspaces: options.showAirspaces ?? true,
    };
  }

  /**
   * Loads aeronautical data from an AIXM 5.1 formatted XML string.
   */
  public loadAixm51(xmlContent: string): void {
    if (this.dataset) {
      this.dataset.free();
    }
    this.dataset = parse_aixm_51(xmlContent);
    this.geoJsonString = this.dataset.to_geojson();
  }

  /**
   * Loads aeronautical data from a GeoJSON-Aviation formatted JSON string.
   */
  public loadGeoJson(jsonContent: string): void {
    if (this.dataset) {
      this.dataset.free();
    }
    this.dataset = parse_geojson_aviation(jsonContent);
    this.geoJsonString = this.dataset.to_geojson();
  }

  /**
   * Directly assigns a pre-parsed WASM aeronautical dataset.
   */
  public setDataset(dataset: WasmAeronauticalDataset): void {
    if (this.dataset && this.dataset !== dataset) {
      this.dataset.free();
    }
    this.dataset = dataset;
    this.geoJsonString = dataset.to_geojson();
  }

  /**
   * Returns the underlying WASM aeronautical dataset if loaded.
   */
  public getDataset(): WasmAeronauticalDataset | null {
    return this.dataset;
  }

  /**
   * Returns the dataset serialized as standard GeoJSON.
   */
  public getGeoJsonString(): string | null {
    return this.geoJsonString;
  }

  /**
   * Total number of loaded aeronautical features.
   */
  public getFeatureCount(): number {
    return this.dataset ? this.dataset.total_feature_count() : 0;
  }

  /**
   * Queries navaids within a given radius from coordinates (lat/lon in radians).
   */
  public findNavaidsNear(latRad: number, lonRad: number, radiusMeters: number): any[] {
    if (!this.dataset) return [];
    try {
      return this.dataset.find_navaids_within_radius(latRad, lonRad, radiusMeters) as any[];
    } catch {
      return [];
    }
  }

  /**
   * Queries airspaces containing the given coordinate (lat/lon in radians).
   */
  public findAirspacesContaining(latRad: number, lonRad: number): any[] {
    if (!this.dataset) return [];
    try {
      return this.dataset.find_airspaces_containing_point(latRad, lonRad) as any[];
    } catch {
      return [];
    }
  }

  /**
   * Renders static geometry on WebGL2.
   */
  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // GPU rendering hook for extruded airspace polyhedrons / vector lines
  }

  /**
   * Renders dynamic navaid symbols and airspace labels on Canvas 2D context.
   */
  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    if (!this.visible || !this.dataset) return;
    // 2D label and overlay rendering hook
  }

  /**
   * Releases WASM dataset allocations.
   */
  public destroy(): void {
    if (this.dataset) {
      this.dataset.free();
      this.dataset = null;
    }
    this.geoJsonString = null;
  }
}
