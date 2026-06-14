import { Layer } from "./layer";
import { RasterTileSource } from "../providers/raster";
import { WasmProjection, lla_to_ecef } from "olayer-wasm";

/**
 * Capa de renderizado para tiles ráster baseados em imagem (como OSM ou WMTS).
 * Desenha os blocos do mapa projetados dinamicamente na GPU usando WebGL2.
 */
export class TileLayer extends Layer {
  private rasterSource: RasterTileSource;
  private program: WebGLProgram | null = null;
  private indexBuffer: WebGLBuffer | null = null;
  private indexCount = 0;

  // Cache de geometria WebGL para cada bloco (VAO e Vertex Buffer)
  private tileGeometries: Map<string, { vao: WebGLVertexArrayObject; vertexBuffer: WebGLBuffer }> = new Map();
  private lastProjection: WasmProjection | null = null;
  private lastViewMode = "";

  // Resolvedores de Uniforms
  private uViewProjMatrixLoc: WebGLUniformLocation | null = null;
  private uTextureLoc: WebGLUniformLocation | null = null;
  private uOpacityLoc: WebGLUniformLocation | null = null;

  constructor(id: string, rasterSource: RasterTileSource) {
    super(id);
    this.rasterSource = rasterSource;
  }

  /**
   * Inicializa shaders e buffers WebGL2.
   */
  private initWebGL(gl: WebGL2RenderingContext): void {
    if (this.program) return;

    const vsSource = `#version 300 es
      in vec3 a_position;
      in vec2 a_texCoord;
      uniform mat4 u_viewProjMatrix;
      out vec2 v_texCoord;
      void main() {
        v_texCoord = a_texCoord;
        gl_Position = u_viewProjMatrix * vec4(a_position, 1.0);
      }
    `;

    const fsSource = `#version 300 es
      precision mediump float;
      in vec2 v_texCoord;
      uniform sampler2D u_texture;
      uniform float u_opacity;
      out vec4 fragColor;
      void main() {
        vec4 texColor = texture(u_texture, v_texCoord);
        // Aplica opacidade e modula levemente para manter aparência escurecida de radar operacional
        fragColor = vec4(texColor.rgb * 0.8, texColor.a * u_opacity);
      }
    `;

    // Compilação de Shaders
    const vs = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vs, vsSource);
    gl.compileShader(vs);
    if (!gl.getShaderParameter(vs, gl.COMPILE_STATUS)) {
      throw new Error(`VS Compilation error: ${gl.getShaderInfoLog(vs)}`);
    }

    const fs = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fs, fsSource);
    gl.compileShader(fs);
    if (!gl.getShaderParameter(fs, gl.COMPILE_STATUS)) {
      throw new Error(`FS Compilation error: ${gl.getShaderInfoLog(fs)}`);
    }

    this.program = gl.createProgram()!;
    gl.attachShader(this.program, vs);
    gl.attachShader(this.program, fs);
    gl.linkProgram(this.program);

    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) {
      throw new Error(`Shader Link error: ${gl.getProgramInfoLog(this.program)}`);
    }

    this.uViewProjMatrixLoc = gl.getUniformLocation(this.program, "u_viewProjMatrix");
    this.uTextureLoc = gl.getUniformLocation(this.program, "u_texture");
    this.uOpacityLoc = gl.getUniformLocation(this.program, "u_opacity");

    this.indexBuffer = gl.createBuffer();

    // Calcula os índices de triangulação estáticos uma vez
    const subdivision = 4;
    const indexData: number[] = [];
    for (let r = 0; r < subdivision; r++) {
      for (let c = 0; c < subdivision; c++) {
        const i0 = r * (subdivision + 1) + c;
        const i1 = i0 + 1;
        const i2 = (r + 1) * (subdivision + 1) + c;
        const i3 = i2 + 1;

        // Triângulo 1 (i0, i1, i2)
        indexData.push(i0, i1, i2);
        // Triângulo 2 (i1, i3, i2)
        indexData.push(i1, i3, i2);
      }
    }
    const indices = new Uint16Array(indexData);
    this.indexCount = indices.length;

    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indexBuffer);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indices, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, null);
  }

  /**
   * Determina o nível de zoom dos tiles baseado no nível de escala da câmera.
   */
  private getTileZoom(zoom: number): number {
    // Escala logarítmica para nível de zoom de tile OSM (tipicamente entre 1 e 18)
    const z = Math.floor(Math.log2(zoom) + 11.5);
    return Math.max(1, Math.min(18, z));
  }

  /**
   * Converte longitude em radianos para coordenada X do Grid.
   */
  private lonToTileX(lonRad: number, z: number): number {
    const lonDeg = lonRad * (180 / Math.PI);
    return Math.floor(((lonDeg + 180) / 360) * Math.pow(2, z));
  }

  /**
   * Converte latitude em radianos para coordenada Y do Grid.
   */
  private latToTileY(latRad: number, z: number): number {
    const latDeg = latRad * (180 / Math.PI);
    const latRadClamped = Math.max(-85.0511, Math.min(85.0511, latDeg)) * (Math.PI / 180);
    return Math.floor(
      ((1 - Math.log(Math.tan(latRadClamped) + 1 / Math.cos(latRadClamped)) / Math.PI) / 2) * Math.pow(2, z)
    );
  }

  /**
   * Converte X do Grid de volta para Longitude em graus.
   */
  private tileXToLon(x: number, z: number): number {
    return (x / Math.pow(2, z)) * 360 - 180;
  }

  /**
   * Converte Y do Grid de volta para Latitude em graus.
   */
  private tileYToLat(y: number, z: number): number {
    const n = Math.PI - (2 * Math.PI * y) / Math.pow(2, z);
    return (180 / Math.PI) * Math.atan(0.5 * (Math.exp(n) - Math.exp(-n)));
  }

  /**
   * Calculates the bounding box of tiles currently visible in the camera viewport.
   */
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

  /**
   * Limpa o cache de geometrias e remove os buffers e VAOs correspondentes da GPU.
   */
  private clearGeometryCache(gl: WebGL2RenderingContext): void {
    for (const geom of this.tileGeometries.values()) {
      gl.deleteVertexArray(geom.vao);
      gl.deleteBuffer(geom.vertexBuffer);
    }
    this.tileGeometries.clear();
  }

  /**
   * Implementação da renderização estática na GPU.
   */
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

    // Detecta mudança na projeção ativa ou modo de visualização para invalidar o cache
    if (this.lastProjection !== projection || this.lastViewMode !== viewMode) {
      this.clearGeometryCache(gl);
      this.lastProjection = projection;
      this.lastViewMode = viewMode;
    }

    const z = this.getTileZoom(zoom);
    const bounds = this.getVisibleTileBounds(controller, z);

    // Limpeza seletiva do cache para evitar vazamento de recursos sem destruir tiles visíveis
    if (this.tileGeometries.size > 300) {
      const keysToDelete: string[] = [];
      for (const [key, geom] of this.tileGeometries.entries()) {
        const [tzStr, txStr, tyStr] = key.split("/");
        const tz = parseInt(tzStr, 10);
        const tx = parseInt(txStr, 10);
        const ty = parseInt(tyStr, 10);

        if (
          tz !== z ||
          tx < bounds.minX ||
          tx > bounds.maxX ||
          ty < bounds.minY ||
          ty > bounds.maxY
        ) {
          gl.deleteVertexArray(geom.vao);
          gl.deleteBuffer(geom.vertexBuffer);
          keysToDelete.push(key);
        }
      }
      for (const key of keysToDelete) {
        this.tileGeometries.delete(key);
      }
    }

    if ((this as any)._lastLoggedBounds !== JSON.stringify(bounds)) {
      (this as any)._lastLoggedBounds = JSON.stringify(bounds);
      console.log(`[TileLayer] Zoom: ${z}, bounds: minX=${bounds.minX}, maxX=${bounds.maxX}, minY=${bounds.minY}, maxY=${bounds.maxY}`);
    }

    // Recompila os buffers de geometria se a câmera ou zoom se alterou fisicamente
    const tilesToDraw: { x: number; y: number; texture: WebGLTexture }[] = [];

    for (let ty = bounds.minY; ty <= bounds.maxY; ty++) {
      for (let tx = bounds.minX; tx <= bounds.maxX; tx++) {
        // Dispara o carregamento assíncrono (se não estiver no cache)
        this.rasterSource.loadTile(tx, ty, z).catch(() => {});
        
        const texture = this.rasterSource.getTileTexture(tx, ty, z);
        if (texture) {
          tilesToDraw.push({ x: tx, y: ty, texture });
        }
      }
    }

    if (tilesToDraw.length === 0) return;

    // Configura o pipeline WebGL
    gl.useProgram(this.program!);

    gl.uniformMatrix4fv(this.uViewProjMatrixLoc, false, viewProjMatrix);
    gl.uniform1f(this.uOpacityLoc, this.opacity);
    gl.uniform1i(this.uTextureLoc, 0);

    gl.activeTexture(gl.TEXTURE0);

    // Ativa transparência para mesclagem de bordas
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

    const subdivision = 4;
    const vertexCount = (subdivision + 1) * (subdivision + 1);
    const attribsPerVertex = 5; // X, Y, Z, U, V

    // Renderiza cada tile ativo
    for (const tile of tilesToDraw) {
      const key = `${z}/${tile.x}/${tile.y}`;
      let tileGeom = this.tileGeometries.get(key);

      if (!tileGeom) {
        console.log(`[TileLayer] Generating geometry for tile key: ${key}`);
        const vertices = new Float32Array(vertexCount * attribsPerVertex);
        
        // Calcula os vértices projetados do tile subdividido
        let offset = 0;
        for (let r = 0; r <= subdivision; r++) {
          const v = r / subdivision; // 0.0 to 1.0 (vertical tile coord)
          const tileY = tile.y + v;
          const latDeg = this.tileYToLat(tileY, z);
          const latRad = latDeg * (Math.PI / 180);

          for (let c = 0; c <= subdivision; c++) {
            const u = c / subdivision; // 0.0 to 1.0 (horizontal tile coord)
            const tileX = tile.x + u;
            const lonDeg = this.tileXToLon(tileX, z);
            const lonRad = lonDeg * (Math.PI / 180);

            let px = 0, py = 0, pz = 0;

            try {
              if (viewMode === "3D") {
                const ecef = lla_to_ecef(latRad, lonRad, 0.0);
                px = ecef[0];
                py = ecef[1];
                pz = ecef[2];
              } else {
                const flatPos = projection.project(latRad, lonRad, 0.0);
                px = flatPos[0];
                py = flatPos[1];
                pz = 0.0;
              }
            } catch {
              // Ignora erros de projeção de bordas de mapa
            }

            // Atributos: X, Y, Z
            vertices[offset] = px;
            vertices[offset + 1] = py;
            vertices[offset + 2] = pz;
            // Atributos: U, V (inverte V do WebGL)
            vertices[offset + 3] = u;
            vertices[offset + 4] = v;

            offset += attribsPerVertex;
          }
        }

        const vao = gl.createVertexArray()!;
        const vertexBuffer = gl.createBuffer()!;

        gl.bindVertexArray(vao);
        gl.bindBuffer(gl.ARRAY_BUFFER, vertexBuffer);
        gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);

        const aPosLoc = gl.getAttribLocation(this.program!, "a_position");
        gl.enableVertexAttribArray(aPosLoc);
        gl.vertexAttribPointer(aPosLoc, 3, gl.FLOAT, false, 5 * 4, 0); // (X,Y,Z)

        const aTexLoc = gl.getAttribLocation(this.program!, "a_texCoord");
        gl.enableVertexAttribArray(aTexLoc);
        gl.vertexAttribPointer(aTexLoc, 2, gl.FLOAT, false, 5 * 4, 3 * 4); // (U,V)

        gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indexBuffer!);
        gl.bindVertexArray(null);

        tileGeom = { vao, vertexBuffer };
        this.tileGeometries.set(key, tileGeom);
      }

      gl.bindTexture(gl.TEXTURE_2D, tile.texture);
      gl.bindVertexArray(tileGeom.vao);
      
      // Desenha o tile triangulado usando os elementos do VAO
      gl.drawElements(gl.TRIANGLES, this.indexCount, gl.UNSIGNED_SHORT, 0);
    }

    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
  }

  public renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void {
    // Tiles ráster de fundo não possuem elementos interativos 2D desenhados na CPU
  }

  /**
   * Liberação de recursos WebGL do layer.
   */
  public destroy(gl: WebGL2RenderingContext): void {
    this.clearGeometryCache(gl);
    if (this.indexBuffer) {
      gl.deleteBuffer(this.indexBuffer);
      this.indexBuffer = null;
    }
    if (this.program) {
      gl.deleteProgram(this.program);
      this.program = null;
    }
  }
}
