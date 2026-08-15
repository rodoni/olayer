# Binding Conformance Matrix

The Rust core is the behavioral contract. Boundary APIs may intentionally omit
features, but each omission must be recorded here rather than inferred from
documentation.

| Contract | Rust core | WASM | C-FFI | Boundary rule |
|---|---|---|---|---|
| Angles | Radians | Radians for camera/interpolation; degrees for terrain convenience APIs | Radians for interpolation and `_rad` terrain APIs; degrees for unsuffixed terrain APIs | Names and docs identify degree/radian boundaries |
| Heights | Metres above WGS84 ellipsoid | Metres | Metres | No implicit unit conversion |
| Invalid state | `Result<T, Error>` | `JsValue` error | Negative status code | Errors never become valid data |
| Prediction status | `InterpolationBatch` | JSON value from `interpolate_all_with_status` | Quality integer on `C_InterpolatedTarget` | `0 valid`, `1 stale`, `2 clock-skewed`, `3 unavailable` |
| Unknown terrain | `ElevationSample { elevation_meters: None }` | `{ elevation_meters: null }` | Status `1`, output value unused | Legacy elevation methods retain `0.0` compatibility behavior |
| Ownership | Rust-owned values dropped by Rust | WASM objects require `free()` | Returned arrays require matching `_free()` | Allocator and deallocator stay on the same side |
| Camera views | 2D, 2.5D, 3D matrices | Exposed through `WasmProjection` | Native controller uses the same core camera contract | Shared camera fixtures remain pending |

## Verification

- Rust tests exercise core units, prediction status, terrain null handling, and
  C-FFI error codes.
- `wasm-pack build --target web` regenerates the WASM declarations.
- Native builds regenerate `sdk/native/libolayer_native.h`; CI checks that the
  generated header is committed and current.

The matrix is intentionally compact. Detailed signatures remain in
[`api_reference.md`](api_reference.md).

The module-level exposure inventory is maintained in
[`core_api_inventory.md`](core_api_inventory.md).
