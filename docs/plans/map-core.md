# Implementation roadmap

This page tracks unfinished integration and publication work. Implemented behavior is documented in [map guides](../guides/maps/README.md), [terrain/altitude](../guides/terrain.md), [architecture](../architecture/README.md) and [API reference](../reference/README.md); those contracts are not duplicated here.

## Map Core and OGC milestones

| Phase | Status | Delivered | Remaining |
|---|---|---|---|
| 0: specification | Complete implementation gate | Workspace separation and transport-free boundaries | Formal API-review record |
| 1: Map Core foundation | Complete | CRS/bounds/tile/format contracts and validated response models | No foundation deliverables remaining |
| 2: pure OGC builders | Complete | WMS/WMTS/WFS/WCS queries, capability parsing and invalid-input tests | No builder deliverables remaining |
| 3: native providers | Complete | Injected transports, WMS/WFS/WCS decoders/sources and WMTS adapter | Further transport policy depends on host use cases |
| 4: browser providers | Complete | Cancellable/retriable/limited client, decoders and source adapters | All-Map-Core generated bindings are a publication task |
| 5: 2D/2.5D/3D integration | Partial | WMS, WCS node-grid terrain, WFS outlines/fills/holes, shared depth and fixture/GPU checks | Property/style rules, broader coverage adapters, adaptive terrain/globe coverage, advanced transparency and production/manual visual checks |
| 6: publication/compatibility | Pending | Individual terrain/triangulation boundary additions exist | Map Core error/model mappings, generated artifacts/declarations, compatibility/changelog and release gates |

“Complete” applies to the phase's delivered increment, not to every possible OGC service variant. Unit, provider and GPU gates are listed in the [testing guide](../guides/testing.md); test counts and historical timings are intentionally omitted.

## Next integration work

1. Add WFS style/property rules without losing feature metadata or changing fill topology/lifecycle guarantees.
2. Broaden WCS decoding, raster registration and vertical-datum/unit adapters. Keep scientific grids distinct from explicit ellipsoidal elevation inputs.
3. Add adaptive terrain tessellation/LOD and larger globe coverage. Exercise ridge/overhang production scenes and smaller nodata/peak structures than fixed sampling can resolve.
4. Extend transparency and compatible depth/pass choices to other visualization pipelines. Preserve the existing opaque/zero-alpha map contracts.
5. Perform manual browser/native production visual checks in addition to deterministic fixture tests.

## Binding and release work

- Publish the intended Map Core APIs/models and stable error mapping through the chosen WASM/C interfaces; record omissions in [binding exposure](../reference/bindings.md#core-api-exposure-inventory).
- Regenerate and verify the C header and TypeScript/WASM declarations; keep compatibility notes tied to the actual exported APIs.
- Add changelog/release gates after supported formats, errors and ownership contracts are frozen.
- Add shared camera/projection fixtures across SDKs, including attitude, poles and antimeridian cases. Existing map fixtures exercise modes but are not a complete camera-conformance suite.

## Altitude follow-ups

The initial altitude-model/controller/binding implementation is present. Remaining work from the former altitude implementation plan is to expose equivalent controls in the native demo and provide a first-class effective mesh-height provider for `RelativeToMesh`. Use [the terrain guide](../guides/terrain.md) for current behavior.

## Acceptance rules

Changes must keep Core/Map Core free of network/filesystem dependencies, units/CRS explicit, unknown/nodata preserved and work/allocation bounded. Verify geometry separately from transport and rendered pixels. Compare browser/native behavior through shared fixtures. Run the relevant [workspace and GPU gates](../guides/testing.md) before treating an increment as complete.
