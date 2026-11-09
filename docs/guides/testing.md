# Testing and validation

Build WASM before installing/running the TypeScript SDK, as described in [getting started](../getting-started/README.md). Commands below are the maintained verification entry points; counts/timings vary with the checkout and environment.

## Rust without a desktop window

From the repository root:

```sh
cargo test --workspace --exclude olayer-desktop-demo
cargo clippy --workspace --exclude olayer-desktop-demo --all-targets
cargo fmt --all --check
```

Core and Map Core tests cover pure math/contracts. Native tests cover adapters, geometry and C-FFI. Rust workspace tests include pure WASM-crate tests; browser-specific wasm-bindgen tests are separate. The desktop demo is excluded from headless operations.

Native builds regenerate [the C header](../../sdk/native/libolayer_native.h). CI verifies that it matches the committed artifact. Existing Clippy warnings are not equivalent to a warnings-as-errors gate.

## TypeScript and WASM

With `sdk/ts` as the working directory:

```sh
npm run test:run
npm run typecheck
npm run build
```

For wasm-bindgen browser tests, use `sdk/ts/wasm`:

```sh
wasm-pack test --headless --chrome
```

Vitest tests that use real WASM initialize it synchronously, for example:

```ts
import { initSync } from "olayer-wasm";
import { readFileSync } from "node:fs";
initSync({ module: readFileSync("wasm/pkg/olayer_wasm_bg.wasm") });
```

## GPU integration checks

From the repository root, with a headless WGPU adapter available:

```sh
cargo test -p olayer-native -- --ignored --nocapture
```

The explicitly ignored native GPU checks cover WMS, WFS outlines, polygon fills/holes, per-feature WFS paint snapshots, shared terrain/imagery/feature depth, and translucent volumetric depth participation. Mesa software rendering can supply the adapter. These are distinct from the ordinary headless unit-test run.

With `sdk/ts` as the working directory:

```sh
npm run test:wms:browser
npm run test:wfs:browser
npm run test:wfs-fill:browser
npm run test:depth:browser
npm run test:volumetric:browser
npm run test:package
```

The `test:*:browser` commands start a local Vite server and headless Chrome through Puppeteer. They use local assets under [the shared fixtures](../../tests/fixtures), not a live GeoServer. `test:package` packs and installs the SDK in an isolated consumer, checks generated types and exercises ESM/CommonJS WASM initialization. Browser tests need permission to open a local port and launch a browser. See the [map guides](maps/README.md) for each scenario and its limits.

## Symbols and performance

With `tools/symbol-compiler` as the working directory:

```sh
npm ci
npm test
```

From the repository root:

```sh
cargo bench -p olayer-core
cargo bench -p olayer-native --bench grid_generation
```

Record toolchain, hardware and scenarios alongside benchmark results. Historical sample timings are not performance guarantees. The [performance workflow](../../.github/workflows/performance.yml) provides the repository's scheduled benchmark command set.

## Documentation

```sh
python3 tools/check-docs.py
```

This checks local paths, Markdown heading anchors and documents reachable from the [documentation index](../README.md). It does not fetch external URLs or execute code snippets. The [CI workflow](../../.github/workflows/ci.yml) is the source for configured CI jobs; it uses Node 24 and checks Rust formatting, TypeScript contracts and SDK declaration/bundle generation alongside tests.
