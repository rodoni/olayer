import { Layer } from "./layer";
import { generate_airspace_volume_mesh } from "olayer-wasm";
import type { WasmVolumetricMesh } from "olayer-wasm";
import type { AltitudeMode, AltitudeResolver } from "../types/altitude";
import type { LayerRenderContext } from "./layer";
import { drawWithLayerDepth } from "./depth_state";

export interface VolumetricAirspaceOptions {
  /** Solid `#RRGGBB`, `#RRGGBBAA`, `rgb(...)` or `rgba(...)` CSS color. */
  baseColor?: string;
  /** Same supported color forms as `baseColor`. */
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

interface GpuAirspace {
  readonly vao: WebGLVertexArrayObject;
  readonly vertexBuffer: WebGLBuffer;
  readonly indexBuffer: WebGLBuffer;
  readonly indexCount: number;
}

const MAX_AIRSPACE_FOOTPRINT_POSITIONS = 10_000;
const MAX_AIRSPACE_VERTICES = 100_000;
const MAX_AIRSPACE_INDICES = 600_000;
const CSS_RGB = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(?:\s*,\s*([\d.]+))?\s*\)$/i;
const CSS_HEX = /^#([\da-f]{6}|[\da-f]{8})$/i;

function parseColor(value: string): readonly [number, number, number, number] {
  const hex = CSS_HEX.exec(value);
  if (hex?.[1]) {
    const digits = hex[1];
    const channel = (index: number): number => Number.parseInt(digits.slice(index, index + 2), 16) / 255;
    return [channel(0), channel(2), channel(4), digits.length === 8 ? channel(6) : 1];
  }
  const rgb = CSS_RGB.exec(value);
  if (!rgb) throw new TypeError(`Unsupported volumetric CSS color: ${value}`);
  const red = Number(rgb[1]), green = Number(rgb[2]), blue = Number(rgb[3]), alpha = rgb[4] === undefined ? 1 : Number(rgb[4]);
  if (![red, green, blue, alpha].every(Number.isFinite) || [red, green, blue].some(channel => channel < 0 || channel > 255) || alpha < 0 || alpha > 1) {
    throw new RangeError("Volumetric color channels are outside their valid range");
  }
  return [red / 255, green / 255, blue / 255, alpha];
}

/**
 * 3D Volumetric Airspace Layer for rendering extruded 3D airspace polyhedrons (CTRs, TMAs, FIR sectors).
 * Computes 3D ECEF meshes with sidewalls, floor/ceiling caps, and Fresnel edge glow parameters.
 */
export class VolumetricAirspaceLayer extends Layer {
  public baseColor: string;
  public edgeColor: string;
  public override opacity: number;
  public fresnelIntensity: number;
  public altitudeMode: AltitudeMode;
  private altitudeResolver: AltitudeResolver | undefined;

  private airspaces: Map<string, AirspaceMeshRecord> = new Map();
  private readonly gpuAirspaces = new Map<string, GpuAirspace>();
  private gpuOwner: WebGL2RenderingContext | null = null;
  private program: WebGLProgram | null = null;
  private viewProjectionLocation: WebGLUniformLocation | null = null;
  private baseColorLocation: WebGLUniformLocation | null = null;
  private edgeColorLocation: WebGLUniformLocation | null = null;
  private opacityLocation: WebGLUniformLocation | null = null;
  private fresnelLocation: WebGLUniformLocation | null = null;

  constructor(id: string, options: VolumetricAirspaceOptions = {}) {
    super(id);
    this.baseColor = options.baseColor ?? "rgba(0, 229, 255, 0.22)"; // Cyan semi-transparent
    this.edgeColor = options.edgeColor ?? "#00e5ff";
    this.opacity = options.opacity ?? 0.85;
    this.fresnelIntensity = options.fresnelIntensity ?? 0.5;
    this.altitudeMode = options.altitudeMode ?? "absolute";
    this.altitudeResolver = options.altitudeResolver;
    if (!Number.isFinite(this.opacity) || this.opacity < 0 || this.opacity > 1
      || !Number.isFinite(this.fresnelIntensity) || this.fresnelIntensity < 0 || this.fresnelIntensity > 1) {
      throw new RangeError("Volumetric opacity and Fresnel intensity must be in [0, 1]");
    }
    parseColor(this.baseColor);
    parseColor(this.edgeColor);
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
    if (!id || polygonDeg.length < 6 || polygonDeg.length % 2 !== 0 || polygonDeg.length / 2 > MAX_AIRSPACE_FOOTPRINT_POSITIONS
      || polygonDeg.some((coordinate) => !Number.isFinite(coordinate)) || !Number.isFinite(floorM) || !Number.isFinite(ceilingM) || floorM >= ceilingM) {
      throw new RangeError("Invalid or over-budget volumetric airspace definition");
    }
    const [baseLatDeg, baseLonDeg] = polygonDeg;
    if (baseLatDeg !== undefined && baseLonDeg !== undefined && this.altitudeMode !== "absolute" && this.altitudeResolver && Number.isFinite(baseLatDeg) && Number.isFinite(baseLonDeg)) {
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
      if (!Number.isSafeInteger(vertexCount) || vertexCount < 0 || vertexCount > MAX_AIRSPACE_VERTICES
        || !Number.isSafeInteger(indexCount) || indexCount < 0 || indexCount > MAX_AIRSPACE_INDICES) {
        throw new RangeError("Volumetric mesh exceeds the browser render budget");
      }

      const airspace: AirspaceMeshRecord = {
        id,
        polygonDeg: [...polygonDeg],
        floorM,
        ceilingM,
        vertices,
        indices,
        vertexCount,
        indexCount,
      };
      const nextGpu = this.gpuOwner ? this.createGpuMesh(this.gpuOwner, airspace) : undefined;
      const previousGpu = this.gpuAirspaces.get(id);
      this.airspaces.set(id, airspace);
      if (nextGpu) this.gpuAirspaces.set(id, nextGpu);
      if (previousGpu && this.gpuOwner) this.deleteGpuMesh(this.gpuOwner, previousGpu);
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
    const removed = this.airspaces.delete(id);
    const gpu = this.gpuAirspaces.get(id);
    if (gpu && this.gpuOwner) this.deleteGpuMesh(this.gpuOwner, gpu);
    this.gpuAirspaces.delete(id);
    return removed;
  }

  /**
   * Clears all volumetric airspaces.
   */
  public clear(): void {
    this.airspaces.clear();
    if (this.gpuOwner) for (const gpu of this.gpuAirspaces.values()) this.deleteGpuMesh(this.gpuOwner, gpu);
    this.gpuAirspaces.clear();
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
  public getAllAirspaceMeshes(): ReadonlyMap<string, AirspaceMeshRecord> {
    return new Map(this.airspaces);
  }

  public override renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void {
    if (!this.visible || this.opacity <= 0 || this.airspaces.size === 0) return;
    if (context && context.controller.getViewMode() !== "3D") return;
    this.ensureProgram(gl);
    if (!this.program) return;
    this.gpuOwner = gl;
    for (const airspace of this.airspaces.values()) {
      if (!this.gpuAirspaces.has(airspace.id)) this.gpuAirspaces.set(airspace.id, this.createGpuMesh(gl, airspace));
    }
    if (!Number.isFinite(this.opacity) || this.opacity < 0 || this.opacity > 1
      || !Number.isFinite(this.fresnelIntensity) || this.fresnelIntensity < 0 || this.fresnelIntensity > 1) {
      throw new RangeError("Volumetric opacity and Fresnel intensity must be in [0, 1]");
    }
    const baseColor = parseColor(this.baseColor), edgeColor = parseColor(this.edgeColor);
    const previousProgram = gl.getParameter(gl.CURRENT_PROGRAM) as WebGLProgram | null;
    const previousVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING) as WebGLVertexArrayObject | null;
    gl.useProgram(this.program);
    gl.uniformMatrix4fv(this.viewProjectionLocation, false, viewProjMatrix);
    gl.uniform4fv(this.baseColorLocation, baseColor);
    gl.uniform4fv(this.edgeColorLocation, edgeColor);
    gl.uniform1f(this.opacityLocation, this.opacity);
    gl.uniform1f(this.fresnelLocation, this.fresnelIntensity);
    const mode = context?.controller.getViewMode() ?? "3D";
    try {
      drawWithLayerDepth(gl, mode, () => {
        for (const gpu of this.gpuAirspaces.values()) {
          gl.bindVertexArray(gpu.vao);
          gl.drawElements(gl.TRIANGLES, gpu.indexCount, gl.UNSIGNED_INT, 0);
        }
      }, false, false);
    } finally {
      gl.bindVertexArray(previousVao);
      gl.useProgram(previousProgram);
    }
  }

  public override renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    // Dynamic overlay hook
  }

  public override destroy(gl: WebGL2RenderingContext): void {
    if (this.gpuOwner && this.gpuOwner !== gl) throw new Error("Volumetric layer belongs to another WebGL context");
    this.clear();
    if (this.program) gl.deleteProgram(this.program);
    this.program = null;
    this.gpuOwner = null;
  }

  private ensureProgram(gl: WebGL2RenderingContext): void {
    if (this.program) {
      if (this.gpuOwner && this.gpuOwner !== gl) throw new Error("Volumetric layer belongs to another WebGL context");
      return;
    }
    const vertexSource = `#version 300 es
      layout(location=0) in vec3 position;
      layout(location=1) in vec3 normal;
      layout(location=2) in float heightRatio;
      layout(location=3) in float isEdge;
      uniform mat4 viewProjection;
      out vec3 worldNormal;
      out float vertexHeight;
      out float edgeFlag;
      void main() { gl_Position = viewProjection * vec4(position, 1.0); worldNormal = normal; vertexHeight = heightRatio; edgeFlag = isEdge; }
    `;
    const fragmentSource = `#version 300 es
      precision highp float;
      in vec3 worldNormal; in float vertexHeight; in float edgeFlag;
      uniform vec4 baseColor; uniform vec4 edgeColor; uniform float opacity; uniform float fresnelIntensity;
      out vec4 color;
      void main() {
        float facing = 1.0 - abs(normalize(worldNormal).z);
        float amount = clamp(vertexHeight * 0.3 + edgeFlag * fresnelIntensity + facing * fresnelIntensity * 0.25, 0.0, 1.0);
        color = mix(baseColor, edgeColor, amount);
        color.a *= opacity;
        if (color.a <= 0.0) discard;
      }
    `;
    const shaders: WebGLShader[] = [];
    const program = gl.createProgram();
    if (!program) throw new Error("Unable to allocate volumetric program");
    try {
      for (const [type, source] of [[gl.VERTEX_SHADER, vertexSource], [gl.FRAGMENT_SHADER, fragmentSource]] as const) {
        const shader = gl.createShader(type);
        if (!shader) throw new Error("Unable to allocate volumetric shader");
        shaders.push(shader);
        gl.shaderSource(shader, source);
        gl.compileShader(shader);
        if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) ?? "Volumetric shader compilation failed");
        gl.attachShader(program, shader);
      }
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? "Volumetric program link failed");
      this.program = program;
      this.viewProjectionLocation = gl.getUniformLocation(program, "viewProjection");
      this.baseColorLocation = gl.getUniformLocation(program, "baseColor");
      this.edgeColorLocation = gl.getUniformLocation(program, "edgeColor");
      this.opacityLocation = gl.getUniformLocation(program, "opacity");
      this.fresnelLocation = gl.getUniformLocation(program, "fresnelIntensity");
    } catch (error: unknown) {
      gl.deleteProgram(program);
      throw error;
    } finally {
      for (const shader of shaders) gl.deleteShader(shader);
    }
  }

  private createGpuMesh(gl: WebGL2RenderingContext, airspace: AirspaceMeshRecord): GpuAirspace {
    const vao = gl.createVertexArray(), vertexBuffer = gl.createBuffer(), indexBuffer = gl.createBuffer();
    if (!vao || !vertexBuffer || !indexBuffer) {
      if (vao) gl.deleteVertexArray(vao);
      if (vertexBuffer) gl.deleteBuffer(vertexBuffer);
      if (indexBuffer) gl.deleteBuffer(indexBuffer);
      throw new Error("Unable to allocate volumetric buffers");
    }
    const previousVao = gl.getParameter(gl.VERTEX_ARRAY_BINDING) as WebGLVertexArrayObject | null;
    const previousArrayBuffer = gl.getParameter(gl.ARRAY_BUFFER_BINDING) as WebGLBuffer | null;
    try {
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, vertexBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, airspace.vertices, gl.STATIC_DRAW);
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, indexBuffer);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, airspace.indices, gl.STATIC_DRAW);
      const stride = 8 * Float32Array.BYTES_PER_ELEMENT;
      for (const [location, size, offset] of [[0, 3, 0], [1, 3, 3], [2, 1, 6], [3, 1, 7]] as const) {
        gl.enableVertexAttribArray(location);
        gl.vertexAttribPointer(location, size, gl.FLOAT, false, stride, offset * Float32Array.BYTES_PER_ELEMENT);
      }
    } catch (error: unknown) {
      gl.deleteVertexArray(vao);
      gl.deleteBuffer(vertexBuffer);
      gl.deleteBuffer(indexBuffer);
      throw error;
    } finally {
      gl.bindVertexArray(previousVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, previousArrayBuffer);
    }
    return { vao, vertexBuffer, indexBuffer, indexCount: airspace.indexCount };
  }

  private deleteGpuMesh(gl: WebGL2RenderingContext, gpu: GpuAirspace): void {
    gl.deleteVertexArray(gpu.vao);
    gl.deleteBuffer(gpu.vertexBuffer);
    gl.deleteBuffer(gpu.indexBuffer);
  }
}
