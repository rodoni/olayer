import { describe, it, expect, vi } from "vitest";
import { CPURenderer } from "./cpu";

function createMockCtx(): CanvasRenderingContext2D {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    translate: vi.fn(),
    fillRect: vi.fn(),
    strokeRect: vi.fn(),
    fillText: vi.fn(),
    beginPath: vi.fn(),
    arc: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    setLineDash: vi.fn(),
    drawImage: vi.fn(),
    measureText: vi.fn(() => ({ width: 50 })),
    font: "",
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    globalAlpha: 1,
  } as unknown as CanvasRenderingContext2D;
}

function createMockProjection(): any {
  return {
    project: vi.fn((lat: number, lon: number, _height: number) => [lat * 1000, lon * 1000]),
  };
}

describe("CPURenderer", () => {
  it("should begin frame and clear occupied rects", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    renderer.beginFrame();
    // No direct assertion, but beginFrame should not throw
  });

  it("should project to screen in 2D mode", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();

    const pos = renderer.projectToScreen(
      proj, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600, "2D"
    );
    assert(pos !== null);
    expect(pos.x).toBe(400);
    expect(pos.y).toBe(300);
  });

  it("should project to screen with rotation", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();

    const pos = renderer.projectToScreen(
      proj, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, Math.PI / 2, 100000.0, 800, 600, "2D"
    );
    assert(pos !== null);
    // 90° rotation swaps x/y for center point
    expect(pos.x).toBe(400);
    expect(pos.y).toBe(300);
  });

  it("should return null for projection failure", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();
    proj.project = vi.fn(() => { throw new Error("fail"); });

    const pos = renderer.projectToScreen(
      proj, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600, "2D"
    );
    expect(pos).toBeNull();
  });

  it("should draw a target without atlas", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();
    const target = {
      id: "T1",
      position: { lat: 0.0, lon: 0.0, height: 1000.0 },
      heading_rad: 0.0,
    };

    renderer.beginFrame();
    renderer.drawTarget(
      target,
      { x: 400, y: 300 },
      proj, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600,
      100.0, null, undefined, "2D"
    );

    expect(ctx.save).toHaveBeenCalled();
    expect(ctx.beginPath).toHaveBeenCalled();
    expect(ctx.arc).toHaveBeenCalled();
    expect(ctx.fill).toHaveBeenCalled();
  });

  it("should draw velocity vector for moving target", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();
    const target = {
      id: "T2",
      position: { lat: 0.0, lon: 0.0, height: 1000.0 },
      heading_rad: 0.0,
    };

    renderer.beginFrame();
    renderer.drawTarget(
      target,
      { x: 400, y: 300 },
      proj, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600,
      200.0, null, undefined, "2D"
    );

    expect(ctx.setLineDash).toHaveBeenCalledWith([2, 2]);
    expect(ctx.beginPath).toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it("should draw data block with anti-cluttering", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();
    const target = {
      id: "T3",
      position: { lat: 0.0, lon: 0.0, height: 1000.0 },
      heading_rad: 0.0,
    };

    renderer.beginFrame();
    renderer.drawTarget(
      target,
      { x: 400, y: 300 },
      proj, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600,
      100.0, null, undefined, "2D"
    );

    expect(ctx.measureText).toHaveBeenCalled();
    expect(ctx.fillText).toHaveBeenCalled();
  });

  it("should use atlas texture when available", () => {
    const ctx = createMockCtx();
    const renderer = new CPURenderer(ctx);
    const proj = createMockProjection();
    const target = {
      id: "T4",
      position: { lat: 0.0, lon: 0.0, height: 1000.0 },
      heading_rad: 0.0,
    };
    const atlas = document.createElement("canvas");
    atlas.width = 512;
    atlas.height = 512;
    const uv = { u0: 0, v0: 0, u1: 0.1, v1: 0.1, width: 32, height: 32 };

    renderer.beginFrame();
    renderer.drawTarget(
      target,
      { x: 400, y: 300 },
      proj, 0.0, 0.0, 1.0, 0.0, 100000.0, 800, 600,
      0.0, atlas, uv, "2D"
    );

    expect(ctx.drawImage).toHaveBeenCalled();
  });
});
