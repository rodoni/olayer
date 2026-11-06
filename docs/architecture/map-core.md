# Map Core and SDK provider architecture

Map Core is the transport-independent OGC contract layer. Its [public reference](../reference/map-core.md) links to the actual modules and generated Rust documentation.

| Layer | Owns | Does not own |
|---|---|---|
| `olayer-core` | Geodesy, projection/camera math, terrain sampling, triangulation | Service URLs, HTTP, filesystem I/O, GPU resources |
| `olayer-map-core` | CRS/bounds/tile metadata, validated request/response models, OGC query serialization and capabilities | Transports, decoder/browser state, renderer resources |
| Browser providers | Fetch, cancellation/retry/timeout policy, response limits and decoding, source metadata | An implicit coordinate or vertical-datum conversion |
| Native providers | Injected transports, decoder limits and source/cache lifecycle | Core network dependencies |
| Layers/GPU handles | Projection/ECEF conversion, tessellation, terrain placement, horizon/depth and resource release | Changing the source request merely because display mode changed |

Native providers use `NativeOgcClient<T>` and `OgcTransport`, with a `UreqTransport` adapter. Browser providers use `OgcHttpClient` and the TypeScript OGC builders. Generation/publication of all Map Core models across WASM/C remains separate work.

## Coordinate and response rules

Bounds are explicit x/y in source CRS units. EPSG:4326 placement uses longitude/latitude degrees at the SDK boundary; geodetic math uses radians internally. EPSG:3857/EPSG:900913 use Web Mercator metres. Unsupported transformations must fail rather than silently reinterpret units. WMS 1.3.0 EPSG:4326 wire serialization uses latitude/longitude axis order; caller bounds remain x/y.

A WMS response carries decoded pixels, dimensions, bounds and CRS. WFS preserves feature identifiers, original geometry and properties. WCS preserves dimensions, sample metadata and nodata. A scientific coverage becomes terrain only through an explicit elevation/registration/datum adapter. Pixel dimensions, finite coordinates and allocation/work limits are validated by the corresponding contract/SDK layer.

## Rendering and lifecycle

One fetched image/collection can be reused across 2D, 2.5D and 3D. Sampled mesh geometry changes with projection, mode, camera and terrain revision. Unknown terrain leaves gaps. Filled WFS polygons cache validated source triangulation; view changes recompute sampling/projection/clipping.

Depth attachments, transparency ordering and native pass compatibility are explicit host responsibilities. See [shared depth](../guides/maps/depth.md), [WMS](../guides/maps/wms.md), [WFS including holes](../guides/maps/wfs.md), and [WCS terrain](../guides/maps/wcs-terrain.md) for the maintained usage contracts.

## Validation and remaining scope

Request construction, transport execution and GPU integration are separate gates. Pure builder tests do not establish rendered correctness; local fixture/GPU checks do not establish complete globe LOD or all WCS format support. The [testing guide](../guides/testing.md) lists verification commands and the [roadmap](../plans/map-core.md) tracks unfinished publication, formats, styling and visual coverage.
