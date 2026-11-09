import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync } from "olayer-wasm";
import { buildTerrainGrid, buildTerrainGridWithLod } from "./terrain_mesh";
import { TerrainLayer } from "./terrain";
import type { WmsMeshContext } from "./wms_mesh";
beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));
const context: WmsMeshContext = { viewMode: "2.5D", centerLat: 0, centerLon: 0, zoom: 1, project: (lat, lon) => [lon * 6378137, lat * 6378137], elevation: () => 500 };
describe("terrain occlusion geometry", () => {
  it("keeps grid indexing while omitting cells touching unknown samples", () => {
    const mesh = buildTerrainGrid(2, 0, 0, 0.02, 0.02, { ...context, elevation: (lat, lon) => lat === 0 && lon === 0 ? null : 500 }, 1, () => [0, 0]);
    expect(mesh.vertices.length).toBe(9 * 10);
    expect(mesh.indices.length).toBe(0);
    const full = buildTerrainGrid(2, 0, 0, 0.02, 0.02, context, 1, () => [0, 0]);
    expect(full.indices.length).toBe(24);
    expect(full.vertices[2]).toBe(500);
  });
  it("rotates flat-surface normals into ECEF and rejects failed projections as gaps", () => {
    const mesh = buildTerrainGrid(2, 0, 0, 0.02, 0.02, { ...context, viewMode: "3D" }, 1, () => [0, 0]);
    expect(Array.from(mesh.vertices.slice(4 * 10 + 5, 4 * 10 + 8))).toEqual([1, 0, 0]);
    expect(buildTerrainGrid(2, 0, 0, 0.02, 0.02, { ...context, project: () => [NaN, 0] }, 1, () => [0, 0]).indices.length).toBe(0);
  });
});

describe("bounded sampled terrain LOD", () => {
  const build = (elevation: (lat: number, lon: number) => number | null, options = { targetErrorMeters: 0, maxSubdivisions: 4, maxSourceSamples: 25 }) =>
    buildTerrainGridWithLod(1, 0, 0, 0.02, 0.02, { ...context, elevation }, 1, () => [0, 0], options);

  it("retains a flat grid and a planar ramp at the coarsest level", () => {
    const flat = build(() => 10, { targetErrorMeters: 1e-6, maxSubdivisions: 4, maxSourceSamples: 25 });
    expect(flat.gridSize).toBe(1);
    expect(flat.thresholdSatisfied).toBe(true);
    const ramp = build((lat, lon) => lat * 1000 + lon * 1000, { targetErrorMeters: 1e-6, maxSubdivisions: 4, maxSourceSamples: 25 });
    expect(ramp.gridSize).toBe(1);
    expect(ramp.maxSampledErrorMeters).toBe(0);
  });

  it("refines a saddle, reports sampled residuals, and respects source budgets", () => {
    const saddle = build((lat, lon) => (lat * lon < 0 ? 0 : 100));
    expect(saddle.gridSize).toBe(4);
    expect(saddle.maxSampledErrorMeters).toBeGreaterThan(0);
    expect(saddle.thresholdSatisfied).toBe(false);
    const limited = build(() => 10, { targetErrorMeters: 0, maxSubdivisions: 4, maxSourceSamples: 4 });
    expect(limited.gridSize).toBe(1);
    expect(limited.sourceSamples).toBe(4);
    expect(limited.budgetLimited).toBe(true);
  });

  it("caches source samples once and never guarantees a threshold with unknowns", () => {
    let calls = 0;
    const sampled = build(() => { calls += 1; return 5; });
    expect(calls).toBe(sampled.sourceSamples);
    const unknown = build((lat, lon) => lat === 0 && lon === 0 ? null : 5);
    expect(unknown.thresholdSatisfied).toBe(false);
    expect(unknown.unknownSamples).toBe(true);
    expect(unknown.indices.length).toBeGreaterThan(0);
    const noSamples = build(() => null);
    expect(noSamples.maxSampledErrorMeters).toBeNull();
    expect(noSamples.thresholdSatisfied).toBe(false);
  });

  it("documents the finite-probe blind spot for a narrow off-lattice ridge", () => {
    const result = build((lat) => Math.abs(lat - 0.0025) < 1e-8 ? 1000 : 0,
      { targetErrorMeters: 1e-6, maxSubdivisions: 4, maxSourceSamples: 25 });
    expect(result.gridSize).toBe(1);
    expect(result.thresholdSatisfied).toBe(true);
    expect(result.maxSampledErrorMeters).toBe(0);
  });

  it("wraps source sampling across the antimeridian without losing local coverage", () => {
    const longitudes: number[] = [];
    const result = buildTerrainGridWithLod(4, 0, Math.PI - 0.01, 0.1, 0.1, {
      viewMode: "2.5D", centerLat: 0, centerLon: Math.PI - 0.01, zoom: 1,
      project: (lat, lon) => [lon, lat],
      elevation: (_lat, lon) => { longitudes.push(lon); return 5; },
    }, 1, () => [0, 0], { targetErrorMeters: 1000, maxSubdivisions: 8, maxSourceSamples: 81 });
    expect(result.unknownSamples).toBe(false);
    expect(result.triangleCount).toBeGreaterThan(0);
    expect(longitudes.every(lon => Math.abs(lon) <= Math.PI)).toBe(true);
  });

  it("validates the hard sample and subdivision bounds", () => {
    expect(() => build(() => 0, { targetErrorMeters: 1, maxSubdivisions: 65, maxSourceSamples: 4225 })).toThrowError(RangeError);
    expect(() => build(() => 0, { targetErrorMeters: 1, maxSubdivisions: 3, maxSourceSamples: 16 })).toThrowError(RangeError);
    expect(() => build(() => 0, { targetErrorMeters: 1, maxSubdivisions: 4, maxSourceSamples: 4226 })).toThrowError(RangeError);
  });

  it("exposes opt-in bounded LOD through TerrainLayer without changing the default path", () => {
    const layer = new TerrainLayer("terrain-lod", 4);
    expect(layer.getLodReport()).toBeNull();
    layer.setLod({ targetErrorMeters: 20, maxSubdivisions: 16, maxSourceSamples: 289 });
    expect(layer.getLodReport()).toBeNull();
    expect(() => layer.setLod({ targetErrorMeters: 20, maxSubdivisions: 12, maxSourceSamples: 169 })).toThrowError(RangeError);
    layer.setLod(null);
    expect(layer.getLodReport()).toBeNull();
  });
});
