import init from "olayer-wasm";
export default init;

// Export WASM Bindings for easy access
export {
  WasmTerrainEngine,
  WasmInterpolationEngine,
  WasmProjection,
  WasmCameraState,
  WasmProjectionType,
  WasmLatLon,
  WasmTileKey,
  WasmStyleRegistry,
  WasmSymbolRegistry,
  WasmAeronauticalDataset,
  parse_aixm_51,
  parse_geojson_aviation,
  decode_mapbox_rgb,
  decode_terrarium_rgb,
  decode_rgb_elevation,
} from "olayer-wasm";

// Export SDK Controller
export { OlayerController } from "./controller";
export type { OlayerConfig, OlayerMetrics } from "./controller";

// Export SDK Layer System
export { Layer, LayerManager, TileLayer, VectorTileLayer, AeronauticalLayer } from "./layers";
export type { AeronauticalLayerOptions } from "./layers";

// Export Data Managers and Providers
export { DataManager, TerrainTileSource, RgbTerrainSource, CogTerrainSource } from "./providers";
export type { RgbTerrainEncoding, RgbTerrainSourceOptions } from "./providers";
export { RasterTileSource } from "./providers/raster";
export { VectorTileSource } from "./providers/vector";
export type { VectorFeature, VectorGeometryType, VectorCoordinates, VectorTileSourceOptions } from "./providers/vector";
export { MapDataStack } from "./providers/stack";
export type { MapDataSource, TileCacheStats, TileRequestOptions } from "./providers/datasource";


// Export Renderers and Texture Atlas
export { WebGLRenderer } from "./renderer/gpu";
export { CPURenderer } from "./renderer/cpu";
export type { InterpolatedTarget } from "./renderer/cpu";
export { TextureAtlasManager } from "./renderer/atlas";
export type { SymbolUV } from "./renderer/atlas";

// Export Tactical Aeronautical Measurement Tools (GIS-PROP-003)
export { TacticalToolsManager, SnailTrailTracker } from "./tools";
export type {
  LatLonCoords,
  RblMeasurement,
  PplTick,
  PplLeader,
  TurnDirection,
  HoldingPatternConfig,
  IlsConeConfig,
  IlsGeometry,
  RangeRingsConfig,
  HistoryDot,
} from "./tools";

