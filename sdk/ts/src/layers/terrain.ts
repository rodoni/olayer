import { Layer } from "./layer";
import { lla_to_ecef, WasmProjection } from "olayer-wasm";

interface TerrainMesh {
  vao: WebGLVertexArrayObject;
  vertexBuffer: WebGLBuffer;
  indexBuffer: WebGLBuffer;
  indexCount: number;
}

/**
 * Renders a sampled elevation grid from the controller's TerrainEngine.
 * The layer deliberately samples the existing engine instead of owning a
 * second terrain cache, keeping the demo aligned with the SDK data path.
 */
export class TerrainLayer extends Layer {
  private readonly gridSize: number;
  private mesh: TerrainMesh | null = null;
  private program: WebGLProgram | null = null;
  private lastKey = "";
  private verticalExaggeration = 1;
  private lastProjection: WasmProjection | null = null;
  private viewProjLocation: WebGLUniformLocation | null = null;
  private opacityLocation: WebGLUniformLocation | null = null;
  private exaggerationLocation: WebGLUniformLocation | null = null;

  public constructor(id: string, gridSize = 32) {
    super(id);
    this.gridSize = Math.max(4, Math.min(64, Math.floor(gridSize)));
  }

  public setVerticalExaggeration(value: number): void {
    this.verticalExaggeration = Math.max(0, Math.min(10, value));
    this.lastKey = "";
  }

  public getVerticalExaggeration(): number {
    return this.verticalExaggeration;
  }

  public invalidateMesh(): void {
    this.lastKey = "";
    this.lastProjection = null;
  }

  private initWebGL(gl: WebGL2RenderingContext): void {
    if (this.program) return;

    const vertexShader = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vertexShader, `#version 300 es
      in vec3 a_position;
      in float a_elevation;
      uniform mat4 u_viewProjMatrix;
      out float v_elevation;
      void main() {
        gl_Position = u_viewProjMatrix * vec4(a_position, 1.0);
        v_elevation = a_elevation;
      }
    `);
    gl.compileShader(vertexShader);
    if (!gl.getShaderParameter(vertexShader, gl.COMPILE_STATUS)) {
      throw new Error(`Terrain vertex shader error: ${gl.getShaderInfoLog(vertexShader)}`);
    }

    const fragmentShader = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fragmentShader, `#version 300 es
      precision mediump float;
      in float v_elevation;
      uniform float u_opacity;
      uniform float u_exaggeration;
      out vec4 fragColor;
      void main() {
        float normalized = clamp((v_elevation + 100.0) / 1400.0, 0.0, 1.0);
        vec3 low = vec3(0.08, 0.18, 0.12);
        vec3 high = vec3(0.62, 0.48, 0.22);
        vec3 color = mix(low, high, normalized);
        fragColor = vec4(color, u_opacity * 0.78);
      }
    `);
    gl.compileShader(fragmentShader);
    if (!gl.getShaderParameter(fragmentShader, gl.COMPILE_STATUS)) {
      throw new Error(`Terrain fragment shader error: ${gl.getShaderInfoLog(fragmentShader)}`);
    }

    this.program = gl.createProgram()!;
    gl.attachShader(this.program, vertexShader);
    gl.attachShader(this.program, fragmentShader);
    gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
      throw new Error(`Terrain shader link error: ${gl.getProgramInfoLog(this.program)}`);
    }

    this.viewProjLocation = gl.getUniformLocation(this.program, "u_viewProjMatrix");
    this.opacityLocation = gl.getUniformLocation(this.program, "u_opacity");
    this.exaggerationLocation = gl.getUniformLocation(this.program, "u_exaggeration");
  }

  private disposeMesh(gl: WebGL2RenderingContext): void {
    if (!this.mesh) return;
    gl.deleteVertexArray(this.mesh.vao);
    gl.deleteBuffer(this.mesh.vertexBuffer);
    gl.deleteBuffer(this.mesh.indexBuffer);
    this.mesh = null;
  }

  private getMeshKey(controller: any): string {
    const camera = controller.getCameraState();
    const projection = controller.projection as WasmProjection;
    const version = typeof (projection as any).version === "function" ? (projection as any).version() : 0;
    const key = [
      controller.getViewMode(),
      controller.getCenterLat().toFixed(5),
      controller.getCenterLon().toFixed(5),
      camera.zoom.toFixed(3),
      this.verticalExaggeration.toFixed(2),
      version
    ].join(":");
    camera.free();
    return key;
  }

  private rebuildMesh(gl: WebGL2RenderingContext, controller: any): void {
    const camera = controller.getCameraState();
    const centerLat = controller.getCenterLat();
    const centerLon = controller.getCenterLon();
    const spanMeters = camera.viewport_base_meters / camera.zoom;
    const latSpan = Math.min(Math.PI / 3, spanMeters / 111_000.0 / 2 * Math.PI / 180);
    const lonSpan = latSpan / Math.max(0.15, Math.cos(centerLat));
    const vertices: number[] = [];
    const indices: number[] = [];
    const viewMode = controller.getViewMode();
    const projection = controller.projection as WasmProjection;

    for (let row = 0; row <= this.gridSize; row++) {
      const v = row / this.gridSize;
      const lat = centerLat + (0.5 - v) * latSpan;
      for (let column = 0; column <= this.gridSize; column++) {
        const u = column / this.gridSize;
        const lon = centerLon + (u - 0.5) * lonSpan;
        let elevation = 0;
        try {
          elevation = controller.terrainEngine.get_elevation(lat * 180 / Math.PI, lon * 180 / Math.PI) || 0;
        } catch {
          // Unknown terrain is rendered at sea level until a source is available.
        }
        const height = elevation * this.verticalExaggeration;
        let position: number[];
        if (viewMode === "3D") {
          position = Array.from(lla_to_ecef(lat, lon, height));
        } else {
          const projected = projection.project(lat, lon, 0);
          position = [projected[0], projected[1], height];
        }
        vertices.push(position[0], position[1], position[2], elevation);
      }
    }

    for (let row = 0; row < this.gridSize; row++) {
      for (let column = 0; column < this.gridSize; column++) {
        const topLeft = row * (this.gridSize + 1) + column;
        const topRight = topLeft + 1;
        const bottomLeft = topLeft + this.gridSize + 1;
        const bottomRight = bottomLeft + 1;
        indices.push(topLeft, topRight, bottomLeft, topRight, bottomRight, bottomLeft);
      }
    }
    camera.free();

    this.disposeMesh(gl);
    const vao = gl.createVertexArray()!;
    const vertexBuffer = gl.createBuffer()!;
    const indexBuffer = gl.createBuffer()!;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, vertexBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, indexBuffer);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, new Uint32Array(indices), gl.STATIC_DRAW);
    const positionLocation = gl.getAttribLocation(this.program!, "a_position");
    const elevationLocation = gl.getAttribLocation(this.program!, "a_elevation");
    gl.enableVertexAttribArray(positionLocation);
    gl.vertexAttribPointer(positionLocation, 3, gl.FLOAT, false, 16, 0);
    gl.enableVertexAttribArray(elevationLocation);
    gl.vertexAttribPointer(elevationLocation, 1, gl.FLOAT, false, 16, 12);
    gl.bindVertexArray(null);
    this.mesh = { vao, vertexBuffer, indexBuffer, indexCount: indices.length };
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    if (!this.visible || this.opacity <= 0.01) return;
    const controller = (window as any).olayerController;
    if (!controller) return;

    this.initWebGL(gl);
    const key = this.getMeshKey(controller);
    if (key !== this.lastKey || this.lastProjection !== controller.projection || !this.mesh) {
      this.rebuildMesh(gl, controller);
      this.lastKey = key;
      this.lastProjection = controller.projection;
    }
    if (!this.mesh) return;

    gl.useProgram(this.program);
    gl.uniformMatrix4fv(this.viewProjLocation, false, viewProjMatrix);
    gl.uniform1f(this.opacityLocation, this.opacity);
    gl.uniform1f(this.exaggerationLocation, this.verticalExaggeration);
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindVertexArray(this.mesh.vao);
    gl.drawElements(gl.TRIANGLES, this.mesh.indexCount, gl.UNSIGNED_INT, 0);
    gl.bindVertexArray(null);
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    // Terrain is rendered entirely by WebGL.
  }
}

interface ContourPoint {
  x: number;
  y: number;
  z: number;
  elevation: number;
}

/** Draws isolines over the sampled terrain mesh to make relief readable. */
export class TerrainContourLayer extends Layer {
  private readonly gridSize = 48;
  private interval = 100;
  private verticalExaggeration = 1;
  private program: WebGLProgram | null = null;
  private vertexBuffer: WebGLBuffer | null = null;
  private vertexCount = 0;
  private lastKey = "";
  private lastProjection: WasmProjection | null = null;
  private matrixLocation: WebGLUniformLocation | null = null;
  private opacityLocation: WebGLUniformLocation | null = null;

  public constructor(id: string) {
    super(id);
  }

  public setInterval(value: number): void {
    this.interval = Math.max(10, Math.min(2000, value));
    this.invalidate();
  }

  public setVerticalExaggeration(value: number): void {
    this.verticalExaggeration = Math.max(0, Math.min(10, value));
    this.invalidate();
  }

  public invalidate(): void {
    this.lastKey = "";
    this.lastProjection = null;
  }

  private initWebGL(gl: WebGL2RenderingContext): void {
    if (this.program) return;
    const vertexShader = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vertexShader, `#version 300 es
      in vec3 a_position;
      uniform mat4 u_viewProjMatrix;
      void main() { gl_Position = u_viewProjMatrix * vec4(a_position, 1.0); }
    `);
    gl.compileShader(vertexShader);
    if (!gl.getShaderParameter(vertexShader, gl.COMPILE_STATUS)) {
      throw new Error(`Contour vertex shader error: ${gl.getShaderInfoLog(vertexShader)}`);
    }

    const fragmentShader = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fragmentShader, `#version 300 es
      precision mediump float;
      uniform float u_opacity;
      out vec4 fragColor;
      void main() { fragColor = vec4(0.98, 0.78, 0.25, u_opacity); }
    `);
    gl.compileShader(fragmentShader);
    if (!gl.getShaderParameter(fragmentShader, gl.COMPILE_STATUS)) {
      throw new Error(`Contour fragment shader error: ${gl.getShaderInfoLog(fragmentShader)}`);
    }

    this.program = gl.createProgram()!;
    gl.attachShader(this.program, vertexShader);
    gl.attachShader(this.program, fragmentShader);
    gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
      throw new Error(`Contour shader link error: ${gl.getProgramInfoLog(this.program)}`);
    }
    this.matrixLocation = gl.getUniformLocation(this.program, "u_viewProjMatrix");
    this.opacityLocation = gl.getUniformLocation(this.program, "u_opacity");
  }

  private meshKey(controller: any): string {
    const camera = controller.getCameraState();
    const key = [
      controller.getViewMode(),
      controller.getCenterLat().toFixed(5),
      controller.getCenterLon().toFixed(5),
      camera.zoom.toFixed(3),
      this.interval.toFixed(1),
      this.verticalExaggeration.toFixed(2)
    ].join(":");
    camera.free();
    return key;
  }

  private projectPoint(controller: any, lat: number, lon: number, elevation: number): number[] {
    const height = elevation * this.verticalExaggeration + 1;
    if (controller.getViewMode() === "3D") {
      return Array.from(lla_to_ecef(lat, lon, height));
    }
    const projected = (controller.projection as WasmProjection).project(lat, lon, 0);
    return [projected[0], projected[1], height];
  }

  private rebuild(gl: WebGL2RenderingContext, controller: any): void {
    const camera = controller.getCameraState();
    const centerLat = controller.getCenterLat();
    const centerLon = controller.getCenterLon();
    const spanMeters = camera.viewport_base_meters / camera.zoom;
    const latSpan = Math.min(Math.PI / 3, spanMeters / 111_000 / 2 * Math.PI / 180);
    const lonSpan = latSpan / Math.max(0.15, Math.cos(centerLat));
    const samples: ContourPoint[][] = [];
    let minimum = Number.POSITIVE_INFINITY;
    let maximum = Number.NEGATIVE_INFINITY;

    for (let row = 0; row <= this.gridSize; row++) {
      const sampleRow: ContourPoint[] = [];
      const lat = centerLat + (0.5 - row / this.gridSize) * latSpan;
      for (let column = 0; column <= this.gridSize; column++) {
        const lon = centerLon + (column / this.gridSize - 0.5) * lonSpan;
        let elevation = 0;
        try {
          elevation = controller.terrainEngine.get_elevation(lat * 180 / Math.PI, lon * 180 / Math.PI) || 0;
        } catch {
          // Keep uncovered samples at sea level.
        }
        const position = this.projectPoint(controller, lat, lon, elevation);
        sampleRow.push({ x: position[0], y: position[1], z: position[2], elevation });
        minimum = Math.min(minimum, elevation);
        maximum = Math.max(maximum, elevation);
      }
      samples.push(sampleRow);
    }
    camera.free();

    const vertices: number[] = [];
    const firstLevel = Math.ceil(minimum / this.interval) * this.interval;
    for (let row = 0; row < this.gridSize; row++) {
      for (let column = 0; column < this.gridSize; column++) {
        const corners = [
          samples[row][column], samples[row][column + 1],
          samples[row + 1][column + 1], samples[row + 1][column]
        ];
        for (let level = firstLevel; level <= maximum; level += this.interval) {
          const intersections: ContourPoint[] = [];
          for (const [a, b] of [[0, 1], [1, 2], [2, 3], [3, 0]]) {
            const start = corners[a];
            const end = corners[b];
            if ((start.elevation < level) === (end.elevation < level) || start.elevation === end.elevation) continue;
            const fraction = (level - start.elevation) / (end.elevation - start.elevation);
            intersections.push({
              x: start.x + (end.x - start.x) * fraction,
              y: start.y + (end.y - start.y) * fraction,
              z: start.z + (end.z - start.z) * fraction,
              elevation: level
            });
          }
          if (intersections.length === 2) {
            vertices.push(intersections[0].x, intersections[0].y, intersections[0].z, intersections[1].x, intersections[1].y, intersections[1].z);
          }
        }
      }
    }

    gl.deleteBuffer(this.vertexBuffer);
    this.vertexBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    this.vertexCount = vertices.length / 3;
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    if (!this.visible || this.opacity <= 0.01) return;
    const controller = (window as any).olayerController;
    if (!controller) return;
    this.initWebGL(gl);
    const key = this.meshKey(controller);
    if (key !== this.lastKey || this.lastProjection !== controller.projection || !this.vertexBuffer) {
      this.rebuild(gl, controller);
      this.lastKey = key;
      this.lastProjection = controller.projection;
    }
    if (!this.vertexBuffer || this.vertexCount === 0) return;
    gl.useProgram(this.program);
    gl.uniformMatrix4fv(this.matrixLocation, false, viewProjMatrix);
    gl.uniform1f(this.opacityLocation, this.opacity);
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer);
    const positionLocation = gl.getAttribLocation(this.program!, "a_position");
    gl.enableVertexAttribArray(positionLocation);
    gl.vertexAttribPointer(positionLocation, 3, gl.FLOAT, false, 12, 0);
    gl.drawArrays(gl.LINES, 0, this.vertexCount);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
  }

  public renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    // Contours are rendered entirely by WebGL.
  }
}
