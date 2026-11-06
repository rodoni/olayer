import { lla_to_ecef } from "olayer-wasm";
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
}

type Position = [number, number, number];
interface Path { readonly positions: Position[]; readonly point: boolean }
const A = 6378137, B = 6356752.314245179;
const MAX_VERTICES = 1_000_000;

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Validates the complete geometry before installation; properties remain on the collection. */
export function validateWfsGeometry(collection: WfsFeatureCollection, crs: string): void {
  paths(collection, crs);
}

function paths(collection: WfsFeatureCollection, crs: string): Path[] {
  if (!["EPSG:4326", "EPSG:3857", "EPSG:900913"].includes(crs)) throw new RangeError(`Unsupported WFS CRS: ${crs}`);
  if (collection.crs !== undefined && collection.crs !== crs) throw new RangeError("WFS collection CRS conflicts with placement");
  const output: Path[] = [];
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
  const line = (raw: unknown, ring = false): void => {
    const coordinates = array(raw);
    if (coordinates.length === 0) return;
    const positions = coordinates.map(position);
    if (positions.length < (ring ? 4 : 2)) throw new TypeError("WFS line or ring is too short");
    if (ring && positions[0]!.some((v, i) => v !== positions[positions.length - 1]![i])) throw new TypeError("WFS ring must be closed");
    for (let i = 1; i < positions.length; i++) {
      if (Math.abs(positions[i]![1] - positions[i - 1]![1]) > Math.PI) throw new RangeError("Split WFS paths at the antimeridian");
    }
    output.push({ positions, point: false });
  };
  const geometry = (raw: unknown, depth = 0): void => {
    if (raw === null) return;
    if (depth > 16 || !record(raw)) throw new TypeError("Invalid WFS geometry or nesting limit");
    const coordinates = raw["coordinates"];
    switch (raw["type"]) {
      case "Point": output.push({ positions: [position(coordinates)], point: true }); break;
      case "MultiPoint": for (const p of array(coordinates)) output.push({ positions: [position(p)], point: true }); break;
      case "LineString": line(coordinates); break;
      case "MultiLineString": for (const p of array(coordinates)) line(p); break;
      case "Polygon": for (const p of array(coordinates)) line(p, true); break;
      case "MultiPolygon": for (const polygon of array(coordinates)) for (const p of array(polygon)) line(p, true); break;
      case "GeometryCollection": for (const child of array(raw["geometries"])) geometry(child, depth + 1); break;
      default: throw new TypeError("Unsupported WFS geometry type");
    }
  };
  for (const feature of collection.features) geometry(feature.geometry);
  return output;
}

/** Packed line-list vertices. Unknown terrain breaks segments, never joins across gaps. */
export function buildWfsMesh(collection: WfsFeatureCollection, crs: string, context: WmsMeshContext, options: WfsMeshOptions = {}): Float32Array {
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
  const output: number[] = [];
  let work = 0;
  const emit = (a: Position | null, b: Position | null): void => {
    work += 2;
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
  };
  for (const path of paths(collection, crs)) {
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
  }
  const result = new Float32Array(output);
  if (!result.every(Number.isFinite)) throw new RangeError("WFS mesh exceeds GPU precision");
  return result;
}
