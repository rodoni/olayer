# Map Core reference

[`olayer-map-core`](../../map-core/src/lib.rs) is a pure Rust contract crate. It performs no HTTP, filesystem, browser or GPU operations and currently has no dependency on `olayer-core`.

| Area | Public contracts |
|---|---|
| CRS and bounds | `CoordinateReferenceSystem`, `CrsAuthority`, `BoundingBox` |
| Tiles | `TileKey`, `TileMatrix`, `TileAddress` |
| Display/format metadata | `DisplayDimension`, `ImageFormat`, `VectorFormat`, `CoverageSampleType` |
| Request serialization | `UrlParts`, `QueryParameter` |
| WMS | `WmsRequestBuilder`, `WmsGetMapRequest`, `MapImage` |
| WMTS | `WmtsRequestBuilder`, `WmtsGetTileRequest` |
| WFS | `WfsRequestBuilder`, `WfsGetFeatureRequest`, `Feature`, `FeatureCollection` |
| WCS | `WcsRequestBuilder`, `WcsGetCoverageRequest`, `Coverage` |
| Capabilities | `Capabilities`, `CapabilitiesRequest`, `ServiceType` |
| Errors | `MapError`, stable exported map error constants |

Use `cargo doc -p olayer-map-core --no-deps` for exact signatures. The [architecture](../architecture/map-core.md) explains responsibility boundaries and coordinate conventions. [Map guides](../guides/maps/README.md) cover actual SDK execution and rendering.

Publication is incremental: browser OGC builders are currently TypeScript adapters, not generated Map Core WASM exports. Native providers use the Rust crate. Terrain-grid and polygon-triangulation bindings belong to the mathematical engine; they do not mean that all Map Core requests/models are published through WASM/C. See [the binding inventory](bindings.md#core-api-exposure-inventory) and [roadmap](../plans/map-core.md).
