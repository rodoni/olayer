import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
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
  public getStationGeometry(id: string): readonly number[] | undefined {
    const geometry = this.cachedGeometries.get(id);
    return geometry ? [...geometry] : undefined;
  }

  /**
   * Returns a map of all cached geometries.
   */
  public getAllGeometries(): ReadonlyMap<string, readonly number[]> {
    return new Map([...this.cachedGeometries].map(([id, geometry]) => [id, [...geometry]] as const));
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // GPU line batch rendering hook
  }

  public renderDynamic(ctx: CanvasRenderingContext2D, _currentTime: number, context?: LayerRenderContext): void {
    if (!this.visible || this.stations.size === 0 || !context) return;
    const controller = context.controller;
    const width = controller.canvas2D.width;
    const height = controller.canvas2D.height;
    const center = controller.projection.project(controller.getCenterLat(), controller.getCenterLon(), 0);
    const aspect = width / height;
    const viewportWidth = controller.getViewportBaseMeters() / controller.getZoom();
    const viewportHeight = viewportWidth / aspect;
    const toScreen = (latDeg: number, lonDeg: number): [number, number] | null => {
      try {
        const point = controller.projection.project(latDeg * Math.PI / 180, lonDeg * Math.PI / 180, 0);
        const dx = (point[0] ?? 0) - (center[0] ?? 0);
        const dy = (point[1] ?? 0) - (center[1] ?? 0);
        return [((dx / (viewportWidth / 2)) + 1) * width / 2, (1 - dy / (viewportHeight / 2)) * height / 2];
      } catch {
        return null;
      }
    };
    ctx.save();
    ctx.strokeStyle = this.strokeColor;
    ctx.lineWidth = this.lineWidth;
    for (const [stationId, station] of this.stations) {
      const geometry = this.cachedGeometries.get(stationId);
      if (!geometry) continue;
      ctx.beginPath();
      for (let index = 0; index + 3 < geometry.length; index += 4) {
        const start = toScreen(geometry[index] ?? 0, geometry[index + 1] ?? 0);
        const end = toScreen(geometry[index + 2] ?? 0, geometry[index + 3] ?? 0);
        if (!start || !end) continue;
        ctx.moveTo(start[0], start[1]);
        ctx.lineTo(end[0], end[1]);
      }
      ctx.stroke();
      const anchor = toScreen(station.latDeg, station.lonDeg);
      if (anchor) {
        ctx.beginPath();
        ctx.arc(anchor[0], anchor[1], 2, 0, 2 * Math.PI);
        ctx.fillStyle = this.strokeColor;
        ctx.fill();
      }
    }
    ctx.restore();
  }

  public override destroy(): void {
    this.stations.clear();
    this.cachedGeometries.clear();
  }
}
