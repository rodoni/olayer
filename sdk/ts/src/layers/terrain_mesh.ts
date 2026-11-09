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

export interface TerrainLodOptions {
  readonly targetErrorMeters: number;
  /** Maximum grid level; must be `baseSize * 2^n` and no greater than 64. */
  readonly maxSubdivisions: number;
  /** Total source lattice samples, capped at 4,225. */
  readonly maxSourceSamples: number;
}

export interface TerrainLodResult {
  readonly vertices: Float32Array;
  readonly indices: Uint32Array;
  readonly gridSize: number;
  readonly triangleCount: number;
  readonly sourceSamples: number;
  readonly knownSamples: number;
  readonly projectedSamples: number;
  /** Maximum residual over valid probes performed at every candidate level; null if none were valid. */
  readonly maxSampledErrorMeters: number | null;
  readonly thresholdSatisfied: boolean;
  readonly budgetLimited: boolean;
  readonly unknownSamples: boolean;
}

function wrapLongitudeRadians(lon: number): number {
  return ((lon + Math.PI) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI) - Math.PI;
}

/** Selects a global power-of-two terrain level using sampled XYZ residuals only. */
export function buildTerrainGridWithLod(
  size: number, centerLat: number, centerLon: number, latSpan: number, lonSpan: number,
  context: WmsMeshContext, exaggeration: number,
  uv: (lat: number, lon: number) => readonly [number, number], options: TerrainLodOptions,
): TerrainLodResult {
  const { targetErrorMeters, maxSubdivisions, maxSourceSamples } = options;
  if (!Number.isSafeInteger(size) || size < 1 || !Number.isFinite(targetErrorMeters) || targetErrorMeters < 0
    || !Number.isSafeInteger(maxSubdivisions) || maxSubdivisions < size || maxSubdivisions > 64
    || !Number.isInteger(maxSubdivisions / size) || !Number.isInteger(Math.log2(maxSubdivisions / size))
    || !Number.isSafeInteger(maxSourceSamples) || maxSourceSamples < 1 || maxSourceSamples > 4225) {
    throw new RangeError("Invalid terrain LOD size, error threshold, or sample budget");
  }
  let maximum = size;
  while (maximum <= Math.floor(maxSubdivisions / 2)) maximum *= 2;
  const sampleCount = (maximum + 1) * (maximum + 1);
  if (!Number.isSafeInteger(sampleCount)) throw new RangeError("Terrain LOD lattice size is unsafe");
  let allowed = maximum;
  while (allowed > size && (allowed + 1) * (allowed + 1) > maxSourceSamples) allowed /= 2;
  if ((allowed + 1) * (allowed + 1) > maxSourceSamples) throw new RangeError("Terrain LOD source sample budget is too small");

  const elevations = new Float64Array((allowed + 1) * (allowed + 1));
  const known = new Uint8Array(elevations.length);
  const cache = new Map<string, number | null>();
  const coordinate = (n: number, count: number): number => n / count;
  const readElevation = (lat: number, lon: number): number | null => {
    const queryLon = wrapLongitudeRadians(lon);
    const key = `${lat},${queryLon}`;
    if (cache.has(key)) return cache.get(key)!;
    let value = Math.abs(lat) <= Math.PI / 2 ? context.elevation?.(lat, queryLon) ?? null : null;
    if (value !== null && (!Number.isFinite(value) || Math.abs(value) > 3.4028234663852886e38)) value = null;
    cache.set(key, value);
    return value;
  };
  for (let row = 0; row <= allowed; row++) for (let col = 0; col <= allowed; col++) {
    const lat = centerLat + (0.5 - coordinate(row, allowed)) * latSpan;
    const lon = centerLon + (coordinate(col, allowed) - 0.5) * lonSpan;
    const index = row * (allowed + 1) + col;
    const value = readElevation(lat, lon);
    if (value !== null) { elevations[index] = value; known[index] = 1; }
  }
  const xyz = new Float64Array(elevations.length * 3);
  const valid = new Uint8Array(elevations.length);
  for (let row = 0; row <= allowed; row++) for (let col = 0; col <= allowed; col++) {
    const i = row * (allowed + 1) + col;
    if (!known[i]) continue;
    const lat = centerLat + (0.5 - row / allowed) * latSpan;
    const lon = centerLon + (col / allowed - 0.5) * lonSpan;
    const h = context.viewMode === "2D" ? 0 : elevations[i]! * exaggeration;
    try {
      const p = context.viewMode === "3D" ? lla_to_ecef(lat, lon, h) : context.project(lat, lon);
      const x = p[0], y = p[1], z = context.viewMode === "3D" ? p[2] : h;
      if (x === undefined || y === undefined || z === undefined || ![x, y, z].every(Number.isFinite)) continue;
      xyz.set([x, y, z], i * 3); valid[i] = 1;
    } catch { /* Invalid projection remains an unknown sample. */ }
  }
  let chosen = size, maxError: number | null = null, unknownSamples = known.some(value => value === 0) || valid.some(value => value === 0), thresholdSatisfied = false;
  let budgetLimited = allowed < maximum;
  for (let level = size; level <= allowed; level *= 2) {
    let levelError = 0, levelUnknown = false;
    let hasProbe = false;
    if (level < allowed) {
      const fineLevel = level * 2, stride = allowed / fineLevel, coarseStride = stride * 2;
      for (let row = 0; row <= fineLevel; row++) for (let col = 0; col <= fineLevel; col++) {
        if (row % 2 === 0 && col % 2 === 0) continue;
        const fi = row * stride * (allowed + 1) + col * stride;
        const cellRow = Math.min(Math.floor(row / 2), level - 1), cellCol = Math.min(Math.floor(col / 2), level - 1);
        const a = cellRow * coarseStride * (allowed + 1) + cellCol * coarseStride;
        const b = a + coarseStride * (allowed + 1);
        const localU = (col - cellCol * 2) / 2, localV = (row - cellRow * 2) / 2;
        const firstTriangle = localU + localV <= 1;
        const tri = firstTriangle ? [a, a + coarseStride, b] : [a + coarseStride, b + coarseStride, b];
        if (![fi, ...tri].every(i => valid[i] === 1)) { levelUnknown = true; continue; }
        hasProbe = true;
        const weights = firstTriangle ? [1 - localU - localV, localU, localV] : [1 - localV, localU + localV - 1, 1 - localU];
        let error2 = 0;
        for (let axis = 0; axis < 3; axis++) {
          const predicted = weights[0]! * xyz[tri[0]! * 3 + axis]! + weights[1]! * xyz[tri[1]! * 3 + axis]! + weights[2]! * xyz[tri[2]! * 3 + axis]!;
          const d = xyz[fi * 3 + axis]! - predicted; error2 += d * d;
        }
        levelError = Math.max(levelError, Math.sqrt(error2));
      }
    }
    if (hasProbe) maxError = maxError === null ? levelError : Math.max(maxError, levelError);
    unknownSamples ||= levelUnknown;
    if (hasProbe && levelError <= targetErrorMeters && !unknownSamples) { chosen = level; thresholdSatisfied = true; break; }
    chosen = Math.min(level * 2, allowed);
    if (level >= allowed / 2) { budgetLimited = true; break; }
  }
  const cachedContext: WmsMeshContext = { ...context, elevation: readElevation };
  const mesh = buildTerrainGrid(chosen, centerLat, centerLon, latSpan, lonSpan, cachedContext, exaggeration, uv);
  if (unknownSamples) {
    const stride = allowed / chosen, side = allowed + 1, outputSide = chosen + 1;
    const indices: number[] = [];
    for (let row = 0; row < chosen; row++) for (let col = 0; col < chosen; col++) {
      let cellKnown = true;
      for (let fineRow = row * stride; fineRow <= (row + 1) * stride && cellKnown; fineRow++) {
        for (let fineCol = col * stride; fineCol <= (col + 1) * stride; fineCol++) {
          const fineIndex = fineRow * side + fineCol;
          if (known[fineIndex] !== 1 || valid[fineIndex] !== 1) { cellKnown = false; break; }
        }
      }
      if (!cellKnown) continue;
      const a = row * outputSide + col, b = a + outputSide;
      indices.push(a, a + 1, b, a + 1, b + 1, b);
    }
    const knownSamples = known.reduce((count, value) => count + value, 0);
    const projectedSamples = valid.reduce((count, value) => count + value, 0);
    return { ...mesh, indices: new Uint32Array(indices), gridSize: chosen, triangleCount: indices.length / 3, sourceSamples: cache.size,
      knownSamples, projectedSamples, maxSampledErrorMeters: maxError, thresholdSatisfied: false, budgetLimited, unknownSamples };
  }
  return { ...mesh, gridSize: chosen, triangleCount: mesh.indices.length / 3, sourceSamples: cache.size,
    knownSamples: known.reduce((count, value) => count + value, 0),
    projectedSamples: valid.reduce((count, value) => count + value, 0),
    maxSampledErrorMeters: maxError, thresholdSatisfied, budgetLimited, unknownSamples };
}
