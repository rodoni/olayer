# WFS polygon fills with interior rings

WFS can now render filled Polygon/MultiPolygon geometries, including concave exteriors and interior holes, in the browser and native SDKs. GeometryCollection members are supported too. Fill is opt-in; existing point crosses, lines and outlines retain their behavior.

## Browser

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

## Native

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

Adding `fill_polygons` extends the Rust options struct. Existing callers using `..Default::default()` need no changes; exhaustive struct literals must add `fill_polygons: false` to retain outline-only rendering.

## Geometry contract

Coordinates, CRS metadata, antimeridian restrictions and height modes follow [WFS integration](wfs_integration.md). Polygon topology is evaluated in normalized longitude/latitude radians after source CRS conversion. All rings must be closed, with matching final/initial x/y/height. The first ring is the exterior; subsequent rings are holes. Ring winding does not affect the result.

A shared, transport-independent Rust tessellator partitions the polygon into vertical slabs, sorts edge intersections and triangulates the interior trapezoids. This supports concavity and multiple disjoint holes without joining their boundaries. Newly introduced boundary vertices interpolate height from the original edge. Triangle interiors use barycentric interpolation of latitude, longitude and height; a nonplanar polygon therefore receives a deterministic piecewise surface, not a geodesic or vertical-datum conversion.

Source triangles are subdivided into `subdivisions²` triangles (1..64 per axis), sampling terrain at generated nodes. Unknown ground omits affected subtriangles. Globe vertices use WGS84 ECEF; triangles crossing the ellipsoid horizon are clipped. The result shares [depth integration](depth_integration.md) with WMS and sampled terrain. Raster depth bias now uses one constant unit and zero slope bias, so the slope adjustment does not pull background imagery in front of modestly elevated features.

Invalid fills are rejected, including self-intersections, nonadjacent touching edges, duplicate/backtracking edges, holes outside the exterior, intersecting/touching/nested holes, non-finite values and near-zero area. Near-degeneracy is tested after normalizing the polygon extent, using a 1e-12 tolerance. Empty polygons remain empty; an explicitly empty ring in a nonempty polygon is invalid for filling. Separate polygon members are independent; overlap between different features is governed by depth/layer order.

## Bounds and bindings

The triangulator accepts at most 2,048 nodes per polygon, counting closure nodes across every ring, and emits at most 196,608 triangle vertices. Topology validation/slab construction have bounded quadratic work. Each SDK fill buffer has a 1,000,000 generated-work budget and a separate 1,000,000 output-vertex budget. The expanded work estimate is checked during preparation, so dense geometry may require fewer subdivisions. Horizon clipping can increase output and is checked during rebuild. Existing line/point geometry retains its own budgets.

The pure Rust function is `olayer_core::volumetric::triangulate_polygon_rings(coords, ring_lengths)`, returning packed x/y/height triangle vertices with coded volumetric errors. It accepts generic planar coordinates and performs no CRS conversion. The browser package exports a matching WASM `triangulate_polygon_rings(Float64Array, Uint32Array)`, whose lengths count nodes and whose flat coordinate array contains three values per node.

The generated C header exposes `olayer_triangulate_polygon_rings`. Inputs are flat f64 coordinates plus u32 ring lengths. `coords_len`, output capacity and returned length count f64 values. A null output with zero capacity performs a size query; return -4 reports the required length. Return 0 means success, -1 invalid pointer/capacity, -2 invalid input lengths, and -3 invalid topology. An undersized buffer is not modified. The caller owns the output buffer; no allocator/free pairing is needed.

## Verification

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
