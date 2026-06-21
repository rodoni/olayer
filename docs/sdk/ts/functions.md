# Olayer TypeScript SDK Developer Reference Guide

This document provides a comprehensive API reference for the Olayer TypeScript SDK, describing all major classes, interfaces, and methods available to developers integrating the Olayer GIS ATC framework into their applications.

---

## Table of Contents
1. [Core Architecture & Initialization](#1-core-architecture--initialization)
2. [OlayerController](#2-olayercontroller)
3. [Layer Management (`Layer` and `LayerManager`)](#3-layer-management-layer-and-layermanager)
4. [Layer Implementations (`TileLayer` and `VectorTileLayer`)](#4-layer-implementations-tilelayer-and-vectortilelayer)
5. [Data Providers (`MapDataSource`, `MapDataStack`, and sources)](#5-data-providers-mapdatasource-mapdatastack-and-sources)
6. [Renderers (`WebGLRenderer`, `CPURenderer`, and `TextureAtlasManager`)](#6-renderers-webglrenderer-cpurenderer-and-textureatlasmanager)
7. [WASM Engine Bindings](#7-wasm-engine-bindings)

---

## 1. Core Architecture & Initialization

The Olayer TS SDK relies on an underlying WebAssembly module (`olayer-wasm`) for precise geodesic transformations and camera matrix calculations. Before using any SDK components, you must initialize the WebAssembly module.

### `init` (Default Export)
```typescript
import init from "olayer-sdk";

// Initialize the WebAssembly module
await init();
```

---

## 2. OlayerController

`OlayerController` is the orchestrator of the map viewport. It coordinates WebGL context, Canvas 2D contexts, active map projections, layer rendering loops, and mouse/touch interactions (zoom, panning, rotation, and tilt).

### Configuration Interface: `OlayerConfig`
Passed to the `OlayerController` constructor:
```typescript
export interface OlayerConfig {
  glCanvas: HTMLCanvasElement;              // Canvas element used for GPU WebGL2 rendering
  canvas2D: HTMLCanvasElement;              // Canvas element used for CPU overlays (labels, aircraft symbols)
  projection: WasmProjection;               // WebAssembly active projection instance
  initialCenterLatRad?: number;             // Initial camera latitude in radians (default: 0.0)
  initialCenterLonRad?: number;             // Initial camera longitude in radians (default: 0.0)
  initialZoom?: number;                     // Initial scale zoom level (default: 1.0)
  viewportBaseMeters?: number;              // Reference viewport width in meters (default: 100000.0)
}
```

### Class: `OlayerController`

#### Constructor
```typescript
constructor(config: OlayerConfig)
```

#### Properties
- `glCanvas: HTMLCanvasElement` - Reference to the WebGL canvas.
- `canvas2D: HTMLCanvasElement` - Reference to the 2D overlay canvas.
- `gl: WebGL2RenderingContext` - WebGL2 rendering context.
- `ctx2d: CanvasRenderingContext2D` - Canvas 2D context.
- `terrainEngine: WasmTerrainEngine` - Reference to the WASM terrain elevation lookup engine.
- `interpolator: WasmInterpolationEngine` - Reference to the WASM target state interpolation engine.
- `projection: WasmProjection` - Reference to the current WASM projection engine.
- `layerManager: LayerManager` - Manager governing map visualization layers.
- `dataManager: MapDataStack` - Unified interface managing raster, vector, and terrain data caches.
- `currentViewProjMatrix: Float32Array` - Flat 16-element view-projection matrix of the current frame.

#### Methods
- `startLoop(): void` - Starts the animation render loop (`requestAnimationFrame`). Frame rate automatically toggles between active interaction rendering (60 FPS) and idle rendering (15 FPS).
- `stopLoop(): void` - Stops the animation loop.
- `triggerActive(): void` - Temporarily forces the renderer into high-responsiveness mode (60 FPS) for visual smoothness. Called automatically on drag/wheel interactions.
- `destroy(): void` - Stops rendering loops and fully deallocates WebGL textures, buffers, and WASM memory allocations.
- `getFPS(): number` - Returns the actual current frame rate.

**Camera Setters & Getters:**
- `setCenter(latRad: number, lonRad: number): void` - Centers the map camera at coordinates (in radians).
- `getCenterLat(): number` / `getCenterLon(): number` / `getCenterHeight(): number` - Returns current camera target coordinate values.
- `setZoom(zoom: number): void` - Sets the camera zoom level.
- `getZoom(): number` - Gets the current camera zoom level.
- `setRotation(rotationRad: number): void` - Sets the camera bearing/heading in radians.
- `getRotation(): number` - Gets the current camera bearing/heading.
- `setPitch(pitchRad: number): void` - Sets camera vertical tilt (pitch) in radians. Range: `[-PI, PI]` (`-180°` to `180°`).
- `getPitch(): number` - Gets camera pitch.
- `setRoll(rollRad: number): void` - Sets camera roll in radians (supported in 2.5D and 3D).
- `getRoll(): number` - Gets camera roll.
- `getViewMode(): ViewMode` - Returns current mode: `"2D" | "2.5D" | "3D"`.
- `setViewMode(value: ViewMode): void` - Switches camera viewport mode. (e.g. switches to `2.5D` with standard `35°` pitch).
- `getIs3D(): boolean` / `setIs3D(value: boolean): void` - Helpers to toggle between flat 2D maps and the 3D globe.
- `getCameraState(): WasmCameraState` - Constructs a WASM camera struct holding center coordinates, zoom, rotation, pitch, roll, aspect ratio, and base dimensions.

---

## 3. Layer Management (`Layer` and `LayerManager`)

Olayer utilizes a stacked layer design to draw cartographic grids, raster basemaps, aviation boundaries, and t tactical radar indicators separately.

### Abstract Class: `Layer`
Base class for all rendering layers:
```typescript
export abstract class Layer {
  public id: string;
  public visible: boolean = true;
  public opacity: number = 1.0;

  constructor(id: string);

  // Invoked on camera updates to render GPU-dense visuals
  public abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  // Invoked at up to 60 FPS to draw target labels/plots
  public abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void;
}
```

### Class: `LayerManager`
Maintains the ordering and visibility of the layer hierarchy.

#### Methods
- `addLayer(layer: Layer): void` - Appends a layer to the rendering stack. Throws an error if a layer ID duplicate exists.
- `removeLayer(id: string): boolean` - Removes a layer by ID. Returns `true` if found and removed.
- `reorderLayer(id: string, newIndex: number): void` - Reorders a layer to a specific index in the array stack (defining its draw order/depth).
- `getLayers(): Layer[]` - Returns a shallow copy of the current layers stack.
- `renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void` - Iterates over layers, drawing visible static components.
- `renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void` - Iterates over layers, drawing visible dynamic overlays.

---

## 4. Layer Implementations (`TileLayer` and `VectorTileLayer`)

### Class: `TileLayer` (extends `Layer`)
Handles downloading, decoding, and rendering image tiles (WMTS, OpenStreetMap, or custom raster tiles) on WebGL2.

- **Constructor:**
  ```typescript
  constructor(id: string, rasterSource: RasterTileSource)
  ```
- **Rendering details:** Implements automated geometry subdivision, GPU cache management, vertical image axis alignment correction, and opacity blending.

### Class: `VectorTileLayer` (extends `Layer`)
Draws vector geometry elements (boundaries, airspaces, airways) on WebGL2.

- **Constructor:**
  ```typescript
  constructor(id: string, vectorSource: VectorTileSource)
  ```
- **Rendering details:** Converts lat/lon coordinate strings to WebGL vertex positions, compiling them into static line geometries on the GPU.

---

## 5. Data Providers (`MapDataSource`, `MapDataStack`, and sources)

Data providers handle background loading, caching (using Least-Recently Used eviction), and formatting map files.

### Interface: `MapDataSource`
```typescript
export interface MapDataSource {
  id: string;
  loadTile(x: number, y: number, z?: number): Promise<void>;
  unloadTile(x: number, y: number, z?: number): void;
  clearCache(): void;
}
```

### Class: `MapDataStack`
Stores and orchestrates multiple map data sources.

#### Methods
- `registerSource(source: MapDataSource): void` - Registers a source under its `id`.
- `getSource<T extends MapDataSource>(id: string): T | null` - Retrieves a source, casting it to its original class type.
- `clearCache(): void` - Clears caches of all registered data sources.
- `getCacheSize(): number` - Returns total loaded tiles across all sources.
- `destroy(): void` - Clears caches and removes registered sources.

---

### Class: `RasterTileSource` (implements `MapDataSource`)
Downloads image tiles and uploads them as WebGL textures.

- **Constructor:**
  ```typescript
  constructor(
    gl: WebGL2RenderingContext,
    urlResolver?: string | ((x: number, y: number, z: number) => string),
    maxTiles?: number // Default: 100
  )
  ```
- **Methods:**
  - `loadTile(x: number, y: number, z: number): Promise<void>`
  - `getTileTexture(x: number, y: number, z: number): WebGLTexture | null`
  - `unloadTile(x: number, y: number, z: number): void`
  - `clearCache(): void`
  - `getCacheSize(): number`

---

### Class: `VectorTileSource` (implements `MapDataSource`)
Downloads and parses vector tiles (MVT) or GeoJSON files representing geographical features.

- **Constructor:**
  ```typescript
  constructor(
    urlResolver?: string | ((x: number, y: number, z: number) => string),
    maxTiles?: number // Default: 100
  )
  ```
- **Methods:**
  - `loadTile(x: number, y: number, z: number): Promise<void>`
  - `getTileFeatures(x: number, y: number, z: number): VectorFeature[]`
  - `unloadTile(x: number, y: number, z: number): void`
  - `clearCache(): void`

**Interface `VectorFeature`**:
```typescript
export interface VectorFeature {
  type: "Point" | "LineString" | "Polygon";
  coordinates: number[][]; // Array of [lat_rad, lon_rad] points
  properties: Record<string, any>;
}
```

---

### Class: `TerrainTileSource` (implements `MapDataSource`)
Manages Digital Terrain Elevation Data (DTED) files, feeding them directly into the WebAssembly terrain engine.

- **Constructor:**
  ```typescript
  constructor(
    terrainEngine: WasmTerrainEngine,
    urlResolver?: string | ((lat: number, lon: number) => string),
    maxTiles?: number // Default: 9
  )
  ```
- **Methods:**
  - `loadTile(lat: number, lon: number): Promise<void>` - `z` parameter is ignored.
  - `injectTile(lat: number, lon: number, bytes: Uint8Array): void` - Directly injects a binary tile buffer.
  - `unloadTile(lat: number, lon: number): void`
  - `clearCache(): void`
  - `getCacheSize(): number`

---

## 6. Renderers (`WebGLRenderer`, `CPURenderer`, and `TextureAtlasManager`)

### Class: `TextureAtlasManager`
Constructs a unified, instanced texture atlas (spritesheet) on the GPU. It allows custom SVG and PNG icons to be registered and drawn in a single draw call.

- **Constructor:**
  ```typescript
  constructor(gl: WebGL2RenderingContext, atlasSize?: number) // Default size: 512x512
  ```
- **Methods:**
  - `registerSymbol(id: string, width: number, height: number, drawFn: (ctx: CanvasRenderingContext2D) => void): SymbolUV` - Rasterizes a custom shape drawn in `drawFn` and maps it to a unique UV sub-region of the atlas texture.
  - `getSymbolUV(id: string): SymbolUV | undefined` - Gets the coordinates of a symbol.
  - `getTexture(): WebGLTexture | null` - Gets the WebGL texture reference.
  - `destroy(): void` - Deletes the GPU texture.

**Interface `SymbolUV`**:
```typescript
export interface SymbolUV {
  u0: number; // Left coordinate (0.0 - 1.0)
  v0: number; // Top coordinate (0.0 - 1.0)
  u1: number; // Right coordinate (0.0 - 1.0)
  v1: number; // Bottom coordinate (0.0 - 1.0)
  width: number;  // Width in pixels
  height: number; // Height in pixels
}
```

---

### Class: `WebGLRenderer`
Draws static base graphics like coordinates grid overlays.

- **Constructor:**
  ```typescript
  constructor(gl: WebGL2RenderingContext)
  ```
- **Methods:**
  - `rebuildGrid(projection: WasmProjection | null, viewMode?: string): void` - Recalculates latitude and longitude grid lines.
  - `renderGrid(viewProjMatrix: Float32Array): void` - Renders grid lines onto WebGL.
  - `destroy(): void` - Deallocates GPU buffers and shader programs.

---

### Class: `CPURenderer`
Projects 3D coordinates and renders dynamic tactical overlays (aircraft icons, velocity vectors, radar sweeps, and data labels).

- **Constructor:**
  ```typescript
  constructor(ctx: CanvasRenderingContext2D)
  ```

- **Methods:**
  - `beginFrame(): void` - Clears the current list of occupied screen boxes to prepare for the anti-clutter algorithm.
  - `projectToScreen(...)` - Projects geodetic coordinate inputs (`latRad`, `lonRad`, `height`) to flat canvas coordinates `(x, y)` based on the active projection and camera state. Returns `null` if the coordinate is behind the camera near plane or obscured by the earth horizon (in 3D mode).
  - `drawTarget(...)` - Draws the target aircraft dot, its 1-minute predictive flight path line (vector), and its text labels using an intelligent anti-clutter layout.

**Interface `InterpolatedTarget`**:
```typescript
export interface InterpolatedTarget {
  id: string;
  position: {
    lat: number;
    lon: number;
    height: number;
  };
  heading_rad: number;
}
```

---

## 7. WASM Engine Bindings

Imported directly from `"olayer-wasm"`. Below are the primary classes and functions exposed to TypeScript:

### Class: `WasmProjection`
- `project(latRad: number, lonRad: number, altMeters: number): Float64Array` - Projects ellipsoidal lat/lon to map projection space coordinates `[x, y]`.
- `unproject(xPlanar: number, yPlanar: number): WasmLatLon` - Performs inverse projection back to WGS84 `LatLon`.
- `get_view_proj_matrix(camera: WasmCameraState): Float64Array` - Outputs flat 2D view-projection matrix.
- `get_25d_view_proj_matrix(camera: WasmCameraState): Float64Array` - Outputs flat 2.5D perspective view-projection matrix.
- `get_3d_view_proj_matrix(camera: WasmCameraState): Float64Array` - Outputs flat 3D globe view-projection matrix.

### Class: `WasmCameraState`
- `constructor(lat: number, lon: number, height: number, zoom: number, rotation: number, pitch: number, roll: number, aspect: number, baseMeters: number)` - Represents camera attitude configuration.

### Class: `WasmTerrainEngine`
- `load_tile(bytes: Uint8Array): void` - Injects DTED file bytes.
- `unload_tile(lat: number, lon: number): void` - Removes DTED file of a grid lat/lon sector.
- `get_elevation(latDeg: number, lonDeg: number): number` - Fast $O(1)$ ground height query in decimal degrees.
- `get_elevation_rad(latRad: number, lonRad: number): number` - Fast $O(1)$ ground height query in radians.
- `set_cache_capacity(capacity: number): void` - Sets the maximum number of cached DTED tiles.
- `cache_size(): number` - Returns the current number of cached tiles.
- `clear_cache(): void` - Clears all cached tiles.

### Class: `WasmInterpolationEngine`
- Interpolates the 3D position and orientation of dynamic targets over time using dead reckoning models.

### Global Functions:
- `lla_to_ecef(latRad: number, lonRad: number, altMeters: number): Float64Array` - Converts WGS84 coordinates to Earth-Centered, Earth-Fixed (ECEF) cartesian `[x, y, z]`.
