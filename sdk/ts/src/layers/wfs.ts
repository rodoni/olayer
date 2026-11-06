import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
import { buildWfsMesh, validateWfsGeometry } from "./wfs_mesh";
import type { WfsMeshOptions } from "./wfs_mesh";
import type { WfsVectorSource } from "../map_provider/ogc_sources";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import { throwIfAborted } from "../map_provider/tile_cache";

export interface WfsLayerOptions extends WfsMeshOptions {
  readonly color?: readonly [number, number, number, number];
}

/** WFS point crosses, lines and polygon outlines through the normal layer stack. */
export class WfsLayer extends Layer {
  private collection: WfsFeatureCollection | null = null;
  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private buffer: WebGLBuffer | null = null;
  private owner: WebGL2RenderingContext | null = null;
  private generation = 0;
  private readonly options: WfsLayerOptions;
  private readonly color: readonly [number, number, number, number];

  public constructor(id: string, private readonly source: WfsVectorSource, options: WfsLayerOptions = {}) {
    super(id);
    this.options = { ...options };
    this.color = [...(options.color ?? [0, 0.9, 0.46, 1])];
    if (this.color.length !== 4 || this.color.some(v => !Number.isFinite(v) || v < 0 || v > 1)) throw new RangeError("Invalid WFS color");
    buildWfsMesh({ features: [] }, source.crs, {
      viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1, project: () => [0, 0],
    }, this.options);
  }

  /** Fetch once. A failed or obsolete load preserves the installed collection. */
  public async load(signal?: AbortSignal): Promise<void> {
    const generation = ++this.generation;
    const collection = await this.source.load(signal);
    throwIfAborted(signal);
    if (generation !== this.generation) return;
    validateWfsGeometry(collection, this.source.crs);
    this.collection = collection;
  }

  private claimContext(gl: WebGL2RenderingContext): void {
    if (this.owner && this.owner !== gl) throw new Error("WFS layer belongs to another WebGL context");
    this.owner = gl;
  }

  private initialize(gl: WebGL2RenderingContext): void {
    if (this.program) return;
    const shaders: WebGLShader[] = [];
    const program = gl.createProgram();
    if (!program) throw new Error("Could not allocate WFS program");
    try {
      for (const [type, code] of [
        [gl.VERTEX_SHADER, `#version 300 es
          layout(location=0) in vec3 position;
          uniform mat4 viewProjection;
          void main() { gl_Position = viewProjection * vec4(position, 1.0); }`],
        [gl.FRAGMENT_SHADER, `#version 300 es
          precision highp float;
          uniform vec4 featureColor;
          out vec4 color;
          void main() { color = featureColor; }`],
      ] as const) {
        const shader = gl.createShader(type);
        if (!shader) throw new Error("Could not allocate WFS shader");
        shaders.push(shader);
        gl.shaderSource(shader, code);
        gl.compileShader(shader);
        if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) ?? "WFS shader failed");
        gl.attachShader(program, shader);
      }
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? "WFS program failed");
      this.vao = gl.createVertexArray();
      this.buffer = gl.createBuffer();
      if (!this.vao || !this.buffer) throw new Error("Could not allocate WFS geometry buffers");
      gl.bindVertexArray(this.vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 12, 0);
      gl.bindVertexArray(null);
      this.program = program;
    } catch (error: unknown) {
      gl.deleteProgram(program);
      if (this.vao) gl.deleteVertexArray(this.vao);
      if (this.buffer) gl.deleteBuffer(this.buffer);
      this.vao = null;
      this.buffer = null;
      throw error;
    } finally {
      for (const shader of shaders) gl.deleteShader(shader);
    }
  }

  public renderStatic(gl: WebGL2RenderingContext, matrix: Float32Array, context?: LayerRenderContext): void {
    if (!this.visible || this.opacity <= 0 || !this.collection || !context) return;
    this.claimContext(gl);
    const controller = context.controller;
    const camera = controller.getCameraState();
    let vertices: Float32Array;
    try {
      vertices = buildWfsMesh(this.collection, this.source.crs, {
        viewMode: controller.getViewMode(), centerLat: camera.center_lat, centerLon: camera.center_lon,
        zoom: camera.zoom, rotation: camera.rotation, pitch: camera.pitch, roll: camera.roll,
        project: (lat, lon) => controller.projection.project(lat, lon, 0),
        elevation: (lat, lon) => {
          try {
            const sample: unknown = controller.terrainEngine.get_elevation_status(lat, lon);
            if (typeof sample !== "object" || sample === null || !("elevation_meters" in sample)) return null;
            return typeof sample.elevation_meters === "number" ? sample.elevation_meters : null;
          } catch { return null; }
        },
      }, this.options);
    } finally { camera.free(); }
    if (vertices.length === 0) return;
    this.initialize(gl);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.uniformMatrix4fv(gl.getUniformLocation(this.program!, "viewProjection"), false, matrix);
    gl.uniform4f(gl.getUniformLocation(this.program!, "featureColor"), this.color[0], this.color[1], this.color[2], this.color[3] * this.opacity);
    const blend = gl.isEnabled(gl.BLEND), depth = gl.isEnabled(gl.DEPTH_TEST);
    const srcRgb = gl.getParameter(gl.BLEND_SRC_RGB) as number, dstRgb = gl.getParameter(gl.BLEND_DST_RGB) as number;
    const srcAlpha = gl.getParameter(gl.BLEND_SRC_ALPHA) as number, dstAlpha = gl.getParameter(gl.BLEND_DST_ALPHA) as number;
    const lineWidth = gl.getParameter(gl.LINE_WIDTH) as number;
    const depthFunc = gl.getParameter(gl.DEPTH_FUNC) as number;
    const depthMask = gl.getParameter(gl.DEPTH_WRITEMASK) as boolean;
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.lineWidth(1);
    if (controller.getViewMode() === "2D") gl.disable(gl.DEPTH_TEST);
    else { gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL); gl.depthMask(true); }
    gl.drawArrays(gl.LINES, 0, vertices.length / 3);
    gl.bindVertexArray(null);
    gl.lineWidth(lineWidth);
    gl.blendFuncSeparate(srcRgb, dstRgb, srcAlpha, dstAlpha);
    if (!blend) gl.disable(gl.BLEND);
    gl.depthFunc(depthFunc);
    gl.depthMask(depthMask);
    if (depth) gl.enable(gl.DEPTH_TEST); else gl.disable(gl.DEPTH_TEST);
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _time: number): void {}

  public override destroy(gl: WebGL2RenderingContext): void {
    this.claimContext(gl);
    this.generation++;
    if (this.program) gl.deleteProgram(this.program);
    if (this.vao) gl.deleteVertexArray(this.vao);
    if (this.buffer) gl.deleteBuffer(this.buffer);
    this.program = null;
    this.vao = null;
    this.buffer = null;
    this.collection = null;
    this.owner = null;
  }
}
