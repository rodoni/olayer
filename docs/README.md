# Olayer documentation

Start with [setup and build](getting-started/README.md), then choose a guide for your task. Commands use the repository root unless a working directory is stated.

| Need | Read |
|---|---|
| Install toolchains and build the demos | [Getting started](getting-started/README.md), [system requirements](getting-started/requirements.md) |
| Contribute or extend an engine/SDK | [Development](guides/development.md), [testing](guides/testing.md) |
| Load WMS imagery, WFS features or WCS elevation | [Maps and OGC guides](guides/maps/README.md) |
| Resolve altitude or configure terrain appearance | [Terrain and altitude](guides/terrain.md) |
| Compile SVG symbol libraries | [Symbol workflow](guides/symbols.md) |
| Look up an API or binding contract | [API reference](reference/README.md) |
| Understand components and design decisions | [Architecture](architecture/README.md), [specification](architecture/specification.md) |
| Find unfinished integration/release work | [Implementation roadmap](plans/map-core.md) |
| Run an optional local GeoServer/PostGIS example | [GeoServer example](examples/geoserver/README.md) |

## Organization

- `getting-started/`: environment setup and the build sequence.
- `guides/`: task-oriented usage and contributor workflows.
- `reference/`: API lookup, units, ownership and binding exposure.
- `architecture/`: design rationale, component relationships and module details.
- `plans/`: implementation status and remaining work; proposals are not runtime guarantees.
- `examples/`: optional supporting configurations.

## Keeping documentation current

Public API signatures come from source, generated WASM declarations and the generated C header. The reference pages link to those sources; hand-written snippets are selected API summaries. Keep usage examples with their guide instead of copying complete class definitions into architecture pages.

Update a guide when behavior changes, the binding inventory when exposure changes, and the roadmap when a milestone changes. Keep historical implementation plans in Git once their behavior is documented and remaining work has been moved to the roadmap. Test counts and benchmark timings belong in run results, not permanent capability claims.

Run `python3 tools/check-docs.py` to check local documentation links, heading anchors and reachability from this index.
