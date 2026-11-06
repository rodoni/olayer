# Development guide

Follow [getting started](../getting-started/README.md) for setup and [testing](testing.md) for verification. This guide covers implementation conventions and host integration; API lookup lives in the [reference](../reference/README.md).

## Repository boundaries

| Path | Responsibility |
|---|---|
| `core/` | Pure Rust geodesy, projections, camera, terrain, interpolation, symbols and overlay math |
| `map-core/` | Pure OGC request/response contracts, CRS metadata, tile matrices and capabilities |
| `sdk/ts/wasm/` | wasm-bindgen boundary for the mathematical engine |
| `sdk/ts/src/` | Browser controller, providers, WebGL/Canvas layers and tools |
| `sdk/native/src/` | Native controller/providers, WGPU/egui integration and C-FFI |
| `sdk/native/demo/` | Windowed desktop application |
| `tools/symbol-compiler/` | SVG-to-declarative-library compiler |

Core and Map Core do not perform network or filesystem I/O. SDK providers fetch/decode data and pass validated buffers to the engines. See the [architecture](../architecture/README.md) for diagrams and design decisions.

## Conventions and ownership

- Use radians internally for geodetic angles and `f64` for geodetic math. GPU matrices are column-major `[f32; 16]`.
- `LatLon.height` is metres above the ellipsoid. Orthometric heights need an explicit vertical-reference conversion. [Terrain altitude modes](terrain.md) are a separate placement policy.
- Degree-based convenience APIs are explicit exceptions; see the [binding contract](../reference/bindings.md).
- Core functions return typed `Result` errors with stable module codes. WASM converts failures to `JsValue`; C statuses are function-specific.
- Release WASM objects with `.free()`. `controller.getCameraState()` returns a temporary object the caller must free. Let `controller.destroy()` own teardown of its active layers/engines; release separately owned resources before their engine/context is destroyed.
- C allocations must use their matching free function. Query/caller-owned buffers are described in the generated header.
- Pass a controller through `LayerRenderContext`; avoid global controller lookups and `as any` casts.

## Host integration

Minimal browser host, after supplying two canvas elements:

```ts
import init, { OlayerController, WasmProjection } from "olayer-sdk";

await init();
const controller = new OlayerController({
  glCanvas,
  canvas2D,
  projection: WasmProjection.new_stereographic(0, 0),
  initialCenterLatRad: 0,
  initialCenterLonRad: 0,
});
controller.startLoop();
// On application teardown:
controller.destroy();
```

Use [WMS](maps/wms.md), [WFS including polygon fills](maps/wfs.md) and [WCS terrain](maps/wcs-terrain.md) for complete source/layer examples. The [depth contract](maps/depth.md) explains shared attachments, draw order and transparency limits.

Native host sketch:

```rust,ignore
use olayer_native::{NativeController, WgpuGpuPipeline};

let controller = NativeController::new(0.0, 0.0)?;
let pipeline = WgpuGpuPipeline::with_depth(
    &device, color_format, Some(wgpu::TextureFormat::Depth32Float),
);
// The host owns attachments, resize handling, frame submission and the event loop.
```

A pipeline created with `new` retains its pass-without-depth configuration. Use a matching attachment when opting into depth. Keep UI/overlay pass choices explicit.

## Extending the engines and SDKs

When changing a core API, review both `sdk/ts/wasm/src/lib.rs` and `sdk/native/src/c_ffi_bridge/mod.rs`, then update the [exposure inventory](../reference/bindings.md#core-api-exposure-inventory). Native builds regenerate the C header; wasm-pack regenerates browser declarations.

Camera logic is coordinated by both `sdk/ts/src/controller/index.ts` and `sdk/native/src/native_controller/mod.rs`. Keep mode, angle, projection-center and altitude behavior aligned. Update the projection center when the camera moves; Stereographic and LCC depend on it, while Web Mercator does not.

To add a projection, implement `Projection` in `core/src/projections/`, expose the needed boundary factories, and test round trips, limits and camera matrices. To add a symbol provider, implement `SymbologyProvider`, register/re-export it, and test resolution and registry integration. See the [symbol workflow](symbols.md) for the declarative path.

## Data quality and cache invalidation

Use `interpolate_all_with_status` when consumers need to distinguish valid predictions from stale, clock-skewed or unavailable data. Use terrain status queries to distinguish unknown ground from measured zero. Legacy scalar elevation getters retain their zero fallback.

Terrain revision tokens invalidate sampled geometry after loads/removals. Rebuild native GPU handles after camera/projection/mode/terrain changes; browser map layers rebuild during static rendering. Do not silently substitute zero for missing terrain or join lines across missing samples.

Tile providers accept `AbortSignal` and retry options and expose cache statistics. Explicit OGC source/layer loads have their own lifecycle contracts; consult their guide rather than assuming every provider implements the tile interface.
