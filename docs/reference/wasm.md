# WASM API

All `#[wasm_bindgen]` structs. Errors returned as `JsValue` strings.

## Structs

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
| `WasmAeronauticalDataset` | AIXM 5.1 & GeoJSON-Aviation dataset container |
| `WasmSigmetDataset` | SIGMET weather warning dataset container |
| `WasmVolumetricMesh` | Extruded 3D airspace mesh (`vertices()`, `indices()`, `vertex_count()`, `index_count()`) |
| `WasmRibbonMesh` | Continuous 3D flight trajectory ribbon (`vertices()`, `indices()`, `vertex_count()`, `index_count()`) |

`version(): number` is exposed on `WasmProjection` for cache invalidation but is rarely needed by SDK consumers.

## `WasmTerrainEngine`

```typescript
// DTED Tile Operations
load_tile(data: Uint8Array): WasmTileKey
unload_tile(lat_deg: i32, lon_deg: i32): boolean
set_cache_capacity(capacity: usize): void                 // zero capacity is treated as one
cache_size(): usize
clear_cache(): void

// RGB Tile Operations (Mapbox Terrain-RGB & Terrarium)
load_rgb_tile(z: number, x: number, y: number, encoding: "mapbox" | "terrarium", rgba_bytes: Uint8Array, width: number, height: number): void
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
resolve_altitude(lat_rad: f64, lon_rad: f64, input_height: f64, mode: string, unknown_policy: string, mesh_height?: f64): f64
get_vertical_profile(route_coords: Float64Array, step_meters: f64): Float64Array
  // Input: flat [lat0, lon0, h0, lat1, lon1, h1, ...] in degrees
  // Output: flat [dist0, elev0, lat0, lon0, h0, ...] — 5 values per point
```

### WASM Free Functions (RGB Decoders)

```typescript
decode_mapbox_rgb(r: number, g: number, b: number): number
decode_terrarium_rgb(r: number, g: number, b: number): number
decode_rgb_elevation(r: number, g: number, b: number, encoding: string): number
```

## `WasmInterpolationEngine`

```typescript
// Coordinates are in radians, altitude in metres
update_target(id: string, lat_rad: f64, lon_rad: f64, height: f64, speed_mps: f64, track_heading_rad: f64, vertical_rate_mps: f64, last_ping_time: f64): void
remove_target(id: string): boolean
interpolate_all(current_time: f64): JsValue             // JSON array, parse with JSON.parse()
interpolate_all_with_status(current_time: f64): JsValue // JSON batch with skipped statuses
free(): void
```

`update_target` throws a `JsValue` string if the supplied state is invalid (e.g., negative speed).

## `WasmProjection`

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

## `WasmSymbolRegistry`

```typescript
register_declarative_provider(json_content: string): void
resolve_symbol(code: string, style: WasmStyleRegistry): JsValue  // JSON string
```

## Free Functions (Geodesy & Topocentric Frames)

```typescript
function lla_to_ecef(lat_rad: f64, lon_rad: f64, height: f64): [f64, f64, f64]
function ecef_to_lla(x: f64, y: f64, z: f64): WasmLatLon
```

## `WasmAeronauticalDataset` (Aeronautical Ingestion)

```typescript
class WasmAeronauticalDataset {
  static from_aixm_51(xml_content: string): WasmAeronauticalDataset
  static from_geojson(geojson_content: string): WasmAeronauticalDataset
  to_geojson(): string
  total_feature_count(): number
  find_airspaces_containing_point(lat_deg: number, lon_deg: number, alt_m: number): string
}

function parse_aixm_51(xml_content: string): WasmAeronauticalDataset
function parse_geojson_aviation(geojson_content: string): WasmAeronauticalDataset
```

## `WasmSigmetDataset` & Weather Functions

```typescript
class WasmSigmetDataset {
  static from_geojson(geojson_content: string): WasmSigmetDataset
  total_count(): number
  find_hazards_containing_point(lat_deg: number, lon_deg: number, alt_m: number): string
}

function colorize_dbz_grid(dbz_grid: Float32Array, width: number, height: number, palette: "Nexrad15" | "IcaoDbz"): Uint8Array
function dbz_to_rgba(dbz: number, palette: "Nexrad15" | "IcaoDbz"): Uint8Array
function generate_wind_barb_geometry(origin_x: number, origin_y: number, speed_kts: number, dir_rad: number, staff_length_px: number, southern_hemisphere: boolean): string
function generate_isolines(grid: Float32Array, width: number, height: number, threshold: number, min_val: number, max_val: number): string
```

## `WasmVolumetricMesh` & `WasmRibbonMesh` (3D Meshes)

```typescript
class WasmVolumetricMesh {
  vertices(): Float32Array // flat [x, y, z, nx, ny, nz, u, v, ...]
  indices(): Uint32Array
  vertex_count(): number
  index_count(): number
}

class WasmRibbonMesh {
  vertices(): Float32Array // flat [x, y, z, nx, ny, nz, along_track, v, scalar, ...]
  indices(): Uint32Array
  vertex_count(): number
  index_count(): number
}

function generate_airspace_volume_mesh(polygon_flat_deg: Float64Array, floor_m: number, ceiling_m: number): WasmVolumetricMesh
function generate_trajectory_ribbon_mesh(waypoints_flat_deg: Float64Array, ribbon_width_m: number, scalars?: Float64Array): WasmRibbonMesh
```

## 8-Octant Label Anti-Cluttering Solver

```typescript
function solve_label_placements_flat(targets_flat: Float32Array, leader_length_px: number, safety_margin_px: number): Float32Array
function solve_label_placements_json(targets_json: string, leader_length_px: number): string
```

## Grid and fill bindings

`WasmTerrainEngine` exposes `load_elevation_grid(id, epsg, bounds, width, height, samples, nodata?)`, `unload_elevation_grid(id)` and `revision()`. Bounds are x/y degrees for EPSG:4326 or metres for EPSG:3857/900913. See [WCS terrain](../guides/maps/wcs-terrain.md).

The free function `triangulate_polygon_rings(coords: Float64Array, ring_lengths: Uint32Array): Float64Array` returns packed triangle coordinates. Its [fill contract](../guides/maps/wfs.md#polygon-fills-and-interior-holes) defines closure, holes, counts and limits. These engine bindings do not publish the entire Map Core crate.
