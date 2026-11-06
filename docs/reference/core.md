# Core Rust API

All angles are in **radians** unless noted otherwise. Height/altitude is in **metres**
above the WGS84 ellipsoid.

## `geodesy` — Geodetic Mathematics

Sub-modules: `conversions`, `coords`, `ellipsoid`, `errors`, `math`, `solvers`. Public items are re-exported from `olayer_core::geodesy`.

### Structs

| Struct | Fields | Notes |
|--------|--------|-------|
| `LatLon` | `pub lat: f64`, `pub lon: f64`, `pub height: f64` | Radians; `Copy`, `Serialize`/`Deserialize` |
| `Ecef` | `pub x: f64`, `pub y: f64`, `pub z: f64` | Metres; `Copy` |
| `Enu` | `pub east: f64`, `pub north: f64`, `pub up: f64` | Metres; `Copy` |
| `Ellipsoid` | `pub a: f64`, `pub b: f64`, `pub f: f64`, `pub e_sq: f64`, `pub e_prime_sq: f64`, `pub authalic_radius: f64` | `Copy` |
| `GeodeticResult` | `pub distance: f64`, `pub initial_bearing: f64`, `pub final_bearing: f64` | Bearings in radians; `Copy` |

### `impl LatLon`

```rust
pub const fn new(lat_rad: f64, lon_rad: f64, height_meters: f64) -> Self
pub fn from_degrees(lat_deg: f64, lon_deg: f64, height_meters: f64) -> Self
pub fn to_degrees(&self) -> (f64, f64, f64)
pub fn validate(&self) -> Result<(), GeodesyError>
```

### `impl Ecef`

```rust
pub const fn new(x: f64, y: f64, z: f64) -> Self
pub fn chord_distance(&self, other: &Self) -> f64
```

### `impl Enu`

```rust
pub const fn new(east: f64, north: f64, up: f64) -> Self
pub fn distance_2d(&self) -> f64
pub fn distance_3d(&self) -> f64
```

### `impl Ellipsoid`

```rust
pub const fn new(a: f64, f: f64) -> Self
pub const fn wgs84() -> Self
pub fn radius_of_curvature_prime_vertical(&self, lat_rad: f64) -> f64
```

### `impl GeodeticResult`

```rust
pub const fn new(distance: f64, initial_bearing: f64, final_bearing: f64) -> Self
```

### Free Conversion Functions

```rust
pub fn lla_to_ecef(lla: &LatLon, ellipsoid: &Ellipsoid) -> Ecef
pub fn ecef_to_lla(ecef: &Ecef, ellipsoid: &Ellipsoid) -> LatLon
pub fn ecef_to_enu(ecef: &Ecef, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu
pub fn enu_to_ecef(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> Ecef
pub fn lla_to_enu(lla: &LatLon, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu
pub fn enu_to_lla(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> LatLon
```

### Free Math Utilities

```rust
pub fn normalize_bearing(rad: f64) -> f64     // → [0, 2π)
pub fn normalize_longitude(rad: f64) -> f64    // → [-π, π)
```

### `trait GeodeticSolver`

```rust
pub trait GeodeticSolver {
    const IS_ELLIPSOIDAL: bool;
    const EXPECTED_ACCURACY_METERS: f64;
    fn inverse(&self, p1: &LatLon, p2: &LatLon, ellipsoid: &Ellipsoid) -> Result<GeodeticResult, GeodesyError>;
    fn direct(&self, p1: &LatLon, bearing_rad: f64, distance_meters: f64, ellipsoid: &Ellipsoid) -> Result<LatLon, GeodesyError>;
}
```

### Solvers

| Struct | Trait | Accuracy | Fallback |
|--------|-------|----------|----------|
| `HaversineSolver` | `GeodeticSolver` | ~1 m (spherical) | — |
| `VincentySolver` | `GeodeticSolver` | ~1 mm (ellipsoidal) | Falls back to Haversine on non-convergence |

Both implement `Default`.

### Error

```rust
pub enum GeodesyError {
    LatitudeOutOfRange(f64),
    LongitudeOutOfRange(f64),
}
```

`impl Error` + `Display`.


## `camera` — Camera State & VP Matrices

The `camera` module contains the public `CameraState` struct. It is also re-exported from `olayer_core::projections` for convenience.

### Struct

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

### `impl CameraState`

```rust
pub const fn new(center: LatLon, zoom: f64, rotation: f64, aspect_ratio: f64, viewport_base_meters: f64) -> Self
pub const fn with_attitude(center: LatLon, zoom: f64, rotation: f64, pitch: f64, roll: f64, aspect_ratio: f64, viewport_base_meters: f64) -> Self
pub fn validate(&self) -> Result<(), CameraError>
pub fn get_2d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError>
pub fn get_25d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError>
pub fn get_3d_view_proj_matrix(&self) -> Result<[f32; 16], CameraError>
```

### Error

```rust
pub enum CameraError {
    InvalidZoom,
    InvalidAspectRatio,
    InvalidViewportBase,
    Projection(ProjectionError),
}
```

`impl From<ProjectionError>`.


## `projections` — Cartographic Projections

Modules: `lambert_conformal_conic`, `stereographic`, `web_mercator`, `matrix`, `errors`.
Public re-exports from `olayer_core::projections`: `Projection`, `CameraState`, `LambertConformalConic`, `Stereographic`, `WebMercator`, `ProjectionError`, `Matrix4`.

### `trait Projection`

```rust
pub trait Projection {
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError>;
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError>;
    fn update_center(&mut self, center_lat_rad: f64, center_lon_rad: f64);
    fn get_view_proj_matrix(&self, camera: &CameraState) -> Result<[f32; 16], ProjectionError>;
}
```

`get_view_proj_matrix` has a default implementation (2D ortho). `update_center` defaults to no-op.

### Projection Types

| Struct | Constructor | Behaviour |
|--------|-------------|-----------|
| `Stereographic` | `new(center_lat, center_lon, ellipsoid)` | Azimuthal; antipode → `Singularity` |
| `LambertConformalConic` | `new(sp1, sp2, origin_lat, origin_lon, ellipsoid)` | Conic; clamps lat to ±89.9° |
| `WebMercator` | `new(ellipsoid)` + `Default` | Cylindrical; clamps lat to ±85.05° |

All implement `Projection`. Stereographic and LCC implement `update_center`.

### Error

```rust
pub enum ProjectionError {
    InvalidCameraState,
    Singularity,
    ConvergenceFailed,
}
```

### `Matrix4`

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


## `terrain` — DTED Elevation Engine

Sub-modules: `errors`, `tile`, `engine`. Public items are re-exported from `olayer_core::terrain`.

### Structs

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

### `impl DtedTile`

```rust
pub fn from_bytes(data: &[u8]) -> Result<Self, TerrainError>
pub fn get_cell_elevation(&self, row: usize, col: usize) -> Option<i16>
```

### `impl TerrainEngine`

```rust
// Constructors
pub fn new() -> Self
pub fn with_capacity(capacity: usize) -> Self

// DTED Cache management
pub fn set_cache_capacity(&self, capacity: usize)  // returns () directly; zero capacity is treated as one
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

// Clear all sources (scalar grids, DTED, RGB, GeoTIFF)
pub fn clear_all(&self)

// Elevation lookup: scalar grids, then DTED/RGB/GeoTIFF
pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, TerrainError>
pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Result<f64, TerrainError>
pub fn get_elevation_status(&self, lat_rad: f64, lon_rad: f64) -> Result<ElevationSample, TerrainError>
pub fn get_elevation_with_policy(&self, lat_rad: f64, lon_rad: f64, policy: UnknownTerrainPolicy) -> Result<Option<f64>, TerrainError>
pub fn get_vertical_profile_status(&self, route: &[LatLon], step_meters: f64, policy: UnknownTerrainPolicy) -> Result<Vec<ProfilePointStatus>, TerrainError>
pub fn calculate_clearance(&self, lat_rad: f64, lon_rad: f64, aircraft_height_meters: f64, minimum_clearance_meters: f64, policy: UnknownTerrainPolicy) -> Result<ClearanceResult, TerrainError>
pub fn resolve_altitude(&self, lat_rad: f64, lon_rad: f64, input_height: f64, mode: AltitudeMode, policy: AltitudeUnknownPolicy, mesh_height: Option<f64>) -> Result<f64, TerrainError>

// Vertical profile
pub fn get_vertical_profile(&self, route: &[LatLon], step_meters: f64) -> Result<Vec<ProfilePoint>, TerrainError>
```

`impl Default`.

### Free Functions (RGB Decoders)

```rust
pub fn decode_mapbox_rgb(r: u8, g: u8, b: u8) -> f64
pub fn decode_terrarium_rgb(r: u8, g: u8, b: u8) -> f64
pub fn decode_rgb_elevation(r: u8, g: u8, b: u8, encoding: RgbElevationEncoding) -> f64
pub fn decode_rgba_buffer(rgba: &[u8], encoding: RgbElevationEncoding) -> Result<Vec<f32>, TerrainError>
```

### Error

```rust
pub enum TerrainError {
    InvalidHeader(String),
    MalformedData(String),
    TileNotLoaded(i32, i32),  // (lat_deg, lon_deg)
    RgbDecodeError(String),
    GeoTiffError(String),
}
```


## `sld` — OGC SLD XML Parser

Sub-modules: `errors`, `styles`, `parser`. Public items are re-exported from `olayer_core::sld`.

### Structs

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

### Free Function

```rust
pub fn parse(xml_content: &str) -> Result<StyleRegistry, SldError>
```

### Error

```rust
pub enum SldError { XmlError(String), InvalidValue(String) }
```


## `symbol_registry` — Pluggable Symbology

Sub-modules: `errors`, `state`, `engine`; submodules `providers` (`declarative`, `nato`, `icao`). Public items are re-exported from `olayer_core::symbol_registry` and `olayer_core::symbol_registry::providers`.

### Primitives (all `Serialize`/`Deserialize`)

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

### `trait SymbologyProvider`

```rust
pub trait SymbologyProvider {
    fn name(&self) -> &str;
    fn can_resolve(&self, code: &str) -> bool;
    fn resolve(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>;
}
```

### `impl SymbolRegistry`

```rust
pub fn new() -> Self                                             // + Default
pub fn register_provider(&mut self, provider: Box<dyn SymbologyProvider + Send + Sync>)
pub fn resolve_symbol(&self, code: &str, style: &StyleRegistry) -> Result<ResolvedSymbol, SymbologyError>
```

Providers are queried in registration order. The `StyleRegistry` is only used by providers that need styling information; many built-in providers ignore it.

### Built-in Providers

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

### Error

```rust
pub enum SymbologyError {
    ProviderNotFound,
    SymbolNotFound(String),
    InvalidFormat(String),
}
```


## `interpolator` — Dead-Reckoning

Sub-modules: `errors`, `state`, `engine`. Public items are re-exported from `olayer_core::interpolator`.

### Structs

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

### `impl InterpolationEngine`

```rust
pub fn new() -> Self                                    // stale threshold = 30.0 s; + Default
pub fn with_stale_threshold(stale_threshold: f64) -> Self
pub fn update_target(&mut self, state: TargetState) -> Result<(), InterpolatorError>
pub fn remove_target(&mut self, id: &str) -> bool
pub fn interpolate_all(&self, current_time: f64) -> Result<Vec<InterpolatedTarget>, InterpolatorError>
pub fn interpolate_all_with_status(&self, current_time: f64) -> Result<InterpolationBatch, InterpolatorError>
```

`TargetState` is `Clone` but not `Copy` (contains `String`/`Arc<str>` id).

### Error

```rust
pub enum InterpolatorError {
    InvalidState(String),
    NegativeTimeDelta(String),
    GeodesyFailure(GeodesyError),  // impl From<GeodesyError>
}
```


## `aeronautical` — Aeronautical Data Ingestion (AIXM 5.1 & GeoJSON-Aviation)

Sub-modules: `aixm_parser`, `geojson_aviation`, `dataset`, `errors`.

```rust
pub struct AeronauticalAirspace {
    pub uid: String, pub name: String, pub airspace_type: String,
    pub lower_limit_m: f64, pub upper_limit_m: f64, pub boundary: Vec<LatLon>,
}

pub struct AeronauticalNavaid {
    pub ident: String, pub name: String, pub navaid_type: String,
    pub position: LatLon, pub frequency_mhz: Option<f64>,
}

pub struct AeronauticalDataset {
    pub airspaces: Vec<AeronauticalAirspace>,
    pub navaids: Vec<AeronauticalNavaid>,
}

impl AeronauticalDataset {
    pub fn new() -> Self
    pub fn from_aixm_51(xml: &str) -> Result<Self, AeronauticalError>
    pub fn from_geojson(json: &str) -> Result<Self, AeronauticalError>
    pub fn to_geojson(&self) -> Result<String, AeronauticalError>
}
```


## `weather` — Meteorological GIS Overlays (Radar dBZ, Wind Barbs & Isolines)

Sub-modules: `radar_palette`, `wind_barb`, `isoline`, `sigmet`, `errors`.

```rust
pub enum RadarColorPalette { Nexrad, Icao, HighContrast }

pub fn dbz_to_rgba(dbz: f64, palette: RadarColorPalette) -> [u8; 4]
pub fn colorize_dbz_grid(grid: &[f64], width: usize, height: usize, palette: RadarColorPalette) -> Result<Vec<u8>, WeatherError>

pub struct WindBarbGeometry {
    pub origin: LatLon,
    pub staff: (LatLon, LatLon),
    pub barbs: Vec<(LatLon, LatLon)>,
    pub pennants: Vec<[LatLon; 3]>,
    pub calm_circle_radius_m: Option<f64>,
}

pub fn generate_wind_barb(origin: &LatLon, speed_knots: f64, direction_rad: f64, staff_length_meters: f64, is_southern_hemisphere: bool) -> Result<WindBarbGeometry, WeatherError>
pub fn wind_barb_to_flat_lines_deg(geom: &WindBarbGeometry) -> Vec<f64>

pub struct IsolineSegment { pub isovalue: f64, pub start: LatLon, pub end: LatLon }
pub fn generate_isolines_rad(grid: &[f64], width: usize, height: usize, bounds_rad: (f64, f64, f64, f64), isovalues: &[f64]) -> Result<Vec<IsolineSegment>, WeatherError>
pub fn isolines_to_flat_array_deg(segments: &[IsolineSegment]) -> Vec<f64>

pub struct SigmetFeature {
    pub id: String, pub name: String,
    pub hazard_type: SigmetHazardType, pub severity: SigmetSeverity,
    pub polygon: Vec<LatLon>,
    pub floor_m: Option<f64>, pub ceiling_m: Option<f64>,
}

pub struct SigmetDataset {
    pub features: Vec<SigmetFeature>,
}
```


## `volumetric` — 3D Volumetric Airspaces & Trajectory Ribbon Mesh Generation

Sub-modules: `airspace_mesh`, `ribbon_mesh`, `triangulation`, `types`, `errors`.

```rust
pub struct VolumetricVertex {
    pub position_ecef: [f64; 3],
    pub normal: [f32; 3],
    pub height_ratio: f32,
    pub is_edge: f32,
}

pub struct VolumetricMesh {
    pub vertices: Vec<VolumetricVertex>,
    pub indices: Vec<u32>,
}

pub struct RibbonVertex {
    pub position_ecef: [f64; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub scalar: f32,
}

pub struct RibbonMesh {
    pub vertices: Vec<RibbonVertex>,
    pub indices: Vec<u32>,
}

pub fn generate_airspace_volume_mesh(polygon: &[LatLon], floor_m: f64, ceiling_m: f64) -> Result<VolumetricMesh, VolumetricError>
pub fn generate_trajectory_ribbon_mesh(waypoints: &[LatLon], ribbon_width_m: f64, scalar_values: Option<&[f64]>) -> Result<RibbonMesh, VolumetricError>
pub fn triangulate_polygon_2d(points: &[[f64; 2]]) -> Result<Vec<[usize; 3]>, VolumetricError>
pub fn signed_area_2d(points: &[[f64; 2]]) -> f64
```


## `declutter` — 8-Octant Force-Directed Label Anti-Cluttering Engine

Sub-modules: `types`, `spatial_grid`, `engine`.

```rust
pub enum OctantDirection { North, NorthEast, East, SouthEast, South, SouthWest, West, NorthWest }

pub struct Rect2D { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }

pub struct LabelTarget {
    pub id: String, pub x: f32, pub y: f32,
    pub heading_rad: Option<f32>,
    pub width: f32, pub height: f32,
    pub priority: u8,
}

pub struct LabelPlacement {
    pub id: String, pub octant: OctantDirection,
    pub rect: Rect2D,
    pub leader_start: [f32; 2], pub leader_end: [f32; 2],
    pub cost: f32, pub visible: bool,
}

pub struct DeclutterConfig {
    pub leader_length_px: f32, pub safety_margin_px: f32,
    pub weight_overlap: f32, pub weight_heading: f32,
    pub weight_leader_crossing: f32, pub weight_preference: f32,
    pub max_iterations: usize,
}

pub struct DeclutterEngine { /* ... */ }
impl DeclutterEngine {
    pub fn new(config: DeclutterConfig) -> Self
    pub fn with_default_config() -> Self
    pub fn solve(&self, targets: &[LabelTarget]) -> Vec<LabelPlacement>
}
```

## Scalar grids and polygon interiors

`TerrainEngine` additionally exposes `load_elevation_grid(id, grid)`, `unload_elevation_grid(id)` and a wrapping `revision()` token. [ElevationGrid](../../core/src/terrain/grid.rs) validates bounds, dimensions, finite samples and nodata; its geographic Rust bounds use radians. See [WCS terrain](../guides/maps/wcs-terrain.md) for the SDK boundary.

`volumetric::triangulate_polygon_rings(coords, ring_lengths)` triangulates a closed exterior and closed interior holes, returning packed x/y/height triangle vertices. See [polygon fill contracts](../guides/maps/wfs.md#polygon-fills-and-interior-holes) for bounds, interpolation and topology errors. This is separate from the original no-hole `triangulate_polygon_2d` used by airspace extrusion.
