# Getting started

Read the [system requirements](requirements.md) for platform packages. Use current stable Rust with the `wasm32-unknown-unknown` target, wasm-pack, and Node.js 22.13+ within the 22.x line or Node.js 24+. The current jsdom/Puppeteer dependency engines rule out the older Node 18/20 setup instructions.

## Build in dependency order

The TypeScript SDK consumes a locally built WASM package. From the repository root:

```sh
cd sdk/ts/wasm
wasm-pack build --target web
cd ..
npm install
npm run typecheck
npm run build
```

For a headless Rust build, from the repository root:

```sh
cargo build --workspace --exclude olayer-desktop-demo
```

The native crates can be built independently. `cargo build --workspace` also builds the desktop demo, which needs the platform graphics/windowing dependencies.

## Run a demo

Browser, with `sdk/ts` as the working directory:

```sh
npm run dev
```

Open [the web demo](http://localhost:3000/demo/index.html). Vite uses port 3000 in the repository configuration.

Desktop, from the repository root with a display and GPU available:

```sh
cargo run -p olayer-desktop-demo --release
```

The optional [GeoServer/PostGIS example](../examples/geoserver/README.md) is separate from the deterministic local-fixture tests.

## Next steps

- [Run tests](../guides/testing.md).
- [Initialize a host application](../guides/development.md#host-integration).
- [Load map imagery, vector features and terrain](../guides/maps/README.md).
- [Look up SDK exports and ownership rules](../reference/README.md).
