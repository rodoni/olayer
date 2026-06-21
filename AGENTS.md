# Agent Guide: Olayer

This file contains practical guidance for automated agents working on the Olayer repository.

## Project Layout

```text
core/              # Pure Rust mathematical engine (geodesy, projections, terrain, SLD, symbols, interpolation)
sdk/ts/            # Browser TypeScript SDK + web demo + wasm-bindgen bridge (sdk/ts/wasm)
sdk/native/        # Desktop Rust SDK (wgpu + egui) + C-FFI + native demo
tools/symbol-compiler/  # TypeScript CLI that compiles SVG symbol libraries to Core JSON
```

## Build Order

The TypeScript SDK depends on the locally-built WASM package, so the order matters:

1. `cd sdk/ts/wasm && wasm-pack build --target web`
2. `cd sdk/ts && npm install`
3. `cargo build --workspace` (native crates can be built independently)

The desktop demo (`sdk/native/demo`) needs a GPU and windowing system; exclude it from headless operations.

## Test Commands

```bash
# Rust tests (headless)
cargo test --workspace --exclude olayer-desktop-demo

# Linter
cargo clippy --workspace --exclude olayer-desktop-demo --all-targets

# TypeScript SDK tests
cd sdk/ts && npm run test:run
```

## Code Conventions

- **Angles:** All internal Rust APIs use **radians**. Degrees are accepted only at external boundaries (tests, WASM/FFI, CLI tools).
- **Numeric precision:** Geodetic math uses `f64`. GPU matrices use `f32` column-major `[f32; 16]`.
- **Coordinates:** Geodetic coordinates are `LatLon { lat, lon, height }` where `height` is in meters above the ellipsoid.
- **Error handling:** Core engine functions return `Result<T, CustomError>`. WASM bindings convert errors to `JsValue` strings.
- **Projection centers:** When a camera moves, call `Projection::update_center`. Stereographic and LCC use it; Web Mercator does not.

## Common Pitfalls

- Do **not** change `README.md` architecture diagrams without also updating this file and `docs/spec.md` if referenced.
- Any change to the Rust core usually needs corresponding changes in both `sdk/ts/wasm/src/lib.rs` and `sdk/native/src/c_ffi_bridge/mod.rs`.
- `sdk/ts/src/controller/index.ts` duplicates camera logic with `sdk/native/src/native_controller/mod.rs`; keep behavior consistent.
- Avoid adding `as any` casts in TypeScript; prefer adding proper public getters/setters on `OlayerController`.

## Symbol Workflow

1. Place SVG symbols under a `symbols/` directory.
2. Create a JSON config for `tools/symbol-compiler`.
3. Run the compiler to produce a declarative JSON library.
4. Load the library in the SDK via `WasmSymbolRegistry::register_declarative_provider` (WASM) or the equivalent C-FFI call.

## CI Expectations

A healthy PR should pass:

- `cargo test --workspace --exclude olayer-desktop-demo`
- `cargo clippy --workspace --exclude olayer-desktop-demo --all-targets`
- `cd sdk/ts/wasm && wasm-pack build --target web`
- `cd sdk/ts && npm install && npm run test:run`
