import { buildTaggedWfsFillMesh, prepareTaggedWfsFillTriangles } from "./wfs_fill";
import type { WfsPreparedFill } from "./wfs_fill";
import type { WmsMeshContext } from "./wms_mesh";
import { drawWithLayerDepth } from "./depth_state";
import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
import { buildWfsMesh, buildPreparedWfsMesh, paths } from "./wfs_mesh";
import type { WfsMeshOptions, WfsGeometry, WfsRangedMesh } from "./wfs_mesh";
import { evaluateWfsStyle, validateWfsStyleSheet } from "./wfs_style";
import type { WfsStyleSheet, WfsPaint, WfsColor } from "./wfs_style";
import type { WfsVectorSource } from "../map_provider/ogc_sources";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import { throwIfAborted } from "../map_provider/tile_cache";

export interface WfsLayerOptions extends WfsMeshOptions {
  readonly color?: readonly [number, number, number, number];
  readonly fillColor?: readonly [number, number, number, number];
  /** Complete property paints. Cannot be combined with legacy color/fill options. */
  readonly style?: WfsStyleSheet;
}

/** WFS points, lines, polygon outlines and optional fills through the normal layer stack. */
export class WfsLayer extends Layer {
  private collection: WfsFeatureCollection | null = null;
  private fillTriangles: readonly WfsPreparedFill[] = [];
  private geometry: WfsGeometry | null = null;
  private style: WfsStyleSheet | null;
  private paints: readonly WfsPaint[] = [];
  private fillsPrepared = false;
  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private buffer: WebGLBuffer | null = null;
  private owner: WebGL2RenderingContext | null = null;
  private generation = 0;
  private readonly options: WfsLayerOptions;
  private readonly fillColor: readonly [number, number, number, number];
  private readonly color: readonly [number, number, number, number];

  public constructor(id: string, private readonly source: WfsVectorSource, options: WfsLayerOptions = {}) {
    super(id);
    if (options.style !== undefined && (options.color !== undefined || options.fillColor !== undefined || options.fillPolygons !== undefined)) {
      throw new TypeError("WFS stylesheet conflicts with legacy paint options");
    }
    this.style = options.style === undefined ? null : validateWfsStyleSheet(options.style);
    this.options = { ...options };
    this.color = [...(options.color ?? [0, 0.9, 0.46, 1])];
    this.fillColor = [...(options.fillColor ?? this.color)];
    if (this.fillColor.length !== 4 || this.fillColor.some(v => !Number.isFinite(v) || v < 0 || v > 1)) throw new RangeError("Invalid WFS fill color");
    if (this.color.length !== 4 || this.color.some(v => !Number.isFinite(v) || v < 0 || v > 1)) throw new RangeError("Invalid WFS color");
    buildWfsMesh({ features: [] }, source.crs, {
      viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1, project: () => [0, 0],
    }, this.options);
  }

  /**
   * Atomically replace property paints without fetching or triangulating.
   * @param style - Complete stylesheet, or null to restore legacy paint.
   * @throws {TypeError} A loaded outline-only legacy layer cannot enable fill; supply a sheet before load.
   * @throws {RangeError} Invalid colors, operands or stylesheet bounds.
   * @example layer.setStyle({ defaultPaint: { visibility: "hidden" }, rules: [] });
   */
  public setStyle(style: WfsStyleSheet | null): void {
    if (style !== null && (this.options.color !== undefined || this.options.fillColor !== undefined || this.options.fillPolygons !== undefined)) {
      throw new TypeError("WFS stylesheet conflicts with legacy paint options");
    }
    const next = style === null ? null : validateWfsStyleSheet(style);
    if (this.collection && !this.fillsPrepared && next &&
      (next.defaultPaint.fill !== undefined || next.rules.some(rule => rule.paint.fill !== undefined))) {
      throw new TypeError("WFS fill styling must be configured before load");
    }
    const paints = this.collection && next ? evaluateWfsStyle(this.collection, next) : [];
    this.style = next;
    this.paints = paints;
  }

  /** Fetch once. A failed or obsolete load preserves the installed collection. */
  public async load(signal?: AbortSignal): Promise<void> {
    const generation = ++this.generation;
    const collection = await this.source.load(signal);
    throwIfAborted(signal);
    if (generation !== this.generation) return;
    const geometry = paths(collection, this.source.crs);
    const fillsPrepared = this.style !== null || this.options.fillPolygons === true;
    const fillTriangles = fillsPrepared ? prepareTaggedWfsFillTriangles(geometry, this.options) : [];
    const paints = this.style ? evaluateWfsStyle(collection, this.style) : [];
    // Validate aggregate work and draw bounds before installing, independently of visibility.
    const meshContext: WmsMeshContext = { viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1, project: () => [0, 0] };
    const budget = { work: 0, vertices: 0 };
    const lines = buildPreparedWfsMesh(geometry, meshContext, this.options, budget);
    const fills = buildTaggedWfsFillMesh(fillTriangles, meshContext, this.options, budget);
    if (lines.ranges.length + fills.ranges.length > 8192) throw new RangeError("WFS aggregate draw range limit exceeded");
    this.fillTriangles = fillTriangles;
    this.geometry = geometry;
    this.fillsPrepared = fillsPrepared;
    this.paints = paints;
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
          void main() { color = featureColor; if (color.a == 0.0) discard; }`],
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
    if (!this.visible || this.opacity <= 0 || !this.collection || !this.geometry || !context) return;
    this.claimContext(gl);
    const controller = context.controller;
    const camera = controller.getCameraState();
    let lineMesh: WfsRangedMesh;
    let fillMesh: WfsRangedMesh;
    try {
      const meshContext: WmsMeshContext = {
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
      };
      const budget = { work: 0, vertices: 0 };
      lineMesh = buildPreparedWfsMesh(this.geometry, meshContext, this.options, budget);
      fillMesh = buildTaggedWfsFillMesh(this.fillTriangles, meshContext, this.options, budget);
    } finally { camera.free(); }
    const vertices = lineMesh.vertices, fills = fillMesh.vertices;
    if (vertices.length === 0 && fills.length === 0) return;
    this.initialize(gl);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    const data = new Float32Array(fills.length + vertices.length);
    data.set(fills); data.set(vertices, fills.length);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    gl.uniformMatrix4fv(gl.getUniformLocation(this.program!, "viewProjection"), false, matrix);
    const colorLocation = gl.getUniformLocation(this.program!, "featureColor");
    drawWithLayerDepth(gl, controller.getViewMode(), () => {
      const draw = (mesh: WfsRangedMesh, channel: "fill" | "stroke", mode: number, offset: number): void => {
        for (const range of mesh.ranges) {
          const paint = this.style ? this.paints[range.featureIndex] : null;
          if (paint?.visibility === "hidden") continue;
          const color: WfsColor | undefined = paint ? paint[channel] : channel === "fill"
            ? (this.options.fillPolygons ? this.fillColor : undefined) : this.color;
          if (!color || color[3] === 0) continue;
          gl.uniform4f(colorLocation, color[0], color[1], color[2], color[3] * this.opacity);
          gl.drawArrays(mode, offset + range.first, range.count);
        }
      };
      draw(fillMesh, "fill", gl.TRIANGLES, 0);
      draw(lineMesh, "stroke", gl.LINES, fills.length / 3);
    });
    gl.bindVertexArray(null);
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
    this.fillTriangles = [];
    this.geometry = null;
    this.paints = [];
    this.fillsPrepared = false;
    this.owner = null;
  }
}
