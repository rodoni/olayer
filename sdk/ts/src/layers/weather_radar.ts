import { Layer } from "./layer";
import { colorize_dbz_grid, dbz_to_rgba, generate_isolines } from "olayer-wasm";

export type RadarPaletteType = "nexrad" | "icao" | "high_contrast";

export interface WeatherRadarLayerOptions {
  palette?: RadarPaletteType;
  opacity?: number;
}

export interface RadarGridData {
  grid: Float64Array | number[];
  width: number;
  height: number;
  /** Bounding box in degrees: [minLat, minLon, maxLat, maxLon] */
  boundsDeg: [number, number, number, number];
}

/**
 * Meteorological Weather Radar precipitation reflectivity layer (dBZ).
 * Supports standard NOAA NEXRAD and ICAO color scales, dynamic colorization, and Marching Squares isolines.
 */
export class WeatherRadarLayer extends Layer {
  public palette: RadarPaletteType;
  public opacity: number;

  private currentGrid: RadarGridData | null = null;
  private colorizedRgba: Uint8Array | null = null;
  private offscreenCanvas: HTMLCanvasElement | null = null;
  private offscreenCtx: CanvasRenderingContext2D | null = null;

  constructor(id: string, options: WeatherRadarLayerOptions = {}) {
    super(id);
    this.palette = options.palette ?? "nexrad";
    this.opacity = options.opacity ?? 0.85;
  }

  /**
   * Updates the radar grid data and immediately triggers WASM colorization.
   */
  public setDbzGrid(
    grid: Float64Array | number[],
    width: number,
    height: number,
    boundsDeg: [number, number, number, number]
  ): void {
    this.currentGrid = {
      grid,
      width,
      height,
      boundsDeg,
    };

    this.recolorize();
  }

  /**
   * Changes the active color palette and recolorizes the current grid.
   */
  public setPalette(palette: RadarPaletteType): void {
    if (this.palette !== palette) {
      this.palette = palette;
      if (this.currentGrid) {
        this.recolorize();
      }
    }
  }

  /**
   * Sets layer opacity in [0.0, 1.0].
   */
  public setOpacity(opacity: number): void {
    this.opacity = Math.max(0, Math.min(1, opacity));
  }

  /**
   * Returns current grid metadata if set.
   */
  public getGridData(): RadarGridData | null {
    return this.currentGrid;
  }

  /**
   * Returns the colorized raw RGBA pixel buffer (width * height * 4).
   */
  public getColorizedRgba(): Uint8Array | null {
    return this.colorizedRgba;
  }

  /**
   * Maps a single dBZ reflectivity value to RGBA using the active or specified palette.
   */
  public getDbzColor(dbz: number, palette?: RadarPaletteType): Uint8Array {
    return dbz_to_rgba(dbz, palette ?? this.palette);
  }

  /**
   * Generates Marching Squares isoline / contour segments from the current radar grid.
   * Returns flat array: `[isovalue, start_lat_deg, start_lon_deg, end_lat_deg, end_lon_deg, ...]`.
   */
  public generateContourIsolines(isovalues: number[]): number[] {
    if (!this.currentGrid) return [];
    const { grid, width, height, boundsDeg } = this.currentGrid;
    const [minLat, minLon, maxLat, maxLon] = boundsDeg;

    try {
      const flat = generate_isolines(
        Float64Array.from(grid),
        width,
        height,
        minLat,
        minLon,
        maxLat,
        maxLon,
        Float64Array.from(isovalues)
      );
      return Array.from(flat);
    } catch {
      return [];
    }
  }

  private recolorize(): void {
    if (!this.currentGrid) return;
    const { grid, width, height } = this.currentGrid;

    const gridF64 = grid instanceof Float64Array ? grid : Float64Array.from(grid);
    this.colorizedRgba = colorize_dbz_grid(gridF64, width, height, this.palette);

    // Initialize or resize offscreen canvas if in DOM/browser environment
    if (typeof document !== "undefined" && document.createElement) {
      if (!this.offscreenCanvas) {
        this.offscreenCanvas = document.createElement("canvas");
        this.offscreenCtx = this.offscreenCanvas.getContext("2d");
      }
      if (this.offscreenCanvas && this.offscreenCtx) {
        this.offscreenCanvas.width = width;
        this.offscreenCanvas.height = height;

        const imgData = this.offscreenCtx.createImageData(width, height);
        imgData.data.set(this.colorizedRgba);
        this.offscreenCtx.putImageData(imgData, 0, 0);
      }
    }
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // WebGL texture quad rendering hook
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    if (!this.visible || !this.currentGrid || !this.offscreenCanvas) return;
    // Dynamic overlay rendering
  }

  public destroy(): void {
    this.currentGrid = null;
    this.colorizedRgba = null;
    this.offscreenCanvas = null;
    this.offscreenCtx = null;
  }
}
