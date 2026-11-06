import { createWfsTransform, paths, triangulateWfsPolygon } from "./wfs_mesh";
import type { Position, WfsMeshOptions } from "./wfs_mesh";
import type { WmsMeshContext } from "./wms_mesh";
import type { WfsFeatureCollection } from "../map_provider/ogc_client";

/** Packed triangles for polygon interiors. Holes and unknown terrain remain empty. */
export function buildWfsFillMesh(collection: WfsFeatureCollection, crs: string, context: WmsMeshContext, options: WfsMeshOptions = {}): Float32Array {
  createWfsTransform(context, options);
  if (!options.fillPolygons) return new Float32Array();
  return buildPreparedWfsFillMesh(prepareWfsFillTriangles(collection, crs, options), context, options);
}

export function prepareWfsFillTriangles(collection: WfsFeatureCollection, crs: string, options: WfsMeshOptions): Float64Array[] {
  if (!options.fillPolygons) return [];
  const triangles: Float64Array[] = [];
  let vertices = 0;
  const n = options.subdivisions ?? 16;
  for (const rings of paths(collection, crs).polygons) {
    const prepared = triangulateWfsPolygon(rings);
    vertices += prepared.length / 3;
    if (vertices * n * n > 1_000_000) throw new RangeError("WFS fill work limit exceeded");
    triangles.push(prepared);
  }
  return triangles;
}

export function buildPreparedWfsFillMesh(triangles: readonly Float64Array[], context: WmsMeshContext, options: WfsMeshOptions): Float32Array {
  const { project, visibility, subdivisions: n } = createWfsTransform(context, options);
  const output: number[] = [];
  let work = 0;
  const emit = (triangle: Position[]): void => {
    work += 3;
    if (work > 1_000_000) throw new RangeError("WFS fill work limit exceeded");
    const points = triangle.map(project);
    if (points.some(p => p === null)) return;
    const polygon: Position[] = [];
    for (let i = 0; i < 3; i++) {
      const a = points[i]!, b = points[(i + 1) % 3]!;
      const va = visibility(a), vb = visibility(b);
      if (va >= 0) polygon.push(a);
      if ((va >= 0) !== (vb >= 0)) {
        const t = va / (va - vb);
        polygon.push(a.map((v, k) => v + t * (b[k]! - v)) as Position);
      }
    }
    for (let i = 1; i + 1 < polygon.length; i++) {
      if (output.length / 3 + 3 > 1_000_000) throw new RangeError("WFS fill vertex limit exceeded");
      output.push(...polygon[0]!, ...polygon[i]!, ...polygon[i + 1]!);
    }
  };
  for (const vertices of triangles) {
    for (let k = 0; k < vertices.length; k += 9) {
      const a = Array.from(vertices.slice(k, k + 3)) as Position;
      const b = Array.from(vertices.slice(k + 3, k + 6)) as Position;
      const c = Array.from(vertices.slice(k + 6, k + 9)) as Position;
      const point = (i: number, j: number): Position => a.map((v, axis) => v + (b[axis]! - v) * i / n + (c[axis]! - v) * j / n) as Position;
      for (let i = 0; i < n; i++) for (let j = 0; j < n - i; j++) {
        emit([point(i, j), point(i + 1, j), point(i, j + 1)]);
        if (i + j + 1 < n) emit([point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)]);
      }
    }
  }
  const result = new Float32Array(output);
  if (!result.every(Number.isFinite)) throw new RangeError("WFS fill exceeds GPU precision");
  return result;
}
