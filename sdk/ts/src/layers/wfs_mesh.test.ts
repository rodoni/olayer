import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync, lla_to_ecef } from "olayer-wasm";
import { buildWfsMesh, validateWfsGeometry } from "./wfs_mesh";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import type { WmsMeshContext } from "./wms_mesh";

beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));
const fixture: WfsFeatureCollection = JSON.parse(readFileSync(resolve(__dirname, "../../../../tests/fixtures/wfs/features.json"), "utf8"));
const context: WmsMeshContext = { viewMode: "2D", centerLat: 0, centerLon: 0, zoom: 1, project: (lat, lon) => [lon * 6378137, lat * 6378137] };
const collection = (type: string, coordinates: unknown): WfsFeatureCollection => ({ features: [{ geometry: { type, coordinates }, properties: {} }] });

describe("WFS geometry", () => {
  it("preserves points, independent polygon rings and properties", () => {
    const vertices = buildWfsMesh(fixture, "EPSG:4326", context, { subdivisions: 1 });
    expect(vertices.length / 3).toBe(4 + 2 + 8 + 8);
    expect(fixture.features[0]?.properties["metadata"]).toEqual({ kind: "test" });
    expect(Array.from(vertices).every((v, i) => i % 3 !== 2 || v === 0)).toBe(true);
  });

  it("resolves absolute, clamped and relative heights in 2.5D and ECEF", () => {
    const line = collection("LineString", [[0, 0, 100], [1, 1, 200]]);
    const tilted = { ...context, viewMode: "2.5D" as const, elevation: () => 500 };
    expect(buildWfsMesh(line, "EPSG:4326", tilted, { subdivisions: 1 })[2]).toBe(100);
    expect(buildWfsMesh(line, "EPSG:4326", tilted, { altitudeMode: "clamp-to-ground", heightOffset: 10 })[2]).toBe(510);
    expect(buildWfsMesh(line, "EPSG:4326", tilted, { altitudeMode: "relative-to-ground", heightOffset: 10 })[2]).toBe(610);
    const globe = buildWfsMesh(line, "EPSG:4326", { ...tilted, viewMode: "3D" }, { altitudeMode: "relative-to-ground" });
    expect(Array.from(globe.slice(0, 3))).toEqual(Array.from(new Float32Array(lla_to_ecef(0, 0, 600))));
  });

  it("breaks at missing terrain and clips the globe horizon", () => {
    const line = collection("LineString", [[-1, 0], [1, 0]]);
    const vertices = buildWfsMesh(line, "EPSG:4326", { ...context, viewMode: "2.5D", elevation: (_lat, lon) => Math.abs(lon) < 0.001 ? null : 500 }, { subdivisions: 2, altitudeMode: "clamp-to-ground" });
    expect(vertices).toHaveLength(0);
    expect(buildWfsMesh(collection("LineString", [[170, 0], [175, 0]]), "EPSG:4326", { ...context, viewMode: "3D" })).toHaveLength(0);
    const edge = buildWfsMesh(collection("LineString", [[65, 0], [85, 0]]), "EPSG:4326", { ...context, viewMode: "3D" });
    expect(edge.length).toBeGreaterThan(0);
    for (let i = 0; i < edge.length; i += 3) expect(edge[i]).toBeGreaterThanOrEqual(6378137 ** 2 / (6378137 + 15000000) - 1);
  });

  it("handles Mercator, multi geometries and null geometry", () => {
    const geographic = collection("LineString", [[0, 0], [1, 1]]);
    const mercator = collection("LineString", [[0, 0], [Math.PI / 180 * 6378137, Math.asinh(Math.tan(Math.PI / 180)) * 6378137]]);
    expect(buildWfsMesh(mercator, "EPSG:3857", context)).toEqual(buildWfsMesh(geographic, "EPSG:4326", context));
    for (const [type, coords] of [["MultiPoint", [[0, 0], [1, 1]]], ["MultiLineString", [[[0, 0], [1, 1]]]], ["MultiPolygon", [[[[0, 0], [1, 0], [1, 1], [0, 0]]]]]] as const) {
      expect(buildWfsMesh(collection(type, coords), "EPSG:4326", context).length).toBeGreaterThan(0);
    }
    expect(buildWfsMesh({ features: [{ geometry: null, properties: {} }] }, "EPSG:4326", context)).toHaveLength(0);
  });

  it("rejects malformed geometries, unclosed rings, unsupported CRS and seam crossings", () => {
    for (const [type, coords] of [["Point", [0, NaN]], ["Point", [181, 0]], ["LineString", [[0, 0]]], ["Polygon", [[[0, 0], [1, 0], [1, 1], [2, 1]]]], ["LineString", [[179, 0], [-179, 0]]]] as const) {
      expect(() => validateWfsGeometry(collection(type, coords), "EPSG:4326")).toThrow();
    }
    expect(() => buildWfsMesh(fixture, "EPSG:4978", context)).toThrow();
    expect(() => buildWfsMesh(fixture, "EPSG:4326", context, { subdivisions: 65 })).toThrow();
  });
});
