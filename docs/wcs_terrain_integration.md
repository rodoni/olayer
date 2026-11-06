# WCS elevation integration

Decoded WCS elevation grids can now drive the same terrain engine used for imagery, terrain meshes and altitude resolution in both SDKs. Switching between 2D, 2.5D and 3D does not download the coverage again. 2D remains flat; terrain-draped 2.5D uses projected coordinates and sampled height; 3D uses sampled height in ECEF.

## Supported grid contract

This increment supports the existing JSON scalar grid response: `{"width":3,"height":3,"samples":[...],"nodata":-9999}`. The request explicitly uses `FORMAT=application/json`. This is the SDK's JSON adapter contract, not a universal WCS response format; the service must support it. Native decoded `Coverage` values can also be installed directly.

- Bounds are `[west,south,east,north]`, in degrees for EPSG:4326 and metres for EPSG:3857/EPSG:900913. Other CRSs are rejected.
- Rows run north to south, columns west to east. Samples are **nodes**, with the outer nodes on the bounds. A one-node axis is constant across the extent. Cell-centred rasters require an explicit registration conversion first.
- Heights must be metres **above the ellipsoid**. Select an elevation coverage and convert other vertical datums/units before installation. Scientific values are not automatically interpreted as heights.
- Interpolation occurs in the grid's source CRS. Only samples with nonzero interpolation weights contribute. A contributing nodata sentinel produces unknown elevation, including at an overlapping older terrain source. Use `get_elevation_status` to preserve this state; the legacy scalar elevation getters retain their existing zero fallback.
- Grids are validated for finite values, ordered bounds, dimensions and sample count, with a limit of 16,777,216 nodes per grid. NaN nodata sentinels are unsupported; use a finite sentinel.
- Each source owns a string identifier. Installation replaces that identifier atomically after validation. Latest installations take precedence over older grids and DTED/RGB/GeoTIFF sources. The engine bounds the number of grids using its configured cache capacity (default 64); oldest installations are evicted first.
- Unloading removes only the named grid. Clearing all terrain removes grids too. Unknown values leave holes in draped WMS geometry.

## Browser

```ts
import { OgcHttpClient, WcsRasterSource, WcsTerrainSource } from "olayer-sdk";

const terrain = new WcsTerrainSource("elevation", controller.terrainEngine,
  new WcsRasterSource(new OgcHttpClient(), {
    endpoint: "/ows", coverageId: "elevation", crs: "EPSG:4326",
    bounds: [-2, -1, 2, 1], width: 3, height: 3,
  }));
await terrain.load(abortController.signal);
controller.triggerActive();
// WmsLayer must use { drapeTerrain: true } to sample loaded terrain.
// Before freeing the controller:
terrain.unload();
```

`load` retains the previous grid on failure, accepts cancellation, and ignores obsolete completions after a newer load or `unload`. Source identifiers must be unique within an engine. Unload sources before freeing their engine. Bounds/options are snapshotted by `WcsRasterSource`; decoded dimensions must match the request.

## Native and C

```rust,ignore
// WcsCoverageSource<T>, with an injected NativeOgcClient<T>:
source.load_json_terrain(&controller.terrain, "elevation")?;
// Or install a previously decoded Coverage:
olayer_native::install_wcs_terrain(&controller.terrain, "elevation", coverage)?;
// Rebuild WMS geometry after terrain changes (drape_terrain: true).
controller.terrain.unload_elevation_grid("elevation");
```

`fetch_json` now requests JSON instead of TIFF. JSON nodata metadata is preserved; conflicting configured/response sentinels, mismatched request/bounds CRS and mismatched dimensions are rejected. Failed installation leaves the existing grid intact.

The generated C header exposes `olayer_terrain_engine_load_elevation_grid` and `olayer_terrain_engine_unload_elevation_grid`. Load takes an EPSG code, four bounds, width/height, north-up f64 samples and an optional finite nodata sentinel. It returns 0 on success, -1 for invalid pointer/length inputs and -2 for invalid grid data. WASM exposes the equivalent `load_elevation_grid`/`unload_elevation_grid` methods; status queries serialize unknown height as `null`.

## Verification and remaining work

`tests/fixtures/wcs/elevation.json` supplies a shared local coverage. Rust and real-WASM tests exercise decoding, request format, terrain installation, all display dimensions, nodata, replacement and cleanup. Core tests cover orientation, weighted interpolation, Mercator coordinates and cache limits. C-FFI tests check installation, nodata and removal. Browser and native WMS pixel tests now use WCS elevation for terrain draping:

```sh
cargo test --workspace --exclude olayer-desktop-demo
cargo test -p olayer-native wms_image_renders_into_offscreen_pixels_in_all_modes -- --ignored --nocapture
cd sdk/ts
npm run test:run
npm run typecheck
npm run test:wms:browser
```

WFS feature rendering, broader WCS format/registration/datum adapters, full inter-layer depth/terrain occlusion and production manual visual checks remain follow-up work. The phase-5 status remains partial.
