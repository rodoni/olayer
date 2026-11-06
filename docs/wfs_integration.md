# WFS feature rendering

The browser `WfsLayer` and native `WgpuWfsFeatures` render an already-fetched WFS GeoJSON collection in 2D, 2.5D and 3D. Changing camera, projection, terrain or display mode rebuilds geometry without another request. Feature identifiers, properties and original geometries stay on the decoded collection.

## Geometry and coordinate contract

- Supported geometries: Point, MultiPoint, LineString, MultiLineString, Polygon, MultiPolygon and GeometryCollection. Null geometries are skipped. Points draw crosses in projected metres or the local ECEF tangent plane. Lines and independent polygon rings draw one-pixel strokes. Polygon holes are outlined separately; this increment does not fill polygons or apply per-feature styles.
- Source positions are `[x,y]` or `[x,y,height]`. EPSG:4326 uses longitude/latitude degrees. EPSG:3857 and EPSG:900913 use Web Mercator metres. The SDK adapters normalize to latitude/longitude radians. ECEF source coordinates and additional measure axes are rejected.
- The request CRS is explicit. Named GeoJSON CRS metadata must match it. `EPSG:4326`, `urn:ogc:def:crs:EPSG::4326` and `http(s)://www.opengis.net/def/crs/EPSG/0/4326` are recognized forms (likewise for the other supported EPSG codes). Other metadata forms are rejected. Without metadata, the response is interpreted in the requested CRS.
- Closed rings need at least four positions; nonempty lines need at least two. Positions must be finite and within the CRS domain. Invalid geometry rejects the collection, preserving an installed browser collection. Paths crossing the antimeridian must be split before rendering.
- Each source segment is subdivided linearly in normalized longitude/latitude and height, with 16 subdivisions by default (1..64). This approximates curved surfaces; it is not geodesic interpolation or adaptive terrain tessellation. The source-coordinate and generated-work budgets are each limited to 1,000,000 vertices, and geometry nesting is limited to 16 levels.
- 2D always uses zero height. 2.5D uses projected x/y and resolved height. 3D converts resolved positions to WGS84 ECEF and clips line segments against the camera-facing ellipsoid tangent plane. The camera attitude affects this horizon test.

Heights are metres above the ellipsoid. Altitude modes are:

| Mode | Resolved height in 2.5D/3D |
|---|---|
| absolute (default) | source height + height offset |
| clamp-to-ground | terrain height + height offset |
| relative-to-ground | terrain height + source height + height offset |

Missing source heights default to zero. Missing terrain breaks a draped line or hides a point; it never joins across the gap or invents zero ground height. Load WCS terrain through the [coverage adapter](wcs_terrain_integration.md) or an existing terrain provider first. The optional height offset is a visual placement adjustment, not a datum conversion.

## Browser

```ts
import { WfsLayer, WfsVectorSource, OgcHttpClient } from "olayer-sdk";

const features = new WfsLayer("roads", new WfsVectorSource(new OgcHttpClient(), {
  endpoint: "/ows", typeName: "roads", crs: "EPSG:4326",
}), {
  altitudeMode: "relative-to-ground",
  heightOffset: 2,
  color: [0, 1, 0, 1],
  subdivisions: 16,
  pointRadius: 1000,
});
await features.load(abortController.signal);
controller.layerManager.addLayer(features);
controller.triggerActive();
// When removing the layer:
controller.layerManager.removeLayer(features.id);
features.destroy(controller.gl);
```

The layer validates the complete collection before replacement. Failed loads retain existing features; obsolete completions after a newer load or destruction are discarded. Options and source request bounds are snapshotted. GPU resources are created on the first nonempty draw and released by `destroy` or controller teardown. A layer belongs to one WebGL context. Rebuilding geometry on a static render follows camera, projection and terrain changes.

The layer enables depth testing/writes with LEQUAL in 2.5D/3D and disables depth testing for its 2D draw. It restores the host's depth, blending and line-width settings afterward. The host must provide/clear a depth buffer for cross-layer occlusion.

## Native

```rust,ignore
use olayer_native::{WgpuWfsFeatures, WgpuWfsPipeline, WfsMeshOptions, WfsAltitudeMode};

let collection = source.fetch()?; // WfsFeatureSource<T>
let pipeline = WgpuWfsPipeline::new(&device, color_format, Some(wgpu::TextureFormat::Depth32Float));
let mut features = WgpuWfsFeatures::new(&device, collection, &controller, WfsMeshOptions {
    altitude_mode: WfsAltitudeMode::RelativeToGround,
    height_offset: 2.0,
    ..Default::default()
})?;
// After camera/projection/mode/terrain updates:
features.rebuild(&device, &controller)?;
pipeline.set_view(&queue, &view_projection_matrix, [0.0, 1.0, 0.0, 1.0])?;
// In a render pass with matching color and depth formats:
features.render(&pipeline, &mut pass);
```

Dropping the feature handle releases GPU resources. `collection()` exposes the retained feature properties and identifiers; `build_wfs_mesh` provides geometry without a GPU. The native shader maps the engine's OpenGL clip-space depth to WGPU's 0..1 range. A configured depth pipeline writes depth using LessEqual. Supply `None` only for a pass without depth; the host owns pass attachments, clearing and layer order. The pipeline uniform stores one view/color per submission: use separate pipelines for differently styled batches in one pass. Rebuild validates and computes new geometry before replacing the current GPU buffer.

## Verification and remaining work

`tests/fixtures/wfs/features.json` includes a point with altitude, a route with changing altitude, and a polygon with a separate interior ring. Tests cover coordinate validation, all geometry families, height modes, unknown terrain, Mercator equivalence, horizon clipping, property preservation and conflicting CRS metadata.

Browser and native GPU pixel tests fetch the collection once, load WCS terrain, switch through all three view modes and check pixels at the geographic positions of the point, route and both polygon rings. Native checks use a depth attachment; browser checks also verify failed replacement and destruction during a pending load. Both tests are included in CI.

```sh
cargo test --workspace --exclude olayer-desktop-demo
cargo test -p olayer-native wfs_features_render_into_offscreen_pixels_in_all_modes -- --ignored --nocapture
cd sdk/ts
npm run test:run
npm run typecheck
npm run test:wfs:browser
```

Polygon fills, style/property mapping, adaptive tessellation, antimeridian splitting, terrain-aware occlusion, integration with every existing layer's depth pass, and production manual visual checks remain follow-up work. Phase 5 remains partial.
