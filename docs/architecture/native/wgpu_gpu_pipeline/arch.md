# Native GPU map pipeline

Source: [wgpu_gpu_pipeline](../../../../sdk/native/src/wgpu_gpu_pipeline/mod.rs), [WMS](../../../../sdk/native/src/wgpu_gpu_pipeline/wms.rs), [WFS](../../../../sdk/native/src/wgpu_gpu_pipeline/wfs.rs). The host creates the device/surface, owns attachments and resize handling, and submits passes; the SDK does not require a window for offscreen GPU use.

## Resources and representations

`WgpuGpuPipeline` owns grid, raster and terrain pipelines, their bind groups/uniforms, GPU tile buffers/cache and sampled terrain buffers. Grid vertices are three f32 position values. Raster vertices contain position plus UV. Terrain vertices contain position, normal, elevation, slope and a terrain-known flag.

The grid/view uniform allocation is 272 bytes, with the color entry at offset 256 for binding alignment; the terrain-style buffer is 32 bytes. WFS uses its own 96-byte matrix/stroke/fill-color uniform and separate line/triangle buffers/pipelines. Use the [native API reference](../../../reference/native.md) and source for exact layouts/signatures rather than copying them into hosts.

## Coordinates and depth

Geographic input is converted to projected metres for planar views or WGS84 ECEF for 3D. The engine matrices use OpenGL clip z; the native shaders convert it to WGPU's 0..w range. `new` retains the configuration without depth, while `with_depth` requires matching pass attachments. 2D variants preserve layer order; perspective variants use LessEqual and depth writes.

Raster imagery uses a small constant depth bias with zero slope bias. Fully transparent raster/fill fragments and unknown terrain cells are discarded. WFS fills are drawn before outlines and preserve interior holes. See the maintained [depth contract](../../../guides/maps/depth.md) and [WFS guide](../../../guides/maps/wfs.md).

## Sampling and invalidation

Native terrain samples a 32-cell-per-axis local grid. Heights/nodata come from the shared engine; normals/slopes use available samples, and ECEF normals rotate from the local tangent frame. Globe sampling extent uses the globe camera scale, while planar views use the viewport scale. Fixed patches are not complete globe LOD.

The terrain cache key includes exact camera/projection-relevant values and terrain revision. WMS/WFS handles rebuild after view or terrain changes; WFS caches source triangulation so topology is not recomputed on every view change. Handles/buffers release resources when dropped.

## Validation

[Testing](../../../guides/testing.md#gpu-integration-checks) covers geometry, texture/feature placement, holes, transparency and shared terrain/imagery/feature occlusion in offscreen rendering. Windowed demo and production visual checks remain distinct host validation tasks.
