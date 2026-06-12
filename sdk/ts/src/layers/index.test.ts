import { describe, it, expect, vi } from "vitest";
import { Layer, LayerManager } from "./layer";

class MockLayer extends Layer {
  public staticRendered = false;
  public dynamicRendered = false;

  constructor(id: string) {
    super(id);
  }

  renderStatic(_gl: WebGL2RenderingContext, _viewProjMatrix: Float32Array): void {
    this.staticRendered = true;
  }

  renderDynamic(_ctx: CanvasRenderingContext2D, _currentTime: number): void {
    this.dynamicRendered = true;
  }
}

describe("LayerManager", () => {
  it("should add layers and prevent duplicates", () => {
    const manager = new LayerManager();
    const layer = new MockLayer("L1");
    manager.addLayer(layer);

    expect(manager.getLayers()).toHaveLength(1);
    expect(() => manager.addLayer(new MockLayer("L1"))).toThrow(/already exists/);
  });

  it("should remove layers", () => {
    const manager = new LayerManager();
    manager.addLayer(new MockLayer("A"));
    manager.addLayer(new MockLayer("B"));

    expect(manager.removeLayer("A")).toBe(true);
    expect(manager.getLayers().map((l) => l.id)).toEqual(["B"]);
    expect(manager.removeLayer("C")).toBe(false);
  });

  it("should reorder layers", () => {
    const manager = new LayerManager();
    manager.addLayer(new MockLayer("A"));
    manager.addLayer(new MockLayer("B"));
    manager.addLayer(new MockLayer("C"));

    manager.reorderLayer("B", 0);
    expect(manager.getLayers().map((l) => l.id)).toEqual(["B", "A", "C"]);
  });

  it("should throw on invalid reorder", () => {
    const manager = new LayerManager();
    manager.addLayer(new MockLayer("A"));

    expect(() => manager.reorderLayer("A", -1)).toThrow(/Invalid target index/);
    expect(() => manager.reorderLayer("Z", 0)).toThrow(/not found/);
  });

  it("should render only visible layers", () => {
    const manager = new LayerManager();
    const visible = new MockLayer("V");
    const hidden = new MockLayer("H");
    hidden.visible = false;

    manager.addLayer(visible);
    manager.addLayer(hidden);

    const gl = {} as WebGL2RenderingContext;
    const vp = new Float32Array(16);
    manager.renderStaticLayers(gl, vp);
    expect(visible.staticRendered).toBe(true);
    expect(hidden.staticRendered).toBe(false);

    const ctx = {} as CanvasRenderingContext2D;
    manager.renderDynamicLayers(ctx, 0);
    expect(visible.dynamicRendered).toBe(true);
    expect(hidden.dynamicRendered).toBe(false);
  });
});
