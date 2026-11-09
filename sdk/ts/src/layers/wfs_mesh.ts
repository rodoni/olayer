import { lla_to_ecef, triangulate_polygon_rings } from "olayer-wasm";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";
import { globeEye } from "./wms_mesh";
import type { WmsMeshContext } from "./wms_mesh";

export interface WfsMeshOptions {
  readonly altitudeMode?: "absolute" | "clamp-to-ground" | "relative-to-ground";
  /** Metres added after altitude resolution. Default 0. */
  readonly heightOffset?: number;
  /** Linear subdivisions per source segment, 1..64. Default 16. */
  readonly subdivisions?: number;
  /** Half-length of point cross markers in metres. Default 1000. */
  readonly pointRadius?: number;
  /** Fill validated polygon interiors as well as drawing outlines. Default false. */
  readonly fillPolygons?: boolean;
}

export type Position = [number, number, number];
export interface WfsPath { readonly positions: Position[]; readonly point: boolean; readonly featureIndex: number }
export interface WfsPolygon { readonly rings: Position[][]; readonly featureIndex: number }
export interface WfsGeometry { readonly paths: readonly WfsPath[]; readonly polygons: readonly WfsPolygon[] }
export interface WfsDrawRange { readonly featureIndex: number; readonly first: number; readonly count: number }
export interface WfsMeshBudget { work: number; vertices: number }
export interface WfsRangedMesh { readonly vertices: Float32Array; readonly ranges: readonly WfsDrawRange[] }
export function chargeWfsMesh(budget: WfsMeshBudget, work: number, vertices: number): void {
  budget.work += work;
  budget.vertices += vertices;
  if (budget.work > MAX_VERTICES || budget.vertices > MAX_VERTICES) throw new RangeError("WFS aggregate mesh limit exceeded");
}
export function appendWfsRange(ranges: WfsDrawRange[], featureIndex: number, first: number, count: number): void {
  if (!count) return;
  const previous = ranges[ranges.length - 1];
  if (previous?.featureIndex === featureIndex && previous.first + previous.count === first) {
    ranges[ranges.length - 1] = { featureIndex, first: previous.first, count: previous.count + count };
  } else {
    if (ranges.length >= 8192) throw new RangeError("WFS draw range limit exceeded");
    ranges.push({ featureIndex, first, count });
  }
}
const A = 6378137, B = 6356752.314245179;
const MAX_VERTICES = 1_000_000;

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Validates the complete geometry before installation; properties remain on the collection. */
export function validateWfsGeometry(collection: WfsFeatureCollection, crs: string, fillPolygons = false): void {
  const geometry = paths(collection, crs);
  if (fillPolygons) for (const polygon of geometry.polygons) triangulateWfsPolygon(polygon.rings);
}

export function triangulateWfsPolygon(rings: Position[][]): Float64Array {
  return triangulate_polygon_rings(new Float64Array(rings.flat(2)), new Uint32Array(rings.map(ring => ring.length)));
}

export function paths(collection: WfsFeatureCollection, crs: string): WfsGeometry {
  if (!["EPSG:4326", "EPSG:3857", "EPSG:900913"].includes(crs)) throw new RangeError(`Unsupported WFS CRS: ${crs}`);
  if (collection.crs !== undefined && collection.crs !== crs) throw new RangeError("WFS collection CRS conflicts with placement");
  const output: WfsPath[] = [];
  const polygons: WfsPolygon[] = [];
  let featureIndex = 0;
  let objects = 0;
  if (collection.features.length > 100_000) throw new RangeError("WFS feature limit exceeded");
  let count = 0;
  const position = (raw: unknown): Position => {
    if (!Array.isArray(raw) || raw.length < 2 || raw.length > 3 || !raw.every(v => typeof v === "number" && Number.isFinite(v))) {
      throw new TypeError("WFS position requires finite x/y and optional ellipsoidal height");
    }
    if (++count > MAX_VERTICES) throw new RangeError("WFS coordinate limit exceeded");
    const [x, y, height = 0] = raw as number[];
    const limitX = crs === "EPSG:4326" ? 180 : Math.PI * A;
    const limitY = crs === "EPSG:4326" ? 90 : Math.PI * A;
    if (Math.abs(x!) > limitX || Math.abs(y!) > limitY) throw new RangeError("WFS coordinate outside CRS domain");
    return crs === "EPSG:4326" ? [y! * Math.PI / 180, x! * Math.PI / 180, height]
      : [Math.atan(Math.sinh(y! / A)), x! / A, height];
  };
  const array = (raw: unknown): unknown[] => {
    if (!Array.isArray(raw)) throw new TypeError("Invalid WFS coordinate nesting");
    return raw;
  };
  const line = (raw: unknown, ring = false): Position[] => {
    const coordinates = array(raw);
    if (coordinates.length === 0) return [];
    const positions = coordinates.map(position);
    if (positions.length < (ring ? 4 : 2)) throw new TypeError("WFS line or ring is too short");
    if (ring && positions[0]!.some((v, i) => v !== positions[positions.length - 1]![i])) throw new TypeError("WFS ring must be closed");
    for (let i = 1; i < positions.length; i++) {
      if (Math.abs(positions[i]![1] - positions[i - 1]![1]) > Math.PI) throw new RangeError("Split WFS paths at the antimeridian");
    }
    output.push({ positions, point: false, featureIndex });
    return positions;
  };
  const polygon = (raw: unknown): void => {
    const rings = array(raw).map(ring => line(ring, true));
    if (rings.length > 0) polygons.push({ rings, featureIndex });
  };
  const geometry = (raw: unknown, depth = 0): void => {
    if (++objects > 100_000) throw new RangeError("WFS geometry object limit exceeded");
    if (raw === null) return;
    if (depth > 16 || !record(raw)) throw new TypeError("Invalid WFS geometry or nesting limit");
    const coordinates = raw["coordinates"];
    switch (raw["type"]) {
      case "Point": output.push({ positions: [position(coordinates)], point: true, featureIndex }); break;
      case "MultiPoint": for (const p of array(coordinates)) output.push({ positions: [position(p)], point: true, featureIndex }); break;
      case "LineString": line(coordinates); break;
      case "MultiLineString": for (const p of array(coordinates)) line(p); break;
      case "Polygon": polygon(coordinates); break;
      case "MultiPolygon": for (const p of array(coordinates)) polygon(p); break;
      case "GeometryCollection": for (const child of array(raw["geometries"])) geometry(child, depth + 1); break;
      default: throw new TypeError("Unsupported WFS geometry type");
    }
  };
  for (const feature of collection.features) { geometry(feature.geometry); featureIndex++; }
  return { paths: output, polygons };
}

export function createWfsTransform(context: WmsMeshContext, options: WfsMeshOptions) {
  if (options.fillPolygons !== undefined && typeof options.fillPolygons !== "boolean") throw new TypeError("Invalid WFS fillPolygons option");
  const subdivisions = options.subdivisions ?? 16;
  const radius = options.pointRadius ?? 1000;
  const offset = options.heightOffset ?? 0;
  const altitude = options.altitudeMode ?? "absolute";
  if (!Number.isInteger(subdivisions) || subdivisions < 1 || subdivisions > 64 || !Number.isFinite(radius) || radius <= 0
    || !Number.isFinite(offset) || !["absolute", "clamp-to-ground", "relative-to-ground"].includes(altitude)) throw new RangeError("Invalid WFS mesh options");
  if (![context.centerLat, context.centerLon, context.zoom, context.rotation ?? 0, context.pitch ?? 0, context.roll ?? 0].every(Number.isFinite)
    || context.zoom <= 0) throw new RangeError("Invalid WFS camera");
  const globe = context.viewMode === "3D";
  const eye = globe ? globeEye(context) : null;
  const visibility = (p: Position): number => eye ? (p[0] * eye[0]! + p[1] * eye[1]!) / (A * A) + p[2] * eye[2]! / (B * B) - 1 : 1;
  const project = ([lat, lon, z]: Position): Position | null => {
    let height = 0;
    if (context.viewMode !== "2D") {
      const ground = altitude === "absolute" ? 0 : context.elevation?.(lat, lon) ?? null;
      if (ground === null) return null;
      height = ground + (altitude === "clamp-to-ground" ? 0 : z) + offset;
    }
    const p = globe ? lla_to_ecef(lat, lon, height) : context.project(lat, lon);
    const result: Position = [p[0]!, p[1]!, globe ? p[2]! : height];
    if (!result.every(Number.isFinite)) throw new RangeError("WFS projection or elevation is not finite");
    return result;
  };
  return { project, visibility, globe, subdivisions, radius };
}

/** Packed line-list vertices. Unknown terrain breaks segments, never joins across gaps. */
export function buildWfsMesh(collection: WfsFeatureCollection, crs: string, context: WmsMeshContext, options: WfsMeshOptions = {}): Float32Array {
  return buildPreparedWfsMesh(paths(collection, crs), context, options).vertices;
}

/** Transform all paths together so limits cannot reset at feature boundaries. */
export function buildPreparedWfsMesh(geometry: WfsGeometry, context: WmsMeshContext, options: WfsMeshOptions = {}, budget: WfsMeshBudget = { work: 0, vertices: 0 }): WfsRangedMesh {
  const { project, visibility, globe, subdivisions, radius } = createWfsTransform(context, options);
  const output: number[] = [];
  const ranges: WfsDrawRange[] = [];
  let work = 0;
  const emit = (a: Position | null, b: Position | null): void => {
    work += 2;
    chargeWfsMesh(budget, 2, 0);
    if (work > MAX_VERTICES) throw new RangeError("WFS mesh vertex limit exceeded");
    if (!a || !b) return;
    const va = visibility(a), vb = visibility(b);
    if (va < 0 && vb < 0) return;
    if ((va < 0) !== (vb < 0)) {
      const t = va / (va - vb);
      const intersection = a.map((v, i) => v + t * (b![i]! - v)) as Position;
      if (va < 0) a = intersection; else b = intersection;
    }
    if (output.length / 3 + 2 > MAX_VERTICES) throw new RangeError("WFS mesh vertex limit exceeded");
    output.push(...a, ...b);
    chargeWfsMesh(budget, 0, 2);
  };
  for (const path of geometry.paths) {
    const first = output.length / 3;
    if (path.point) {
      const geo = path.positions[0]!;
      const center = project(geo);
      if (!center) continue;
      const [lat, lon] = geo;
      const axes: Position[] = globe ? [[-Math.sin(lon), Math.cos(lon), 0],
        [-Math.sin(lat) * Math.cos(lon), -Math.sin(lat) * Math.sin(lon), Math.cos(lat)]] : [[1, 0, 0], [0, 1, 0]];
      for (const axis of axes) emit(center.map((v, i) => v - radius * axis[i]!) as Position,
        center.map((v, i) => v + radius * axis[i]!) as Position);
    } else {
      for (let i = 1; i < path.positions.length; i++) {
        const a = path.positions[i - 1]!, b = path.positions[i]!;
        let previous = project(a);
        for (let j = 1; j <= subdivisions; j++) {
          const next = project(a.map((v, k) => v + (b[k]! - v) * j / subdivisions) as Position);
          emit(previous, next);
          previous = next;
        }
      }
    }
    appendWfsRange(ranges, path.featureIndex, first, output.length / 3 - first);
  }
  const result = new Float32Array(output);
  if (!result.every(Number.isFinite)) throw new RangeError("WFS mesh exceeds GPU precision");
  return { vertices: result, ranges };
}
