import { drawWithLayerDepth } from "./depth_state";
import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
import { buildWmsMesh, validateWmsBounds } from "./wms_mesh";
import type { WmsMapImage, WmsMapSource } from "../map_provider/ogc_sources";
import { uploadOgcBitmapTexture } from "../map_provider/webgl";
import { throwIfAborted } from "../map_provider/tile_cache";

export interface WmsLayerOptions {
  /** 1..64; defaults to 16. Increase for large extents and terrain detail. */
  readonly subdivisions?: number;
  /** Sample controller terrain in 2.5D/3D. Missing samples leave holes. Default false. */
  readonly drapeTerrain?: boolean;
}

/** A georeferenced WMS image rendered through the normal SDK layer stack. */
export class WmsLayer extends Layer {
  private readonly subdivisions: number;
  private readonly drapeTerrain: boolean;
  private image: Omit<WmsMapImage, "bitmap"> | null = null;
  private texture: WebGLTexture | null = null;
  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private buffer: WebGLBuffer | null = null;
  private generation = 0;
  private owner: WebGL2RenderingContext | null = null;

  public constructor(id: string, private readonly source: WmsMapSource, options: WmsLayerOptions = {}) {
    super(id);
    this.subdivisions = options.subdivisions ?? 16;
    if (!Number.isInteger(this.subdivisions) || this.subdivisions < 1 || this.subdivisions > 64) {
      throw new RangeError("WMS subdivisions must be an integer from 1 to 64");
    }
    this.drapeTerrain = options.drapeTerrain ?? false;
  }

  /** Load explicitly, then trigger controller redraw. Repeated loads replace the image atomically. */
  public async load(gl: WebGL2RenderingContext, signal?: AbortSignal): Promise<void> {
    this.claimContext(gl);
    const generation = ++this.generation;
    const image = await this.source.load(signal);
    let texture: WebGLTexture | null = null;
    try {
      throwIfAborted(signal);
      if (generation !== this.generation) return;
      validateWmsBounds(image.crs, image.bounds);
      const maxTextureSize: unknown = gl.getParameter(gl.MAX_TEXTURE_SIZE);
      if (typeof maxTextureSize !== "number" || image.bitmap.width > maxTextureSize || image.bitmap.height > maxTextureSize) {
        throw new RangeError("WMS image exceeds GPU texture limits");
      }
      texture = gl.createTexture();
      if (!texture) throw new Error("Could not allocate WMS texture");
      uploadOgcBitmapTexture(gl, texture, image.bitmap);
      throwIfAborted(signal);
      if (this.texture) gl.deleteTexture(this.texture);
      this.texture = texture;
      texture = null;
      this.image = { crs: image.crs, bounds: image.bounds };
    } finally {
      if (texture) gl.deleteTexture(texture);
      image.bitmap.close();
    }
  }

  private claimContext(gl: WebGL2RenderingContext): void {
    if (this.owner && this.owner !== gl) throw new Error("WMS layer belongs to another WebGL context");
    this.owner = gl;
  }

  private initialize(gl: WebGL2RenderingContext): void {
    if (this.program) return;
    const shaders: WebGLShader[] = [];
    const program = gl.createProgram();
    if (!program) throw new Error("Could not allocate WMS program");
    try {
      for (const [type, code] of [
        [gl.VERTEX_SHADER, `#version 300 es
          layout(location=0) in vec3 position;
          layout(location=1) in vec2 uv;
          uniform mat4 viewProjection;
          out vec2 texCoord;
          void main() { gl_Position = viewProjection * vec4(position, 1.0); texCoord = uv; }`],
        [gl.FRAGMENT_SHADER, `#version 300 es
          precision highp float;
          in vec2 texCoord;
          uniform sampler2D imagery;
          uniform float opacity;
          out vec4 color;
          void main() { color = texture(imagery, texCoord); color.a *= opacity; if (color.a == 0.0) discard; }`],
      ] as const) {
        const shader = gl.createShader(type);
        if (!shader) throw new Error("Could not allocate WMS shader");
        shaders.push(shader);
        gl.shaderSource(shader, code);
        gl.compileShader(shader);
        if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader) ?? "WMS shader failed");
        gl.attachShader(program, shader);
      }
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? "WMS program failed");
      this.vao = gl.createVertexArray();
      this.buffer = gl.createBuffer();
      if (!this.vao || !this.buffer) throw new Error("Could not allocate WMS geometry buffers");
      gl.bindVertexArray(this.vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 20, 0);
      gl.enableVertexAttribArray(1);
      gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 20, 12);
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
    if (!this.visible || this.opacity <= 0 || !this.image || !this.texture || !context) return;
    this.claimContext(gl);
    const controller = context.controller;
    const camera = controller.getCameraState();
    let vertices: Float32Array;
    try {
      vertices = buildWmsMesh(this.image.crs, this.image.bounds, {
        viewMode: controller.getViewMode(),
        centerLat: camera.center_lat, centerLon: camera.center_lon, zoom: camera.zoom,
        rotation: camera.rotation, pitch: camera.pitch, roll: camera.roll,
        project: (lat, lon) => controller.projection.project(lat, lon, 0),
        ...(this.drapeTerrain ? { elevation: (lat: number, lon: number): number | null => {
          try {
            const sample: unknown = controller.terrainEngine.get_elevation_status(lat, lon);
            if (typeof sample !== "object" || sample === null || !("elevation_meters" in sample)) return null;
            const height = sample.elevation_meters;
            return typeof height === "number" && Number.isFinite(height) ? height : null;
          } catch { return null; }
        } } : {}),
      }, this.subdivisions);
    } finally { camera.free(); }
    if (vertices.length === 0) return;
    this.initialize(gl);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.DYNAMIC_DRAW);
    gl.uniformMatrix4fv(gl.getUniformLocation(this.program!, "viewProjection"), false, matrix);
    gl.uniform1f(gl.getUniformLocation(this.program!, "opacity"), this.opacity);
    gl.uniform1i(gl.getUniformLocation(this.program!, "imagery"), 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    drawWithLayerDepth(gl, controller.getViewMode(), () => {
      gl.drawArrays(gl.TRIANGLES, 0, vertices.length / 5);
    }, true);
    gl.bindVertexArray(null);
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _time: number): void {}

  public override destroy(gl: WebGL2RenderingContext): void {
    this.claimContext(gl);
    this.generation++;
    if (this.texture) gl.deleteTexture(this.texture);
    if (this.program) gl.deleteProgram(this.program);
    if (this.vao) gl.deleteVertexArray(this.vao);
    if (this.buffer) gl.deleteBuffer(this.buffer);
    this.texture = null;
    this.program = null;
    this.vao = null;
    this.buffer = null;
    this.image = null;
    this.owner = null;
  }
}
