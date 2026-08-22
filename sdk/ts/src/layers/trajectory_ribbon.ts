import { Layer } from "./layer";
import { generate_trajectory_ribbon_mesh, WasmRibbonMesh } from "olayer-wasm";

export interface TrajectoryRibbonOptions {
  defaultRibbonWidthMeters?: number;
  colorLow?: string;
  colorHigh?: string;
  opacity?: number;
}

export interface TrajectoryRibbonRecord {
  id: string;
  waypointsDeg: number[];
  ribbonWidthM: number;
  vertices: Float32Array;
  indices: Uint32Array;
  vertexCount: number;
  indexCount: number;
}

/**
 * 3D Flight Trajectory Ribbon Layer for continuous flight path visualization.
 * Generates 3D ECEF ribbon geometry with mitered joints, along-track distance, and altitude color gradients.
 */
export class TrajectoryRibbonLayer extends Layer {
  public defaultRibbonWidthMeters: number;
  public colorLow: string;
  public colorHigh: string;
  public opacity: number;

  private ribbons: Map<string, TrajectoryRibbonRecord> = new Map();

  constructor(id: string, options: TrajectoryRibbonOptions = {}) {
    super(id);
    this.defaultRibbonWidthMeters = options.defaultRibbonWidthMeters ?? 250; // 250 m
    this.colorLow = options.colorLow ?? "#00e5ff"; // Cyan at low altitude
    this.colorHigh = options.colorHigh ?? "#ff1744"; // Vivid red at high altitude
    this.opacity = options.opacity ?? 0.9;
  }

  /**
   * Adds or updates a 3D flight trajectory ribbon.
   *
   * @param id Unique trajectory identifier (e.g. aircraft callsign).
   * @param waypointsDeg Flat array of waypoint coordinates `[lat0, lon0, alt0, lat1, lon1, alt1, ...]`.
   * @param ribbonWidthM Optional custom ribbon width in meters.
   * @param scalars Optional normalized scalar values per waypoint for custom shader gradient mapping.
   */
  public addTrajectory(
    id: string,
    waypointsDeg: number[],
    ribbonWidthM?: number,
    scalars?: number[]
  ): void {
    const width = ribbonWidthM ?? this.defaultRibbonWidthMeters;
    let wasmMesh: WasmRibbonMesh | null = null;

    try {
      wasmMesh = generate_trajectory_ribbon_mesh(
        Float64Array.from(waypointsDeg),
        width,
        scalars ? Float64Array.from(scalars) : undefined
      );

      const vertices = new Float32Array(wasmMesh.vertices());
      const indices = new Uint32Array(wasmMesh.indices());
      const vertexCount = wasmMesh.vertex_count();
      const indexCount = wasmMesh.index_count();

      this.ribbons.set(id, {
        id,
        waypointsDeg: [...waypointsDeg],
        ribbonWidthM: width,
        vertices,
        indices,
        vertexCount,
        indexCount,
      });
    } finally {
      if (wasmMesh) {
        wasmMesh.free();
      }
    }
  }

  /**
   * Removes a flight trajectory ribbon by identifier.
   */
  public removeTrajectory(id: string): boolean {
    return this.ribbons.delete(id);
  }

  /**
   * Clears all trajectory ribbons.
   */
  public clear(): void {
    this.ribbons.clear();
  }

  /**
   * Total count of active trajectory ribbons.
   */
  public getTrajectoryCount(): number {
    return this.ribbons.size;
  }

  /**
   * Returns pre-computed 3D mesh for a trajectory ribbon.
   */
  public getRibbonMesh(id: string): TrajectoryRibbonRecord | undefined {
    return this.ribbons.get(id);
  }

  /**
   * Returns map of all active trajectory ribbon meshes.
   */
  public getAllRibbonMeshes(): Map<string, TrajectoryRibbonRecord> {
    return this.ribbons;
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // WebGL2 Ribbon shader drawing hook
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    // Dynamic overlay hook
  }

  public destroy(): void {
    this.ribbons.clear();
  }
}
