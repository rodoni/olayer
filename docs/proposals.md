# Enhancement Proposals

This document records proposed improvements identified from the current Olayer
architecture and implementation. Items remain here after completion so their
acceptance criteria and implementation history stay traceable. Priorities
reflect operational risk and the amount of shared infrastructure affected.

## Priorities

- **P0:** Required before production use of the affected path.
- **P1:** Important for correctness, long-running sessions, or platform parity.
- **P2:** Valuable hardening or usability improvement.

## P0: Implement Real MVT Decoding

**Status:** Completed. Implemented in `sdk/ts/src/providers/vector.ts` and
`sdk/ts/src/layers/vector.ts` using `@mapbox/vector-tile` and `pbf`.

The provider now decodes binary MVT/PBF tiles, supports multipart geometries,
and keeps GeoJSON as an explicit supported format. GeoJSON coordinate handling
supports `EPSG:900913` by default and `EPSG:4326` through
`VectorTileSourceOptions`. HTTP and decode failures reject without inserting
mock features. MVT layer selection is explicit through `VectorTileSourceOptions.mvtLayer`;
omitting it intentionally decodes all layers.

**Problem:** `sdk/ts/src/providers/vector.ts` attempts to parse fetched data as
JSON and falls back to generated demo features for binary data. A real MVT
(`.pbf`) response can therefore appear to load successfully while displaying
mock geometry instead of the server data.

**Proposal:** Add a production MVT decoder, or isolate decoding behind a
provider interface that can be backed by a maintained MVT library. Define the
input tile coordinate system, extent handling, geometry conversion, supported
geometry types, and layer selection explicitly. Keep GeoJSON support as a
separate, intentional format rather than an implicit fallback.

**Acceptance criteria:**

- Binary MVT tiles produce Point, LineString, and Polygon features without
  mock data.
- Multi-part and polygon-ring geometries retain their structure and winding.
- Tile extent, XYZ/TMS orientation, and source coordinate reference system are
  covered by fixtures and round-trip or known-coordinate tests.
- Decode failures are observable to the host and do not silently replace
  operational data with demo features.

**Dependencies:** TypeScript provider API and test fixtures.

**Verification:** `cd sdk/ts && npm run test:run` passes 43 tests, and
`cd sdk/ts && npm run build` completes successfully.

## P1: Define One Tile Cache and Request Lifecycle

**Status:** Partially completed. Shared bounded LRU/disposal behavior and
in-flight request deduplication are implemented in
`sdk/ts/src/providers/tile_cache.ts`, the web providers, and native WMTS.
Retry, abort-aware boundaries, byte accounting, and cache statistics are now
implemented for the web request layer. Native retry/cancellation and complete
provider-wide request metrics remain pending.

**Problem:** Raster and vector providers each implement their own cache and
in-flight behavior. Raster requests are not deduplicated, vector has a
duplicated cache branch that is unreachable after the earlier return, and the
native WMTS source uses an unbounded `HashMap` rather than the documented LRU
behavior.

**Proposal:** Introduce shared cache policy concepts for web and native
providers: bounded capacity, byte/item accounting, LRU eviction, in-flight
request deduplication, cancellation, retry policy, and explicit cache metrics.
Eviction must release the underlying resource, including WebGL textures and
decoded native pixel buffers.

**Acceptance criteria:**

- Repeated requests for one tile result in one active fetch.
- Capacity limits are enforced consistently in raster, vector, terrain, and
  native WMTS paths.
- Eviction and manual unload release GPU or heap resources.
- Tests cover concurrent requests, failed requests, retry, eviction, and cache
  size reporting.

**Dependencies:** A small shared cache/request abstraction and provider API
changes.

## P1: Make SDK Teardown Complete and Idempotent

**Status:** Completed for the current SDK resources. Web listeners and late
provider completions are cleaned up or ignored, `OlayerController.destroy()`
is idempotent, and native WMTS workers shut down on drop.

**Problem:** `OlayerController.destroy()` stops rendering and frees WASM and
atlas resources, but listeners registered in `setupInteractions()` and the
anonymous window resize listener remain attached. A destroyed controller can
therefore continue receiving events, and repeated construction/destruction can
accumulate listeners. Native `GeoserverWmtsSource` workers also have no
shutdown signal.

**Proposal:** Store listener references and remove them during teardown. Add a
destroyed state so public operations and late asynchronous completions become
no-ops or return a clear lifecycle error. Give native data sources an explicit
shutdown/drop path that closes workers and prevents new requests.

**Acceptance criteria:**

- `destroy()` is safe to call more than once.
- No controller listener remains after teardown.
- Pending web loads cannot mutate destroyed WebGL/WASM resources.
- Native workers terminate when their source is dropped or explicitly closed.
- Browser and native lifecycle tests verify cleanup under pending requests.

## P1: Establish Cross-Binding API and Documentation Conformance

**Status:** In progress. Added `docs/conformance.md` and
`docs/core_api_inventory.md` with explicit decisions
for units, errors, prediction status, unknown terrain, and ownership. Native
header regeneration is now checked in CI. Shared camera fixtures and complete
public-core feature inventory remain pending.

**Problem:** The project exposes the same core concepts through Rust, WASM,
TypeScript, and C-FFI, while camera and data-source logic is duplicated between
the web and native SDKs. The extensive hand-maintained API documentation can
drift from signatures and behavior.

**Proposal:** Treat the Rust core API as the contract source, add compile-time
or generated checks for WASM and C-FFI surfaces, and maintain a small conformance
matrix for angles, ownership, error mapping, and view modes. Generate or verify
the API reference in CI where practical.

**Acceptance criteria:**

- Every public core feature has an explicit WASM and C-FFI decision: exposed,
  intentionally unavailable, or pending.
- Angle units and ownership rules are tested at each boundary.
- Camera behavior has shared fixtures consumed by web and native tests.
- CI detects stale generated headers or documented signatures.

## P1: Add Operational Data Quality and Prediction Status

**Status:** In progress. Core, WASM, and C-FFI status surfaces now distinguish
valid, stale, and clock-skewed predictions, and expose unknown DTED elevation
through status-aware APIs. Profile and elevation consumers now choose whether
unknown terrain is propagated or rejected. Unavailable-target status and a
dedicated MSAW clearance API remain to be completed.

**Problem:** `core/src/interpolator/engine.rs` silently skips targets with
negative time deltas or stale timestamps. `core/src/terrain/engine.rs` treats
DTED null samples as `0.0` metres. These choices avoid disrupting a render
batch, but they can hide stale or incomplete data from an operational host and
can make terrain appear to be sea level.

**Proposal:** Preserve the non-blocking batch behavior while returning status
metadata and counters for skipped targets and invalid terrain samples. Make
the terrain null policy configurable, with a default that distinguishes
unknown elevation from a measured zero. Expose prediction age and quality
state to SDK consumers.

**Acceptance criteria:**

- Hosts can distinguish valid, stale, clock-skewed, and unavailable targets.
- Terrain queries can distinguish a valid zero elevation from missing data.
- MSAW/profile consumers have an explicit policy for unknown terrain.
- Tests cover clock skew, stale thresholds, null DTED samples, and alert policy.

## P2: Improve Rendering and Provider Observability

**Status:** In progress. Cache statistics and optional frame metrics are now
available through the TypeScript SDK. Request/decode timings, GPU failures,
WASM lifecycle counts, and native metrics remain pending.

**Problem:** The rendering and data stacks expose little structured telemetry.
Current paths rely on console logging or silent cache misses, making frame
budget regressions, tile latency, decode failures, and memory growth difficult
to diagnose during long-running sessions.

**Proposal:** Add optional host-supplied instrumentation callbacks or a small
metrics interface covering frame duration, active/idle FPS, tile request state,
decode time, cache hit/miss/eviction counts, GPU upload failures, and WASM
object lifecycle counts. Keep instrumentation disabled or cheap by default.

**Acceptance criteria:**

- Hosts can collect metrics without parsing console output.
- A demo or test harness reports frame and tile metrics.
- Instrumentation does not require the core Rust engine to perform I/O.
- Long-run tests can assert bounded cache and resource counts.

## P2: Build a Headless Integration and Visual Regression Suite

**Status:** In progress. CI now exercises `wasm-pack test --headless --chrome`
and scheduled release benchmarks are configured. Provider cancellation
integration tests, camera snapshots, GPU snapshots, and endurance baselines
remain pending.

**Problem:** CI currently runs Rust tests, Clippy, a WASM build, and TypeScript
unit tests, but not WASM browser tests, end-to-end tile flows, or rendering
regressions. The roadmap also calls for visual validation and load testing that
are not yet represented in CI.

**Proposal:** Add layered verification: browser-level WASM tests, mocked HTTP
provider integration tests, deterministic core-to-SDK camera fixtures, and
headless GPU snapshot tests where the runner supports software rendering.
Keep large stress and 24-hour endurance tests in a scheduled workflow.

**Acceptance criteria:**

- CI exercises the documented `wasm-pack test --headless --chrome` path.
- Provider tests cover successful, malformed, missing, and cancelled tile
  responses.
- Projection/camera snapshots cover 2D, 2.5D, 3D, center changes, and
  antimeridian/pole boundaries.
- A scheduled job tracks target-count, frame-time, and memory baselines.

## Suggested Order

1. Implement real MVT decoding and make failures explicit.
2. Unify cache/request lifecycle and complete web/native teardown.
3. Add cross-binding conformance tests and operational data-quality status.
4. Add instrumentation, headless integration coverage, and scheduled load
   baselines.

These proposals should be reflected in `docs/plan.md` when implementation work
is scheduled. Completed items should be moved to the project changelog or
marked with an implementation link rather than left ambiguous in this backlog.
