# API reference

Use these pages for selected API summaries. Exact signatures and exports are maintained in the source and generated artifacts below; the hand-written pages are not a substitute for generated declarations or a complete release compatibility catalog.

| Layer | Reference | Authoritative source/artifact |
|---|---|---|
| Rust mathematical engine | [Core API](core.md) | [Core modules](../../core/src/lib.rs); `cargo doc -p olayer-core --no-deps` |
| Rust OGC contracts | [Map Core](map-core.md) | [Map Core exports](../../map-core/src/lib.rs); `cargo doc -p olayer-map-core --no-deps` |
| Browser mathematical bridge | [WASM API](wasm.md) | [WASM implementation](../../sdk/ts/wasm/src/lib.rs); `sdk/ts/wasm/pkg/olayer_wasm.d.ts` after wasm-pack build |
| TypeScript SDK | [TypeScript API](typescript.md) | [Public exports](../../sdk/ts/src/index.ts); `sdk/ts/dist/index.d.ts` after build |
| Native Rust SDK | [Native API](native.md) | [Native exports](../../sdk/native/src/lib.rs); `cargo doc -p olayer-native --no-deps` |
| C boundary | [C API](c-ffi.md) | [Generated C header](../../sdk/native/libolayer_native.h), [implementation](../../sdk/native/src/c_ffi_bridge/mod.rs) |
| SVG compiler | [Compiler API](symbol-compiler.md) | [Compiler](../../tools/symbol-compiler/src/compiler.ts), [CLI](../../tools/symbol-compiler/src/cli.ts) |

[Binding contracts and exposure](bindings.md) are the common reference for units, ownership, status semantics and intentional omissions. [Error handling](errors.md) explains module codes and function-specific boundary errors.

For runnable usage patterns rather than signature lists, use the [map guides](../guides/maps/README.md), [terrain guide](../guides/terrain.md), [symbol workflow](../guides/symbols.md) and [development guide](../guides/development.md).
