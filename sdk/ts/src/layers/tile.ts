import { Layer } from "./layer";
import type { LayerRenderContext } from "./layer";
import type { OlayerController } from "../controller";
import { RasterTileSource } from "../providers/raster";
import { WasmProjection, lla_to_ecef } from "olayer-wasm";

const TILE_SUBDIVISION = 16;

export interface GlobeTileCoordinate {
  readonly x: number;
  readonly y: number;
}

/**
 * Selects XYZ tiles whose geographic cells intersect the camera-facing hemisphere.
 *
 * @param centerLatRad - Camera center latitude in radians.
 * @param centerLonRad - Camera center longitude in radians.
 * @param zoom - Integer XYZ zoom level from 1 through 5.
 * @returns Tile coordinates intersecting the camera-facing hemisphere.
 * @throws {RangeError} If the center is non-finite or zoom is outside 1..=5.
 */
export function getGlobeTileCoordinates(
  centerLatRad: number,
  centerLonRad: number,
  zoom: number,
): readonly GlobeTileCoordinate[] {
  if (!Number.isFinite(centerLatRad) || !Number.isFinite(centerLonRad)) {
    throw new RangeError("Globe tile center must be finite");
  }
  if (!Number.isInteger(zoom) || zoom < 1 || zoom > 5) {
    throw new RangeError("Globe tile zoom must be an integer from 1 to 5");
  }

  const tileCount = 2 ** zoom;
  const halfTileLon = Math.PI / tileCount;
  const visible: GlobeTileCoordinate[] = [];
  const sinCenterLat = Math.sin(centerLatRad);
  const cosCenterLat = Math.cos(centerLatRad);

  for (let y = 0; y < tileCount; y++) {
    const latTop = Math.atan(Math.sinh(Math.PI - (2 * Math.PI * y) / tileCount));
    const latBottom = Math.atan(Math.sinh(Math.PI - (2 * Math.PI * (y + 1)) / tileCount));
    const lat = (latTop + latBottom) / 2;
    const sinLat = Math.sin(lat);
    const cosLat = Math.cos(lat);
    let minCornerDot = 1;
    for (const cornerLat of [latTop, latBottom]) {
      for (const cornerLonOffset of [-halfTileLon, halfTileLon]) {
        const cornerDot =
          Math.sin(lat) * Math.sin(cornerLat) +
          Math.cos(lat) * Math.cos(cornerLat) * Math.cos(cornerLonOffset);
        minCornerDot = Math.min(minCornerDot, cornerDot);
      }
    }
    const tileAngularRadius = Math.acos(Math.max(-1, Math.min(1, minCornerDot)));
    const edgeMargin = Math.sin(tileAngularRadius);

    for (let x = 0; x < tileCount; x++) {
      const lon = ((x + 0.5) / tileCount) * 2 * Math.PI - Math.PI;
      const angularDot =
        sinCenterLat * sinLat +
        cosCenterLat * cosLat * Math.cos(lon - centerLonRad);
      if (angularDot >= -edgeMargin) visible.push({ x, y });
    }
  }

  return visible;
}

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
  private lastProjectionVersion = 0;
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
    const subdivision = TILE_SUBDIVISION;
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

  private getGlobeTileZoom(zoom: number): number {
    return Math.max(2, Math.min(3, Math.floor(Math.log2(Math.max(zoom, 0.5)) + 3)));
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
    controller: OlayerController,
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
      const [projectedX, projectedY] = xy;
      if (projectedX === undefined || projectedY === undefined) return defaultBounds;
      cx = projectedX;
      cy = projectedY;
    } catch {
      return defaultBounds;
    }

    const aspect = canvasWidth / canvasHeight;
    const w = viewportBaseMeters / zoom;
    const h = w / aspect;

    const metersPerPixelX = w / canvasWidth;
    const metersPerPixelY = h / canvasHeight;

    const corners: readonly (readonly [number, number])[] = [
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
  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void {
    if (!this.visible || this.opacity <= 0.01) return;

    this.initWebGL(gl);

    const controller = context?.controller;
    if (!controller) return;

    const camera = controller.getCameraState();
    const zoom = camera.zoom;
    const viewMode = controller.getViewMode();
    const projection = controller.projection;

    camera.free();

    // Detecta mudança na projeção ativa, sua versão (centro alterado) ou modo de visualização para invalidar o cache
    const projVersion = projection.version();
    if (
      this.lastProjection !== projection ||
      this.lastProjectionVersion !== projVersion ||
      this.lastViewMode !== viewMode
    ) {
      this.clearGeometryCache(gl);
      this.lastProjection = projection;
      this.lastProjectionVersion = projVersion;
      this.lastViewMode = viewMode;
    }

    const z = viewMode === "3D" ? this.getGlobeTileZoom(zoom) : this.getTileZoom(zoom);
    const globeTiles = viewMode === "3D"
      ? getGlobeTileCoordinates(controller.getCenterLat(), controller.getCenterLon(), z)
      : null;
    const globeTileKeys = globeTiles === null
      ? null
      : new Set(globeTiles.map(({ x, y }) => `${z}/${x}/${y}`));
    const bounds = globeTiles === null ? this.getVisibleTileBounds(controller, z) : null;
    controller.logger.debug("Tile coverage calculated", {
      zoom: z,
      viewMode,
      tileCount: globeTiles?.length,
      bounds,
    });

    // Limpeza seletiva do cache para evitar vazamento de recursos sem destruir tiles visíveis
    if (this.tileGeometries.size > 300) {
      const keysToDelete: string[] = [];
      for (const [key, geom] of this.tileGeometries.entries()) {
        const [tzStr, txStr, tyStr] = key.split("/");
        if (tzStr === undefined || txStr === undefined || tyStr === undefined) continue;
        const tz = parseInt(tzStr, 10);
        const tx = parseInt(txStr, 10);
        const ty = parseInt(tyStr, 10);

        const outsideCurrentCoverage = globeTiles !== null
          ? !globeTileKeys?.has(key)
          : bounds !== null && (
            tz !== z ||
            tx < bounds.minX ||
            tx > bounds.maxX ||
            ty < bounds.minY ||
            ty > bounds.maxY
          );
        if (outsideCurrentCoverage) {
          gl.deleteVertexArray(geom.vao);
          gl.deleteBuffer(geom.vertexBuffer);
          keysToDelete.push(key);
        }
      }
      for (const key of keysToDelete) {
        this.tileGeometries.delete(key);
      }
    }


    // Recompila os buffers de geometria se a câmera ou zoom se alterou fisicamente
    const tilesToDraw: { x: number; y: number; texture: WebGLTexture }[] = [];

    const tileCoordinates = globeTiles ?? (() => {
      const coordinates: GlobeTileCoordinate[] = [];
      if (bounds) {
        for (let ty = bounds.minY; ty <= bounds.maxY; ty++) {
          for (let tx = bounds.minX; tx <= bounds.maxX; tx++) coordinates.push({ x: tx, y: ty });
        }
      }
      return coordinates;
    })();

    for (const { x: tx, y: ty } of tileCoordinates) {
      void this.rasterSource.loadTile(tx, ty, z).catch((error: unknown) => {
        controller.logger.error("Raster tile loading failed", { tx, ty, z, error });
      });

      const texture = this.rasterSource.getTileTexture(tx, ty, z);
      if (texture) {
        tilesToDraw.push({ x: tx, y: ty, texture });
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
    if (viewMode === "3D") {
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.depthMask(true);
    } else {
      gl.disable(gl.DEPTH_TEST);
    }

    const subdivision = TILE_SUBDIVISION;
    const vertexCount = (subdivision + 1) * (subdivision + 1);
    const attribsPerVertex = 5; // X, Y, Z, U, V

    // Renderiza cada tile ativo
    for (const tile of tilesToDraw) {
      const key = `${z}/${tile.x}/${tile.y}`;
      let tileGeom = this.tileGeometries.get(key);

      if (!tileGeom) {
        controller.logger.debug("Generating tile geometry", { key });
        const vertices = new Float32Array(vertexCount * attribsPerVertex);
        
        // Calcula os vértices projetados do tile subdividido
        let offset = 0;
        for (let r = 0; r <= subdivision; r++) {
          const v = r / subdivision; // 0.0 to 1.0 (vertical tile coord)
          const tileY = tile.y + v;
          // Web Mercator ends at about 85 degrees; pinch its boundary row to
          // each geographic pole so the map does not leave a square cap hole.
          const latDeg = viewMode === "3D" && tile.y === 0 && r === 0
            ? 90
            : viewMode === "3D" && tile.y === 2 ** z - 1 && r === subdivision
              ? -90
              : this.tileYToLat(tileY, z);
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
                 const [x, y, zValue] = ecef;
                 if (x === undefined || y === undefined || zValue === undefined) continue;
                 px = x;
                 py = y;
                 pz = zValue;
              } else {
                const flatPos = projection.project(latRad, lonRad, 0.0);
                 const [x, y] = flatPos;
                 if (x === undefined || y === undefined) continue;
                 px = x;
                 py = y;
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
  public override destroy(gl: WebGL2RenderingContext): void {
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
