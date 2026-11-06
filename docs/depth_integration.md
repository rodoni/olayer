# Shared map depth and terrain occlusion

WMS imagery, WFS geometry and sampled terrain can share one depth buffer in the browser and native SDKs. In 2.5D/3D, opaque surfaces hide geometry beneath or behind them regardless of draw order. In 2D, WMS/WFS preserve layer order and do not test or write depth. TerrainLayer remains a 2.5D/3D browser layer.

## Browser

`WmsLayer`, `WfsLayer`, `TerrainLayer`, `TerrainContourLayer` and raster tiles now establish and restore depth/blend state around their draws. They use LEQUAL with depth writes in 2.5D/3D. WMS and raster imagery use a small polygon depth bias to avoid coplanar fighting. Completely transparent WMS pixels are discarded before writing depth. The default framebuffer must have a depth attachment, as the controller's WebGL2 context does by default.

The controller clears depth to 1 with depth writes enabled at the start of every frame, including when an application left depth writes disabled. All participating layers use depth range 0..1. The helper restores the host's depth enable/function/mask/range, blend enable/functions/equations, line width and polygon offset settings after drawing.

For terrain/imagery/feature compositing, load terrain into `controller.terrainEngine`, enable WMS `drapeTerrain`, and add the layers to the existing manager. Opaque surfaces interact through depth; order still matters for 2D and partially transparent content. WFS `heightOffset` can separate ground markings from the terrain surface. WMS has a depth bias rather than changing sampled physical height.

Terrain and contour sampling now preserves unknown/nodata cells. Such cells emit no terrain triangles or contour segments, so they cannot occlude valid features with an invented sea-level surface. Terrain color is opaque at the default layer opacity; set `TerrainLayer.opacity` explicitly for translucency. Previously the shader always multiplied opacity by 0.82.

## Native

The existing `WgpuGpuPipeline::new` keeps its pass-without-depth configuration for compatibility. Opt into a shared depth format and use the same format for WFS:

```rust,ignore
let depth_format = wgpu::TextureFormat::Depth32Float;
let raster = WgpuGpuPipeline::with_depth(&device, color_format, Some(depth_format));
let features = WgpuWfsPipeline::new(&device, color_format, Some(depth_format));
// Allocate a depth texture with RENDER_ATTACHMENT usage at the color target's
// width/height. Recreate it when the target resizes. Clear it to 1 each frame.
```

Render terrain, `WgpuWmsImage`, raster tiles and WFS in a pass with matching color/depth formats and sample count 1. Use the engine's view-projection matrix for both pipelines. Native grid/raster/terrain shaders now convert OpenGL clip-space z (-w..w) to WGPU z (0..w), as WFS already does. The terrain is opaque. Fully unknown cells and fully transparent raster pixels are discarded and do not write depth.

WMS and WFS retain their current display dimension during creation/rebuild. Their 2D pipelines use Always with depth writes disabled; their 2.5D/3D pipelines use LessEqual with writes enabled. The native map pipeline likewise tracks mode during grid/terrain rebuilds and raster upload, so hosts should rebuild after camera/mode/terrain changes as before. Do not mix geometry generated for different dimensions in one frame.

Raster depth bias is negative one unit/slope for coplanar surface imagery. The host owns attachments, resize handling, clearing and pass order. Existing demos/hosts using `new` must explicitly adopt the depth attachment contract. UI pipelines without depth can render in a subsequent color-only pass that loads the existing color attachment.

## Terrain updates and compatibility

`TerrainEngine::revision()` changes on successful terrain loads, removals, cache clearing and capacity changes. It is a wrapping u32 invalidation token, not a globally unique version or timestamp. WASM exposes `revision()` and the generated C header exposes `olayer_terrain_engine_revision`. Invalid loads leave the installed data/revision unchanged. Native/browser sampled terrain caches include this token, so replacing a WCS grid or removing terrain updates occlusion without moving the camera.

Browser terrain mesh keys now use exact camera values instead of rounded values. Globe terrain extents in both SDKs use the camera's 15,000-km/zoom distance scale (a 30,000-km/zoom sampling span before conversion), rather than the unrelated planar viewport width. The local extent is still capped at 60 degrees; these are sampled local patches, not a complete globe terrain LOD system. Unknown coverage remains a hole. Flat terrain normals are rotated into ECEF for browser globe lighting.

Compatibility changes include actual unknown-terrain holes instead of sea-level/purple placeholders, opaque terrain by default, corrected native depth mapping, and depth participation of browser imagery and tiles in 2.5D. Native hosts can continue without a depth attachment via `new`, or migrate explicitly via `with_depth`.

## Reproducible verification

`tests/fixtures/depth/scene.json` defines the same ellipsoidal height and camera scenario for both SDKs. The tests combine a WCS-backed terrain surface, WFS point markers above/below it and opaque/transparent WMS imagery. They verify both draw orders in all three modes, preserved 2D layer order, no invisible depth barriers from transparent imagery, and terrain revision invalidation after an all-nodata replacement. The browser additionally starts with deliberately unusual GL state and verifies that it is restored after rendering.

```sh
cargo test --workspace --exclude olayer-desktop-demo
cargo test -p olayer-native terrain_and_imagery_occlude_wfs_in_shared_depth -- --ignored --nocapture
cd sdk/ts
npm run test:run
npm run typecheck
npm run test:depth:browser
```

Both GPU checks are included in CI. Geometry tests cover missing-cell indexing, failed projections and ECEF normals. C-FFI tests cover revision updates and failed-load preservation.

## Remaining limits

Occlusion follows the sampled meshes and reference ellipsoid. Adaptive terrain tessellation, complete globe terrain coverage, arbitrary ridge/overhang production scenes and production manual visual checks remain. Partially transparent surfaces still require appropriate host ordering; there is no order-independent transparency. Other visualization pipelines, such as volumetric rendering and screen overlays, need their own compatible depth/pass choices. Visual exaggeration should remain consistent across geometry; these integration checks use exaggeration 1. GPU resource creation/destruction remains owned by the existing layer/handle lifecycle.
