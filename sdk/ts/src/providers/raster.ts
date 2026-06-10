import { MapDataSource } from "./datasource";

/**
 * Provedor de tiles de imagens rasterizadas (WMTS / OpenStreetMap / XYZ).
 * Gerencia o download assíncrono de imagens e seu upload/cacheamento como texturas WebGL.
 */
export class RasterTileSource implements MapDataSource {
  public readonly id: string = "wmts_raster";
  private gl: WebGL2RenderingContext;
  private tileCache: Map<string, WebGLTexture> = new Map(); // Key format: "z/x/y"
  private urlResolver: string | ((x: number, y: number, z: number) => string);
  private maxTiles: number;

  constructor(
    gl: WebGL2RenderingContext,
    urlResolver: string | ((x: number, y: number, z: number) => string) = "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
    maxTiles: number = 100
  ) {
    this.gl = gl;
    this.urlResolver = urlResolver;
    this.maxTiles = maxTiles;
  }

  /**
   * Baixa uma imagem de tile ráster via HTTP e cria/carrega a textura WebGL correspondente.
   */
  public async loadTile(x: number, y: number, z: number): Promise<void> {
    const key = `${z}/${x}/${y}`;

    // Se já está no cache, atualiza ordem LRU (First-In, First-Out em chaves do Map JS)
    if (this.tileCache.has(key)) {
      const texture = this.tileCache.get(key)!;
      this.tileCache.delete(key);
      this.tileCache.set(key, texture);
      return;
    }

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

      // Evicção LRU se o cache estourar
      if (this.tileCache.size >= this.maxTiles) {
        const oldestKey = this.tileCache.keys().next().value;
        if (oldestKey) {
          const oldestTexture = this.tileCache.get(oldestKey)!;
          gl.deleteTexture(oldestTexture);
          this.tileCache.delete(oldestKey);
        }
      }

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
    const texture = this.tileCache.get(key);
    if (texture) {
      this.gl.deleteTexture(texture);
      this.tileCache.delete(key);
    }
  }

  /**
   * Limpa cache e desaloca todas as texturas WebGL da GPU.
   */
  public clearCache(): void {
    const gl = this.gl;
    for (const texture of this.tileCache.values()) {
      gl.deleteTexture(texture);
    }
    this.tileCache.clear();
  }

  /**
   * Retorna a quantidade de texturas ativas em cache.
   */
  public getCacheSize(): number {
    return this.tileCache.size;
  }
}
