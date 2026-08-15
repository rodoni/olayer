# Core API Binding Inventory

This inventory records the binding decision for each public core area. It is
intentionally module-level; signatures remain in `docs/api_reference.md`.

| Core area | WASM | C-FFI | Notes |
|---|---|---|---|
| `geodesy` coordinates and ECEF | Exposed | Exposed where used by FFI operations | Radians internally; degree conversion only at documented boundaries |
| `geodesy` solvers | Exposed through core operations | Exposed through profile/interpolation operations | No standalone solver object at either boundary |
| `camera` | Exposed through `WasmCameraState` and projection matrices | Exposed through native controller APIs | Shared camera fixtures remain a follow-up |
| `projections` | Exposed through `WasmProjection` | Exposed through native controller APIs | All angles are radians |
| `terrain` tile/cache/elevation | Exposed through `WasmTerrainEngine` | Exposed through terrain functions | Status and MSAW APIs are exposed |
| `terrain` profiles | Exposed as flat arrays/status JSON | Exposed through allocated `C_ProfilePoint` arrays | Matching free functions are required |
| `interpolator` target state | Exposed through `WasmInterpolationEngine` | Exposed through update/remove functions | IDs are copied into boundary-owned allocations |
| `interpolator` prediction status | Exposed as JSON batch | Exposed as quality integer | Quality values are documented in `docs/conformance.md` |
| `sld` parser and styles | Exposed through `WasmStyleRegistry` | Intentionally unavailable | Native hosts may parse styles independently |
| `symbol_registry` | Exposed through `WasmSymbolRegistry` | Intentionally unavailable | Native rendering uses native registry paths |

Changes to a public core module must update this table and the generated/API
reference checks before merging.
