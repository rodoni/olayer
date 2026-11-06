import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync } from "olayer-wasm";
import { buildTerrainGrid } from "./terrain_mesh";
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
