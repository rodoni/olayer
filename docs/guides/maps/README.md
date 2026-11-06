# Maps and OGC integration

Providers execute requests and decode responses; layers/handles choose the representation for 2D, projected terrain-draped 2.5D, or ECEF 3D. See [Map Core architecture](../../architecture/map-core.md) for the pure Rust contract boundary.

| Task | Guide |
|---|---|
| Display a bounded WMS image | [WMS imagery](wms.md) |
| Display WFS points, lines, outlines and filled polygons with holes | [WFS features](wfs.md) |
| Install WCS elevation in the shared terrain engine | [WCS terrain](wcs-terrain.md) |
| Share depth between images, features and terrain | [Depth and occlusion](depth.md) |

For XYZ/WMTS tile providers, see the [TypeScript reference](../../reference/typescript.md) and [native reference](../../reference/native.md). Request/response contracts are indexed in the [Map Core reference](../../reference/map-core.md).

Load terrain before enabling terrain-dependent placement. Declare the source CRS and height/datum contract explicitly. Unknown terrain, antimeridian crossings, tessellation resolution and partial transparency have documented limits in each guide. These integrations do not imply complete globe LOD, arbitrary WCS format support or full SDK binding publication.

Run the [shared-fixture GPU checks](../testing.md#gpu-integration-checks) to verify a checkout; consult the [roadmap](../../plans/map-core.md) for remaining work.
