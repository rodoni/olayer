export interface SymbolUV {
  u0: number;
  v0: number;
  u1: number;
  v1: number;
  width: number;
  height: number;
}

export class TextureAtlasManager {
  private gl: WebGL2RenderingContext;
  private atlasCanvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private texture: WebGLTexture | null = null;
  
  // Shelf packing state
  private atlasSize = 512;
  private currentX = 0;
  private currentY = 0;
  private rowHeight = 0;
  private padding = 2;

  // Registered symbols map
  private uvs: Map<string, SymbolUV> = new Map();

  constructor(gl: WebGL2RenderingContext, atlasSize: number = 512) {
    this.gl = gl;
    this.atlasSize = atlasSize;

    // Create offscreen Canvas for rasterization
    this.atlasCanvas = document.createElement("canvas");
    this.atlasCanvas.width = this.atlasSize;
    this.atlasCanvas.height = this.atlasSize;
    
    const ctx = this.atlasCanvas.getContext("2d");
    if (!ctx) {
      throw new Error("Failed to get 2D context for offscreen Texture Atlas.");
    }
    this.ctx = ctx;

    // Clear atlas with transparency
    this.ctx.clearRect(0, 0, this.atlasSize, this.atlasSize);

    // Initialize WebGL Texture
    this.initWebGLTexture();
  }

  /**
   * Initializes the WebGL texture for the atlas.
   */
  private initWebGLTexture(): void {
    const gl = this.gl;
    this.texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    
    // Set wrapping and filtering parameters
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);

    // Allocate storage
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA,
      this.atlasSize,
      this.atlasSize,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      null
    );
  }

  /**
   * Registers a new custom symbol by rasterizing it and uploading it to the GPU atlas.
   */
  public registerSymbol(
    id: string,
    width: number,
    height: number,
    drawFn: (ctx: CanvasRenderingContext2D) => void
  ): SymbolUV {
    if (this.uvs.has(id)) {
      return this.uvs.get(id)!;
    }

    // Check shelf fit, wrap if needed
    if (this.currentX + width + this.padding > this.atlasSize) {
      this.currentX = 0;
      this.currentY += this.rowHeight + this.padding;
      this.rowHeight = 0;
    }

    if (this.currentY + height + this.padding > this.atlasSize) {
      throw new Error(`Texture Atlas is full! Cannot fit symbol "${id}" (${width}x${height}).`);
    }

    // Draw to local offscreen canvas region
    this.ctx.save();
    this.ctx.translate(this.currentX, this.currentY);
    this.ctx.clearRect(0, 0, width, height); // Clear target sub-rect
    drawFn(this.ctx);
    this.ctx.restore();

    // Calculate UV coordinates (0.0 to 1.0)
    const uv: SymbolUV = {
      u0: this.currentX / this.atlasSize,
      v0: this.currentY / this.atlasSize,
      u1: (this.currentX + width) / this.atlasSize,
      v1: (this.currentY + height) / this.atlasSize,
      width,
      height,
    };

    // Upload only the modified sub-rectangle to the GPU texture
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    
    // Grab modified region
    const subImageData = this.ctx.getImageData(this.currentX, this.currentY, width, height);
    
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      this.currentX,
      this.currentY,
      width,
      height,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      subImageData.data
    );

    this.uvs.set(id, uv);

    // Update shelf layout state
    this.currentX += width + this.padding;
    this.rowHeight = Math.max(this.rowHeight, height);

    return uv;
  }

  /**
   * Retrieves the UV data of a registered symbol.
   */
  public getSymbolUV(id: string): SymbolUV | undefined {
    return this.uvs.get(id);
  }

  /**
   * Returns the compiled WebGL texture.
   */
  public getTexture(): WebGLTexture | null {
    return this.texture;
  }

  /**
   * Deallocates the GPU WebGL texture.
   */
  public destroy(): void {
    if (this.texture) {
      this.gl.deleteTexture(this.texture);
      this.texture = null;
    }
    this.uvs.clear();
  }
}
export default TextureAtlasManager;
