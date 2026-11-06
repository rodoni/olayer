import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync, lla_to_ecef } from "olayer-wasm";
import { buildWfsFillMesh } from "./wfs_fill";
import { validateWfsGeometry } from "./wfs_mesh";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import type { WmsMeshContext } from "./wms_mesh";
beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));
const fixture: WfsFeatureCollection = JSON.parse(readFileSync(resolve(__dirname, "../../../../tests/fixtures/wfs-fill/features.json"), "utf8"));
const context: WmsMeshContext = { viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1, project: (lat, lon) => [lon * 180 / Math.PI, lat * 180 / Math.PI], elevation: () => 500 };
const options = { fillPolygons: true, subdivisions: 1, altitudeMode: "relative-to-ground" as const };

describe("WFS polygon interiors through shared WASM tessellation", () => {
  it("fills concave polygons and islands while preserving holes and empty concavities", () => {
    const mesh = buildWfsFillMesh(fixture, "EPSG:4326", context, options);
    let area = 0;
    for (let i = 0; i < mesh.length; i += 9) {
      const x = (mesh[i]! + mesh[i + 3]! + mesh[i + 6]!) / 3;
      const y = (mesh[i + 1]! + mesh[i + 4]! + mesh[i + 7]!) / 3;
      expect(x > -1.5 && x < -0.5 && y > -0.8 && y < -0.2).toBe(false);
      area += Math.abs((mesh[i + 3]! - mesh[i]!) * (mesh[i + 7]! - mesh[i + 1]!) - (mesh[i + 4]! - mesh[i + 1]!) * (mesh[i + 6]! - mesh[i]!)) / 2;
    }
    expect(area).toBeCloseTo(4.9 + 0.32, 5);
    expect(buildWfsFillMesh(fixture, "EPSG:4326", context)).toHaveLength(0);
  });
  it("samples heights inside polygons, preserves nodata and clips the globe", () => {
    const tilted = buildWfsFillMesh(fixture, "EPSG:4326", { ...context, viewMode: "2.5D" }, options);
    for (let i = 2; i < tilted.length; i += 3) expect(tilted[i]).toBeCloseTo(750);
    expect(buildWfsFillMesh(fixture, "EPSG:4326", { ...context, viewMode: "2.5D", elevation: () => null }, options)).toHaveLength(0);
    const globe = buildWfsFillMesh(fixture, "EPSG:4326", { ...context, viewMode: "3D" }, options);
    expect(globe.length).toBeGreaterThan(0);
    expect(globe[0]).toBeGreaterThan(6_370_000);
    const far: WfsFeatureCollection = { features: [{ geometry: { type: "Polygon", coordinates: [[[170, 0], [175, 0], [175, 1], [170, 0]]] }, properties: {} }] };
    expect(buildWfsFillMesh(far, "EPSG:4326", { ...context, viewMode: "3D" }, options)).toHaveLength(0);
    const flat = buildWfsFillMesh(fixture, "EPSG:4326", context, options);
    const expected = lla_to_ecef(flat[1]! * Math.PI / 180, flat[0]! * Math.PI / 180, 750);
    for (let i = 0; i < 3; i++) expect(globe[i]).toBeCloseTo(expected[i]!, 0);
  });
  it("rejects crossing rings and holes outside the exterior before installation", () => {
    for (const coordinates of [
      [[[0, 0], [1, 1], [0, 1], [1, 0], [0, 0]]],
      [[[0, 0], [1, 0], [1, 1], [0, 0]], [[2, 2], [3, 2], [3, 3], [2, 2]]],
    ]) {
      const collection = { features: [{ geometry: { type: "Polygon", coordinates }, properties: {} }] };
      expect(() => validateWfsGeometry(collection, "EPSG:4326", true)).toThrow();
    }
  });
});
