export abstract class Layer {
  public id: string;
  public visible: boolean = true;
  public opacity: number = 1.0;

  constructor(id: string) {
    this.id = id;
  }

  /**
   * Called to render static elements on the WebGL/GPU pipeline.
   * This is only re-evaluated when the camera changes or layers are updated.
   */
  public abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  /**
   * Called to render dynamic elements on the CPU/Canvas 2D context.
   * This is evaluated on every frame at up to 60 FPS.
   */
  public abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void;
}

export class LayerManager {
  private layers: Layer[] = [];

  /**
   * Adds a new layer to the stack.
   */
  public addLayer(layer: Layer): void {
    if (this.layers.some(l => l.id === layer.id)) {
      throw new Error(`Layer with id "${layer.id}" already exists.`);
    }
    this.layers.push(layer);
  }

  /**
   * Removes a layer from the stack by its identifier.
   */
  public removeLayer(id: string): boolean {
    const initialLength = this.layers.length;
    this.layers = this.layers.filter(l => l.id !== id);
    return this.layers.length < initialLength;
  }

  /**
   * Reorders a layer to a specific index in the stack.
   */
  public reorderLayer(id: string, newIndex: number): void {
    const currentIndex = this.layers.findIndex(l => l.id === id);
    if (currentIndex === -1) {
      throw new Error(`Layer with id "${id}" not found.`);
    }
    if (newIndex < 0 || newIndex >= this.layers.length) {
      throw new Error(`Invalid target index: ${newIndex}`);
    }

    const [layer] = this.layers.splice(currentIndex, 1);
    this.layers.splice(newIndex, 0, layer);
  }

  /**
   * Returns a copy of the current layers stack.
   */
  public getLayers(): Layer[] {
    return [...this.layers];
  }

  /**
   * Renders all visible static layers onto WebGL.
   */
  public renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    for (const layer of this.layers) {
      if (layer.visible) {
        layer.renderStatic(gl, viewProjMatrix);
      }
    }
  }

  /**
   * Renders all visible dynamic layers onto the Canvas 2D context.
   */
  public renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void {
    for (const layer of this.layers) {
      if (layer.visible) {
        layer.renderDynamic(ctx, currentTime);
      }
    }
  }
}
