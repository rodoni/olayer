# TypeScript SDK API

## `OlayerController`

```typescript
interface OlayerConfig {
  glCanvas: HTMLCanvasElement;
  canvas2D: HTMLCanvasElement;
  projection: WasmProjection;
  initialCenterLatRad?: number;
  initialCenterLonRad?: number;
  initialZoom?: number;
  viewportBaseMeters?: number;
  altitudeMode?: "absolute" | "clamp-to-ground" | "relative-to-ground" | "relative-to-mesh";
  altitudeUnknownPolicy?: "reject" | "use-absolute" | "use-zero";
}

class OlayerController {
  // Public readonly
  readonly glCanvas: HTMLCanvasElement
  readonly canvas2D: HTMLCanvasElement
  readonly gl: WebGL2RenderingContext
  readonly ctx2d: CanvasRenderingContext2D
  readonly terrainEngine: WasmTerrainEngine
  readonly interpolator: WasmInterpolationEngine
  readonly projection: WasmProjection
  readonly symbolRegistry: WasmSymbolRegistry
  readonly atlasManager: TextureAtlasManager
  readonly layerManager: LayerManager
  readonly dataManager: MapDataStack
  currentViewProjMatrix: Float32Array

  constructor(config: OlayerConfig)

  // FPS throttling
  triggerActive(): void
  getFPS(): number

  // Camera control (radians)
  setCenter(latRad: number, lonRad: number): void
  setZoom(zoom: number): void
  setRotation(rotationRad: number): void
  getCenterLat(): number; getCenterLon(): number; getCenterHeight(): number
  getZoom(): number; getRotation(): number
  getPitch(): number; setPitch(pitchRad: number): void
  getRoll(): number; setRoll(rollRad: number): void
  getViewportBaseMeters(): number
  getCameraState(): WasmCameraState

  // View mode
  getViewMode(): "2D" | "2.5D" | "3D"
  setViewMode(value: "2D" | "2.5D" | "3D"): void
  getIs3D(): boolean
  setIs3D(value: boolean): void
  getAltitudeMode(): "absolute" | "clamp-to-ground" | "relative-to-ground" | "relative-to-mesh"
  setAltitudeMode(mode: "absolute" | "clamp-to-ground" | "relative-to-ground" | "relative-to-mesh"): void
  getAltitudeUnknownPolicy(): "reject" | "use-absolute" | "use-zero"
  setAltitudeUnknownPolicy(policy: "reject" | "use-absolute" | "use-zero"): void
  resolveAltitude(latRad: number, lonRad: number, inputHeightMeters: number, mode?: string, policy?: string, meshHeightMeters?: number): number

  // Lifecycle
  startLoop(): void
  stopLoop(): void
  destroy(): void      // frees all WASM + WebGL resources
}
```

The constructor automatically registers a default `TerrainTileSource` in `dataManager`.

## Layers

```typescript
abstract class Layer {
  id: string
  visible: boolean
  opacity: number
  constructor(id: string)
  abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class LayerManager {
  addLayer(layer: Layer): void          // throws on duplicate id
  removeLayer(id: string): boolean
  reorderLayer(id: string, newIndex: number): void   // throws if not found or out of bounds
  getLayers(): Layer[]                  // copy of internal array
  renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void
  renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void
}

class TileLayer extends Layer {
  constructor(id: string, options?: { opacity?: number; minZoom?: number; maxZoom?: number })
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class TerrainLayer extends Layer {
  constructor(id: string, gridSize?: number)
  setRenderMode(mode: "hypsometric" | "hillshade" | "slope" | "hybrid" | "textured"): void
  getRenderMode(): "hypsometric" | "hillshade" | "slope" | "hybrid" | "textured"
  setElevationRange(minElevation: number, maxElevation: number): void
  setHillshade(options: { azimuthDeg?: number; altitudeDeg?: number }): void
  setImageryTemplate(template: string | null): void
  setImageryEnabled(enabled: boolean): void
  setVerticalExaggeration(value: number): void
  getVerticalExaggeration(): number
  invalidateMesh(): void
}

class TerrainContourLayer extends Layer {
  constructor(id: string)
  setInterval(meters: number): void
  setVerticalExaggeration(value: number): void
  invalidate(): void
}

class VectorTileLayer extends Layer {
  constructor(id: string, options?: { opacity?: number; minZoom?: number; maxZoom?: number; filter?: (feature: VectorFeature) => boolean })
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class AeronauticalLayer extends Layer {
  constructor(id: string, options?: { dataset?: WasmAeronauticalDataset; showAirspaces?: boolean; showNavaids?: boolean; showAirways?: boolean; showAirports?: boolean })
  setDataset(dataset: WasmAeronauticalDataset): void
  getDataset(): WasmAeronauticalDataset | undefined
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class WeatherRadarLayer extends Layer {
  constructor(id: string, options?: { palette?: "Nexrad15" | "IcaoDbz"; opacity?: number; textureUnit?: number })
  updateGrid(grid: Float32Array, width: number, height: number): void
  setPalette(palette: "Nexrad15" | "IcaoDbz"): void
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class WindBarbsLayer extends Layer {
  constructor(id: string, options?: { color?: string; staffLengthPx?: number; southernHemisphere?: boolean })
  setStations(stations: Array<{ id: string; latRad: number; lonRad: number; speedKts: number; dirRad: number }>): void
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class SigmetLayer extends Layer {
  constructor(id: string, options?: { dataset?: WasmSigmetDataset; fillAlpha?: number; strokeWidth?: number })
  setDataset(dataset: WasmSigmetDataset): void
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class VolumetricAirspaceLayer extends Layer {
  constructor(id: string, options?: { baseColor?: string; edgeColor?: string; fresnelIntensity?: number; altitudeMode?: AltitudeMode; altitudeResolver?: AltitudeResolver })
  addAirspace(id: string, polygonFlatDeg: number[], floorM: number, ceilingM: number): void
  removeAirspace(id: string): boolean
  getAirspaceMesh(id: string): AirspaceMeshRecord | undefined
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

class TrajectoryRibbonLayer extends Layer {
  constructor(id: string, options?: { defaultRibbonWidthMeters?: number; colorLow?: string; colorHigh?: string; altitudeMode?: AltitudeMode; altitudeResolver?: AltitudeResolver })
  addTrajectory(id: string, waypointsFlatDeg: number[], ribbonWidthM?: number, scalars?: number[]): void
  removeTrajectory(id: string): boolean
  getRibbonMesh(id: string): TrajectoryRibbonRecord | undefined
  renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array, context?: LayerRenderContext): void
  renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number, context?: LayerRenderContext): void
}

type AltitudeMode = "absolute" | "clamp-to-ground" | "relative-to-ground" | "relative-to-mesh"
type AltitudeResolver = (latRad: number, lonRad: number, inputHeightMeters: number, mode: AltitudeMode) => number
```

## Data Sources

```typescript
type TileRequestOptions = { signal?: AbortSignal; maxRetries?: number }
type TileCacheStats = { items: number; bytes: number; hits: number; misses: number; evictions: number }

interface MapDataSource {
  id: string
  loadTile(x: number, y: number, z?: number, options?: TileRequestOptions): Promise<void>
  unloadTile(x: number, y: number, z?: number): void
  clearCache(): void
  getCacheStats?(): TileCacheStats
}

class TerrainTileSource implements MapDataSource {
  id: "terrain_dted"
  constructor(engine: WasmTerrainEngine, urlResolver?: string | ((lat: number, lon: number) => string), maxTiles?: number)
  loadTile(lat: number, lon: number, _unused?: number, options?: TileRequestOptions): Promise<void>
  unloadTile(lat: number, lon: number, _unused?: number): void
  injectTile(lat: number, lon: number, bytes: Uint8Array): void
  clearCache(): void
  getCacheSize(): number
  getCacheStats?(): TileCacheStats
}
// Alias: export { TerrainTileSource as DataManager }

type RgbTerrainEncoding = "Mapbox" | "Terrarium"
interface RgbTerrainSourceOptions {
  encoding?: RgbTerrainEncoding
  tileSize?: number
  maxTiles?: number
  maxRetries?: number
}

class RgbTerrainSource implements MapDataSource {
  id: string
  readonly encoding: RgbTerrainEncoding
  readonly tileSize: number
  constructor(id: string, engine: WasmTerrainEngine, urlTemplate: string | ((z: number, x: number, y: number) => string), options?: RgbTerrainSourceOptions)
  loadTile(x: number, y: number, z?: number, options?: TileRequestOptions): Promise<void>
  unloadTile(x: number, y: number, z?: number): void
  injectRgbaTile(z: number, x: number, y: number, rgbaBytes: Uint8Array, width?: number, height?: number): void
  clearCache(): void
  getElevationAt(latDeg: number, lonDeg: number): number | null
  getCacheStats(): TileCacheStats
}

class CogTerrainSource implements MapDataSource {
  id: string
  constructor(id: string, engine: WasmTerrainEngine, url?: string)
  loadTile(x?: number, y?: number, z?: number, options?: TileRequestOptions): Promise<void>
  injectGeoTiff(data: Uint8Array): number[] // returns bounding box [minLat, minLon, maxLat, maxLon]
  unloadTile(x?: number, y?: number, z?: number): void
  clearCache(): void
  getElevationAt(latDeg: number, lonDeg: number): number | null
  getCacheStats(): TileCacheStats
}

class RasterTileSource implements MapDataSource {
  id: "wmts_raster"
  constructor(gl: WebGL2RenderingContext, urlResolver?: string | ((x: number, y: number, z: number) => string), maxTiles?: number)
  loadTile(x: number, y: number, z: number, options?: TileRequestOptions): Promise<void>
  unloadTile(x: number, y: number, z: number): void
  getTileTexture(x: number, y: number, z: number): WebGLTexture | null
  clearCache(): void
  getCacheSize(): number
  getCacheStats(): TileCacheStats
}

interface VectorFeature {
  type: "Point" | "LineString" | "Polygon"
  coordinates: number[][]   // [lat_rad, lon_rad] arrays
  properties: Record<string, any>
}

class VectorTileSource implements MapDataSource {
  id: "geoserver_mvt"
  constructor(
    urlResolver?: string | ((x: number, y: number, z: number) => string),
    maxTiles?: number,
     options?: {
       geoJsonCrs?: "EPSG:900913" | "EPSG:4326",
       mvtLayer?: string,
       maxRetries?: number
     }
  )
  loadTile(x: number, y: number, z: number, options?: TileRequestOptions): Promise<void>
  unloadTile(x: number, y: number, z: number): void
  getTileFeatures(x: number, y: number, z: number): VectorFeature[]
  clearCache(): void
   getCacheSize(): number
   getCacheStats(): TileCacheStats
}

`loadTile` optionally accepts `{ signal?: AbortSignal, maxRetries?: number }`.

class MapDataStack {
  registerSource(source: MapDataSource): void
  getSource<T extends MapDataSource>(id: string): T | null
  clearCache(): void
  getCacheSize(): number
   getCacheStats(): TileCacheStats
  destroy(): void
}
```

`VectorTileSource` decodes binary Mapbox Vector Tiles (MVT/PBF) and GeoJSON
FeatureCollections. MVT coordinates are converted from tile space to
`[latitude_rad, longitude_rad]`. GeoJSON defaults to GeoServer Web Mercator
(`EPSG:900913`) metres; pass `geoJsonCrs: "EPSG:4326"` for longitude/latitude
degrees. Decode and HTTP failures reject `loadTile` and do not insert fallback
features into the cache. `mvtLayer` restricts binary decoding to one named MVT
layer; when omitted, all layers are decoded.

**Resolver templates:**
- Raster/vector: `{x}`, `{y}`, `{z}`
- Terrain string resolver: `{lat}`, `{lon}`, `{latStr}`, `{lonStr}` (latStr/lonStr are DTED-style `DD0000N`/`DDD0000E` strings)
- A function resolver receives `(lat, lon)` for terrain and `(x, y, z)` for raster/vector.

## Renderers

`WebGLRenderer` manages the static grid geometry; `CPURenderer` draws dynamic overlays (targets, labels) on the Canvas 2D layer. Both use the same `InterpolatedTarget` interface returned by the WASM interpolator.

```typescript
interface InterpolatedTarget {
  id: string
  position: { lat: number; lon: number; height: number }  // radians, metres
  heading_rad: number
}

class WebGLRenderer {
  constructor(gl: WebGL2RenderingContext)
  rebuildGrid(projection: WasmProjection | null, viewMode: string = "2D"): void
  renderGrid(viewProjMatrix: Float32Array): void
  destroy(): void
}

class CPURenderer {
  constructor(ctx: CanvasRenderingContext2D)
  beginFrame(): void
  projectToScreen(
    projection: WasmProjection,
    latRad: number, lonRad: number, height: number,
    cx: number, cy: number, zoom: number, rotation: number,
    viewportBaseMeters: number, canvasWidth: number, canvasHeight: number,
    viewMode: string = "2D",
    viewProjMatrix?: Float32Array,
    centerLat?: number, centerLon?: number
  ): { x: number, y: number } | null
  drawTarget(
    target: InterpolatedTarget,
    screenPos: { x: number; y: number },
    projection: WasmProjection,
    cx: number, cy: number, zoom: number, rotation: number,
    viewportBaseMeters: number, canvasWidth: number, canvasHeight: number,
    speedMps: number,
    atlasTexture: HTMLImageElement | HTMLCanvasElement | null,
    symbolUv: SymbolUV | undefined,
    viewMode: string = "2D",
    viewProjMatrix?: Float32Array,
    centerLat?: number, centerLon?: number
  ): void
}

class TextureAtlasManager {
  constructor(gl: WebGL2RenderingContext, size?: number)  // default 512²
  registerSymbol(id: string, width: number, height: number, drawFn: (ctx: CanvasRenderingContext2D) => void): SymbolUV
  registerWasmSymbol(id: string, registry: any, style: any): SymbolUV
  registerImageSymbol(id: string, src: string | HTMLImageElement, width?: number, height?: number): Promise<SymbolUV>
  getSymbolUV(id: string): SymbolUV | undefined
  getTexture(): WebGLTexture | null
  destroy(): void
}

interface SymbolUV { u0: number, v0: number, u1: number, v1: number, width: number, height: number }
```

## Tactical Aeronautical Measurement Tools (`olayer-sdk/tools`)

```typescript
interface LatLonCoords { lat: number; lon: number; height?: number }
interface RblMeasurement { distanceMeters: number; distanceNm: number; initialBearingRad: number; initialBearingDeg: number; finalBearingRad: number; finalBearingDeg: number }
interface PplTick { lat: number; lon: number; timeSec: number }
interface PplLeader { endLat: number; endLon: number; ticks: PplTick[] }
type TurnDirection = "Right" | "Left"
interface HoldingPatternConfig { inboundBearingRad: number; inboundLegTimeSec: number; turnDirection: TurnDirection; speedMps: number; standardRateDps?: number }
interface IlsConeConfig { runwayHeadingRad: number; glideSlopeAngleRad?: number; localizerHalfAngleRad?: number; rangeMeters?: number }
interface IlsGeometry { centerlineEnd: LatLonCoords; leftSectorBoundary: LatLonCoords; rightSectorBoundary: LatLonCoords; dmeRings: Array<{ distanceMeters: number; altitudeMeters: number }> }
interface RangeRingsConfig { center: LatLonCoords; radiiMeters: number[]; intervalNm?: number; count?: number }

class TacticalToolsManager {
  static measureRbl(p1: LatLonCoords, p2: LatLonCoords): RblMeasurement
  static generatePplLeader(position: LatLonCoords, speedMps: number, headingRad: number, lookaheadMinutes?: number, tickIntervalMinutes?: number): PplLeader
  static generateHoldingPattern(fix: LatLonCoords, config: HoldingPatternConfig, numPointsPerTurn?: number): LatLonCoords[]
  static generateIlsApproach(threshold: LatLonCoords, config: IlsConeConfig): IlsGeometry
  static generateRangeRings(config: RangeRingsConfig, numPointsPerRing?: number): LatLonCoords[][]
  static generateCompassRose(center: LatLonCoords, radiusMeters: number, tickLengthMeters?: number): Array<{ start: LatLonCoords; end: LatLonCoords; bearingDeg: number; isMajor: boolean }>
}

class SnailTrailTracker {
  constructor(maxPoints?: number, decayDurationSec?: number)
  recordPosition(coords: LatLonCoords, timestamp?: number): void
  getActiveTrail(currentTimestamp?: number): HistoryDot[]
  clear(): void
}
```

## Label Anti-Cluttering Engine (`olayer-sdk/renderer`)

```typescript
enum OctantDirection { North = 0, NorthEast = 1, East = 2, SouthEast = 3, South = 4, SouthWest = 5, West = 6, NorthWest = 7 }

interface DeclutterTargetInput {
  id: string;
  x: number;
  y: number;
  headingRad?: number;
  width: number;
  height: number;
  priority?: number;
}

interface SolvedLabelPlacement {
  id: string;
  rect: { x: number; y: number; width: number; height: number };
  leaderStart: [number, number];
  leaderEnd: [number, number];
  octant: OctantDirection;
  cost: number;
}

class LabelAntiClutterEngine {
  defaultLeaderLengthPx: number;
  defaultSafetyMarginPx: number;
  constructor(leaderLengthPx?: number, safetyMarginPx?: number);
  solve(targets: DeclutterTargetInput[], leaderLengthPx?: number, safetyMarginPx?: number): SolvedLabelPlacement[];
}
```

## OGC map sources and layers

The [public exports](../../sdk/ts/src/index.ts) include `OgcHttpClient`, OGC builders, `WmsMapSource`, `WfsVectorSource`, `WcsRasterSource`, `WcsTerrainSource`, `WmsLayer` and `WfsLayer`. Use the [map guides](../guides/maps/README.md) for complete request/options/lifecycle contracts. WFS supports opt-in fills/holes, separate fill color, explicit altitude modes and cached triangulation.

`LayerRenderContext` supplies the controller explicitly. Layers own their GPU resources and implement `destroy(gl)`; `LayerManager.removeLayer` removes a reference but does not by itself destroy it. Controller teardown destroys active layers.
