# Symbol workflow

The symbol compiler converts SVG assets into a declarative library consumed by `WasmSymbolRegistry` or native registry paths. Configuration and output use `library_name`; older examples using `libraryName` do not match the compiler.

## Compile a library

Create `symbols.config.json` next to the SVG paths it references:

```json
{
  "library_name": "CustomATC",
  "symbols": [
    {
      "id": "civil:plane",
      "svg_path": "symbols/aircraft.svg",
      "bbox": [0, 0, 32, 32],
      "anchor": [16, 16]
    }
  ]
}
```

With `tools/symbol-compiler` as the working directory:

```sh
npm ci
npm run build
node dist/cli.js --config /absolute/path/symbols.config.json --output /absolute/path/compiled.json --strict --verbose
```

The compiler resolves SVG references relative to the config file, supports strict validation and writes output atomically. The [compiler reference](../reference/symbol-compiler.md) describes the API and options; [compiler tests](testing.md#symbols-and-performance) verify supported SVG behavior.

## Register a library

After WASM initialization, with a live registry owned by the application/controller:

```ts
const response = await fetch("/assets/compiled.json");
if (!response.ok) throw new Error(`Symbol library request failed: ${response.status}`);
registry.register_declarative_provider(await response.text());
```

Custom raster assets can be registered through `TextureAtlasManager.registerImageSymbol`. Programmatic Rust providers implement `SymbologyProvider`; see the [symbol registry design](../architecture/core/symbol_registry/arch.md) and [core API](../reference/core.md).

The C-FFI does not currently expose the full symbol registry. Native Rust hosts use native registry APIs; see the [binding inventory](../reference/bindings.md#core-api-exposure-inventory).
