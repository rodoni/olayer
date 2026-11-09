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
| Wire v1 bridge | `interop::build_request_json`, `parse_capabilities_json`, `validate_model_json`; stateless JSON DTO operations with bounded inputs |

Use `cargo doc -p olayer-map-core --no-deps` for exact signatures. The [architecture](../architecture/map-core.md) explains responsibility boundaries and coordinate conventions. [Map guides](../guides/maps/README.md) cover actual SDK execution and rendering.

The versioned pure wire-v1 operations are exposed by the browser WASM facade and caller-buffer C ABI, with stable Map Core error codes. They are request/model helpers only: they do not perform transport, authentication, codec work, projection transforms or rendering. Direct TypeScript `ogc.ts`/`wmts.ts` builders remain ergonomic SDK adapters. Terrain-grid and polygon-triangulation bindings belong to the mathematical engine and are not Map Core publications. See [the binding inventory](bindings.md#core-api-exposure-inventory) and [roadmap](../plans/map-core.md).
