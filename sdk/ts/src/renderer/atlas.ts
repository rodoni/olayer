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
   * Registers a procedural symbol resolved from the WASM Symbol Registry and SLD Style.
   */
  public registerWasmSymbol(
    id: string,
    registry: any,
    style: any
  ): SymbolUV {
    if (this.uvs.has(id)) {
      return this.uvs.get(id)!;
    }

    const resolved = registry.resolve_symbol(id, style);
    if (!resolved) {
      throw new Error(`Failed to resolve symbol: ${id}`);
    }

    const bbox = resolved.bbox;
    const width = Math.ceil(bbox[2] - bbox[0]) + 4;
    const height = Math.ceil(bbox[3] - bbox[1]) + 4;
    const anchorX = resolved.anchor[0];
    const anchorY = resolved.anchor[1];

    return this.registerSymbol(id, width, height, (ctx) => {
      ctx.translate(width / 2 - anchorX, height / 2 - anchorY);

      for (const prim of resolved.primitives) {
        if (prim.type === "Path") {
          const parts = prim.commands.split(/(?=[MmLlZz])/);
          ctx.beginPath();
          for (const part of parts) {
            const trimmed = part.trim();
            if (!trimmed) continue;
            const action = trimmed[0];
            const nums = (trimmed.slice(1).match(/-?\d+(?:\.\d+)?/g) || []).map(Number);
            if (action === "M" || action === "m") {
              ctx.moveTo(nums[0], nums[1]);
            } else if (action === "L" || action === "l") {
              ctx.lineTo(nums[0], nums[1]);
            } else if (action === "Z" || action === "z") {
              ctx.closePath();
            }
          }
          if (prim.fill) {
            ctx.fillStyle = `rgba(${prim.fill.r}, ${prim.fill.g}, ${prim.fill.b}, ${prim.fill.a / 255})`;
            ctx.fill();
          }
          if (prim.stroke) {
            ctx.strokeStyle = `rgba(${prim.stroke.color.r}, ${prim.stroke.color.g}, ${prim.stroke.color.b}, ${prim.stroke.color.a / 255})`;
            ctx.lineWidth = prim.stroke.width;
            if (prim.stroke.dash_array) {
              ctx.setLineDash(prim.stroke.dash_array);
            } else {
              ctx.setLineDash([]);
            }
            ctx.stroke();
          }
        } else if (prim.type === "Circle") {
          ctx.beginPath();
          ctx.arc(prim.cx, prim.cy, prim.r, 0, 2 * Math.PI);
          if (prim.fill) {
            ctx.fillStyle = `rgba(${prim.fill.r}, ${prim.fill.g}, ${prim.fill.b}, ${prim.fill.a / 255})`;
            ctx.fill();
          }
          if (prim.stroke) {
            ctx.strokeStyle = `rgba(${prim.stroke.color.r}, ${prim.stroke.color.g}, ${prim.stroke.color.b}, ${prim.stroke.color.a / 255})`;
            ctx.lineWidth = prim.stroke.width;
            if (prim.stroke.dash_array) {
              ctx.setLineDash(prim.stroke.dash_array);
            } else {
              ctx.setLineDash([]);
            }
            ctx.stroke();
          }
        } else if (prim.type === "Text") {
          ctx.fillStyle = `rgba(${prim.color.r}, ${prim.color.g}, ${prim.color.b}, ${prim.color.a / 255})`;
          ctx.font = `${prim.font_size}px sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(prim.content, prim.offset_x, prim.offset_y);
        }
      }
    });
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
