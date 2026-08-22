import { describe, it, expect, beforeAll } from "vitest";
import { initSync } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";
import { VolumetricAirspaceLayer } from "./volumetric_airspace";
import { TrajectoryRibbonLayer } from "./trajectory_ribbon";

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync({ module: wasmBuffer });
});

describe("3D Volumetric Airspaces & Trajectory Ribbon GPU Shaders (GIS-PROP-007)", () => {
  describe("VolumetricAirspaceLayer", () => {
    it("should initialize with default options", () => {
      const layer = new VolumetricAirspaceLayer("airspace-layer");
      expect(layer.baseColor).toBe("rgba(0, 229, 255, 0.22)");
      expect(layer.edgeColor).toBe("#00e5ff");
      expect(layer.getAirspaceCount()).toBe(0);
    });

    it("should add and generate 3D volumetric airspace meshes", () => {
      const layer = new VolumetricAirspaceLayer("airspace-layer");

      // Quad footprint in London TMA
      const polygon = [
        51.0, -0.5,
        51.0, 0.5,
        51.5, 0.5,
        51.5, -0.5,
      ];

      layer.addAirspace("EGLL_TMA", polygon, 1000.0, 6000.0);
      expect(layer.getAirspaceCount()).toBe(1);

      const record = layer.getAirspaceMesh("EGLL_TMA");
      expect(record).toBeDefined();
      expect(record!.vertexCount).toBe(28); // 4 walls * 4 + 2 caps * 2 * 3 = 28
      expect(record!.indexCount).toBe(36); // 4 walls * 6 + 2 caps * 2 * 3 = 36
      expect(record!.vertices.length).toBe(28 * 8);
      expect(record!.indices.length).toBe(36);

      // Remove and clear
      layer.removeAirspace("EGLL_TMA");
      expect(layer.getAirspaceCount()).toBe(0);
    });
  });

  describe("TrajectoryRibbonLayer", () => {
    it("should initialize with default options", () => {
      const layer = new TrajectoryRibbonLayer("ribbon-layer");
      expect(layer.defaultRibbonWidthMeters).toBe(250);
      expect(layer.colorLow).toBe("#00e5ff");
      expect(layer.colorHigh).toBe("#ff1744");
      expect(layer.getTrajectoryCount()).toBe(0);
    });

    it("should add and generate 3D trajectory ribbons with altitude gradients", () => {
      const layer = new TrajectoryRibbonLayer("ribbon-layer");

      // 4-point climb trajectory
      const waypoints = [
        40.0, -74.0, 1000.0,
        40.5, -73.5, 5000.0,
        41.0, -73.0, 10000.0,
        41.5, -72.5, 12000.0,
      ];

      layer.addTrajectory("AFR123", waypoints, 300.0);
      expect(layer.getTrajectoryCount()).toBe(1);

      const record = layer.getRibbonMesh("AFR123");
      expect(record).toBeDefined();
      expect(record!.vertexCount).toBe(8); // 4 waypoints * 2
      expect(record!.indexCount).toBe(18); // 3 segments * 6
      expect(record!.vertices.length).toBe(8 * 9);
      expect(record!.indices.length).toBe(18);

      // Verify scalar gradient (vertex 0 scalar is 0.0, vertex 7 scalar is 1.0)
      // Vertex 0: offset 8 (stride 9) -> scalar = 0.0
      expect(record!.vertices[8]).toBe(0.0);
      // Vertex 7: offset 7 * 9 + 8 = 71 -> scalar = 1.0
      expect(record!.vertices[71]).toBe(1.0);

      layer.clear();
      expect(layer.getTrajectoryCount()).toBe(0);
    });
  });
});
