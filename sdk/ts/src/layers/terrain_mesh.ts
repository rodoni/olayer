import { lla_to_ecef } from "olayer-wasm";
import type { WmsMeshContext } from "./wms_mesh";

/** Builds a north-up terrain grid without inventing surfaces in missing cells. */
export function buildTerrainGrid(
  size: number, centerLat: number, centerLon: number, latSpan: number, lonSpan: number,
  context: WmsMeshContext, exaggeration: number,
  uv: (lat: number, lon: number) => readonly [number, number],
): { vertices: Float32Array; indices: Uint32Array } {
  const width = size + 1;
  const vertices = new Float32Array(width * width * 10);
  const elevations: Array<number | null> = [];
  const coordinates: Array<readonly [number, number]> = [];
  for (let row = 0; row <= size; row++) {
    for (let column = 0; column <= size; column++) {
      const lat = centerLat + (0.5 - row / size) * latSpan;
      const lon = centerLon + (column / size - 0.5) * lonSpan;
      coordinates.push([lat, lon]);
      let elevation = Math.abs(lat) <= Math.PI / 2 && Math.abs(lon) <= Math.PI ? context.elevation?.(lat, lon) ?? null : null;
      if (elevation !== null && !Number.isFinite(elevation)) elevation = null;
      const index = row * width + column;
      elevations.push(elevation);
      if (elevation === null) continue;
      const height = context.viewMode === "2D" ? 0 : elevation * exaggeration;
      let p: ArrayLike<number>;
      try { p = context.viewMode === "3D" ? lla_to_ecef(lat, lon, height) : context.project(lat, lon); }
      catch { elevations[index] = null; continue; }
      const position = [p[0]!, p[1]!, context.viewMode === "3D" ? p[2]! : height];
      if (!position.every(v => Number.isFinite(v) && Math.abs(v) <= 3.4028234663852886e38)) {
        elevations[index] = null; continue;
      }
      vertices.set([...position, elevation, 0, 0, 0, 1, ...uv(lat, lon)], index * 10);
    }
  }
  const slope = (minus: number | null | undefined, plus: number | null | undefined, center: number, spacing: number): number => {
    if (minus != null && plus != null) return (plus - minus) / (2 * spacing);
    if (plus != null) return (plus - center) / spacing;
    if (minus != null) return (center - minus) / spacing;
    return 0;
  };
  const dx = Math.max(1, lonSpan * 6378137 * Math.abs(Math.cos(centerLat)) / size);
  const dy = Math.max(1, latSpan * 6378137 / size);
  for (let row = 0; row <= size; row++) {
    for (let column = 0; column <= size; column++) {
      const index = row * width + column, elevation = elevations[index];
      if (elevation == null) continue;
      const east = slope(column > 0 ? elevations[index - 1] : null, column < size ? elevations[index + 1] : null, elevation, dx);
      const south = slope(row > 0 ? elevations[index - width] : null, row < size ? elevations[index + width] : null, elevation, dy);
      const length = Math.hypot(east, south, 1);
      const nx = -east / length, ny = south / length, nz = 1 / length;
      const [lat, lon] = coordinates[index]!;
      const normal = context.viewMode === "3D" ? [
        -Math.sin(lon) * nx - Math.sin(lat) * Math.cos(lon) * ny + Math.cos(lat) * Math.cos(lon) * nz,
        Math.cos(lon) * nx - Math.sin(lat) * Math.sin(lon) * ny + Math.cos(lat) * Math.sin(lon) * nz,
        Math.cos(lat) * ny + Math.sin(lat) * nz,
      ] : [nx, ny, nz];
      vertices[index * 10 + 4] = Math.atan(Math.hypot(east, south)) * 180 / Math.PI;
      vertices.set(normal, index * 10 + 5);
    }
  }
  const indices: number[] = [];
  for (let row = 0; row < size; row++) {
    for (let column = 0; column < size; column++) {
      const a = row * width + column, b = a + width;
      // A cell containing unknown ground cannot occlude anything.
      if ([a, a + 1, b, b + 1].some(i => elevations[i] == null)) continue;
      indices.push(a, a + 1, b, a + 1, b + 1, b);
    }
  }
  return { vertices, indices: new Uint32Array(indices) };
}
