import { Layer } from "./layer";
import { generate_airspace_volume_mesh, WasmVolumetricMesh } from "olayer-wasm";
import type { AltitudeMode, AltitudeResolver } from "../types/altitude";

export interface VolumetricAirspaceOptions {
  baseColor?: string;
  edgeColor?: string;
  opacity?: number;
  fresnelIntensity?: number;
  altitudeMode?: AltitudeMode;
  altitudeResolver?: AltitudeResolver;
}

export interface AirspaceMeshRecord {
  id: string;
  polygonDeg: number[];
  floorM: number;
  ceilingM: number;
  vertices: Float32Array;
  indices: Uint32Array;
  vertexCount: number;
  indexCount: number;
}

/**
 * 3D Volumetric Airspace Layer for rendering extruded 3D airspace polyhedrons (CTRs, TMAs, FIR sectors).
 * Computes 3D ECEF meshes with sidewalls, floor/ceiling caps, and Fresnel edge glow parameters.
 */
export class VolumetricAirspaceLayer extends Layer {
  public baseColor: string;
  public edgeColor: string;
  public opacity: number;
  public fresnelIntensity: number;
  public altitudeMode: AltitudeMode;
  private altitudeResolver?: AltitudeResolver;

  private airspaces: Map<string, AirspaceMeshRecord> = new Map();

  constructor(id: string, options: VolumetricAirspaceOptions = {}) {
    super(id);
    this.baseColor = options.baseColor ?? "rgba(0, 229, 255, 0.22)"; // Cyan semi-transparent
    this.edgeColor = options.edgeColor ?? "#00e5ff";
    this.opacity = options.opacity ?? 0.85;
    this.fresnelIntensity = options.fresnelIntensity ?? 0.5;
    this.altitudeMode = options.altitudeMode ?? "absolute";
    this.altitudeResolver = options.altitudeResolver;
  }

  /**
   * Adds or updates an extruded 3D airspace volume.
   *
   * @param id Unique airspace identifier.
   * @param polygonDeg Flat array of footprint coordinates `[lat0, lon0, lat1, lon1, ...]`.
   * @param floorM Lower altitude limit in meters above WGS84 ellipsoid.
   * @param ceilingM Upper altitude limit in meters above WGS84 ellipsoid.
   */
  public addAirspace(id: string, polygonDeg: number[], floorM: number, ceilingM: number): void {
    const [baseLatDeg, baseLonDeg] = polygonDeg;
    if (this.altitudeMode !== "absolute" && this.altitudeResolver && Number.isFinite(baseLatDeg) && Number.isFinite(baseLonDeg)) {
      const latRad = baseLatDeg * Math.PI / 180;
      const lonRad = baseLonDeg * Math.PI / 180;
      floorM = this.altitudeResolver(latRad, lonRad, floorM, this.altitudeMode);
      ceilingM = this.altitudeResolver(latRad, lonRad, ceilingM, this.altitudeMode);
    }
    let wasmMesh: WasmVolumetricMesh | null = null;
    try {
      wasmMesh = generate_airspace_volume_mesh(
        Float64Array.from(polygonDeg),
        floorM,
        ceilingM
      );

      const vertices = new Float32Array(wasmMesh.vertices());
      const indices = new Uint32Array(wasmMesh.indices());
      const vertexCount = wasmMesh.vertex_count();
      const indexCount = wasmMesh.index_count();

      this.airspaces.set(id, {
        id,
        polygonDeg: [...polygonDeg],
        floorM,
        ceilingM,
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

  public setAltitudeResolver(resolver: AltitudeResolver | undefined): void {
    this.altitudeResolver = resolver;
  }

  /**
   * Removes an airspace by identifier.
   */
  public removeAirspace(id: string): boolean {
    return this.airspaces.delete(id);
  }

  /**
   * Clears all volumetric airspaces.
   */
  public clear(): void {
    this.airspaces.clear();
  }

  /**
   * Total number of loaded volumetric airspaces.
   */
  public getAirspaceCount(): number {
    return this.airspaces.size;
  }

  /**
   * Returns pre-computed 3D mesh for an airspace.
   */
  public getAirspaceMesh(id: string): AirspaceMeshRecord | undefined {
    return this.airspaces.get(id);
  }

  /**
   * Returns map of all active airspace meshes.
   */
  public getAllAirspaceMeshes(): Map<string, AirspaceMeshRecord> {
    return this.airspaces;
  }

  public renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    // WebGL2 Volumetric shader drawing hook
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    // Dynamic overlay hook
  }

  public destroy(): void {
    this.airspaces.clear();
  }
}
