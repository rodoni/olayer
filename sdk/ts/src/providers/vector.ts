import { VectorTile } from "@mapbox/vector-tile";
import { PbfReader } from "pbf";
import { MapDataSource, TileCacheStats, TileRequestOptions } from "./datasource";
import { retryTileRequest, TileCache, throwIfAborted } from "./tile_cache";

type Position = [number, number];

export type VectorGeometryType =
  | "Point"
  | "LineString"
  | "Polygon"
  | "MultiPoint"
  | "MultiLineString"
  | "MultiPolygon";

export type VectorCoordinates =
  | Position[]
  | Position[][]
  | Position[][][];

export interface VectorFeature {
  type: VectorGeometryType;
  /** Coordinates are [latitude_rad, longitude_rad]. */
  coordinates: VectorCoordinates;
  properties: Record<string, unknown>;
}

export interface VectorTileSourceOptions {
  /** Coordinate system used by GeoJSON responses. */
  geoJsonCrs?: "EPSG:900913" | "EPSG:4326";
  /** Optional MVT layer to decode. If omitted, all layers are decoded. */
  mvtLayer?: string;
  /** Number of retries for failed network/decode requests. */
  maxRetries?: number;
}

interface GeoJsonFeature {
  geometry?: {
    type?: string;
    coordinates?: unknown;
  } | null;
  properties?: Record<string, unknown> | null;
}

const WEB_MERCATOR_RADIUS = 6378137.0;

/** Converts a Web Mercator coordinate in metres to [lat, lon] radians. */
function webMercatorToLatLon([x, y]: Position): Position {
  const lon = x / WEB_MERCATOR_RADIUS;
  const lat = 2 * Math.atan(Math.exp(y / WEB_MERCATOR_RADIUS)) - Math.PI / 2;
  return [lat, lon];
}

/** Converts a GeoJSON coordinate in degrees to [lat, lon] radians. */
function degreesToLatLon([lon, lat]: Position): Position {
  return [lat * Math.PI / 180, lon * Math.PI / 180];
}

function transformCoordinateTree(
  coordinates: unknown,
  transform: (position: Position) => Position
): unknown {
  if (!Array.isArray(coordinates)) {
    throw new Error("Vector geometry coordinates must be an array");
  }

  if (coordinates.length === 0) return [];

  if (typeof coordinates[0] === "number") {
    if (coordinates.length < 2 || typeof coordinates[1] !== "number") {
      throw new Error("Vector position must contain numeric x and y values");
    }
    return transform([coordinates[0], coordinates[1]]);
  }

  return coordinates.map((child) => transformCoordinateTree(child, transform));
}

function normalizeGeoJsonFeature(
  feature: GeoJsonFeature,
  transform: (position: Position) => Position
): VectorFeature | null {
  const geometry = feature.geometry;
  if (!geometry || !geometry.type || geometry.coordinates === undefined) return null;

  const supportedTypes: VectorGeometryType[] = [
    "Point",
    "LineString",
    "Polygon",
    "MultiPoint",
    "MultiLineString",
    "MultiPolygon",
  ];
  if (!supportedTypes.includes(geometry.type as VectorGeometryType)) {
    throw new Error(`Unsupported vector geometry type: ${geometry.type}`);
  }

  const transformed = transformCoordinateTree(geometry.coordinates, transform);
  const coordinates = geometry.type === "Point"
    ? [transformed]
    : transformed;

  return {
    type: geometry.type as VectorGeometryType,
    coordinates: coordinates as VectorCoordinates,
    properties: feature.properties ?? {},
  };
}

function parseGeoJson(
  buffer: ArrayBuffer,
  transform: (position: Position) => Position
): VectorFeature[] {
  const text = new TextDecoder().decode(buffer);
  const parsed = JSON.parse(text) as { features?: GeoJsonFeature[] };
  if (!Array.isArray(parsed.features)) {
    throw new Error("GeoJSON response must be a FeatureCollection");
  }

  return parsed.features
    .map((feature) => normalizeGeoJsonFeature(feature, transform))
    .filter((feature): feature is VectorFeature => feature !== null);
}

function parseMvt(buffer: ArrayBuffer, x: number, y: number, z: number, layerName?: string): VectorFeature[] {
  const tile = new VectorTile(new PbfReader(new Uint8Array(buffer)));
  const features: VectorFeature[] = [];

  const layers = layerName
    ? tile.layers[layerName]
      ? [tile.layers[layerName]]
      : []
    : Object.values(tile.layers);
  if (layerName && layers.length === 0) {
    throw new Error(`MVT layer not found: ${layerName}`);
  }

  for (const layer of layers) {
    for (let index = 0; index < layer.length; index++) {
      const geoJson = layer.feature(index).toGeoJSON(x, y, z) as GeoJsonFeature;
      const feature = normalizeGeoJsonFeature(geoJson, degreesToLatLon);
      if (feature) features.push(feature);
    }
  }

  return features;
}

/** Loads GeoJSON or Mapbox Vector Tiles into a bounded in-memory cache. */
export class VectorTileSource implements MapDataSource {
  public readonly id: string = "geoserver_mvt";
  private tileCache: TileCache<VectorFeature[]>;
  private readonly loadingTiles = new Map<string, Promise<void>>();
  private readonly invalidated = new Set<string>();
  private generation = 0;
  private urlResolver: string | ((x: number, y: number, z: number) => string);
  private maxTiles: number;
  private geoJsonCrs: "EPSG:900913" | "EPSG:4326";
  private mvtLayer?: string;
  private maxRetries: number;

  constructor(
    urlResolver: string | ((x: number, y: number, z: number) => string) = "",
    maxTiles: number = 100,
    options: VectorTileSourceOptions = {}
  ) {
    if (!Number.isInteger(maxTiles) || maxTiles <= 0) {
      throw new Error("Vector tile cache capacity must be a positive integer");
    }
    this.urlResolver = urlResolver;
    this.maxTiles = maxTiles;
    this.tileCache = new TileCache(maxTiles, () => {}, (features) => JSON.stringify(features).length);
    this.geoJsonCrs = options.geoJsonCrs ?? "EPSG:900913";
    this.mvtLayer = options.mvtLayer;
    this.maxRetries = options.maxRetries ?? 2;
    if (!Number.isInteger(this.maxRetries) || this.maxRetries < 0) {
      throw new Error("Vector tile maxRetries must be a non-negative integer");
    }
  }

  public async loadTile(x: number, y: number, z: number, options: TileRequestOptions = {}): Promise<void> {
    const key = `${z}/${x}/${y}`;
    if (this.tileCache.get(key)) return;
    const existing = this.loadingTiles.get(key);
    if (existing) return existing;
    this.invalidated.delete(key);
    if (!this.urlResolver) {
      throw new Error("A vector tile URL resolver is required");
    }

    const url = typeof this.urlResolver === "function"
      ? this.urlResolver(x, y, z)
      : this.urlResolver
        .replace("{x}", x.toString())
        .replace("{y}", y.toString())
        .replace("{z}", z.toString());

    const requestGeneration = this.generation;
    const request = (async () => {
      try {
      const response = await retryTileRequest(
        () => fetch(url, { signal: options.signal }),
        options.maxRetries ?? this.maxRetries,
        options.signal,
      );
      if (!response.ok) {
        throw new Error(`Failed to fetch vector tile: HTTP ${response.status}`);
      }

       const buffer = await response.arrayBuffer();
       throwIfAborted(options.signal);
      let features: VectorFeature[];
      try {
        const geoJsonTransform = this.geoJsonCrs === "EPSG:4326"
          ? degreesToLatLon
          : webMercatorToLatLon;
        features = parseGeoJson(buffer, geoJsonTransform);
      } catch (geoJsonError) {
        try {
          features = parseMvt(buffer, x, y, z, this.mvtLayer);
        } catch (mvtError) {
          throw new Error(
            `Unable to decode vector tile as GeoJSON or MVT: ${String(mvtError)}`,
            { cause: geoJsonError }
          );
        }
      }

        if (requestGeneration === this.generation && !this.invalidated.has(key)) {
          this.tileCache.set(key, features);
        }
      } finally {
        this.loadingTiles.delete(key);
      }
    })();
    this.loadingTiles.set(key, request);
    return request;
  }

  public getTileFeatures(x: number, y: number, z: number): VectorFeature[] {
    return this.tileCache.get(`${z}/${x}/${y}`) ?? [];
  }

  public unloadTile(x: number, y: number, z: number): void {
    const key = `${z}/${x}/${y}`;
    this.invalidated.add(key);
    this.tileCache.delete(key);
  }

  public clearCache(): void {
    this.generation++;
    this.invalidated.clear();
    this.tileCache.clear();
  }

  public getCacheSize(): number {
    return this.tileCache.size();
  }

  public getCacheStats(): TileCacheStats {
    return this.tileCache.stats();
  }
}

export default VectorTileSource;
