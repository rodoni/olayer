# SDK TS Component: Layer Manager (`sdk/ts/src/layers`)

The **Layer Manager** manages the visual layer stack (Layer Stack), defining the drawing order (*z-index*), visibility, opacity, and repaint optimization through segregated rendering pipelines.

---

## 1. Responsibilities
* **Stack Composition:** Organize static layers (base map, borders, airways) and dynamic layers (weather radar, air traffic, distance rings).
* **Repaint Segregation (Optimization):**
  * **Static Painting (WebGL):** Evaluated only under physical camera interactions (Pan, Zoom, Rotation), saving results in static GPU buffers.
  * **Dynamic Painting (Canvas 2D):** Drawn in real-time in each frame (up to 60 FPS) on top of the static background, without cost of reprocessing the map background.
* **Visualization Lifecycle:** Encapsulate and trigger the rendering triggers of child layers in an ordered manner.

---

## 2. Interfaces and Class Structure

```typescript
/**
 * Abstract interface for all Olayer layers.
 */
export abstract class Layer {
  public id: string;
  public visible: boolean = true;
  public opacity: number = 1.0;

  constructor(id: string) {
    this.id = id;
  }

  /**
   * Called to render static elements residing on the GPU (WebGL/WebGPU).
   * Only triggered when the camera changes or the map is updated.
   */
  public abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  /**
   * Called to draw quick dynamic overlays using the Canvas 2D context.
   * Triggered in every active tactical frame (up to 60 FPS).
   */
  public abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void;
}

/**
 * Coordinator of the Olayer visual layer stack.
 */
export class LayerManager {
  private layers: Layer[] = [];

  /**
   * Inserts a new layer into the visualization stack.
   */
  public addLayer(layer: Layer): void;

  /**
   * Removes a layer by ID.
   */
  public removeLayer(id: string): boolean;

  /**
   * Reorders the relative positioning of a layer in the stack.
   */
  public reorderLayer(id: string, newIndex: number): void;

  /**
   * Returns all loaded layers.
   */
  public getLayers(): Layer[];

  /**
   * Iterates and renders visible WebGL static layers.
   */
  public renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  /**
   * Iterates and renders visible Canvas 2D dynamic layers.
   */
  public renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void;
}
```
