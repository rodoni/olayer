# Changelog

All notable changes to Olayer are documented here.

## [Unreleased]

### Fixed

- Enforce browser OGC response budgets while consuming streams, including chunked bodies without `Content-Length`; cancellation cleanup no longer masks the limit error or delays rejection.
- Reject native WCS raster sample conversions that could silently mislabel unsigned, multiband or converted luminance values as scientific elevation. Exact single-band UInt8/UInt16 remains supported; use the JSON adapter for other numeric sample types.
- Resolve explicit `RelativeToMesh` heights without requiring unrelated terrain coverage. The absent-mesh ground fallback remains compatible.
- Add bounded feature-indexed WFS property styles to browser/native prepared renderers; default legacy WFS rendering remains compatible, and style updates reuse installed topology.
- Add a versioned, bounded Map Core JSON facade for OGC request URLs, capabilities summaries and scalar coverage/feature-envelope validation, with matching WASM errors and caller-buffer C exports.
- Add shared Rust/WASM/C request fixtures for WMS axis order/query serialization and correct the camera fixture's analytical attitude reference.
- Use exact single-band WCS sample values and validate JSON sample ranges/dimensions before installing a coverage.
- Add WebGL2 rendering for bounded 3D volumetric airspaces with depth testing, non-writing translucent fragments, host-state restoration and explicit GPU cleanup.
- Serialize WMS 1.3.0 EPSG:4326 bounds in latitude/longitude wire order from canonical x/y bounds; reject conflicting WCS CRS and zero WFS result counts.
- Bound capabilities XML parsing by bytes, nesting and identifier counts; classify legacy WMS roots and collect WCS coverage identifiers correctly.

### Build and packaging

- Align CI with the documented Node 24 runtime and add explicit Rust formatting, TypeScript typecheck, declaration/bundle and isolated-package consumer gates.

## [0.1.0] - 2026-09-25

### Fixed

- Removed undefined-behavior risks when freeing C FFI arrays and documented safety contracts for the exported unsafe functions.
- Added bounds for FFI inputs, geometry generation, WMTS response bodies, and decoded tile dimensions.
- Replaced panics for invalid native controller centers and RBL coordinates with typed errors.
- Corrected terrain cache invalidation, ECEF terrain normals, unknown-elevation rendering, tile validation, and HiDPI coordinate conversion.
- Added bounded eviction for GPU and egui tile textures and avoided copying cached tile pixels on each frame.
- Made surface acquisition errors recoverable in the native demo.

### Changed

- Added typed errors to native map, layer-manager, controller, RBL, and SVG rasterization APIs.
- Changed native controller construction and RBL calculation to return typed errors.
- Regenerated the C header with valid opaque type declarations.
- Limited WMTS image decoding to 256×256 RGBA tiles and replaced direct console output with logging.
- Bounded geometry generation and removed expired empty tracks.
- Updated the grid-generation benchmark for the fallible native-controller constructor.

### Build and packaging

- Ignore generated `wasm-pack` files in Git while retaining them in the TypeScript SDK npm package.

### Tests

- Added regression coverage for FFI allocation layouts, input limits, invalid coordinates, cache behavior, terrain rendering, HiDPI conversion, and SVG errors.
