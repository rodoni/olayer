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
export { Layer, LayerManager } from "./layers";

// Export Data Managers and Providers
export { DataManager } from "./providers";

// Export Renderers and Texture Atlas
export { WebGLRenderer } from "./renderer/gpu";
export { CPURenderer } from "./renderer/cpu";
export type { InterpolatedTarget } from "./renderer/cpu";
export { TextureAtlasManager } from "./renderer/atlas";
export type { SymbolUV } from "./renderer/atlas";
