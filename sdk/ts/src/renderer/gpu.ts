import { WasmProjection, lla_to_ecef } from "olayer-wasm";

export class WebGLRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram | null = null;
  private gridBuffer: WebGLBuffer | null = null;
  private gridLineCount = 0;

  // Uniform locations
  private uViewProjMatrixLoc: WebGLUniformLocation | null = null;
  private uColorLoc: WebGLUniformLocation | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.initShaders();
  }

  /**
   * Compiles the standard shaders for static geometries.
   */
  private initShaders(): void {
    const gl = this.gl;

    const vsSource = `#version 300 es
      in vec3 a_position;
      uniform mat4 u_viewProjMatrix;
      void main() {
        gl_Position = u_viewProjMatrix * vec4(a_position, 1.0);
      }
    `;

    const fsSource = `#version 300 es
      precision mediump float;
      uniform vec4 u_color;
      out vec4 fragColor;
      void main() {
        fragColor = u_color;
      }
    `;

    const vs = this.compileShader(gl.VERTEX_SHADER, vsSource);
    const fs = this.compileShader(gl.FRAGMENT_SHADER, fsSource);

    const program = gl.createProgram();
    if (!program) throw new Error("Failed to create WebGL program.");

    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);

    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(`Failed to link shader program: ${gl.getProgramInfoLog(program)}`);
    }

    this.program = program;
    this.uViewProjMatrixLoc = gl.getUniformLocation(program, "u_viewProjMatrix");
    this.uColorLoc = gl.getUniformLocation(program, "u_color");
    this.gridBuffer = gl.createBuffer();
  }

  private compileShader(type: number, source: string): WebGLShader {
    const gl = this.gl;
    const shader = gl.createShader(type);
    if (!shader) throw new Error("Failed to create WebGL shader.");

    gl.shaderSource(shader, source);
    gl.compileShader(shader);

    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      const log = gl.getShaderInfoLog(shader);
      gl.deleteShader(shader);
      throw new Error(`Failed to compile shader: ${log}`);
    }
    return shader;
  }

  /**
   * Rebuilds the latitude/longitude grid buffer using the active projection or sphere.
   */
  public rebuildGrid(projection: WasmProjection | null, viewMode: string = "2D"): void {
    const gl = this.gl;
    const coords: number[] = [];

    if (viewMode === "3D") {
      const stepDeg = 10; // Grid interval in degrees
      const density = 60; // Smoothness of circles

      // 1. Longitude lines (Meridians)
      for (let lon = -180; lon <= 180; lon += stepDeg) {
        const lonRad = (lon * Math.PI) / 180;
        for (let i = 0; i < density; i++) {
          const lat0 = -90 + (180 / density) * i;
          const lat1 = -90 + (180 / density) * (i + 1);

          const lat0Rad = (lat0 * Math.PI) / 180;
          const lat1Rad = (lat1 * Math.PI) / 180;

          const p0 = lla_to_ecef(lat0Rad, lonRad, 0.0);
          const p1 = lla_to_ecef(lat1Rad, lonRad, 0.0);
          coords.push(p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]);
        }
      }

      // 2. Latitude lines (Parallels)
      for (let lat = -80; lat <= 80; lat += stepDeg) {
        const latRad = (lat * Math.PI) / 180;
        for (let i = 0; i < density; i++) {
          const lon0 = -180 + (360 / density) * i;
          const lon1 = -180 + (360 / density) * (i + 1);

          const lon0Rad = (lon0 * Math.PI) / 180;
          const lon1Rad = (lon1 * Math.PI) / 180;

          const p0 = lla_to_ecef(latRad, lon0Rad, 0.0);
          const p1 = lla_to_ecef(latRad, lon1Rad, 0.0);
          coords.push(p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]);
        }
      }
    } else {
      if (!projection) return;
      const stepDeg = 5; // grid interval in degrees
      const density = 20; // segments per line

      // 1. Longitude lines (Meridians)
      for (let lon = -180; lon <= 180; lon += stepDeg) {
        const lonRad = (lon * Math.PI) / 180;
        for (let i = 0; i < density; i++) {
          const lat0 = -80 + (160 / density) * i;
          const lat1 = -80 + (160 / density) * (i + 1);

          const lat0Rad = (lat0 * Math.PI) / 180;
          const lat1Rad = (lat1 * Math.PI) / 180;

          try {
            const p0 = projection.project(lat0Rad, lonRad, 0.0);
            const p1 = projection.project(lat1Rad, lonRad, 0.0);
            coords.push(p0[0], p0[1], 0.0, p1[0], p1[1], 0.0);
          } catch {
            // ignore out of bounds projection singularities
          }
        }
      }

      // 2. Latitude lines (Parallels)
      for (let lat = -80; lat <= 80; lat += stepDeg) {
        const latRad = (lat * Math.PI) / 180;
        for (let i = 0; i < density; i++) {
          const lon0 = -180 + (360 / density) * i;
          const lon1 = -180 + (360 / density) * (i + 1);

          const lon0Rad = (lon0 * Math.PI) / 180;
          const lon1Rad = (lon1 * Math.PI) / 180;

          try {
            const p0 = projection.project(latRad, lon0Rad, 0.0);
            const p1 = projection.project(latRad, lon1Rad, 0.0);
            coords.push(p0[0], p0[1], 0.0, p1[0], p1[1], 0.0);
          } catch {
            // ignore out of bounds projection singularities
          }
        }
      }
    }

    const vertexData = new Float32Array(coords);
    this.gridLineCount = coords.length / 3;

    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertexData, gl.STATIC_DRAW);
  }

  /**
   * Renders the grid using the active projection matrix.
   */
  public renderGrid(viewProjMatrix: Float32Array): void {
    if (!this.program || this.gridLineCount === 0) return;

    const gl = this.gl;
    gl.useProgram(this.program);

    // Bind buffer
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuffer);
    
    // Enable attribute
    const aPosLoc = gl.getAttribLocation(this.program, "a_position");
    gl.enableVertexAttribArray(aPosLoc);
    gl.vertexAttribPointer(aPosLoc, 3, gl.FLOAT, false, 0, 0);

    // Set uniforms
    gl.uniformMatrix4fv(this.uViewProjMatrixLoc, false, viewProjMatrix);
    
    // Sleek green grid color (alpha 0.15) for high contrast radar feel
    gl.uniform4f(this.uColorLoc, 0.0, 0.8, 0.4, 0.15);

    // Enable transparency blending
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

    // Draw
    gl.drawArrays(gl.LINES, 0, this.gridLineCount);

    gl.disable(gl.BLEND);
  }

  /**
   * Releases WebGL resources.
   */
  public destroy(): void {
    const gl = this.gl;
    if (this.gridBuffer) {
      gl.deleteBuffer(this.gridBuffer);
      this.gridBuffer = null;
    }
    if (this.program) {
      gl.deleteProgram(this.program);
      this.program = null;
    }
  }
}
