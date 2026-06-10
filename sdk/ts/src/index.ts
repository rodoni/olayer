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
} from "olayer-wasm";

// Export SDK Controller
export { OlayerController } from "./controller";
export type { OlayerConfig } from "./controller";

// Export SDK Layer System
export { Layer, LayerManager, TileLayer, VectorTileLayer } from "./layers";

// Export Data Managers and Providers
export { DataManager, TerrainTileSource } from "./providers";
export { RasterTileSource } from "./providers/raster";
export { VectorTileSource } from "./providers/vector";
export { MapDataStack } from "./providers/stack";
export type { MapDataSource } from "./providers/datasource";


// Export Renderers and Texture Atlas
export { WebGLRenderer } from "./renderer/gpu";
export { CPURenderer } from "./renderer/cpu";
export type { InterpolatedTarget } from "./renderer/cpu";
export { TextureAtlasManager } from "./renderer/atlas";
export type { SymbolUV } from "./renderer/atlas";
