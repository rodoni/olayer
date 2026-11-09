# WFS feature rendering

The browser `WfsLayer` and native `WgpuWfsFeatures` render an already-fetched WFS GeoJSON collection in 2D, 2.5D and 3D. Changing camera, projection, terrain or display mode rebuilds geometry without another request. Feature identifiers, properties and original geometries stay on the decoded collection.

## Geometry and coordinate contract

- Supported geometries: Point, MultiPoint, LineString, MultiLineString, Polygon, MultiPolygon and GeometryCollection. Null geometries are skipped. Points draw crosses in projected metres or the local ECEF tangent plane. Lines and independent polygon rings draw one-pixel strokes. Polygon holes are outlined separately. Optional `fillPolygons`/`fill_polygons` fills validated interiors, preserving holes; see [polygon fills](#polygon-fills-and-interior-holes). A bounded opt-in property style sheet is available below.
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

Missing source heights default to zero. Missing terrain breaks a draped line or hides a point; it never joins across the gap or invents zero ground height. Load WCS terrain through the [coverage adapter](wcs-terrain.md) or an existing terrain provider first. The optional height offset is a visual placement adjustment, not a datum conversion.

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

The layer enables depth testing/writes with LEQUAL in 2.5D/3D and disables depth testing for its 2D draw. It restores the host's depth, blending and line-width settings afterward. The host must provide/clear a depth buffer for cross-layer occlusion; the browser controller now clears it with writes enabled each frame.

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

## Outline verification

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

See [shared depth](depth.md) for sampled terrain/imagery/feature occlusion and the [roadmap](../../plans/map-core.md) for remaining styling, coverage and publication work.

## Polygon fills and interior holes

WFS can now render filled Polygon/MultiPolygon geometries, including concave exteriors and interior holes, in the browser and native SDKs. GeometryCollection members are supported too. Fill is opt-in; existing point crosses, lines and outlines retain their behavior.

### Browser

```ts
const layer = new WfsLayer("areas", source, {
  fillPolygons: true,
  fillColor: [0.1, 0.6, 0.3, 1],
  color: [1, 1, 1, 1], // outlines, lines and point markers
  altitudeMode: "relative-to-ground",
  subdivisions: 16,
});
await layer.load(abortController.signal);
controller.layerManager.addLayer(layer);
controller.triggerActive();
```

`fillColor` defaults to `color`; both multiply alpha by the layer's opacity. Fills draw before outlines using the same shader/depth contract. Fully transparent fragments are discarded and write no depth. Partially transparent fills still require appropriate host ordering; there is no order-independent transparency.

The layer validates and prepares polygon topology during `load`. A failed fill/topology/work-limit validation retains the previous collection. Camera, projection and terrain changes reuse the prepared triangulation and rebuild only the sampled/projected geometry and horizon clipping. Destruction discards pending loads, cached triangles and GPU buffers.

### Native

```rust,ignore
let options = WfsMeshOptions {
    fill_polygons: true,
    altitude_mode: WfsAltitudeMode::RelativeToGround,
    ..Default::default()
};
let mut features = WgpuWfsFeatures::new(&device, collection, &controller, options)?;
// After changing camera/projection/mode/terrain:
features.rebuild(&device, &controller)?;
pipeline.set_view(&queue, &view_projection, [1.0, 1.0, 1.0, 1.0])?;
pipeline.set_fill_color(&queue, [0.1, 0.6, 0.3, 1.0])?;
features.render(&pipeline, &mut pass);
```

`set_view` resets fill color to the supplied stroke color; call `set_fill_color` afterward for a separate fill style. One pipeline stores one view/style per submission, as before. Fill and outline buffers share the same bind group, with triangle/line pipeline variants for planar layer order and perspective depth. The feature handle caches validated source triangles. `build_wfs_fill_mesh` exposes the same operation without a GPU.

### Bounded per-feature property styling

WFS layers accept an optional complete style sheet. When omitted, the existing layer-wide `color`/`fillColor` and `fillPolygons` behavior is unchanged. A style sheet cannot be mixed with those legacy paint options.

```ts
const layer = new WfsLayer("areas", source, {
  style: {
    defaultPaint: { visibility: "visible", stroke: [1, 1, 1, 1], fill: [0.1, 0.3, 0.8, 1] },
    priority: "first-match",
    rules: [{
      predicate: { kind: "equals", property: "class", value: "restricted" },
      paint: { visibility: "visible", stroke: [1, 0, 0, 1], fill: [0.8, 0.1, 0.1, 1] },
    }],
  },
});
```

Predicates access literal, case-sensitive own property names only. `exists` matches present null, scalar, array or object values; `equals` is strict JSON-scalar equality, with no coercion. Missing properties do not match. The first matching rule wins by default; `last-match` selects the final matching rule. Paints are complete rather than cascading: absent stroke/fill disables that channel. RGBA channels are finite normalized straight-alpha values; zero alpha emits no draw. Limits include 64 rules, 256 UTF-8 bytes per property key, 4096 bytes per string operand, 100,000 features, and aggregate geometry/topology/output/draw-range budgets.

Style replacement is atomic and does not refetch data or retriangulate polygons. Geometry is validated even for hidden features; collection indices, not optional/duplicate GeoJSON IDs, associate paints with prepared ranges. Draw order remains all fills followed by all strokes, preserving feature order within each phase.

Browser `WfsLayer.setStyle(sheetOrNull)` replaces paints; `null` restores the sheetless legacy path. Native’s opt-in prepared path is `PreparedWfsFeatures::new` → `prepare_draw` → `WgpuWfsDraw::render`. It creates immutable per-range uniform bindings, so subsequent paint changes do not mutate a draw already encoded for submission. The legacy `WgpuWfsFeatures` renderer remains single-paint and keeps its existing behavior.

Adding `fill_polygons` extends the Rust options struct. Existing callers using `..Default::default()` need no changes; exhaustive struct literals must add `fill_polygons: false` to retain outline-only rendering.

### Geometry contract

Coordinates, CRS metadata, antimeridian restrictions and height modes follow [the coordinate contract](#geometry-and-coordinate-contract). Polygon topology is evaluated in normalized longitude/latitude radians after source CRS conversion. All rings must be closed, with matching final/initial x/y/height. The first ring is the exterior; subsequent rings are holes. Ring winding does not affect the result.

A shared, transport-independent Rust tessellator partitions the polygon into vertical slabs, sorts edge intersections and triangulates the interior trapezoids. This supports concavity and multiple disjoint holes without joining their boundaries. Newly introduced boundary vertices interpolate height from the original edge. Triangle interiors use barycentric interpolation of latitude, longitude and height; a nonplanar polygon therefore receives a deterministic piecewise surface, not a geodesic or vertical-datum conversion.

Source triangles are subdivided into `subdivisions²` triangles (1..64 per axis), sampling terrain at generated nodes. Unknown ground omits affected subtriangles. Globe vertices use WGS84 ECEF; triangles crossing the ellipsoid horizon are clipped. The result shares [depth integration](depth.md) with WMS and sampled terrain. Raster depth bias now uses one constant unit and zero slope bias, so the slope adjustment does not pull background imagery in front of modestly elevated features.

Invalid fills are rejected, including self-intersections, nonadjacent touching edges, duplicate/backtracking edges, holes outside the exterior, intersecting/touching/nested holes, non-finite values and near-zero area. Near-degeneracy is tested after normalizing the polygon extent, using a 1e-12 tolerance. Empty polygons remain empty; an explicitly empty ring in a nonempty polygon is invalid for filling. Separate polygon members are independent; overlap between different features is governed by depth/layer order.

### Bounds and bindings

The triangulator accepts at most 2,048 nodes per polygon, counting closure nodes across every ring, and emits at most 196,608 triangle vertices. Topology validation/slab construction have bounded quadratic work. Each SDK fill buffer has a 1,000,000 generated-work budget and a separate 1,000,000 output-vertex budget. The expanded work estimate is checked during preparation, so dense geometry may require fewer subdivisions. Horizon clipping can increase output and is checked during rebuild. Existing line/point geometry retains its own budgets.

The pure Rust function is `olayer_core::volumetric::triangulate_polygon_rings(coords, ring_lengths)`, returning packed x/y/height triangle vertices with coded volumetric errors. It accepts generic planar coordinates and performs no CRS conversion. The browser package exports a matching WASM `triangulate_polygon_rings(Float64Array, Uint32Array)`, whose lengths count nodes and whose flat coordinate array contains three values per node.

The generated C header exposes `olayer_triangulate_polygon_rings`. Inputs are flat f64 coordinates plus u32 ring lengths. `coords_len`, output capacity and returned length count f64 values. A null output with zero capacity performs a size query; return -4 reports the required length. Return 0 means success, -1 invalid pointer/capacity, -2 invalid input lengths, and -3 invalid topology. An undersized buffer is not modified. The caller owns the output buffer; no allocator/free pairing is needed.

### Verification

`tests/fixtures/wfs-fill/features.json` contains a concave polygon with a hole and an island nested inside a GeometryCollection/MultiPolygon. Native and browser GPU tests check green interior pixels and blue background pixels through the hole/empty concavity in all three dimensions. In perspective modes the background draws after the fills, verifying that holes and zero-alpha fills leave no depth barrier. They also verify source reuse, invalid replacement preservation and pending-load destruction in the browser.

Core tests check area, multiple holes, winding, translated/scaled coordinates, interpolated height and invalid/bounded inputs. Real-WASM tests check filled area, terrain heights/nodata and globe culling. C-FFI tests check size queries, insufficient buffers and invalid topology. Existing WMS/WFS/shared-depth tests remain regression gates; the new GPU checks are in CI.

```sh
cargo test --workspace --exclude olayer-desktop-demo
cargo test -p olayer-native wfs_polygon_fills_preserve_holes_in_offscreen_pixels -- --ignored --nocapture
cd sdk/ts
npm run test:run
npm run typecheck
npm run test:wfs-fill:browser
```

Per-feature property/style rules, adaptive terrain/globe tessellation, antimeridian splitting and production manual visual checks remain follow-up work. Sampling can miss terrain holes or peaks smaller than the subdivision spacing. Fill validation is stricter than outline-only rendering, which can still display paths without interpreting their interior topology.
