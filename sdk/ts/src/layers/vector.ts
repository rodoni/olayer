import { Layer } from "./layer";
import { VectorTileSource, VectorFeature } from "../providers/vector";
import { WasmProjection, lla_to_ecef } from "olayer-wasm";

/**
 * Capa de renderizado para dados vetoriais (fronteiras, aerovias, setores).
 * Desenha linhas e polígonos na GPU usando WebGL2.
 */
export class VectorTileLayer extends Layer {
  private vectorSource: VectorTileSource;
  private program: WebGLProgram | null = null;
  private vertexBuffer: WebGLBuffer | null = null;
  private vao: WebGLVertexArrayObject | null = null;

  // Uniform locations
  private uViewProjMatrixLoc: WebGLUniformLocation | null = null;
  private uColorLoc: WebGLUniformLocation | null = null;

  constructor(id: string, vectorSource: VectorTileSource) {
    super(id);
    this.vectorSource = vectorSource;
  }

  /**
   * Inicializa shaders para linhas vetoriais.
   */
  private initWebGL(gl: WebGL2RenderingContext): void {
    if (this.program) return;

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

    const vs = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vs, vsSource);
    gl.compileShader(vs);
    if (!gl.getShaderParameter(vs, gl.COMPILE_STATUS)) {
      throw new Error(`Vector VS Error: ${gl.getShaderInfoLog(vs)}`);
    }

    const fs = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fs, fsSource);
    gl.compileShader(fs);
    if (!gl.getShaderParameter(fs, gl.COMPILE_STATUS)) {
      throw new Error(`Vector FS Error: ${gl.getShaderInfoLog(fs)}`);
    }

    this.program = gl.createProgram()!;
    gl.attachShader(this.program, vs);
    gl.attachShader(this.program, fs);
    gl.linkProgram(this.program);

    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
      throw new Error(`Vector Link Error: ${gl.getProgramInfoLog(this.program)}`);
    }

    this.uViewProjMatrixLoc = gl.getUniformLocation(this.program, "u_viewProjMatrix");
    this.uColorLoc = gl.getUniformLocation(this.program, "u_color");

    this.vertexBuffer = gl.createBuffer();
    this.vao = gl.createVertexArray();

    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer);

    const aPosLoc = gl.getAttribLocation(this.program, "a_position");
    gl.enableVertexAttribArray(aPosLoc);
    gl.vertexAttribPointer(aPosLoc, 3, gl.FLOAT, false, 0, 0);

    gl.bindVertexArray(null);
  }

  private lonToTileX(lonRad: number, z: number): number {
    const lonDeg = lonRad * (180 / Math.PI);
    return Math.floor(((lonDeg + 180) / 360) * Math.pow(2, z));
  }

  private latToTileY(latRad: number, z: number): number {
    const latDeg = latRad * (180 / Math.PI);
    const latRadClamped = Math.max(-85.0511, Math.min(85.0511, latDeg)) * (Math.PI / 180);
    return Math.floor(
      ((1 - Math.log(Math.tan(latRadClamped) + 1 / Math.cos(latRadClamped)) / Math.PI) / 2) * Math.pow(2, z)
    );
  }

  private getVisibleTileBounds(
    controller: any,
    z: number
  ): { minX: number; maxX: number; minY: number; maxY: number } {
    const camera = controller.getCameraState();
    const centerLat = camera.center_lat;
    const centerLon = camera.center_lon;
    const zoom = camera.zoom;
    const rotation = camera.rotation;
    const viewportBaseMeters = camera.viewport_base_meters;
    const projection = controller.projection;
    const canvasWidth = controller.glCanvas.width;
    const canvasHeight = controller.glCanvas.height;
    camera.free();

    const centerTileX = this.lonToTileX(centerLon, z);
    const centerTileY = this.latToTileY(centerLat, z);
    const defaultBounds = {
      minX: Math.max(0, centerTileX - 1),
      maxX: Math.min(Math.pow(2, z) - 1, centerTileX + 1),
      minY: Math.max(0, centerTileY - 1),
      maxY: Math.min(Math.pow(2, z) - 1, centerTileY + 1),
    };

    if (controller.getViewMode() === "3D") {
      return {
        minX: Math.max(0, centerTileX - 2),
        maxX: Math.min(Math.pow(2, z) - 1, centerTileX + 2),
        minY: Math.max(0, centerTileY - 2),
        maxY: Math.min(Math.pow(2, z) - 1, centerTileY + 2),
      };
    }

    let cx = 0, cy = 0;
    try {
      const xy = projection.project(centerLat, centerLon, 0.0);
      cx = xy[0];
      cy = xy[1];
    } catch {
      return defaultBounds;
    }

    const aspect = canvasWidth / canvasHeight;
    const w = viewportBaseMeters / zoom;
    const h = w / aspect;

    const metersPerPixelX = w / canvasWidth;
    const metersPerPixelY = h / canvasHeight;

    const corners = [
      [-canvasWidth / 2, -canvasHeight / 2],
      [canvasWidth / 2, -canvasHeight / 2],
      [-canvasWidth / 2, canvasHeight / 2],
      [canvasWidth / 2, canvasHeight / 2],
    ];

    let minTileX = Infinity;
    let maxTileX = -Infinity;
    let minTileY = Infinity;
    let maxTileY = -Infinity;

    const cosTheta = Math.cos(rotation);
    const sinTheta = Math.sin(rotation);

    for (const [dx, dy] of corners) {
      const mx = dx * metersPerPixelX;
      const my = -dy * metersPerPixelY;

      const rx = mx * cosTheta - my * sinTheta;
      const ry = mx * sinTheta + my * cosTheta;

      const px = cx + rx;
      const py = cy + ry;

      try {
        const lla = projection.unproject(px, py);
        const tx = this.lonToTileX(lla.lon, z);
        const ty = this.latToTileY(lla.lat, z);
        lla.free();

        if (tx < minTileX) minTileX = tx;
        if (tx > maxTileX) maxTileX = tx;
        if (ty < minTileY) minTileY = ty;
        if (ty > maxTileY) maxTileY = ty;
      } catch {
        // Ignore edge of projection errors
      }
    }

    if (minTileX === Infinity || maxTileX === -Infinity || minTileY === Infinity || maxTileY === -Infinity) {
      return defaultBounds;
    }

    const margin = 1;
    const maxVal = Math.pow(2, z) - 1;
    return {
      minX: Math.max(0, minTileX - margin),
      maxX: Math.min(maxVal, maxTileX + margin),
      minY: Math.max(0, minTileY - margin),
      maxY: Math.min(maxVal, maxTileY + margin),
    };
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    if (!this.visible || this.opacity <= 0.01) return;

    this.initWebGL(gl);

    const controller = (window as any).olayerController;
    if (!controller) return;

    const camera = controller.getCameraState();
    const zoom = camera.zoom;
    const viewMode = controller.getViewMode();
    const projection = controller.projection;
    camera.free();

    // Determina nível de zoom dos tiles
    const z = Math.max(1, Math.min(18, Math.floor(Math.log2(zoom) + 11.5)));

    const bounds = this.getVisibleTileBounds(controller, z);

    const featuresToDraw: VectorFeature[] = [];

    for (let ty = bounds.minY; ty <= bounds.maxY; ty++) {
      for (let tx = bounds.minX; tx <= bounds.maxX; tx++) {
        // Dispara o carregamento assíncrono (se não estiver no cache)
        this.vectorSource.loadTile(tx, ty, z).catch(() => {});
        const tileFeatures = this.vectorSource.getTileFeatures(tx, ty, z);
        if (tileFeatures && tileFeatures.length > 0) {
          featuresToDraw.push(...tileFeatures);
        }
      }
    }

    if (featuresToDraw.length === 0) return;

    gl.useProgram(this.program!);
    gl.bindVertexArray(this.vao!);
    gl.uniformMatrix4fv(this.uViewProjMatrixLoc, false, viewProjMatrix);

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

    for (const feature of featuresToDraw) {
      // Se for um ponto, desenha um pequeno quadrado/marcador ao redor de suas coordenadas
      if (feature.type === "Point" && feature.coordinates.length > 0) {
        const lat = feature.coordinates[0][0];
        const lon = feature.coordinates[0][1];
        
        // Define a largura do quadrado em metros (ex: 1500 metros)
        const boxSizeMeters = 1500;
        const R = 6378137.0;
        const dLat = boxSizeMeters / R;
        const dLon = boxSizeMeters / (R * Math.cos(lat));

        const boxCoords: number[] = [];
        const corners = [
          [lat - dLat, lon - dLon],
          [lat - dLat, lon + dLon],
          [lat + dLat, lon + dLon],
          [lat + dLat, lon - dLon],
          [lat - dLat, lon - dLon]
        ];

        for (const pt of corners) {
          try {
            if (viewMode === "3D") {
              const ecef = lla_to_ecef(pt[0], pt[1], 0.0);
              boxCoords.push(ecef[0], ecef[1], ecef[2]);
            } else {
              const flat = projection.project(pt[0], pt[1], 0.0);
              boxCoords.push(flat[0], flat[1], 0.0);
            }
          } catch {}
        }

        if (boxCoords.length >= 15) {
          gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer!);
          gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(boxCoords), gl.DYNAMIC_DRAW);
          // Verde claro para pontos/cidades
          gl.uniform4f(this.uColorLoc, 0.0, 0.9, 0.46, 0.8 * this.opacity);
          gl.drawArrays(gl.LINE_STRIP, 0, boxCoords.length / 3);
        }
        continue;
      }

      const coords: number[] = [];

      // Converte coordenadas geodésicas (radianos) para espaço de tela projetado
      for (const pt of feature.coordinates) {
        const lat = pt[0];
        const lon = pt[1];
        try {
          if (viewMode === "3D") {
            const ecef = lla_to_ecef(lat, lon, 0.0);
            coords.push(ecef[0], ecef[1], ecef[2]);
          } else {
            const flat = projection.project(lat, lon, 0.0);
            coords.push(flat[0], flat[1], 0.0);
          }
        } catch {
          // ignora falhas de limite de projeção
        }
      }

      if (coords.length < 6) continue;

      gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer!);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(coords), gl.DYNAMIC_DRAW);

      // Define estilo visual diferenciado com base no tipo da feição (Airway vs Boundary vs Fallback)
      if (feature.properties.type === "airway") {
        // Azul claro para rotas/aerovias operacionais
        gl.uniform4f(this.uColorLoc, 0.0, 0.69, 1.0, 0.4 * this.opacity);
        gl.drawArrays(gl.LINE_STRIP, 0, coords.length / 3);
      } else if (feature.properties.type === "boundary") {
        // Laranja/âmbar para limites de espaço aéreo (CTA/TMA)
        gl.uniform4f(this.uColorLoc, 1.0, 0.5, 0.0, 0.25 * this.opacity);
        gl.drawArrays(gl.LINE_STRIP, 0, coords.length / 3);
      } else {
        // Fallback para outras feições (verde esverdeado)
        gl.uniform4f(this.uColorLoc, 0.0, 0.9, 0.46, 0.5 * this.opacity);
        gl.drawArrays(gl.LINE_STRIP, 0, coords.length / 3);
      }
    }

    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
  }

  public renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void {
    // Linhas de fronteira estruturais não possuem sobreposição dinâmica CPU
  }

  public destroy(gl: WebGL2RenderingContext): void {
    if (this.vertexBuffer) {
      gl.deleteBuffer(this.vertexBuffer);
      this.vertexBuffer = null;
    }
    if (this.vao) {
      gl.deleteVertexArray(this.vao);
      this.vao = null;
    }
    if (this.program) {
      gl.deleteProgram(this.program);
      this.program = null;
    }
  }
}
export default VectorTileLayer;
