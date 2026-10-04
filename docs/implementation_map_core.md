# Map Core and OGC Provider Implementation Plan

## 1. Objective

Create a transport-independent map contract layer shared by the TypeScript and Native SDKs while keeping `core` free of network and filesystem I/O.

The solution must support:

- 2D projected maps;
- 2.5D maps draped over terrain;
- 3D ellipsoidal/ECEF globe rendering;
- WMS rendered images;
- WMTS raster tiles;
- WFS vector features;
- WCS raster coverages, elevation and scientific grids.

## 2. Target workspace architecture

```text
core/                         # Pure geodesy, projections and domain math
map-core/                     # New pure Rust workspace crate
  src/
    lib.rs
    crs.rs                    # CRS identity and supported transformations
    bounds.rs                 # Geographic/projected bounding boxes
    tile.rs                   # TileKey, TileMatrix, TileAddress
    dimensions.rs             # 2D, 2.5D, 3D, time and vertical dimensions
    format.rs                 # Image, vector and coverage formats
    request.rs                # Transport-independent request envelope
    wms.rs                    # WMS GetMap request builder
    wmts.rs                   # WMTS GetTile request builder
    wfs.rs                    # WFS GetFeature request builder
    wcs.rs                    # WCS GetCoverage request builder
    capabilities.rs           # Validated service capability models
    errors.rs                 # Stable OL-MAP error codes
sdk/ts/src/map_provider/      # Browser execution and source adapters
sdk/native/src/map_provider/  # Native HTTP/filesystem execution adapters
```

`olayer-map-core` may depend on `olayer-core` for domain coordinate and projection types. It must not depend on `reqwest`, browser APIs, `tokio`, `wgpu`, filesystem APIs or SDK crates.

## 3. Domain contracts

### 3.1 Coordinate reference systems

Define a validated CRS value:

```rust
pub struct CoordinateReferenceSystem {
    pub authority: CrsAuthority,
    pub code: NonZeroU32,
}
```

Initial supported CRS values:

- EPSG:4326 — geographic longitude/latitude;
- EPSG:3857 — Web Mercator;
- EPSG:4978 — ECEF;
- EPSG:900913 — Web Mercator compatibility alias.

Rules:

1. Requests must declare their CRS explicitly.
2. Providers must not silently reinterpret degrees as radians.
3. Transformations must use `olayer-core::projections` or a documented adapter.
4. Unsupported CRS combinations must return a typed error.

### 3.2 Tile and matrix contracts

```rust
pub struct TileKey {
    pub x: u32,
    pub y: u32,
    pub z: u8,
}

pub struct TileMatrix {
    pub identifier: String,
    pub crs: CoordinateReferenceSystem,
    pub matrix_width: NonZeroU32,
    pub matrix_height: NonZeroU32,
    pub tile_width: NonZeroU32,
    pub tile_height: NonZeroU32,
}
```

Validation must reject:

- invalid zoom levels;
- `x`/`y` outside the matrix;
- integer overflow in matrix dimensions;
- incompatible tile sizes;
- non-finite geographic bounds.

### 3.3 Display dimensions

Use a semantic type instead of boolean flags:

```rust
pub enum DisplayDimension {
    TwoD,
    TwoPointFiveD,
    ThreeD,
}
```

The display dimension affects tessellation, visibility, altitude application, depth and coordinate conversion. It must not change the underlying WMS/WMTS/WFS/WCS request contract.

### 3.4 Response models

#### WMS

Decoded image plus CRS and bounds:

```rust
pub struct MapImage {
    pub pixels: Box<[u8]>,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub format: ImageFormat,
    pub crs: CoordinateReferenceSystem,
    pub bounds: BoundingBox,
}
```

#### WMTS

Decoded tile plus `TileKey`, matrix identifier, dimensions and format.

#### WFS

Validated feature collection with geometry, properties and CRS. The model must preserve unsupported properties rather than discarding them silently.

#### WCS

Validated coverage containing bounds, dimensions, sample type, nodata value, CRS and samples. Elevation and scientific grids must preserve unknown/nodata cells explicitly.

## 4. Request builders

Builders must be pure and return typed request values or a typed `MapError`:

```rust
pub struct WmtsGetTileRequest {
    pub endpoint: UrlParts,
    pub layer: String,
    pub style: String,
    pub matrix_set: String,
    pub tile: TileKey,
    pub format: ImageFormat,
}
```

The request model should expose canonical query parameters without requiring the core to open a connection. Serialization/escaping must be deterministic and tested.

Required builders:

- `WmsRequestBuilder::get_map`;
- `WmtsRequestBuilder::get_tile`;
- `WfsRequestBuilder::get_feature`;
- `WcsRequestBuilder::get_coverage`;
- capabilities request and parser models.

Authentication headers, tokens, cookies, proxies and TLS settings remain provider concerns.

## 5. Stable error catalog

Use the existing `ErrorCode` design for map errors:

```text
OL-CORE-MAP-0001  invalid CRS
OL-CORE-MAP-0002  unsupported CRS transformation
OL-CORE-MAP-0003  invalid tile key
OL-CORE-MAP-0004  tile outside matrix
OL-CORE-MAP-0005  invalid bounding box
OL-CORE-MAP-0006  invalid WMS request
OL-CORE-MAP-0007  invalid WMTS request
OL-CORE-MAP-0008  invalid WFS request
OL-CORE-MAP-0009  invalid WCS request
OL-CORE-MAP-0010  unsupported format
OL-CORE-MAP-0011  invalid coverage dimensions
OL-CORE-MAP-0012  invalid capabilities document
```

Codes are stable, never renumbered and never reused. Transport errors must be represented by SDK-specific errors and may retain the request code as context.

## 6. SDK provider responsibilities

### 6.1 TypeScript provider

Extend `sdk/ts/src/map_provider/` with:

- browser `fetch` execution;
- `AbortSignal` cancellation;
- retry and timeout policy;
- response byte/decode limits;
- image/bitmap decoding;
- WFS JSON validation;
- WCS binary/raster decoding;
- WebGL texture upload;
- bounded cache and eviction;
- conversion of coded Rust errors into JavaScript `Error` objects.

Existing `RasterTileSource`, `VectorTileSource`, `TerrainTileSource` and `CogTerrainSource` should migrate incrementally to the shared request builders.

### 6.2 Native provider

Extend `sdk/native/src/map_provider/` with:

- HTTP client and filesystem adapters;
- worker lifecycle;
- request cancellation and retries;
- bounded response and decode limits;
- WMS/WMTS/WFS/WCS decoders;
- CPU/GPU cache policy;
- conversion to native renderer inputs.

`GeoserverWmtsSource` becomes the first adapter using `olayer-map-core::wmts`.

## 7. Rendering integration

### 2D

- WMS/WMTS become plane textures.
- WFS geometries use the active planar projection.
- WCS is an image, analysis layer or raster overlay.

### 2.5D

- WMS/WMTS textures are draped over a terrain mesh.
- WCS elevation controls vertex height.
- WFS features are projected onto the terrain or rendered at explicit altitude.

### 3D

- WMTS/WMS tiles are subdivided and transformed to ECEF.
- Tile selection uses camera-facing hemisphere/frustum visibility.
- WCS supplies elevation, weather and volumetric data.
- WFS features are transformed to ECEF or rendered as billboards.
- Depth testing and horizon culling are mandatory.

The map provider returns data; renderers choose the dimension-specific representation.

## 8. Atomic implementation phases

### Phase 0 — Specification gate

- Freeze public domain types and invariants.
- Decide whether `olayer-map-core` is introduced immediately as a workspace crate.
- Add `[workspace.dependencies]` entries at the workspace root.
- Gate: review API signatures before code.

### Phase 1 — Map Core foundation

- Add crate and module skeleton.
- Implement `ErrorCode`, `MapError`, CRS, bounds, tile keys and formats.
- Add property tests for tile bounds and matrix limits.
- Gate: `cargo check -p olayer-map-core` and `cargo test -p olayer-map-core`.

### Phase 2 — Pure OGC builders

- Implement WMS and WMTS builders first.
- Add WFS feature request and WCS coverage request.
- Add deterministic URL/query tests and capability fixtures.
- Gate: `cargo clippy -p olayer-map-core --all-targets -- -D warnings`.

### Phase 3 — Native provider migration

- Add provider interfaces and transport adapter.
- Migrate `GeoserverWmtsSource` without changing cache behavior.
- Add WMS/WFS/WCS adapters incrementally.
- Gate: native unit/integration tests and bounded-response tests.

### Phase 4 — TypeScript provider migration

- Add TypeScript request adapter using generated WASM/core types where appropriate.
- Migrate raster/vector/terrain providers one at a time.
- Preserve `AbortSignal`, retry and cache contracts.
- Gate: `npm run typecheck`, provider tests and full Vitest suite.

### Phase 5 — 2D/2.5D/3D integration

- Add shared fixtures for one request in all display dimensions.
- Validate projection, terrain draping, ECEF tessellation, horizon culling and depth.
- Add visual/manual demo checks and deterministic geometry tests.

### Phase 6 — Publication and compatibility

- Update WASM and C-FFI error mappings.
- Update generated headers and TypeScript declarations.
- Update architecture docs and changelog.
- Run complete workspace gates before release.

## 9. Verification matrix

| Area | Required gate |
|---|---|
| Map Core | unit, property and snapshot request tests |
| OGC builders | deterministic WMS/WMTS/WFS/WCS query tests |
| CRS | projection and invalid-CRS tests |
| Providers | fake transport, timeout, retry, cancellation and cache tests |
| WFS | malformed feature/property rejection tests |
| WCS | dimensions, nodata, sample type and allocation-limit tests |
| 2D | projected texture/vector rendering tests |
| 2.5D | terrain draping and altitude tests |
| 3D | ECEF mesh, horizon, depth and tile coverage tests |
| Bindings | WASM error-code and C-FFI contract tests |
| Workspace | `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, TypeScript typecheck/tests/build |

## 10. Explicit non-goals

- No HTTP client in `olayer-core` or `olayer-map-core`.
- No credentials or authentication policy in shared request builders.
- No renderer-specific types in OGC request models.
- No assumption that WMS/WMTS/WFS/WCS payloads are interchangeable.
- No silent CRS conversion or implicit degrees/radians conversion.
