import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync } from "olayer-wasm";
import { evaluateWfsStyle, validateWfsStyleSheet } from "./wfs_style";
import type { WfsPaint, WfsPredicate, WfsStyleSheet } from "./wfs_style";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import { WfsLayer } from "./wfs";
import { OgcHttpClient } from "../map_provider/ogc_client";
import { WfsVectorSource } from "../map_provider/ogc_sources";
import { buildPreparedWfsMesh, paths } from "./wfs_mesh";
import { buildTaggedWfsFillMesh, prepareTaggedWfsFillTriangles } from "./wfs_fill";

beforeAll(() => initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) }));
const hidden: WfsPaint = { visibility: "hidden" };
const green: WfsPaint = { visibility: "visible", fill: [0, 1, 0, 1] };
const red: WfsPaint = { visibility: "visible", stroke: [1, 0, 0, 1] };
const context = { viewMode: "2D" as const, centerLat: 0, centerLon: 0, zoom: 1, project: (lat: number, lon: number): readonly number[] => [lon, lat] };
function sheet(predicate: WfsPredicate): WfsStyleSheet {
  return { defaultPaint: hidden, rules: [{ predicate, paint: green }] };
}
function evaluate(properties: Readonly<Record<string, unknown>>, predicate: WfsPredicate): WfsPaint | undefined {
  return evaluateWfsStyle({ features: [{ geometry: null, properties }] }, validateWfsStyleSheet(sheet(predicate)))[0];
}
function sourceFor(collection: WfsFeatureCollection): WfsVectorSource {
  const source = new WfsVectorSource(new OgcHttpClient(), { endpoint: "https://example.test/wfs", typeName: "features", crs: "EPSG:4326" });
  source.load = async () => collection;
  return source;
}

describe("bounded WFS property paints", () => {
  it("distinguishes own missing, null, prototype and literal dotted keys", () => {
    const equalsNull: WfsPredicate = { kind: "equals", property: "x", value: null };
    expect(evaluate({}, equalsNull)).toEqual(hidden);
    expect(evaluate({ x: null }, equalsNull)).toEqual(green);
    const inherited: Record<string, unknown> = Object.create({ x: null });
    expect(evaluate(inherited, equalsNull)).toEqual(hidden);
    expect(evaluate(inherited, { kind: "exists", property: "x" })).toEqual(hidden);
    expect(evaluate({ x: null }, { kind: "exists", property: "x" })).toEqual(green);
    expect(evaluate({ x: {} }, { kind: "exists", property: "x" })).toEqual(green);
    expect(evaluate({ x: [] }, equalsNull)).toEqual(hidden);
    expect(evaluate({ "a.b": 1 }, { kind: "equals", property: "a.b", value: 1 })).toEqual(green);
    expect(evaluate({ a: { b: 1 } }, { kind: "exists", property: "a.b" })).toEqual(hidden);
    expect(evaluate({}, { kind: "exists", property: "toString" })).toEqual(hidden);
    expect(evaluate(JSON.parse('{"__proto__":null}'), { kind: "equals", property: "__proto__", value: null })).toEqual(green);
  });

  it("uses strict scalar equality without coercion or numeric tolerance", () => {
    for (const value of [null, false, true, "1", 1, 0, -0, 0.25, Number.MAX_SAFE_INTEGER]) {
      expect(evaluate({ x: value }, { kind: "equals", property: "x", value })).toEqual(green);
    }
    expect(evaluate({ x: "1" }, { kind: "equals", property: "x", value: 1 })).toEqual(hidden);
    expect(evaluate({ x: false }, { kind: "equals", property: "x", value: 0 })).toEqual(hidden);
    expect(evaluate({ x: -0 }, { kind: "equals", property: "x", value: 0 })).toEqual(green);
    expect(evaluate({ x: 0.1 + 0.2 }, { kind: "equals", property: "x", value: 0.3 })).toEqual(hidden);
  });

  it("selects a complete first or last paint, defaults without merging, and snapshots inputs", () => {
    const collection = { features: [{ geometry: null, properties: { x: null } }] };
    const input: WfsStyleSheet = { defaultPaint: red, rules: [
      { predicate: { kind: "exists", property: "x" }, paint: green },
      { predicate: { kind: "equals", property: "x", value: null }, paint: hidden },
    ] };
    const validated = validateWfsStyleSheet(input);
    expect(evaluateWfsStyle(collection, validated)).toEqual([green]);
    expect(evaluateWfsStyle(collection, validateWfsStyleSheet({ ...input, priority: "last-match" }))).toEqual([hidden]);
    expect(evaluateWfsStyle(collection, validateWfsStyleSheet({ ...input, rules: [] }))).toEqual([red]);
    expect(evaluateWfsStyle(collection, validated)[0]?.stroke).toBeUndefined();
    expect(Object.isFrozen(validated.rules)).toBe(true);
    expect(validated.defaultPaint).not.toBe(input.defaultPaint);
  });

  it("rejects unsafe numeric values, undefined referenced properties, oversized UTF-8 and invalid colors", () => {
    for (const value of [NaN, Infinity, -Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => validateWfsStyleSheet(sheet({ kind: "equals", property: "x", value }))).toThrowError(RangeError);
      expect(evaluate({ x: value }, { kind: "exists", property: "x" })).toEqual(green);
      expect(() => evaluate({ x: value }, { kind: "equals", property: "x", value: 1 })).toThrowError(RangeError);
    }
    expect(() => evaluate({ x: undefined }, { kind: "exists", property: "x" })).toThrowError(TypeError);
    expect(() => validateWfsStyleSheet(sheet({ kind: "exists", property: "é".repeat(129) }))).toThrowError(RangeError);
    expect(() => validateWfsStyleSheet(sheet({ kind: "equals", property: "x", value: "é".repeat(2049) }))).toThrowError(RangeError);
    expect(() => validateWfsStyleSheet({ defaultPaint: hidden, rules: Array.from({ length: 65 }, () => ({ predicate: { kind: "exists" as const, property: "x" }, paint: red })) })).toThrowError(RangeError);
    for (const color of [[NaN, 0, 0, 1], [0, Infinity, 0, 1], [1.1, 0, 0, 1], [-1, 0, 0, 1], new Array(4)]) {
      const malformed: unknown = { defaultPaint: { visibility: "visible", stroke: color }, rules: [] };
      // Exercise the runtime JS boundary rather than trusting compile-time tuple constraints.
      expect(() => Reflect.apply(validateWfsStyleSheet, undefined, [malformed])).toThrowError(RangeError);
    }
  });

  it("snapshots each color channel once and validates evaluator runtime input", () => {
    let reads = 0;
    const changingColor = [0, 0, 0, 1];
    Object.defineProperty(changingColor, "0", { configurable: true, get: () => ++reads === 1 ? 0.25 : 2 });
    const valid = validateWfsStyleSheet({ defaultPaint: { visibility: "visible", stroke: changingColor }, rules: [] });
    expect(valid.defaultPaint.stroke?.[0]).toBe(0.25);
    expect(reads).toBe(1);
    expect(() => validateWfsStyleSheet(null)).toThrowError(TypeError);
    expect(() => evaluateWfsStyle({ features: {} }, valid)).toThrowError(TypeError);
    expect(() => evaluateWfsStyle(null, valid)).toThrowError(TypeError);
    expect(evaluate({ x: { nested: true } }, { kind: "exists", property: "x" })).toEqual(green);
    expect(evaluate({ x: { nested: true } }, { kind: "equals", property: "x", value: null })).toEqual(hidden);
  });

  it("keeps MultiPolygon/GeometryCollection holes and duplicate/missing IDs associated by index", () => {
    const ring = (x: number): number[][] => [[x, 0], [x + 3, 0], [x + 3, 3], [x, 3], [x, 0]];
    const hole = [[1, 1], [1, 2], [2, 2], [2, 1], [1, 1]];
    const collection: WfsFeatureCollection = { features: [
      { id: "duplicate", properties: {}, geometry: { type: "MultiPolygon", coordinates: [[ring(0), hole], [ring(5)]] } },
      { id: "duplicate", properties: {}, geometry: { type: "GeometryCollection", geometries: [{ type: "Polygon", coordinates: [ring(10)] }, { type: "Point", coordinates: [20, 0] }] } },
      { properties: {}, geometry: { type: "Polygon", coordinates: [ring(15)] } },
    ] };
    const geometry = paths(collection, "EPSG:4326");
    const prepared = prepareTaggedWfsFillTriangles(geometry, { subdivisions: 1 });
    expect(prepared.map(p => p.featureIndex)).toEqual([0, 0, 1, 2]);
    const fills = buildTaggedWfsFillMesh(prepared, context, { subdivisions: 1 });
    const lines = buildPreparedWfsMesh(geometry, context, { subdivisions: 1 });
    expect(fills.ranges.map(r => r.featureIndex)).toEqual([0, 1, 2]);
    expect(lines.ranges.map(r => r.featureIndex)).toEqual([0, 1, 2]);
    const range = fills.ranges[0]!;
    let area = 0;
    const toDegrees = 180 / Math.PI;
    for (let k = range.first * 3; k < (range.first + range.count) * 3; k += 9) {
      const v = fills.vertices;
      const x = (v[k]! + v[k + 3]! + v[k + 6]!) / 3 * toDegrees;
      const y = (v[k + 1]! + v[k + 4]! + v[k + 7]!) / 3 * toDegrees;
      expect(x > 1 && x < 2 && y > 1 && y < 2).toBe(false);
      area += Math.abs((v[k + 3]! - v[k]!) * (v[k + 7]! - v[k + 1]!) - (v[k + 4]! - v[k + 1]!) * (v[k + 6]! - v[k]!)) / 2 * toDegrees ** 2;
    }
    expect(area).toBeCloseTo(17, 5);
  });

  it("charges candidate vertices across features and line/fill phases, even if terrain removes output", () => {
    const collection = { features: Array.from({ length: 8000 }, () => ({ properties: {}, geometry: { type: "LineString", coordinates: [[0, 0], [1, 0]] } })) };
    const geometry = paths(collection, "EPSG:4326");
    expect(() => buildPreparedWfsMesh(geometry, { ...context, viewMode: "2.5D", elevation: () => null }, { subdivisions: 64, altitudeMode: "clamp-to-ground" })).toThrowError(RangeError);
    const triangle = paths({ features: [{ properties: {}, geometry: { type: "Polygon", coordinates: [[[0, 0], [1, 0], [1, 1], [0, 0]]] } }] }, "EPSG:4326");
    const budget = { work: 999_990, vertices: 0 };
    buildPreparedWfsMesh(triangle, context, { subdivisions: 1 }, budget);
    const fills = prepareTaggedWfsFillTriangles(triangle, { subdivisions: 1 });
    expect(() => buildTaggedWfsFillMesh(fills, context, { subdivisions: 2 }, budget)).toThrowError(RangeError);
  });

  it("validates constructor/update boundaries and never loads during a style change", async () => {
    const source = sourceFor({ features: [{ geometry: null, properties: { x: null } }] });
    let loads = 0;
    source.load = async () => { loads++; return { features: [{ geometry: null, properties: { x: null } }] }; };
    expect(() => new WfsLayer("conflict", source, { style: sheet({ kind: "exists", property: "x" }), color: [1, 0, 0, 1] })).toThrowError(TypeError);
    const layer = new WfsLayer("styled", source, { style: sheet({ kind: "exists", property: "x" }) });
    await layer.load();
    layer.setStyle({ defaultPaint: hidden, rules: [] });
    layer.setStyle(sheet({ kind: "equals", property: "x", value: null }));
    expect(() => layer.setStyle(sheet({ kind: "equals", property: "x", value: NaN }))).toThrowError(RangeError);
    expect(loads).toBe(1);
    const hiddenBad = new WfsLayer("hidden-bad", sourceFor({ features: [{ properties: {}, geometry: { type: "Polygon", coordinates: [[[0, 0], [1, 1], [0, 1], [1, 0], [0, 0]]] } }] }), { style: { defaultPaint: hidden, rules: [] } });
    await expect(hiddenBad.load()).rejects.toBeDefined();
  });
});
