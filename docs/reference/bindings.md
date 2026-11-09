# Binding contracts and exposure

The Rust core is the behavioral contract. Boundary APIs may intentionally omit
features, but each omission must be recorded here rather than inferred from
documentation.

| Contract | Rust core | WASM | C-FFI | Boundary rule |
|---|---|---|---|---|
| Angles | Radians | Radians for camera/interpolation; degrees for terrain convenience APIs | Radians for interpolation and `_rad` terrain APIs; degrees for unsuffixed terrain APIs | Names and docs identify degree/radian boundaries |
| Heights | Metres; `Height` identifies Ellipsoidal or Orthometric datum | Metres | Metres | No implicit unit conversion; terrain altitude modes resolve explicitly |
| Invalid state | `Result<T, Error>` | `JsValue` error | Negative status code | Errors never become valid data |
| Prediction status | `InterpolationBatch` | JSON value from `interpolate_all_with_status` | Quality integer on `C_InterpolatedTarget` | `0 valid`, `1 stale`, `2 clock-skewed`, `3 unavailable` |
| Unknown terrain | `ElevationSample { elevation_meters: None }` | `{ elevation_meters: null }` | Status `1`, output value unused | Legacy elevation methods retain `0.0` compatibility behavior |
| Altitude modes | `AltitudeMode` + `AltitudeUnknownPolicy` | String mode/policy in `resolve_altitude` | Integer mode/policy codes | `Absolute` is the default; visual exaggeration is not applied |
| Ownership | Rust-owned values dropped by Rust | Generated WASM handles require `free()`; returned strings/typed arrays are JS-owned copies | Returned arrays require matching `_free()`; Map Core output uses caller-owned buffers | Allocator and deallocator stay on the same side; Map Core wire facade retains no inputs |
| Camera views | 2D, 2.5D, 3D matrices | Exposed through `WasmProjection` | Native controller uses the same core camera contract | Shared attitude/pole/antimeridian fixtures are implemented; all-FFI test gate remains |

## Verification

- Rust tests exercise core units, prediction status, terrain null handling, and
  C-FFI error codes.
- `wasm-pack build --target web` regenerates the WASM declarations.
- Native builds regenerate `sdk/native/libolayer_native.h`; CI checks that the
  generated header is committed and current.
- `npm run test:package` installs the generated SDK tarball in an isolated
  consumer and checks declarations plus ESM/CommonJS WASM loading; this is a
  smoke gate, not a substitute for publishing or a downstream application test.

The matrix is intentionally compact. Layer-specific API summaries are indexed in [the reference](README.md); exact signatures come from generated declarations/header and source.

The module-level exposure inventory follows below.

## Core API exposure inventory

This inventory records the binding decision for each public core area. It is
intentionally module-level; signatures are indexed in [the reference](README.md).

| Core area | WASM | C-FFI | Notes |
|---|---|---|---|
| `geodesy` coordinates, ECEF, and vertical references | Coordinate operations exposed; no standalone `Height`/`VerticalDatum` wrapper | Coordinate operations exposed; no standalone `Height`/`VerticalDatum` wrapper | Vertical-reference types remain in Rust Core; radians internally and no terrain dependency |
| `geodesy` solvers | Exposed through core operations | Exposed through profile/interpolation operations | No standalone solver object at either boundary |
| `camera` | Exposed through `WasmCameraState` and projection matrices | Exposed through native controller APIs | Shared attitude/pole/antimeridian fixtures exercise Core and real WASM; no separate C camera conformance export is defined |
| `projections` | Exposed through `WasmProjection` | Exposed through native controller APIs | All angles are radians |
| `terrain` tile/cache/elevation/altitude modes | Exposed through `WasmTerrainEngine.resolve_altitude` | Exposed through `olayer_terrain_engine_resolve_altitude` | DTED, Mapbox/Terrarium RGB tiles, COG/GeoTIFF ingestion, status/MSAW, `Absolute`, `ClampToGround`, `RelativeToGround`, `RelativeToMesh` |
| `terrain` profiles | Exposed as flat arrays/status JSON | Exposed through allocated `C_ProfilePoint` arrays | Matching free functions are required |
| `interpolator` target state | Exposed through `WasmInterpolationEngine` | Exposed through update/remove functions | IDs are copied into boundary-owned allocations |
| `interpolator` prediction status | Exposed as JSON batch | Exposed as quality integer | Quality values are listed in the contract table above |
| `sld` parser and styles | Exposed through `WasmStyleRegistry` | Intentionally unavailable | Native hosts may parse styles independently |
| `symbol_registry` | Exposed through `WasmSymbolRegistry` | Intentionally unavailable | Native rendering uses native registry paths |
| `aeronautical` data ingestion | Exposed through `WasmAeronauticalDataset` | Exposed through `olayer_aeronautical_*` | AIXM 5.1 XML and GeoJSON-Aviation parsing, spatial queries, and GeoJSON export |
| `weather` overlays | Exposed through `WasmSigmetDataset`, `colorize_dbz_grid`, `generate_wind_barb_geometry`, `generate_isolines` | Exposed through `olayer_weather_*` and `olayer_sigmet_*` | Radar dBZ colorization (NEXRAD/ICAO), aviation wind barbs, Marching Squares isolines, SIGMET/AIRMET hazard polygon containment |
| `volumetric` airspaces & ribbons | Exposed through `WasmVolumetricMesh`, `WasmRibbonMesh`, `generate_airspace_volume_mesh`, `generate_trajectory_ribbon_mesh` | Exposed through `olayer_volumetric_*` | 3D extruded airspace volume mesh generation (sidewalls + caps) and 3D flight trajectory ribbon extrusion with altitude/scalar gradient mapping |
| `declutter` label anti-cluttering | Exposed through `solve_label_placements_flat`, `solve_label_placements_json` | Exposed through `olayer_declutter_solve_labels` | 8-octant force-directed label anti-cluttering engine with spatial grid partitioning and heading deconfliction |
| `terrain` scalar grids/revision | `WasmTerrainEngine.load_elevation_grid`, `unload_elevation_grid`, `revision` | Matching `olayer_terrain_engine_*` functions | [WCS terrain](../guides/maps/wcs-terrain.md) documents CRS, nodata and ownership |
| `volumetric` planar polygon interiors | `triangulate_polygon_rings` | `olayer_triangulate_polygon_rings` with caller-owned output/size query | [WFS fills](../guides/maps/wfs.md#polygon-fills-and-interior-holes) use the same pure tessellator |
| `olayer-map-core` request/response facade (wire v1) | Exposes `map_build_request_json`, `map_parse_capabilities_json`, and `map_validate_model_json`; copied strings/byte slices and coded JavaScript `Error` (`module: "MAP"`) | Caller-buffer `olayer_map_*_json` operations; stable error suffix via caller-owned output; generated header and linked C consumer verified | WMS/WMTS/WFS/WCS URL DTOs, bounded capabilities summary, scalar coverage and feature-envelope validation only. No HTTP, credentials, codecs, renderer handles, topology repair or automatic WCS-to-terrain conversion. The legacy browser request builders remain SDK adapters; wire-v1 is the canonical cross-FFI contract. |

Map Core wire-v1 C operations use a two-call caller-buffer contract: `(NULL, 0)` queries the required byte count and returns `1`; insufficient capacity returns `1` without writing partial output; success returns `0`. Invalid ABI arguments return `-1`, Map Core validation failures return `-2` and set `error_code` to the stable numeric suffix `1..=12`, and a caught panic returns `-3`. `required_length` is mandatory; `error_code` is optional. The input is UTF-8 and limited to 1 MiB. Input, result and output parameters must be valid for the call and non-overlapping. Returned JSON is length-delimited UTF-8, not NUL-terminated. The consumer allocates and frees its own output storage.

Changes to a public core module must update this table and the generated/API
reference checks before merging.
