import { lla_to_ecef } from "olayer-wasm";
import type { OgcBounds } from "../map_provider/ogc";

const RADIUS = 6378137;
const POLAR_RADIUS = 6356752.314245179;

export interface WmsMeshContext {
  readonly viewMode: "2D" | "2.5D" | "3D";
  readonly project: (latRad: number, lonRad: number) => ArrayLike<number>;
  /** Radians, matching the controller camera. */
  readonly centerLat: number;
  readonly centerLon: number;
  readonly zoom: number;
  readonly rotation?: number;
  readonly pitch?: number;
  readonly roll?: number;
  /** Metres; null leaves a hole instead of inventing ground elevation. */
  readonly elevation?: (latRad: number, lonRad: number) => number | null;
}

/** Validates image placement without interpreting display mode as request state. */
export function validateWmsBounds(crs: string, bounds: OgcBounds): void {
  if (!["EPSG:4326", "EPSG:3857", "EPSG:900913"].includes(crs)) {
    throw new RangeError(`Unsupported WMS image CRS: ${crs}`);
  }
  const limitX = crs === "EPSG:4326" ? 180 : Math.PI * RADIUS;
  const limitY = crs === "EPSG:4326" ? 90 : Math.PI * RADIUS;
  if (!bounds.every(Number.isFinite) || bounds[0] >= bounds[2] || bounds[1] >= bounds[3]
    || bounds[0] < -limitX || bounds[2] > limitX || bounds[1] < -limitY || bounds[3] > limitY) {
    throw new RangeError("WMS image bounds must be ordered and inside the supported CRS domain");
  }
}

type Vertex = [number, number, number, number, number, number];

export function globeEye(context: WmsMeshContext): number[] {
  const eye = [0, 0, RADIUS + 15000000 / context.zoom];
  // Inverse of the core camera view rotations, applied in reverse order.
  for (const [axis, angle] of [
    [1, -(context.roll ?? 0)], [0, -(context.pitch ?? 0)], [2, context.rotation ?? 0],
    [0, Math.PI / 2 - context.centerLat], [2, context.centerLon + Math.PI / 2],
  ]) {
    const a = (axis! + 1) % 3, b = (axis! + 2) % 3;
    const x = eye[a]!, y = eye[b]!;
    eye[a] = Math.cos(angle!) * x - Math.sin(angle!) * y;
    eye[b] = Math.sin(angle!) * x + Math.cos(angle!) * y;
  }
  return eye;
}

/** Clips textured triangles to the camera-facing ellipsoid tangent plane. */
function clipTriangle(input: Vertex[]): Vertex[] {
  const polygon: Vertex[] = [];
  for (let i = 0; i < input.length; i++) {
    const a = input[i]!;
    const b = input[(i + 1) % input.length]!;
    if (a[5] >= 0) polygon.push(a);
    if ((a[5] >= 0) !== (b[5] >= 0)) {
      const t = a[5] / (a[5] - b[5]);
      polygon.push(a.map((value, axis) => value + t * (b[axis]! - value)) as Vertex);
    }
  }
  return polygon;
}

/** Packed triangle vertices: x/y/z/u/v. Texture origin is the image's north-west corner. */
export function buildWmsMesh(
  crs: string, bounds: OgcBounds, context: WmsMeshContext, subdivisions = 16,
): Float32Array {
  validateWmsBounds(crs, bounds);
  if (!Number.isInteger(subdivisions) || subdivisions < 1 || subdivisions > 64) {
    throw new RangeError("WMS subdivisions must be an integer from 1 to 64");
  }
  if (![context.centerLat, context.centerLon, context.zoom, context.rotation ?? 0, context.pitch ?? 0, context.roll ?? 0].every(Number.isFinite) || context.zoom <= 0) {
    throw new RangeError("WMS camera must be finite with positive zoom");
  }
  const eye = context.viewMode === "3D"
    ? globeEye(context)
    : null;
  const vertices: Array<Vertex | null> = [];
  for (let row = 0; row <= subdivisions; row++) {
    const v = row / subdivisions;
    const y = bounds[3] - v * (bounds[3] - bounds[1]);
    for (let column = 0; column <= subdivisions; column++) {
      const u = column / subdivisions;
      const x = bounds[0] + u * (bounds[2] - bounds[0]);
      const lat = crs === "EPSG:4326" ? y * Math.PI / 180 : Math.atan(Math.sinh(y / RADIUS));
      const lon = crs === "EPSG:4326" ? x * Math.PI / 180 : x / RADIUS;
      const height = context.viewMode === "2D" ? 0 : context.elevation?.(lat, lon) ?? (context.elevation ? null : 0);
      if (height === null) { vertices.push(null); continue; }
      const projected = context.viewMode === "3D" ? lla_to_ecef(lat, lon, height) : context.project(lat, lon);
      const px = projected[0], py = projected[1];
      const pz = context.viewMode === "3D" ? projected[2] : height;
      if (px === undefined || py === undefined || pz === undefined || ![px, py, pz, height].every(Number.isFinite)) {
        throw new RangeError("WMS mesh projection or elevation is not finite");
      }
      const visibility = eye
        ? px * eye[0]! / (RADIUS * RADIUS) + py * eye[1]! / (RADIUS * RADIUS)
          + pz * eye[2]! / (POLAR_RADIUS * POLAR_RADIUS) - 1
        : 1;
      vertices.push([px, py, pz, u, v, visibility]);
    }
  }
  const output: number[] = [];
  for (let row = 0; row < subdivisions; row++) {
    for (let column = 0; column < subdivisions; column++) {
      const a = row * (subdivisions + 1) + column;
      const b = a + subdivisions + 1;
      for (const triangle of [[a, b, a + 1], [a + 1, b, b + 1]]) {
        const points = triangle.map(index => vertices[index]);
        if (points.some(point => point === null || point === undefined)) continue;
        const polygon = clipTriangle(points as Vertex[]);
        for (let i = 1; i + 1 < polygon.length; i++) {
          for (const vertex of [polygon[0]!, polygon[i]!, polygon[i + 1]!]) output.push(...vertex.slice(0, 5));
        }
      }
    }
  }
  const result = new Float32Array(output);
  if (!result.every(Number.isFinite)) throw new RangeError("WMS mesh exceeds GPU precision limits");
  return result;
}
