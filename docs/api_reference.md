# API Reference

This is a pure technical reference for Olayer. For tutorials, how-to guides, and
architectural explanation, see the other documents in `docs/`.

---

## 1. Core Rust Engine (`olayer-core`)

All angles are in **radians** unless noted otherwise. Height/altitude is in **metres**
above the WGS84 ellipsoid.

### 1.1 `geodesy` — Geodetic Mathematics

Sub-modules: `conversions`, `coords`, `ellipsoid`, `errors`, `math`, `solvers`. Public items are re-exported from `olayer_core::geodesy`.

#### Structs

| Struct | Fields | Notes |
|--------|--------|-------|
| `LatLon` | `pub lat: f64`, `pub lon: f64`, `pub height: f64` | Radians; `Copy`, `Serialize`/`Deserialize` |
| `Ecef` | `pub x: f64`, `pub y: f64`, `pub z: f64` | Metres; `Copy` |
| `Enu` | `pub east: f64`, `pub north: f64`, `pub up: f64` | Metres; `Copy` |
| `Ellipsoid` | `pub a: f64`, `pub b: f64`, `pub f: f64`, `pub e_sq: f64`, `pub e_prime_sq: f64`, `pub authalic_radius: f64` | `Copy` |
| `GeodeticResult` | `pub distance: f64`, `pub initial_bearing: f64`, `pub final_bearing: f64` | Bearings in radians; `Copy` |

#### `impl LatLon`

```rust
pub const fn new(lat_rad: f64, lon_rad: f64, height_meters: f64) -> Self
pub fn from_degrees(lat_deg: f64, lon_deg: f64, height_meters: f64) -> Self
pub fn to_degrees(&self) -> (f64, f64, f64)
pub fn validate(&self) -> Result<(), GeodesyError>
```

#### `impl Ecef`

```rust
pub const fn new(x: f64, y: f64, z: f64) -> Self
pub fn chord_distance(&self, other: &Self) -> f64
```

#### `impl Enu`

```rust
pub const fn new(east: f64, north: f64, up: f64) -> Self
pub fn distance_2d(&self) -> f64
pub fn distance_3d(&self) -> f64
```

#### `impl Ellipsoid`

```rust
pub const fn new(a: f64, f: f64) -> Self
pub const fn wgs84() -> Self
pub fn radius_of_curvature_prime_vertical(&self, lat_rad: f64) -> f64
```

#### `impl GeodeticResult`

```rust
pub const fn new(distance: f64, initial_bearing: f64, final_bearing: f64) -> Self
```

#### Free Conversion Functions

```rust
pub fn lla_to_ecef(lla: &LatLon, ellipsoid: &Ellipsoid) -> Ecef
pub fn ecef_to_lla(ecef: &Ecef, ellipsoid: &Ellipsoid) -> LatLon
pub fn ecef_to_enu(ecef: &Ecef, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu
pub fn enu_to_ecef(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> Ecef
pub fn lla_to_enu(lla: &LatLon, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu
pub fn enu_to_lla(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> LatLon
```

#### Free Math Utilities

```rust
pub fn normalize_bearing(rad: f64) -> f64     // → [0, 2π)
pub fn normalize_longitude(rad: f64) -> f64    // → [-π, π)
```

#### `trait GeodeticSolver`

```rust
pub trait GeodeticSolver {
    const IS_ELLIPSOIDAL: bool;
    const EXPECTED_ACCURACY_METERS: f64;
    fn inverse(&self, p1: &LatLon, p2: &LatLon, ellipsoid: &Ellipsoid) -> Result<GeodeticResult, GeodesyError>;
    fn direct(&self, p1: &LatLon, bearing_rad: f64, distance_meters: f64, ellipsoid: &Ellipsoid) -> Result<LatLon, GeodesyError>;
}
```

#### Solvers

| Struct | Trait | Accuracy | Fallback |
|--------|-------|----------|----------|
| `HaversineSolver` | `GeodeticSolver` | ~1 m (spherical) | — |
| `VincentySolver` | `GeodeticSolver` | ~1 mm (ellipsoidal) | Falls back to Haversine on non-convergence |

Both implement `Default`.

#### Error

```rust
pub enum GeodesyError {
    LatitudeOutOfRange(f64),
    LongitudeOutOfRange(f64),
}
```

`impl Error` + `Display`.

---

### 1.2 `camera` — Camera State & VP Matrices

The `camera` module contains the public `CameraState` struct. It is also re-exported from `olayer_core::projections` for convenience.

#### Struct

```rust
pub struct CameraState {
    pub center: LatLon,
    pub zoom: f64,                  // linear scale
    pub rotation: f64,              // yaw/bearing, radians
    pub pitch: f64,                 // tilt, radians (nadir = 0)
    pub roll: f64,                  // lateral roll, radians
    pub aspect_ratio: f64,          // width / height
    pub viewport_base_meters: f64,  // base viewport extent in metres
}
```

`Copy`.

#### `impl CameraState`

```rust
pub const fn new(center: LatLon, zoom: f64, rotation: f64, aspect_ratio: f64, viewport_base_meters: f64) -> Self
pub const fn with_attitude(center: LatLon, zoom: f64, rotation: f64, pitch: f64, roll: f64, aspect_ratio: f64, viewport_base_meters: f64) -> Self
pub fn validate(&self) -> Result<(), CameraError>
pub fn get_2d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError>
pub fn get_25d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError>
pub fn get_3d_view_proj_matrix(&self) -> Result<[f32; 16], CameraError>
```

#### Error

```rust
pub enum CameraError {
    InvalidZoom,
    InvalidAspectRatio,
    InvalidViewportBase,
    Projection(ProjectionError),
}
```

`impl From<ProjectionError>`.

---

### 1.3 `projections` — Cartographic Projections

Modules: `lambert_conformal_conic`, `stereographic`, `web_mercator`, `matrix`, `errors`.
Public re-exports from `olayer_core::projections`: `Projection`, `CameraState`, `LambertConformalConic`, `Stereographic`, `WebMercator`, `ProjectionError`, `Matrix4`.

#### `trait Projection`

```rust
pub trait Projection {
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError>;
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError>;
    fn update_center(&mut self, center_lat_rad: f64, center_lon_rad: f64);
    fn get_view_proj_matrix(&self, camera: &CameraState) -> Result<[f32; 16], ProjectionError>;
}
```

`get_view_proj_matrix` has a default implementation (2D ortho). `update_center` defaults to no-op.

#### Projection Types

| Struct | Constructor | Behaviour |
|--------|-------------|-----------|
| `Stereographic` | `new(center_lat, center_lon, ellipsoid)` | Azimuthal; antipode → `Singularity` |
| `LambertConformalConic` | `new(sp1, sp2, origin_lat, origin_lon, ellipsoid)` | Conic; clamps lat to ±89.9° |
| `WebMercator` | `new(ellipsoid)` + `Default` | Cylindrical; clamps lat to ±85.05° |

All implement `Projection`. Stereographic and LCC implement `update_center`.

#### Error

```rust
pub enum ProjectionError {
    InvalidCameraState,
    Singularity,
    ConvergenceFailed,
}
```

#### `Matrix4`

Column-major `[f32; 16]`. `Copy`, `Default` (identity).

```rust
impl Matrix4 {
    pub fn identity() -> Self
    pub fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> Self
    pub fn translation(tx: f32, ty: f32, tz: f32) -> Self
    pub fn rotation_z(angle_rad: f32) -> Self
    pub fn rotation_x(angle_rad: f32) -> Self
    pub fn rotation_y(angle_rad: f32) -> Self
    pub fn perspective(fovy_rad: f32, aspect: f32, near: f32, far: f32) -> Self
    pub const fn as_slice(&self) -> &[f32; 16]
    pub fn as_mut_slice(&mut self) -> &mut [f32; 16]
    pub const fn into_array(self) -> [f32; 16]
    pub fn multiply(&self, other: &Self) -> Self
}
```

Implements `Mul<&Matrix4>` and `Mul<Matrix4>` for all ref/owned combinations.

---

### 1.4 `terrain` — DTED Elevation Engine

Sub-modules: `errors`, `tile`, `engine`. Public items are re-exported from `olayer_core::terrain`.

#### Structs

```rust
pub struct TileKey { pub lat_deg: i32, pub lon_deg: i32 }  // Copy, Eq, Hash

pub struct DtedTile {
    pub origin_lat: i32,       pub origin_lon: i32,
    pub num_rows: usize,        pub num_cols: usize,
    pub lat_spacing_arcsec: u32,pub lon_spacing_arcsec: u32,
    pub elevations: Vec<i16>,
}

pub struct ProfilePoint {
    pub distance_meters: f64,
    pub ground_elevation: f64,
    pub coords: LatLon,
}
```

#### `impl DtedTile`

```rust
pub fn from_bytes(data: &[u8]) -> Result<Self, TerrainError>
pub fn get_cell_elevation(&self, row: usize, col: usize) -> i16
```

#### `impl TerrainEngine`

```rust
// Constructors
pub fn new() -> Self
pub fn with_capacity(capacity: usize) -> Self

// DTED Cache management
pub fn set_cache_capacity(&self, capacity: usize)  // returns () directly; capacity == 0 panics
pub fn cache_size(&self) -> usize
pub fn clear_cache(&self)

// DTED Tile loading
pub fn load_tile(&mut self, data: &[u8]) -> Result<TileKey, TerrainError>
pub fn unload_tile(&mut self, key: &TileKey) -> bool

// Civil RGB Elevation Tile loading (Mapbox RGB & Terrarium)
pub fn load_rgb_tile(&self, z: u32, x: u32, y: u32, width: usize, height: usize, rgba_buffer: &[u8], encoding: RgbElevationEncoding) -> Result<SlippyTileKey, TerrainError>
pub fn unload_rgb_tile(&self, key: &SlippyTileKey) -> bool
pub fn clear_rgb_cache(&self)
pub fn rgb_cache_size(&self) -> usize

// Cloud-Optimized GeoTIFF raster loading
pub fn load_geotiff_tile(&self, data: &[u8]) -> Result<(f64, f64, f64, f64), TerrainError> // returns bounds (min_lat, min_lon, max_lat, max_lon)
pub fn clear_geotiff_cache(&self)
pub fn geotiff_cache_size(&self) -> usize

// Clear all sources (DTED, RGB, GeoTIFF)
pub fn clear_all(&self)

// Multi-Source Elevation queries (DTED -> RGB Tile -> GeoTIFF tiered fallback)
pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError>
pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Result<f64, TerrainError>
pub fn get_elevation_status(&self, lat_rad: f64, lon_rad: f64) -> Result<ElevationSample, TerrainError>
pub fn get_elevation_with_policy(&self, lat_rad: f64, lon_rad: f64, policy: UnknownTerrainPolicy) -> Result<Option<f64>, TerrainError>
pub fn get_vertical_profile_status(&self, route: &[LatLon], step_meters: f64, policy: UnknownTerrainPolicy) -> Result<Vec<ProfilePointStatus>, TerrainError>
pub fn calculate_clearance(&self, lat_rad: f64, lon_rad: f64, aircraft_height_meters: f64, minimum_clearance_meters: f64, policy: UnknownTerrainPolicy) -> Result<ClearanceResult, TerrainError>

// Vertical profile
pub fn get_vertical_profile(&self, route: &[LatLon], step_meters: f64) -> Result<Vec<ProfilePoint>, TerrainError>
```

`impl Default`.

#### Free Functions (RGB Decoders)

```rust
pub fn decode_mapbox_rgb(r: u8, g: u8, b: u8) -> f64
pub fn decode_terrarium_rgb(r: u8, g: u8, b: u8) -> f64
pub fn decode_rgb_elevation(r: u8, g: u8, b: u8, encoding: RgbElevationEncoding) -> f64
pub fn decode_rgba_buffer(rgba: &[u8], encoding: RgbElevationEncoding) -> Result<Vec<f32>, TerrainError>
```

#### Error

```rust
pub enum TerrainError {
    InvalidHeader(String),
    MalformedData(String),
    TileNotLoaded(i32, i32),  // (lat_deg, lon_deg)
    RgbDecodeError(String),
    GeoTiffError(String),
}
```

---

### 1.5 `sld` — OGC SLD XML Parser

Sub-modules: `errors`, `styles`, `parser`. Public items are re-exported from `olayer_core::sld`.

#### Structs

```rust
pub struct StyleRegistry { pub layers: HashMap<String, Vec<RuleStyle>> }
impl StyleRegistry {
    pub fn new() -> Self
    pub fn get_applicable_rules(&self, name: &str, scale: f64) -> Vec<&RuleStyle>
}

pub struct RuleStyle {
    pub name: String,
    pub min_scale: Option<f64>, pub max_scale: Option<f64>,
    pub stroke: Option<StrokeStyle>, pub fill: Option<FillStyle>,
    pub text: Option<TextStyle>, pub point: Option<PointStyle>,
}

pub struct StrokeStyle { pub color: String, pub width: f32, pub dash_array: Option<Vec<f32>> }
pub struct FillStyle { pub color: String, pub opacity: f32 }
pub struct TextStyle { pub label_expression: String, pub font_family: String, pub font_size: f32, pub fill_color: String }
pub struct PointStyle { pub well_known_name: String, pub size: f32, pub fill_color: Option<String>, pub stroke_color: Option<String>, pub stroke_width: Option<f32> }
```

#### Free Function

```rust
pub fn parse(xml_content: &str) -> Result<StyleRegistry, SldError>
```

#### Error

```rust
pub enum SldError { XmlError(String), InvalidValue(String) }
```

---

### 1.6 `symbol_registry` — Pluggable Symbology

Sub-modules: `errors`, `state`, `engine`; submodules `providers` (`declarative`, `nato`, `icao`). Public items are re-exported from `olayer_core::symbol_registry` and `olayer_core::symbol_registry::providers`.

#### Primitives (all `Serialize`/`Deserialize`)

```rust
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }  // Copy
impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self
}

pub struct Stroke { pub color: Color, pub width: f32, pub dash_array: Option<Vec<f32>> }
impl Stroke {
    pub fn new(color: Color, width: f32) -> Self
    pub fn with_dash_array(color: Color, width: f32, dash_array: Vec<f32>) -> Self
}
```

```rust
#[serde(tag = "type")]
pub enum SymbolPrimitive {
    Path { commands: String, fill: Option<Color>, stroke: Option<Stroke> },
    Circle { cx: f64, cy: f64, r: f64, fill: Option<Color>, stroke: Option<Stroke> },
    Text { content: String, offset_x: f64, offset_y: f64, font_size: f32, color: Color },
}

pub struct ResolvedSymbol {
    pub symbol_id: String,
    pub primitives: Vec<SymbolPrimitive>,
    pub bbox: (f64, f64, f64, f64),  // (min_x, min_y, max_x, max_y)
    pub anchor: (f64, f64),
}
```

#### `trait SymbologyProvider`

```rust
pub trait SymbologyProvider {
    fn name(&self) -> &str;
    fn can_resolve(&self, code: &str) -> bool;
    fn resolve(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>;
}
```

#### `impl SymbolRegistry`

```rust
pub fn new() -> Self                                             // + Default
pub fn register_provider(&mut self, provider: Box<dyn SymbologyProvider + Send + Sync>)
pub fn resolve_symbol(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>
```

Providers are queried in registration order. The `StyleRegistry` is only used by providers that need styling information; many built-in providers ignore it.

#### Built-in Providers

| Provider | Prefix | Description |
|----------|--------|-------------|
| `DeclarativeProvider` | (any) | `from_json(json: &str) -> Result<Self, SymbologyError>` |
| `NatoProvider` | `nato:`, `mil:` | `new()` + `Default` |
| `IcaoProvider` | `icao:` | `new()` + `Default` |

**NatoProvider** supported codes:
- Compact: `nato:friend:fighter`, `nato:hostile:armor`, `mil:neutral:ship`, `nato:unknown:air`, `nato:friend:submarine`, `nato:friend:satellite`
- Full SIDC: 15-char string after prefix; position 2 = affiliation (F/H/N/U/P/A/S/0-6), position 3 = dimension (Z/A/G/S/N/U/0-6)

**IcaoProvider** supported codes:

| Code | Navaid |
|------|--------|
| `icao:vor` | VOR (hexagon + dot) |
| `icao:vordme` | VOR-DME (hexagon + DME box) |
| `icao:vortac` | VORTAC (hexagon + TACAN triangle) |
| `icao:dme` | DME (rectangle) |
| `icao:ndb` | NDB (circle, magenta) |
| `icao:tacan` | TACAN (circle + triangle) |
| `icao:airport` | Airport (circle + runway) |
| `icao:heliport` | Heliport (circle + "H") |
| `icao:waypoint` | Waypoint (triangle) |
| `icao:intersection` | Intersection (triangle) |
| `icao:runway` | Runway Threshold (arrow) |

#### Error

```rust
pub enum SymbologyError {
    ProviderNotFound,
    SymbolNotFound(String),
    InvalidFormat(String),
}
```

---

### 1.7 `interpolator` — Dead-Reckoning

Sub-modules: `errors`, `state`, `engine`. Public items are re-exported from `olayer_core::interpolator`.

#### Structs

```rust
pub struct TargetState {
    pub id: String,
    pub last_position: LatLon,   // radians, metres
    pub speed_mps: f64,
    pub track_heading_rad: f64,   // [0, 2π)
    pub vertical_rate_mps: f64,
    pub last_ping_time: f64,      // seconds
}
// validate() -> Result<(), InterpolatorError>

pub struct InterpolatedTarget {
    pub id: String,
    pub position: LatLon,
    pub heading_rad: f64,
    pub quality: PredictionQuality,
}

pub enum PredictionQuality { Valid, Stale, ClockSkewed, Unavailable }

pub struct SkippedTarget {
    pub id: String,
    pub quality: PredictionQuality,
    pub age_seconds: f64,
}

pub struct InterpolationBatch {
    pub targets: Vec<InterpolatedTarget>,
    pub skipped: Vec<SkippedTarget>,
}
```

#### `impl InterpolationEngine`

```rust
pub fn new() -> Self                                    // stale threshold = 30.0 s; + Default
pub fn with_stale_threshold(stale_threshold: f64) -> Self
pub fn update_target(&mut self, state: TargetState) -> Result<(), InterpolatorError>
pub fn remove_target(&mut self, id: &str) -> bool
pub fn interpolate_all(&self, current_time: f64) -> Result<Vec<InterpolatedTarget>, InterpolatorError>
pub fn interpolate_all_with_status(&self, current_time: f64) -> Result<InterpolationBatch, InterpolatorError>
```

`TargetState` is `Clone` but not `Copy` (contains `String`/`Arc<str>` id).

#### Error

```rust
pub enum InterpolatorError {
    InvalidState(String),
    NegativeTimeDelta(String),
    GeodesyFailure(GeodesyError),  // impl From<GeodesyError>
}
```

---

## 2. WASM Bridge (`olayer-wasm`)

All `#[wasm_bindgen]` structs. Errors returned as `JsValue` strings.

### 2.1 Structs

| JS Class | Fields / Notes |
|----------|---------------|
| `WasmLatLon` | `pub lat: f64`, `pub lon: f64`, `pub height: f64` |
| `WasmTileKey` | `pub lat_deg: i32`, `pub lon_deg: i32` |
| `WasmTerrainEngine` | `new()` + `Default` |
| `WasmInterpolationEngine` | `new()` + `with_stale_threshold(f64)` |
| `WasmCameraState` | `pub center_lat` / `center_lon` / `center_height` / `zoom` / `rotation` / `pitch` / `roll` / `aspect_ratio` / `viewport_base_meters` (all `f64`) |
| `WasmProjection` | Factory methods below |
| `WasmProjectionType` | Enum: `Lcc`, `Stereographic`, `WebMercator` |
| `WasmStyleRegistry` | `parse(xml: &str)` |
| `WasmSymbolRegistry` | `new()` |

`version(): number` is exposed on `WasmProjection` for cache invalidation but is rarely needed by SDK consumers.

### 2.2 `WasmTerrainEngine`

```typescript
// DTED Tile Operations
load_tile(data: Uint8Array): WasmTileKey
unload_tile(lat_deg: i32, lon_deg: i32): boolean
set_cache_capacity(capacity: usize): void                 // throws if capacity == 0
cache_size(): usize
clear_cache(): void

// RGB Tile Operations (Mapbox Terrain-RGB & Terrarium)
load_rgb_tile(z: number, x: number, y: number, encoding: "Mapbox" | "Terrarium", rgba_bytes: Uint8Array, width: number, height: number): void
unload_rgb_tile(z: number, x: number, y: number): boolean
clear_rgb_cache(): void
rgb_cache_size(): number

// GeoTIFF Operations
load_geotiff_tile(data: Uint8Array): [number, number, number, number] // [min_lat_deg, min_lon_deg, max_lat_deg, max_lon_deg]
clear_geotiff_cache(): void
geotiff_cache_size(): number

// Global cleanup
clear_all_terrain(): void
free(): void                                              // MUST call to release WASM heap

// Multi-Source Elevation Queries
get_elevation(lat_deg: f64, lon_deg: f64): f64            // throws JsValue on error
get_elevation_rad(lat_rad: f64, lon_rad: f64): f64        // throws JsValue on error
get_elevation_status(lat_rad: f64, lon_rad: f64): JsValue // { elevation_meters: number | null }
get_vertical_profile_status(route_coords: Float64Array, step_meters: f64, reject_unknown: boolean): JsValue
calculate_clearance(lat_rad: f64, lon_rad: f64, aircraft_height_meters: f64, minimum_clearance_meters: f64, reject_unknown: boolean): JsValue
get_vertical_profile(route_coords: Float64Array, step_meters: f64): Float64Array
  // Input: flat [lat0, lon0, h0, lat1, lon1, h1, ...] in degrees
  // Output: flat [dist0, elev0, lat0, lon0, h0, ...] — 5 values per point
```

#### WASM Free Functions (RGB Decoders)

```typescript
decode_mapbox_rgb(r: number, g: number, b: number): number
decode_terrarium_rgb(r: number, g: number, b: number): number
decode_rgb_elevation(r: number, g: number, b: number, encoding: string): number
```

### 2.3 `WasmInterpolationEngine`

```typescript
// Coordinates are in radians, altitude in metres
update_target(id: string, lat_rad: f64, lon_rad: f64, height: f64, speed_mps: f64, track_heading_rad: f64, vertical_rate_mps: f64, last_ping_time: f64): void
remove_target(id: string): boolean
interpolate_all(current_time: f64): JsValue             // JSON array, parse with JSON.parse()
interpolate_all_with_status(current_time: f64): JsValue // JSON batch with skipped statuses
free(): void
```

`update_target` throws a `JsValue` string if the supplied state is invalid (e.g., negative speed).

### 2.4 `WasmProjection`

```typescript
// Factory (all coordinates in radians)
static new_lcc(std_par1: f64, std_par2: f64, origin_lat: f64, origin_lon: f64): WasmProjection
static new_stereographic(center_lat: f64, center_lon: f64): WasmProjection
static new_web_mercator(): WasmProjection

update_center(center_lat: f64, center_lon: f64): void
project(lat_rad: f64, lon_rad: f64, height: f64): [f64, f64]
unproject(x: f64, y: f64): WasmLatLon
get_view_proj_matrix(camera: WasmCameraState): Float32Array   // [f32; 16]
get_3d_view_proj_matrix(camera: WasmCameraState): Float32Array
get_25d_view_proj_matrix(camera: WasmCameraState): Float32Array
```

### 2.5 `WasmSymbolRegistry`

```typescript
register_declarative_provider(json_content: string): void
resolve_symbol(code: string, style: WasmStyleRegistry): JsValue  // JSON string
```

### 2.6 Free Functions

```typescript
function lla_to_ecef(lat_rad: f64, lon_rad: f64, height: f64): [f64, f64, f64]
function ecef_to_lla(x: f64, y: f64, z: f64): WasmLatLon
```

---

## 3. TypeScript SDK (`olayer-sdk`)

### 3.1 `OlayerController`

```typescript
interface OlayerConfig {
  glCanvas: HTMLCanvasElement;
  canvas2D: HTMLCanvasElement;
  projection: WasmProjection;
  initialCenterLatRad?: number;
  initialCenterLonRad?: number;
  initialZoom?: number;
  viewportBaseMeters?: number;
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

  // Lifecycle
  startLoop(): void
  stopLoop(): void
  destroy(): void      // frees all WASM + WebGL resources
}
```

The constructor automatically registers a default `TerrainTileSource` in `dataManager`.

### 3.2 Layers

```typescript
abstract class Layer {
  id: string
  visible: boolean
  opacity: number
  constructor(id: string)
  abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void
  abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void
}

class LayerManager {
  addLayer(layer: Layer): void          // throws on duplicate id
  removeLayer(id: string): boolean
  reorderLayer(id: string, newIndex: number): void   // throws if not found or out of bounds
  getLayers(): Layer[]                  // copy of internal array
  renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void
  renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void
}

class TileLayer extends Layer { ... }
class VectorTileLayer extends Layer { ... }
```

### 3.3 Data Sources

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

### 3.4 Renderers

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
```

---

## 4. Native SDK (`olayer-native`)

### Re-exports from `lib.rs`

```rust
pub use native_controller::NativeController;
pub use native_layer_manager::{Layer, NativeLayerManager};
pub use native_map_data_stack::{MapDataSource, NativeMapDataStack, TerrainDataSource, GeoserverWmtsSource};
pub use wgpu_gpu_pipeline::{RasterTileUpload, WgpuGpuPipeline, RasterVertex, WgpuRasterTile};
pub use wgpu_cpu_vertex_pipeline::{WgpuCpuVertexPipeline, project_lla_to_screen, rasterize_svg};
```

### 4.1 `NativeController`

```rust
pub struct NativeController {
    pub terrain: TerrainEngine,
    pub interpolator: InterpolationEngine,
    pub projection: Box<dyn Projection + Send + Sync>,
    pub camera: CameraState,
    pub view_mode: String,  // "2D" | "2.5D" | "3D"
}

impl NativeController {
    pub fn new(center_lat: f64, center_lon: f64) -> Self
    pub fn create_geoserver_source(&self, id: &str, base_url: &str, layer_name: &str) -> GeoserverWmtsSource
    pub fn trigger_active(&mut self)
    pub fn check_active(&mut self) -> bool
    pub fn get_target_fps(&mut self) -> u32   // 60 (active) or 15 (idle)
}
```

Public fields: `terrain: TerrainEngine`, `interpolator: InterpolationEngine`, `projection: Box<dyn Projection + Send + Sync>`, `camera: CameraState`, `view_mode: String`.

### 4.2 `Layer` trait & `NativeLayerManager`

```rust
pub trait Layer {
    fn id(&self) -> &str;
    fn is_visible(&self) -> bool;
    fn set_visible(&mut self, visible: bool);
    fn is_static(&self) -> bool;           // true = redraw only on camera change
}

pub struct NativeLayerManager {
    pub show_grid: bool,
    pub show_targets: bool,
    pub show_hud: bool,
    pub show_terrain: bool,
}

impl NativeLayerManager {
    pub fn new() -> Self                                 // + Default
    pub fn add_layer(&mut self, layer: Box<dyn Layer>) -> Result<(), String>
    pub fn remove_layer(&mut self, id: &str) -> bool
    pub fn reorder_layer(&mut self, id: &str, new_index: usize) -> Result<(), String>
    pub fn get_layers(&self) -> &[Box<dyn Layer>]
    pub fn visible_static_layers(&self) -> Vec<&dyn Layer>
    pub fn visible_dynamic_layers(&self) -> Vec<&dyn Layer>
    pub fn set_layer_visibility(&mut self, id: &str, visible: bool) -> Result<(), String>
    pub fn set_all_visibility(&mut self, visible: bool)
}
```

The manager stores layers in back-to-front render order. `show_*` toggles are hard-wired convenience flags used by the native demo and do not affect custom layers. "Static" layers are regenerated only on camera/projection changes; "dynamic" layers are redrawn every frame.

### 4.3 `MapDataSource` trait & `NativeMapDataStack`

```rust
pub trait MapDataSource {
    fn id(&self) -> &str;
    fn clear_cache(&mut self);
    fn cache_size(&self) -> usize;
}

impl NativeMapDataStack {
    pub fn new() -> Self
    pub fn register_source(&mut self, source: Box<dyn MapDataSource>) -> Result<(), String>
    pub fn get_source(&self, id: &str) -> Option<&dyn MapDataSource>
    pub fn clear_cache(&mut self)
    pub fn get_cache_size(&self) -> usize
    pub fn load_dted_file(&self, path: &str, terrain: &mut TerrainEngine) -> Result<(), String>
    pub fn load_dted_buffer(&self, buffer: &[u8], terrain: &mut TerrainEngine) -> Result<(), String>
}

impl TerrainDataSource {
    pub fn new(id: &str) -> Self
    pub fn load_file(&mut self, path: &str) -> Result<(), String>
    pub fn load_buffer(&mut self, buffer: &[u8]) -> Result<(), String>
    pub fn unload_tile(&mut self, lat_deg: i32, lon_deg: i32) -> bool
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, String>
    pub fn clear_cache(&mut self)        // unloads all tracked tiles
    pub fn cache_size(&self) -> usize    // number of tracked tiles
}
// impl MapDataSource for TerrainDataSource

impl GeoserverWmtsSource {                                      // + Clone
    pub fn new(id: &str, base_url: &str, layer_name: &str) -> Self  // spawns background worker thread
    pub fn load_tile(&self, x: u32, y: u32, z: u32)                // async, non-blocking
    pub fn get_tile_pixels(&self, x: u32, y: u32, z: u32) -> Option<Vec<u8>>  // RGBA8
    pub fn get_cached_keys(&self) -> Vec<String>                   // e.g. ["z/x/y", ...]
    pub fn base_url(&self) -> &str
    pub fn layer_name(&self) -> &str
    pub fn clear_cache(&mut self)
    pub fn cache_size(&self) -> usize
}
// impl MapDataSource for GeoserverWmtsSource
```

### 4.4 `WgpuGpuPipeline`

#### Raster Tile Structs

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RasterVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

pub struct WgpuRasterTile {
    pub key: String,
    pub x: u32, pub y: u32, pub z: u32,
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

pub struct RasterTileUpload<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub key: &'a str,
    pub pixels: &'a [u8],       // RGBA8
    pub x: u32, pub y: u32, pub z: u32,
    pub controller: &'a NativeController,
}
```

#### `WgpuGpuPipeline` Struct

```rust
pub struct WgpuGpuPipeline {
    // Grid rendering
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub grid_vertex_buffer: Option<wgpu::Buffer>,
    pub grid_vertices_len: usize,
    // Raster rendering
    pub raster_pipeline: wgpu::RenderPipeline,
    pub raster_bind_group_layout: wgpu::BindGroupLayout,
    pub raster_sampler: wgpu::Sampler,
    pub loaded_gpu_tiles: HashMap<String, WgpuRasterTile>,
}
```

All fields are public so advanced users can bind their own uniforms; most callers only need the methods below.

#### `impl WgpuGpuPipeline`

```rust
pub fn new(device: &wgpu::Device, config_format: wgpu::TextureFormat) -> Self
pub fn generate_grid_vertices(controller: &NativeController) -> Vec<f32>   // pure CPU, no GPU needed
pub fn rebuild_grid_buffers(&mut self, controller: &NativeController, device: &wgpu::Device, queue: &wgpu::Queue)
pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>)
pub fn upload_raster_tile(&mut self, upload: RasterTileUpload<'_>)         // hard-coded 256x256 RGBA8 texture
pub fn rebuild_raster_tile_buffers(&mut self, device: &wgpu::Device, controller: &NativeController)
pub fn clear_raster_tiles(&mut self)
pub fn render_raster_tiles<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, visible_keys: &HashSet<String>)
```

`render` draws the grid; `render_raster_tiles` draws visible quads. The raster pipeline expects an already-bound uniform bind group at group 0 and a per-tile texture/sampler bind group at group 1. Tile vertex positions are generated in either ECEF (3D) or projected planar metres (2D/2.5D) based on `controller.view_mode`.

### 4.5 `WgpuCpuVertexPipeline`

```rust
#[derive(Default)]
pub struct WgpuCpuVertexPipeline {}

impl WgpuCpuVertexPipeline {
    pub fn new() -> Self
    pub fn draw_targets(
        &self, painter: &egui::Painter,
        targets: &[InterpolatedTarget],
        selected_target_id: &Option<Arc<str>>,
        controller: &NativeController,
        view_proj_matrix: &[f32; 16],
        width: u32, height: u32,
        simulated_speeds: &HashMap<Arc<str>, f64>,
    )
}

pub fn project_lla_to_screen(
    lat: f64, lon: f64, alt: f64, view_mode: &str,
    camera: &CameraState, projection: &dyn Projection,
    view_proj_matrix: &[f32; 16], width: u32, height: u32,
) -> Option<egui::Pos2>

pub fn rasterize_svg(svg_data: &str, width: u32, height: u32) -> Result<Vec<u8>, String>
```

`project_lla_to_screen` understands `"2D"`, `"2.5D"`, and `"3D"` view modes. In `"3D"` it performs horizon occlusion culling and multiplies by the supplied `view_proj_matrix`; in `"2.5D"` it projects through the planar projection and applies perspective; in `"2D"` it maps planar coordinates through camera rotation/zoom to screen pixels.

---

## 5. C-FFI Bridge (`libolayer_native.h`)

All functions return `int32_t`. `>= 0` = success. Negative codes: `-1` null, `-2` parse/missing, `-3` invalid UTF-8, `-4` invalid state, `-99` panic caught.

### 5.1 C-Compatible Structs

```c
struct C_LatLon { double lat; double lon; double height; };
struct C_InterpolatedTarget { char *id; double lat; double lon; double height; double heading_rad; int32_t quality; };
struct C_ProfilePoint { double distance_meters; double ground_elevation; double lat; double lon; double height; };
```

### 5.2 Terrain Functions

```c
TerrainEngine* olayer_terrain_engine_create(void);

int olayer_terrain_engine_load_tile(
    TerrainEngine* engine, const uint8_t* data, size_t length,
    int32_t* out_lat_deg, int32_t* out_lon_deg);

int olayer_terrain_engine_unload_tile(TerrainEngine* engine, int32_t lat_deg, int32_t lon_deg);

int olayer_terrain_engine_load_rgb_tile(
    TerrainEngine* engine, uint32_t z, uint32_t x, uint32_t y,
    int encoding_code, const uint8_t* rgba_data, size_t rgba_len,
    size_t width, size_t height); // encoding_code: 0 = MapboxRgb, 1 = Terrarium

int olayer_terrain_engine_load_geotiff(
    TerrainEngine* engine, const uint8_t* data, size_t length,
    double* out_min_lat_deg, double* out_min_lon_deg,
    double* out_max_lat_deg, double* out_max_lon_deg);

int olayer_terrain_decode_mapbox_rgb(uint8_t r, uint8_t g, uint8_t b, double* out_elevation);
int olayer_terrain_decode_terrarium_rgb(uint8_t r, uint8_t g, uint8_t b, double* out_elevation);

int olayer_terrain_engine_get_elevation(
    TerrainEngine* engine, double lat_deg, double lon_deg, double* out_elevation);

int olayer_terrain_engine_get_elevation_rad(
    TerrainEngine* engine, double lat_rad, double lon_rad, double* out_elevation);

// Returns 0 for valid, 1 for unknown/null elevation, or a negative error.
int olayer_terrain_engine_get_elevation_status(
    TerrainEngine* engine, double lat_rad, double lon_rad, double* out_elevation);

// Returns 0 safe, 1 warning, 2 unknown terrain, or a negative error.
int olayer_terrain_engine_calculate_clearance(
    TerrainEngine* engine, double lat_rad, double lon_rad,
    double aircraft_height_meters, double minimum_clearance_meters,
    bool reject_unknown, double* out_clearance);

int olayer_terrain_engine_get_vertical_profile(
    TerrainEngine* engine,
    const double* route_lat, const double* route_lon, const double* route_height,
    size_t route_len, double step_meters,
    struct C_ProfilePoint** out_profile, size_t* out_count);
// route_lat/route_lon are in degrees; route_height in metres

int olayer_terrain_engine_set_cache_capacity(TerrainEngine* engine, size_t capacity);
size_t olayer_terrain_engine_cache_size(TerrainEngine* engine);
void olayer_terrain_engine_clear_cache(TerrainEngine* engine);
void olayer_terrain_engine_free(TerrainEngine* engine);
void olayer_profile_points_free(struct C_ProfilePoint* points, size_t count);
```

### 5.3 Interpolator Functions

```c
InterpolationEngine* olayer_interpolator_create(void);
InterpolationEngine* olayer_interpolator_create_with_threshold(double stale_threshold);

int olayer_interpolator_update(
    InterpolationEngine* engine, const char* id,
    double lat, double lon, double height,       // radians, metres
    double speed_mps, double track_heading_rad,
    double vertical_rate_mps, double time);

int olayer_interpolator_remove(InterpolationEngine* engine, const char* id);

int olayer_interpolator_interpolate_all(
    InterpolationEngine* engine, double current_time,
    struct C_InterpolatedTarget** out_targets, size_t* out_count);

void olayer_interpolated_targets_free(struct C_InterpolatedTarget* targets, size_t count);
void olayer_interpolator_free(InterpolationEngine* engine);
```

### 5.4 Memory Ownership Rules

| Allocation | Deletion | Rule |
|-----------|----------|------|
| `_create()` | `_free()` | Rust allocates, Rust frees |
| `const uint8_t* data` | Host | Host allocates, Host frees (Rust reads only) |
| `_get_*` output arrays | `_free()` on returned pointer + count | Rust allocates, caller frees via paired free function |

---

## 6. Symbols CLI (`tools/symbol-compiler`)

### Command

```bash
npx tsx src/cli.ts -c <config.json> -o <output.json>
```

### Config Format

```json
{
  "libraryName": "MyLibrary",
  "symbols": {
    "my:symbol": "path/to/file.svg"
  }
}
```

### `compiler.ts` Public API

```typescript
interface Color { r: number; g: number; b: number; a: number; }
interface Stroke { color: Color; width: number; dashArray?: number[]; }

type SymbolPrimitive =
  | { type: "Path"; commands: string; fill?: Color; stroke?: Stroke }
  | { type: "Circle"; cx: number; cy: number; r: number; fill?: Color; stroke?: Stroke }
  | { type: "Text"; content: string; offsetX: number; offsetY: number; fontSize: number; color: Color };

interface DeclarativeSymbolDto { bbox: [number,number,number,number]; anchor: [number,number]; primitives: SymbolPrimitive[]; }
interface DeclarativeLibraryDto { libraryName: string; symbols: Record<string, DeclarativeSymbolDto>; }

function parseColor(str: string, opacity?: number): Color | null
function compileSvg(svgContent: string): DeclarativeSymbolDto
function compileLibrary(configPath: string, rootDir: string): DeclarativeLibraryDto
```

### SVG Support

- Elements: `<path>`, `<circle>`, `<text>`
- Style inheritance through `<g>` and `<svg>`
- Color formats: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `rgb()`, `rgba()`
- 14 named colors: transparent, none, black, white, red, green, blue, yellow, cyan, magenta, gray, grey, orange, purple, pink
- Opacity multiplication: `fill-opacity * stroke-opacity * opacity`

---

## 7. Error Handling Summary

| Layer | Error Mechanism | Convention |
|-------|----------------|------------|
| Rust Core | `Result<T, ModuleError>` | Typed enum, `Display` + `Error` |
| WASM | `Result<T, JsValue>` | `.map_err(|e| JsValue::from_str(&e.to_string()))` |
| C-FFI | `int32_t` return code | `0` = OK, negative = error code |
| TypeScript | Exceptions thrown | Caught via try/catch; WASM errors are JS strings |

### C-FFI Error Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `-1` | Null pointer / invalid argument |
| `-2` | Parse error / tile not loaded / symbol not found |
| `-3` | Invalid UTF-8 in string parameter |
| `-4` | Invalid target state (e.g., negative speed, out-of-range heading) |
| `-99` | Rust panic caught via `catch_unwind` |

---

## 8. Version / Changelog Notes

- `WasmProjection` now exposes a `version()` getter used internally by the TypeScript controller to detect projection-center changes.
- `TerrainEngine` gained `get_elevation_rad`, `set_cache_capacity`, `cache_size`, and `clear_cache` in all bindings.
- `NativeController` gained `create_geoserver_source`, `check_active`, and `get_target_fps`.
- `WgpuGpuPipeline` gained raster tile upload/rendering alongside the existing grid pipeline.
- `CPURenderer`/`WgpuCpuVertexPipeline` signatures were unified to accept explicit camera/projection parameters for 2D, 2.5D, and 3D modes.
