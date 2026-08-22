import { Layer } from "./layer";
import { generate_wind_barb_geometry } from "olayer-wasm";

export interface WindStation {
  id: string;
  latDeg: number;
  lonDeg: number;
  speedKnots: number;
  directionDeg: number;
  staffLengthMeters?: number;
  altitudeM?: number;
}

export interface WindBarbsLayerOptions {
  defaultStaffLengthMeters?: number;
  strokeColor?: string;
  lineWidth?: number;
}

/**
 * Meteorological Wind Barbs layer for synoptic and aerodrome wind observation visualization.
 * Generates aviation standard pennants (50kt), full barbs (10kt), half barbs (5kt), and calm circles (< 2.5kt).
 */
export class WindBarbsLayer extends Layer {
  public defaultStaffLengthMeters: number;
  public strokeColor: string;
  public lineWidth: number;

  private stations: Map<string, WindStation> = new Map();
  private cachedGeometries: Map<string, number[]> = new Map();

  constructor(id: string, options: WindBarbsLayerOptions = {}) {
    super(id);
    this.defaultStaffLengthMeters = options.defaultStaffLengthMeters ?? 25000; // 25 km
    this.strokeColor = options.strokeColor ?? "#00e5ff"; // High-visibility cyan
    this.lineWidth = options.lineWidth ?? 1.5;
  }

  /**
   * Replaces all active wind stations and regenerates barb geometries.
   */
  public setStations(stations: WindStation[]): void {
    this.stations.clear();
    this.cachedGeometries.clear();
    for (const st of stations) {
      this.addStation(st);
    }
  }

  /**
   * Adds or updates a single wind station.
   */
  public addStation(station: WindStation): void {
    this.stations.set(station.id, station);
    const staffLen = station.staffLengthMeters ?? this.defaultStaffLengthMeters;
    const isSH = station.latDeg < 0.0;

    try {
      const flat = generate_wind_barb_geometry(
        station.latDeg,
        station.lonDeg,
        station.speedKnots,
        station.directionDeg,
        staffLen,
        isSH
      );
      this.cachedGeometries.set(station.id, Array.from(flat));
    } catch {
      // Ignore invalid station parameters
    }
  }

  /**
   * Removes a wind station by id.
   */
  public removeStation(id: string): boolean {
    this.cachedGeometries.delete(id);
    return this.stations.delete(id);
  }

  /**
   * Clears all wind stations.
   */
  public clear(): void {
    this.stations.clear();
    this.cachedGeometries.clear();
  }

  /**
   * Returns the count of active wind stations.
   */
  public getStationCount(): number {
    return this.stations.size;
  }

  /**
   * Returns precomputed flat line coordinates for a station: `[lat0, lon0, lat1, lon1, ...]`.
   */
  public getStationGeometry(id: string): number[] | undefined {
    return this.cachedGeometries.get(id);
  }

  /**
   * Returns a map of all cached geometries.
   */
  public getAllGeometries(): Map<string, number[]> {
    return this.cachedGeometries;
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // GPU line batch rendering hook
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    if (!this.visible || this.stations.size === 0) return;
    // Canvas 2D barb rendering hook
  }

  public destroy(): void {
    this.stations.clear();
    this.cachedGeometries.clear();
  }
}
