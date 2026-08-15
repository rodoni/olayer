import { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";
import { TileCache, throwIfAborted } from "./tile_cache";

/**
 * Provedor de tiles de imagens rasterizadas (WMTS / OpenStreetMap / XYZ).
 * Gerencia o download assíncrono de imagens e seu upload/cacheamento como texturas WebGL.
 */
export class RasterTileSource implements MapDataSource {
  public readonly id: string = "wmts_raster";
  private gl: WebGL2RenderingContext;
  private tileCache: TileCache<WebGLTexture>;
  private readonly pending = new Map<string, Promise<void>>();
  private readonly invalidated = new Set<string>();
  private generation = 0;
  private urlResolver: string | ((x: number, y: number, z: number) => string);
  private maxTiles: number;

  constructor(
    gl: WebGL2RenderingContext,
    urlResolver: string | ((x: number, y: number, z: number) => string) = "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
    maxTiles: number = 100
  ) {
    this.gl = gl;
    this.urlResolver = urlResolver;
    if (!Number.isInteger(maxTiles) || maxTiles <= 0) {
      throw new Error("Raster tile cache capacity must be a positive integer");
    }
    this.maxTiles = maxTiles;
    this.tileCache = new TileCache(maxTiles, (texture) => this.gl.deleteTexture(texture));
  }

  /**
   * Baixa uma imagem de tile ráster via HTTP e cria/carrega a textura WebGL correspondente.
   */
  public async loadTile(x: number, y: number, z: number, options: TileRequestOptions = {}): Promise<void> {
    const key = `${z}/${x}/${y}`;

    if (this.tileCache.get(key)) return;
    const existing = this.pending.get(key);
    if (existing) return existing;
    this.invalidated.delete(key);
    const requestGeneration = this.generation;
    throwIfAborted(options.signal);
    const request = this.loadTileInternal(key, x, y, z, requestGeneration, options);
    this.pending.set(key, request);
    try { await request; } finally { this.pending.delete(key); }
  }

  private async loadTileInternal(
    key: string,
    x: number,
    y: number,
    z: number,
    requestGeneration: number,
    options: TileRequestOptions,
  ): Promise<void> {

    // Resolve a URL final
    let url = "";
    if (typeof this.urlResolver === "function") {
      url = this.urlResolver(x, y, z);
    } else {
      url = this.urlResolver
        .replace("{x}", x.toString())
        .replace("{y}", y.toString())
        .replace("{z}", z.toString());
    }

    try {
      const img = new Image();
      img.crossOrigin = "anonymous";

      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = (e) => reject(new Error(`Failed to load image at URL: ${url}`));
        img.src = url;
      });
      throwIfAborted(options.signal);

      // Cria a textura WebGL
      const gl = this.gl;
      const texture = gl.createTexture();
      if (!texture) {
        throw new Error("Failed to create WebGL texture.");
      }

      gl.bindTexture(gl.TEXTURE_2D, texture);

      // Define parâmetros de interpolação e repetição do tile
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);

      // Envia os pixels da imagem para o buffer de textura da GPU
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);

      if (requestGeneration !== this.generation || this.invalidated.has(key)) {
        gl.deleteTexture(texture);
        return;
      }
      throwIfAborted(options.signal);
      this.tileCache.set(key, texture);
    } catch (err) {
      console.error(`Failed to load raster tile [z:${z}, x:${x}, y:${y}] from ${url}:`, err);
      throw err;
    }
  }

  /**
   * Obtém a textura WebGL já carregada para um tile específico.
   * Retorna null se não estiver carregada ainda.
   */
  public getTileTexture(x: number, y: number, z: number): WebGLTexture | null {
    const key = `${z}/${x}/${y}`;
    return this.tileCache.get(key) || null;
  }

  /**
   * Descarrega o tile da GPU e o remove do cache.
   */
  public unloadTile(x: number, y: number, z: number): void {
    const key = `${z}/${x}/${y}`;
    this.invalidated.add(key);
    this.tileCache.delete(key);
  }

  /**
   * Limpa cache e desaloca todas as texturas WebGL da GPU.
   */
  public clearCache(): void {
    this.generation++;
    this.invalidated.clear();
    this.tileCache.clear();
  }

  /**
   * Retorna a quantidade de texturas ativas em cache.
   */
  public getCacheSize(): number {
    return this.tileCache.size();
  }

  public getCacheStats(): TileCacheStats {
    return this.tileCache.stats();
  }
}
