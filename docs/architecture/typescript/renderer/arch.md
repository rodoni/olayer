# SDK TS Component: Render Pipelines & Texture Atlas (`sdk/ts/src/renderer`)

The **Rendering** layer of the TypeScript SDK is responsible for projecting and drawing all mathematical and tactical data on screen (WebGL/GPU for terrain and background map; Canvas 2D/CPU for targets and labels). The **Texture Atlas** centralizes symbols into unified GPU buffers to maximize performance.

---

## 1. Responsibilities
* **GPU Render Pipeline (`WebGLRenderer`):**
  * Compile native WebGL/WebGPU shaders.
  * Upload data and geographic positioning of the cartographic grid.
  * Bind $4 \times 4$ Projection-Visualization matrices and draw three-dimensional meshes (such as 3D globe ellipsoids).
* **CPU Render Pipeline (`CPURenderer`):**
  * Draw targets, data blocks (data blocks), and heading vectors in real-time (60 FPS).
  * Project three-dimensional WGS84 coordinates to screen coordinates $(X,Y)$ using the transformation matrix of the Olayer Controller.
  * Execute **Anti-cluttering (anti-overlap)** algorithms to keep labels readable.
* **Texture Atlas Manager (`TextureAtlasManager`):**
  * Centralize all symbols (procedural, SVG, or PNG) in a single shared GPU texture.
  * Render symbols using dynamic billboards (flat plates oriented front-facing to the camera), ensuring readability in 3D.
  * Execute instanced rendering (`drawElementsInstanced`) to plot thousands of aircraft in a single draw call.

---

## 2. Implementation Details and Algorithms

### 2.1 Label Overlap Prevention (Anti-cluttering)
For operational air traffic control screens to maintain readability in high traffic density:
1. **Projection Fraction:** Converts interpolated 3D geodetic positions (WGS84 `LatLon`) of radar targets into screen coordinates $(X,Y)$.
2. **Label Bounding Box:** Calculates the data block rectangle (text size + speed + heading).
3. **Occupancy Mapping:** A static 2D tree or screen collision table (*Grid Collision Table*) stores rectangles of priority targets.
4. **Conflict Resolution (Alternating Offset):** If there is a conflict, the label tries to orbit around the symbol in predefined compass positions (Northeast -> Southeast -> Southwest -> Northwest). If none works, the secondary label is temporarily hidden.

---

## 3. Interfaces and Class Structure

### 3.1 WebGLRenderer
```typescript
export class WebGLRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram | null = null;
  private gridBuffer: WebGLBuffer | null = null;
  private gridLineCount = 0;

  constructor(gl: WebGL2RenderingContext);
  public rebuildGrid(projection: any, viewMode: string): void;
  public renderGrid(viewProjMatrix: Float32Array): void;
  public destroy(): void;
}
```

### 3.2 TextureAtlasManager
```typescript
export class TextureAtlasManager {
  private gl: WebGL2RenderingContext;
  private texture: WebGLTexture | null = null;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;

  constructor(gl: WebGL2RenderingContext);
  public registerSymbol(id: string, drawFn: (ctx: CanvasRenderingContext2D) => void, width: number, height: number): SymbolUV;
  public registerWasmSymbol(id: string, registry: any, style: any): SymbolUV;
  public registerImageSymbol(id: string, src: string | HTMLImageElement, width?: number, height?: number): Promise<SymbolUV>;
  public getSymbolUV(id: string): SymbolUV | undefined;
  public getTexture(): WebGLTexture | null;
  public destroy(): void;
}
```

---

## 4. Memory Management & Lifecycle (ADR-004)

Since WebAssembly operates in a virtual machine with isolated linear memory and without Garbage Collector (GC) monitoring, the TS SDK implements strict resource freeing:

```typescript
export class OlayerController {
  // ...
  
  /**
   * Explicit destructor that must be invoked by the Host application
   * when unmounting the map component.
   */
  public destroy(): void {
    this.stopLoop();
    this.dataManager.clearCache();
    
    // Explicit deallocation on the WebAssembly Heap
    this.terrainEngine.free();
    this.interpolator.free();
    this.projection.free();
  }
}
```
Explicit disposal prevents memory leaks that could compromise the long-term stability of the ATC console.
