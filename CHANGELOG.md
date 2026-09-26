# Changelog

All notable changes to Olayer are documented here.

## [Unreleased]

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
