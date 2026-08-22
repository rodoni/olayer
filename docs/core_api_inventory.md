# Core API Binding Inventory

This inventory records the binding decision for each public core area. It is
intentionally module-level; signatures remain in `docs/api_reference.md`.

| Core area | WASM | C-FFI | Notes |
|---|---|---|---|
| `geodesy` coordinates and ECEF | Exposed | Exposed where used by FFI operations | Radians internally; degree conversion only at documented boundaries |
| `geodesy` solvers | Exposed through core operations | Exposed through profile/interpolation operations | No standalone solver object at either boundary |
| `camera` | Exposed through `WasmCameraState` and projection matrices | Exposed through native controller APIs | Shared camera fixtures remain a follow-up |
| `projections` | Exposed through `WasmProjection` | Exposed through native controller APIs | All angles are radians |
| `terrain` tile/cache/elevation | Exposed through `WasmTerrainEngine` | Exposed through terrain functions | DTED, Mapbox/Terrarium RGB tiles, and COG/GeoTIFF ingestion, Status and MSAW APIs |
| `terrain` profiles | Exposed as flat arrays/status JSON | Exposed through allocated `C_ProfilePoint` arrays | Matching free functions are required |
| `interpolator` target state | Exposed through `WasmInterpolationEngine` | Exposed through update/remove functions | IDs are copied into boundary-owned allocations |
| `interpolator` prediction status | Exposed as JSON batch | Exposed as quality integer | Quality values are documented in `docs/conformance.md` |
| `sld` parser and styles | Exposed through `WasmStyleRegistry` | Intentionally unavailable | Native hosts may parse styles independently |
| `symbol_registry` | Exposed through `WasmSymbolRegistry` | Intentionally unavailable | Native rendering uses native registry paths |
| `aeronautical` data ingestion | Exposed through `WasmAeronauticalDataset` | Exposed through `olayer_aeronautical_*` | AIXM 5.1 XML and GeoJSON-Aviation parsing, spatial queries, and GeoJSON export |
| `weather` overlays | Exposed through `WasmSigmetDataset`, `colorize_dbz_grid`, `generate_wind_barb_geometry`, `generate_isolines` | Exposed through `olayer_weather_*` and `olayer_sigmet_*` | Radar dBZ colorization (NEXRAD/ICAO), aviation wind barbs, Marching Squares isolines, SIGMET/AIRMET hazard polygon containment |
| `volumetric` airspaces & ribbons | Exposed through `WasmVolumetricMesh`, `WasmRibbonMesh`, `generate_airspace_volume_mesh`, `generate_trajectory_ribbon_mesh` | Exposed through `olayer_volumetric_*` | 3D extruded airspace volume mesh generation (sidewalls + caps) and 3D flight trajectory ribbon extrusion with altitude/scalar gradient mapping |
| `declutter` label anti-cluttering | Exposed through `solve_label_placements_flat`, `solve_label_placements_json` | Exposed through `olayer_declutter_solve_labels` | 8-octant force-directed label anti-cluttering engine with spatial grid partitioning and heading deconfliction |

Changes to a public core module must update this table and the generated/API
reference checks before merging.
