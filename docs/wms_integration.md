# WMS rendering integration

WMS images can now be rendered by the browser layer stack and the native WGPU raster pipeline. The source fetches one image; changing display dimension rebuilds geometry without changing the request or downloading the image again.

## Coordinate and terrain contract

- Bounds are `[west, south, east, north]` in CRS units. EPSG:4326 uses longitude/latitude degrees; EPSG:3857 and EPSG:900913 use Web Mercator metres.
- WMS 1.3.0 request serialization swaps EPSG:4326 bounds into latitude/longitude axis order. This corrects the previous x/y wire serialization. Applications that manually pre-swapped bounds must now supply ordinary x/y bounds.
- Image UV `(0, 0)` is the north-west corner. Rows progress southward.
- The image is tessellated into a grid (16 cells per axis by default, configurable from 1 through 64).
- 2D uses the controller projection and zero height. 2.5D uses projected x/y and terrain height when draping is enabled. 3D uses WGS84 ECEF and clips triangles to the camera-facing ellipsoid tangent plane, interpolating UVs at the horizon.
- Draping is opt-in. Without it, imagery lies at zero ellipsoidal height. With it, terrain is queried through the existing terrain engine; unknown samples leave holes. The renderer does not silently convert missing elevation to zero.
- Extents that cross the antimeridian must be split into separate images. Unsupported CRS and out-of-domain bounds are rejected.

The horizon test models the reference ellipsoid. Exact terrain occlusion, depth interaction with other layers, adaptive tessellation, and production visual validation remain follow-up work.

## Browser

```ts
import { OgcHttpClient, WmsMapSource, WmsLayer } from "olayer-sdk";

const source = new WmsMapSource(new OgcHttpClient(), {
  endpoint: "https://maps.example/ows",
  layers: "imagery",
  crs: "EPSG:4326",
  bounds: [-47, -24, -46, -23],
  width: 512,
  height: 512,
});
const layer = new WmsLayer("imagery", source, {
  subdivisions: 32,
  drapeTerrain: true,
});
// Load terrain into controller.terrainEngine before enabling draping.
await layer.load(controller.gl, abortController.signal);
controller.layerManager.addLayer(layer);
controller.triggerActive();
```

`WmsLayer.load` owns and closes decoded bitmaps after upload, replaces existing textures only after a successful upload, and ignores obsolete loads after another load or destruction. Call `destroy(gl)` when removing the layer; the controller also destroys its active layers during teardown. Geometry is rebuilt on each static render so projection, mode, camera, and terrain updates take effect. The host retains its depth configuration.

## Native

```rust,ignore
use olayer_native::{WgpuWmsImage, WmsMeshOptions};

let image = source.fetch()?; // WmsMapSource<T>, with injected NativeOgcClient<T>
let mut gpu_image = WgpuWmsImage::new(
    &device, &queue, &pipeline, &image, &controller,
    WmsMeshOptions { subdivisions: 32, drape_terrain: true },
)?;
// After changing the camera, projection, view mode, or loaded terrain:
gpu_image.rebuild(&device, &controller)?;
// Update pipeline.uniform_buffer with the controller's current view-projection matrix,
// then draw inside the same color pass as the existing raster tiles:
gpu_image.render(&pipeline, &mut render_pass);
```

Dropping `WgpuWmsImage` releases its GPU resources. The source rejects conflicting CRS metadata and responses whose decoded dimensions differ from the request. GPU upload validates RGBA byte length and device texture limits. `build_wms_mesh` is available separately for geometry inspection without a GPU.

## Reproducible verification

`tests/fixtures/wms/` contains a shared four-color PNG, geographic bounds, and a 500-metre Mapbox RGB terrain sample. These fixtures are entirely local.

```sh
cargo test --workspace --exclude olayer-desktop-demo
# Requires a headless WGPU adapter; Mesa software rendering is sufficient.
cargo test -p olayer-native wms_image_renders_into_offscreen_pixels_in_all_modes -- --ignored --nocapture
cd sdk/ts
npm run test:run
npm run typecheck
# Starts a local Vite fixture server and headless Chrome through Puppeteer.
npm run test:wms:browser
```

The native offscreen test fetches and decodes the fixture through `WmsMapSource`, uploads it once, changes display modes, draws through the raster pipeline, and reads the framebuffer. The browser test uses `WmsMapSource`, `WmsLayer`, the actual controller terrain engine, and `LayerManager`; it checks corner colors at their projected geographic locations in each mode and asserts that exactly one correctly ordered WMS request was made. Unit tests cover terrain heights and holes, image orientation, clipping, CRS conversion, cancellation, failed uploads, and cleanup.

WCS JSON coverage-to-terrain integration is documented in [wcs_terrain_integration.md](wcs_terrain_integration.md). The GPU pixel checks now drape imagery over the shared WCS elevation fixture. WFS point, line and polygon-outline rendering is documented in [wfs_integration.md](wfs_integration.md); phase 5 remains partial.
