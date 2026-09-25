import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
import type { OlayerController } from "../controller";
import { lla_to_ecef, WasmProjection } from "olayer-wasm";

interface TerrainMesh {
  vao: WebGLVertexArrayObject;
  vertexBuffer: WebGLBuffer;
  indexBuffer: WebGLBuffer;
  indexCount: number;
}

export type TerrainRenderMode = "hypsometric" | "hillshade" | "slope" | "hybrid" | "textured" | "taws";

export interface HillshadeOptions {
  azimuthDeg?: number;
  altitudeDeg?: number;
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
  private modeLocation: WebGLUniformLocation | null = null;
  private minElevationLocation: WebGLUniformLocation | null = null;
  private maxElevationLocation: WebGLUniformLocation | null = null;
  private lightDirLocation: WebGLUniformLocation | null = null;
  private aircraftAltitudeLocation: WebGLUniformLocation | null = null;
  private contoursEnabledLocation: WebGLUniformLocation | null = null;
  private contourIntervalLocation: WebGLUniformLocation | null = null;
  private mode: TerrainRenderMode = "hypsometric";
  private minElevation = 0;
  private maxElevation = 2500;
  private hillshadeAzimuth = 315;
  private hillshadeAltitude = 45;
  private aircraftAltitude = 1200;
  private contoursEnabled = false;
  private contourInterval = 100;
  private imageryEnabled = false;
  private imageryTemplate: string | null = null;
  private imageryTexture: WebGLTexture | null = null;
  private imageryLocation: WebGLUniformLocation | null = null;
  private imageryEnabledLocation: WebGLUniformLocation | null = null;
  private imageryRequestKey = "";
  private imageryLoadedKey = "";

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

  public setRenderMode(mode: TerrainRenderMode): void {
    this.mode = mode;
  }

  public getRenderMode(): TerrainRenderMode {
    return this.mode;
  }

  public setElevationRange(minElevation: number, maxElevation: number): void {
    if (Number.isFinite(minElevation) && Number.isFinite(maxElevation) && maxElevation > minElevation) {
      this.minElevation = minElevation;
      this.maxElevation = maxElevation;
    }
  }

  public setHillshade(options: HillshadeOptions): void {
    if (options.azimuthDeg !== undefined) this.hillshadeAzimuth = options.azimuthDeg;
    if (options.altitudeDeg !== undefined) this.hillshadeAltitude = Math.max(1, Math.min(89, options.altitudeDeg));
  }

  public setTawsReferenceAltitude(altitudeMeters: number): void {
    if (Number.isFinite(altitudeMeters)) {
      this.aircraftAltitude = altitudeMeters;
    }
  }

  public getTawsReferenceAltitude(): number {
    return this.aircraftAltitude;
  }

  public setContours(enabled: boolean, intervalMeters = 100): void {
    this.contoursEnabled = enabled;
    if (intervalMeters > 1) {
      this.contourInterval = intervalMeters;
    }
  }

  public isContoursEnabled(): boolean {
    return this.contoursEnabled;
  }

  public getContourInterval(): number {
    return this.contourInterval;
  }

  public setImageryTemplate(template: string | null): void {
    this.imageryTemplate = template;
    this.imageryRequestKey = "";
    this.imageryLoadedKey = "";
  }

  public setImageryEnabled(enabled: boolean): void {
    this.imageryEnabled = enabled;
  }

  public isImageryEnabled(): boolean {
    return this.imageryEnabled;
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
      in float a_slope;
      in vec3 a_normal;
      in vec2 a_uv;
      uniform mat4 u_viewProjMatrix;
      out float v_elevation;
      out float v_slope;
      out vec3 v_normal;
      out vec2 v_uv;
      void main() {
        gl_Position = u_viewProjMatrix * vec4(a_position, 1.0);
        v_elevation = a_elevation;
        v_slope = a_slope;
        v_normal = a_normal;
        v_uv = a_uv;
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
      in float v_slope;
      in vec3 v_normal;
      in vec2 v_uv;
      uniform float u_opacity;
      uniform float u_exaggeration;
      uniform int u_mode;
      uniform float u_minElevation;
      uniform float u_maxElevation;
      uniform vec3 u_lightDir;
      uniform float u_aircraftAltitude;
      uniform bool u_contoursEnabled;
      uniform float u_contourInterval;
      uniform sampler2D u_imagery;
      uniform bool u_imageryEnabled;
      out vec4 fragColor;

      void main() {
        float normalized = clamp((v_elevation - u_minElevation) / max(1.0, u_maxElevation - u_minElevation), 0.0, 1.0);
        vec3 hypsometric = mix(vec3(0.08, 0.28, 0.12), vec3(0.72, 0.52, 0.22), normalized);
        hypsometric = mix(hypsometric, vec3(0.95, 0.95, 0.92), smoothstep(0.78, 1.0, normalized));
        vec3 slopeColor = mix(vec3(0.08, 0.55, 0.18), vec3(0.9, 0.08, 0.04), clamp(v_slope / 55.0, 0.0, 1.0));
        
        vec3 norm = normalize(v_normal);
        float hillshade = max(0.0, dot(norm, u_lightDir));
        vec3 shadeColor = vec3(0.12, 0.2, 0.12) + vec3(0.76, 0.72, 0.5) * hillshade;

        // TAWS / CFIT mode (mode 5):
        // Red: terrain within 150m below or above aircraft altitude (critical hazard)
        // Yellow: terrain between 150m and 600m below aircraft (caution)
        // Green/Muted: terrain safe (> 600m below aircraft)
        float delta = v_elevation - u_aircraftAltitude;
        vec3 tawsColor;
        if (delta >= -150.0) {
          tawsColor = vec3(0.92, 0.12, 0.12);
        } else if (delta >= -600.0) {
          tawsColor = vec3(0.95, 0.82, 0.15);
        } else {
          tawsColor = mix(vec3(0.08, 0.35, 0.14), vec3(0.15, 0.45, 0.2), clamp((delta + 1500.0) / 900.0, 0.0, 1.0));
        }
        tawsColor *= (0.45 + 0.55 * hillshade);

        vec3 color = hypsometric;
        if (u_mode == 1) color = shadeColor;
        else if (u_mode == 2) color = slopeColor;
        else if (u_mode == 3) color = mix(hypsometric, shadeColor, 0.55);
        else if (u_mode == 5) color = tawsColor;

        if (u_imageryEnabled && (u_mode == 3 || u_mode == 4)) {
          vec3 imagery = texture(u_imagery, v_uv).rgb;
          color = u_mode == 4 ? imagery : imagery * (0.45 + 0.75 * hillshade);
        }

        // Procedural contour lines via screen derivative fwidth
        if (u_contoursEnabled && u_contourInterval > 1.0) {
          float c = abs(fract(v_elevation / u_contourInterval - 0.5) - 0.5) / max(0.0001, fwidth(v_elevation / u_contourInterval));
          float contourAlpha = 1.0 - clamp(c - 0.5, 0.0, 1.0);
          vec3 contourColor = vec3(0.98, 0.85, 0.28);
          color = mix(color, contourColor, contourAlpha * 0.85);
        }

        fragColor = vec4(color, u_opacity * 0.82);
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
    this.modeLocation = gl.getUniformLocation(this.program, "u_mode");
    this.minElevationLocation = gl.getUniformLocation(this.program, "u_minElevation");
    this.maxElevationLocation = gl.getUniformLocation(this.program, "u_maxElevation");
    this.lightDirLocation = gl.getUniformLocation(this.program, "u_lightDir");
    this.aircraftAltitudeLocation = gl.getUniformLocation(this.program, "u_aircraftAltitude");
    this.contoursEnabledLocation = gl.getUniformLocation(this.program, "u_contoursEnabled");
    this.contourIntervalLocation = gl.getUniformLocation(this.program, "u_contourInterval");
    this.imageryLocation = gl.getUniformLocation(this.program, "u_imagery");
    this.imageryEnabledLocation = gl.getUniformLocation(this.program, "u_imageryEnabled");
    this.imageryTexture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.imageryTexture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([70, 90, 70, 255]));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindTexture(gl.TEXTURE_2D, null);
  }

  private disposeMesh(gl: WebGL2RenderingContext): void {
    if (!this.mesh) return;
    gl.deleteVertexArray(this.mesh.vao);
    gl.deleteBuffer(this.mesh.vertexBuffer);
    gl.deleteBuffer(this.mesh.indexBuffer);
    this.mesh = null;
  }

  private getMeshKey(controller: OlayerController): string {
    const camera = controller.getCameraState();
    const projection = controller.projection;
    const version = projection.version();
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

  private rebuildMesh(gl: WebGL2RenderingContext, controller: OlayerController): void {
    const camera = controller.getCameraState();
    const centerLat = controller.getCenterLat();
    const centerLon = controller.getCenterLon();
    const spanMeters = camera.viewport_base_meters / camera.zoom;
    const latSpan = Math.min(Math.PI / 3, spanMeters / 111_000.0 / 2 * Math.PI / 180);
    const lonSpan = latSpan / Math.max(0.15, Math.cos(centerLat));
    const vertices: number[] = [];
    const indices: number[] = [];
    const elevations: number[][] = [];
    const viewMode = controller.getViewMode();
    const projection = controller.projection;
    const tile = this.imageryTile(controller);

    for (let row = 0; row <= this.gridSize; row++) {
      const elevationRow: number[] = [];
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
        elevationRow.push(elevation);
        const height = elevation * this.verticalExaggeration;
        let position: number[];
        if (viewMode === "3D") {
          position = Array.from(lla_to_ecef(lat, lon, height));
        } else {
          const projected = projection.project(lat, lon, 0);
          const [x, y] = projected;
          if (x === undefined || y === undefined) continue;
          position = [x, y, height];
        }
        // Vertex layout: position(3), elevation(1), slope(1), normal(3), uv(2) -> 10 floats = 40 bytes
         const [positionX, positionY, positionZ] = position;
         if (positionX === undefined || positionY === undefined || positionZ === undefined) continue;
         vertices.push(positionX, positionY, positionZ, elevation, 0, 0, 0, 1, ...this.imageryUv(lat, lon, tile));
      }
      elevations.push(elevationRow);
    }

    const dx = Math.max(1, spanMeters / this.gridSize);
    const dy = dx;
    for (let row = 0; row <= this.gridSize; row++) {
      for (let column = 0; column <= this.gridSize; column++) {
        const currentRow = elevations[row];
        const previousRow = elevations[Math.max(0, row - 1)];
        const nextRow = elevations[Math.min(this.gridSize, row + 1)];
        if (!currentRow || !previousRow || !nextRow) continue;
        const left = currentRow[Math.max(0, column - 1)];
        const right = currentRow[Math.min(this.gridSize, column + 1)];
        const north = previousRow[column];
        const south = nextRow[column];
        if (left === undefined || right === undefined || north === undefined || south === undefined) continue;
        const dzdx = (right - left) / (column === 0 || column === this.gridSize ? dx : 2 * dx);
        const dzdy = (south - north) / (row === 0 || row === this.gridSize ? dy : 2 * dy);
        const slope = Math.atan(Math.hypot(dzdx, dzdy)) * 180 / Math.PI;
        const nx = -dzdx;
        const ny = -dzdy;
        const nz = 1;
        const length = Math.hypot(nx, ny, nz);
        const offset = (row * (this.gridSize + 1) + column) * 10;
        vertices[offset + 4] = slope;
        vertices[offset + 5] = nx / length;
        vertices[offset + 6] = ny / length;
        vertices[offset + 7] = nz / length;
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

    // Stride: 10 floats = 40 bytes
    const stride = 40;
    const positionLocation = gl.getAttribLocation(this.program!, "a_position");
    const elevationLocation = gl.getAttribLocation(this.program!, "a_elevation");
    const slopeLocation = gl.getAttribLocation(this.program!, "a_slope");
    const normalLocation = gl.getAttribLocation(this.program!, "a_normal");
    const uvLocation = gl.getAttribLocation(this.program!, "a_uv");

    gl.enableVertexAttribArray(positionLocation);
    gl.vertexAttribPointer(positionLocation, 3, gl.FLOAT, false, stride, 0);
    gl.enableVertexAttribArray(elevationLocation);
    gl.vertexAttribPointer(elevationLocation, 1, gl.FLOAT, false, stride, 12);
    gl.enableVertexAttribArray(slopeLocation);
    gl.vertexAttribPointer(slopeLocation, 1, gl.FLOAT, false, stride, 16);
    gl.enableVertexAttribArray(normalLocation);
    gl.vertexAttribPointer(normalLocation, 3, gl.FLOAT, false, stride, 20);
    gl.enableVertexAttribArray(uvLocation);
    gl.vertexAttribPointer(uvLocation, 2, gl.FLOAT, false, stride, 32);
    gl.bindVertexArray(null);
    this.mesh = { vao, vertexBuffer, indexBuffer, indexCount: indices.length };
  }

  private imageryTile(controller: OlayerController): { z: number; x: number; y: number } {
    const camera = controller.getCameraState();
    const spanMeters = camera.viewport_base_meters / camera.zoom;
    const latSpan = Math.min(Math.PI / 3, spanMeters / 111_000.0 / 2 * Math.PI / 180);
    const centerLat = controller.getCenterLat();
    const lonSpan = latSpan / Math.max(0.15, Math.cos(centerLat));
    const lonSpanDeg = lonSpan * (180 / Math.PI) * 2;
    // Calibrate zoom so that 1 tile closely matches the geographic footprint of the mesh
    const z = Math.max(1, Math.min(18, Math.floor(Math.log2(360 / Math.max(0.001, lonSpanDeg)))));
    const lat = centerLat;
    const lon = controller.getCenterLon();
    const n = 2 ** z;
    const x = Math.floor((lon * 180 / Math.PI + 180) / 360 * n);
    const mercatorY = (1 - Math.asinh(Math.tan(lat)) / Math.PI) / 2;
    const y = Math.floor(mercatorY * n);
    camera.free();
    return { z, x: Math.max(0, Math.min(n - 1, x)), y: Math.max(0, Math.min(n - 1, y)) };
  }

  private imageryUv(lat: number, lon: number, tile: { z: number; x: number; y: number }): [number, number] {
    const n = 2 ** tile.z;
    const u = (lon * 180 / Math.PI + 180) / 360 * n - tile.x;
    const v = (1 - Math.asinh(Math.tan(lat)) / Math.PI) / 2 * n - tile.y;
    return [u, v];
  }

  private ensureImagery(gl: WebGL2RenderingContext, controller: OlayerController): void {
    if (!this.imageryTemplate || !this.imageryEnabled || !this.imageryTexture) return;
    const tile = this.imageryTile(controller);
    const key = `${tile.z}/${tile.x}/${tile.y}`;
    if (this.imageryRequestKey === key) return;
    this.imageryRequestKey = key;
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.onload = () => {
      gl.bindTexture(gl.TEXTURE_2D, this.imageryTexture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.bindTexture(gl.TEXTURE_2D, null);
      this.imageryLoadedKey = key;
      controller.triggerActive();
    };
    image.onerror = () => { this.imageryLoadedKey = ""; };
    image.src = this.imageryTemplate.replace("{z}", String(tile.z)).replace("{x}", String(tile.x)).replace("{y}", String(tile.y));
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void {
    if (!this.visible || this.opacity <= 0.01) return;
    const controller = context?.controller;
    if (!controller) return;
    if (controller.getViewMode() === "2D") return;

    this.initWebGL(gl);
    this.ensureImagery(gl, controller);
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
    const modes: Record<TerrainRenderMode, number> = {
      hypsometric: 0,
      hillshade: 1,
      slope: 2,
      hybrid: 3,
      textured: 4,
      taws: 5,
    };
    gl.uniform1i(this.modeLocation, modes[this.mode]);
    gl.uniform1f(this.minElevationLocation, this.minElevation);
    gl.uniform1f(this.maxElevationLocation, this.maxElevation);

    // Dynamic light direction (Phong hillshade)
    const azimuth = this.hillshadeAzimuth * Math.PI / 180;
    const altitude = this.hillshadeAltitude * Math.PI / 180;
    const lightX = Math.sin(azimuth) * Math.cos(altitude);
    const lightY = Math.cos(azimuth) * Math.cos(altitude);
    const lightZ = Math.sin(altitude);
    gl.uniform3f(this.lightDirLocation, lightX, lightY, lightZ);

    gl.uniform1f(this.aircraftAltitudeLocation, this.aircraftAltitude);
    gl.uniform1i(this.contoursEnabledLocation, this.contoursEnabled ? 1 : 0);
    gl.uniform1f(this.contourIntervalLocation, this.contourInterval);

    gl.uniform1i(this.imageryLocation, 0);
    gl.uniform1i(this.imageryEnabledLocation, this.imageryEnabled && this.imageryLoadedKey !== "" ? 1 : 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.imageryTexture);
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindVertexArray(this.mesh.vao);
    gl.drawElements(gl.TRIANGLES, this.mesh.indexCount, gl.UNSIGNED_INT, 0);
    gl.bindVertexArray(null);
    gl.bindTexture(gl.TEXTURE_2D, null);
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

  private meshKey(controller: OlayerController): string {
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

  private projectPoint(controller: OlayerController, lat: number, lon: number, elevation: number): number[] {
    const height = elevation * this.verticalExaggeration + 1;
    if (controller.getViewMode() === "3D") {
      return Array.from(lla_to_ecef(lat, lon, height));
    }
    const projected = controller.projection.project(lat, lon, 0);
    const [x, y] = projected;
    if (x === undefined || y === undefined) return [0, 0, height];
    return [x, y, height];
  }

  private rebuild(gl: WebGL2RenderingContext, controller: OlayerController): void {
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
        const [x, y, z] = position;
        if (x === undefined || y === undefined || z === undefined) continue;
        sampleRow.push({ x, y, z, elevation });
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
        const currentSampleRow = samples[row];
        const nextSampleRow = samples[row + 1];
        if (!currentSampleRow || !nextSampleRow) continue;
        const topLeft = currentSampleRow[column];
        const topRight = currentSampleRow[column + 1];
        const bottomRight = nextSampleRow[column + 1];
        const bottomLeft = nextSampleRow[column];
        if (!topLeft || !topRight || !bottomRight || !bottomLeft) continue;
        const contourCorners: readonly ContourPoint[] = [topLeft, topRight, bottomRight, bottomLeft];
        for (let level = firstLevel; level <= maximum; level += this.interval) {
          const intersections: ContourPoint[] = [];
          for (const [a, b] of [[0, 1], [1, 2], [2, 3], [3, 0]] as const) {
            const start = contourCorners[a];
            const end = contourCorners[b];
            if (!start || !end) continue;
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
            const first = intersections[0];
            const second = intersections[1];
            if (first && second) {
              vertices.push(first.x, first.y, first.z, second.x, second.y, second.z);
            }
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

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void {
    if (!this.visible || this.opacity <= 0.01) return;
    const controller = context?.controller;
    if (!controller) return;
    if (controller.getViewMode() === "2D") return;
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
