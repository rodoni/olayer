# Implementation roadmap

This page tracks unfinished integration and publication work. Implemented behavior is documented in [map guides](../guides/maps/README.md), [terrain/altitude](../guides/terrain.md), [architecture](../architecture/README.md) and [API reference](../reference/README.md); those contracts are not duplicated here.

## Map Core and OGC milestones

| Phase | Status | Delivered | Remaining |
|---|---|---|---|
| 0: specification | Complete | Workspace separation, transport-free boundaries and documented four-dimension review gate | No specification gate deliverable remaining; signature freeze still gates final release |
| 1: Map Core foundation | Complete | CRS/bounds/tile/format contracts and validated response models | No foundation deliverables remaining |
| 2: pure OGC builders | Complete | WMS/WMTS/WFS/WCS queries, capability parsing and invalid-input tests | No builder deliverables remaining |
| 3: native providers | Complete | Injected transports, WMS/WFS/WCS decoders/sources and WMTS adapter | Further transport policy depends on host use cases |
| 4: browser providers | Complete | Cancellable/retriable/limited client, decoders, ImageBitmap/WebGL helpers and WMS/WFS/WCS source adapters | Wire-v1 bindings are documented under Phase 6; further service codecs remain subject to supported-format policy |
| 5: 2D/2.5D/3D integration | Partial | WMS/WFS/WCS shared rendering; bounded WFS feature property paint rules (browser + native opt-in); bounded sampled whole-grid terrain LOD/report integrated with browser/native terrain rebuilds; local terrain elevation queries wrap at the antimeridian; WCS cell-center/node registration, explicit feet/metre conversion and injectable orthometric geoid correction; CPU sampling of actual rendered triangles for mesh-relative altitude; native demo shared-depth pass; native/browser volumetric depth-tested no-write variants; shared Rust/WASM camera fixtures; WFS/WMS/depth/LOD/volumetric/mesh-altitude pixel tests | Coordinated tiled-globe geometry/imagery seams and shared multi-overlay LOD, native demo altitude controls, depth adoption by other visualizations, production/manual visual checks |
| 6: publication/compatibility | Complete for Map Core wire-v1 surface | Versioned pure JSON facade for WMS/WMTS/WFS/WCS requests, bounded capabilities summary and model validation; WASM coded-error wrappers; caller-buffer C exports with status/error contract; generated C header and linked external C consumer; shared Rust/WASM/C fixtures; strict isolated npm package consumer; header/declaration CI gates and unreleased compatibility notes | Registry publication/release is intentionally not performed by this implementation task |

“Complete” applies to the phase's delivered increment, not to every possible OGC service variant. Unit, provider and GPU gates are listed in the [testing guide](../guides/testing.md); test counts and historical timings are intentionally omitted.

## Next integration work

1. Broaden WCS scientific codecs/registrations beyond the bounded scalar-grid adapter. Keep scientific grids distinct from explicit ellipsoidal elevation inputs; the native image decoder rejects lossy/mislabeled sample conversions.
2. Extend bounded sampled adaptive terrain tessellation/LOD to coordinated tiled-globe geometry/imagery seams and shared multi-overlay LOD. Local sample queries wrap at the antimeridian, but independent patch UV/geometry seams remain. Exercise ridge/overhang production scenes and smaller nodata/peak structures than fixed sampling can resolve; finite probes are not a global terrain-error guarantee (an off-lattice ridge can be missed).
3. Extend transparency and compatible depth/pass choices to remaining visualization pipelines. Preserve existing opaque/zero-alpha map contracts; volumetric airspace demonstrates ordered alpha blending, not general OIT.
4. Wire the rendered-mesh height provider into object placement and add native-demo altitude controls; `RelativeToMesh` explicit-height precedence is fixed, but absent mesh height still uses the compatibility ground fallback.
5. Perform manual browser/native production visual checks in addition to deterministic fixture tests.

## Binding and release work

### Publication API review gate

The published wire-v1 surface is intentionally pure: versioned WMS/WMTS/WFS/WCS URL construction, capabilities request/summary parsing, and validated response descriptors. Transport execution, authentication, renderer/cache handles, codecs, topology repair and automatic scientific-grid-to-terrain conversion are not Map Core binding responsibilities.

- **Safety:** validate boundary DTOs rather than trusting public Rust fields; reject malformed/unbounded XML, inconsistent CRS and invalid dimensions before allocation or rendering. C inputs are borrowed for the call, and outputs must have explicit ownership/capacity contracts. Do not introduce process-global last-error state.
- **API ergonomics:** expose canonical request operations and stable `OL-CORE-MAP-*` codes; distinguish general WMTS matrix metadata from XYZ convenience APIs. Preserve existing SDK adapters or record intentional behavior changes in the changelog.
- **Performance/allocations:** bound XML bytes/depth/identifier counts and transport reads before materializing complete bodies. Include bridge copies and decoded numeric samples in memory budgets.
- **Contracts/testing:** use shared fixtures for Rust/WASM/C request/error behavior, generated declaration/header consumer checks, and package imports outside workspace aliases. Run `cargo check` between atomic Rust changes and the relevant workspace/browser/GPU gates before acceptance.

The current audit identified prerequisite fixes in query/fragment handling, WMS axis serialization, legacy capabilities identification, malformed XML rejection, streaming response budgets and mesh-altitude precedence; these are addressed and covered by fixtures. Phase 6 is considered complete for the local wire-v1 API only after Rust, browser, C-consumer and package gates pass. Publishing a release or registry artifact is not part of this roadmap increment.

- The intended Map Core operations/models and stable error mapping are available through WASM and C; intentional omissions are recorded in [binding exposure](../reference/bindings.md#core-api-exposure-inventory).
- The C header and TypeScript/WASM declarations regenerate in CI; an isolated strict TypeScript package consumer and linked external C consumer exercise them.
- Changelog notes and local release compatibility gates are present; no package registry or release action is performed.
- Shared camera/projection fixtures cover attitude, poles and antimeridian in Rust and real WASM. The C Map Core consumer exercises the shared canonical WMS query/error fixture family.

## Altitude follow-ups

The altitude-model/controller/binding implementation is present. Browser and native terrain renderers now sample effective height from retained CPU triangles. Remaining work is wiring that provider into object-placement workflows and adding equivalent placement controls in the native demo. The core resolver preserves its compatibility ground fallback if no mesh height is supplied. Use [the terrain guide](../guides/terrain.md) for current behavior.

## Acceptance rules

Changes must keep Core/Map Core free of network/filesystem dependencies, units/CRS explicit, unknown/nodata preserved and work/allocation bounded. Verify geometry separately from transport and rendered pixels. Compare browser/native behavior through shared fixtures. Run the relevant [workspace and GPU gates](../guides/testing.md) before treating an increment as complete.
