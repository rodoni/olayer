# Symbol compiler API

Source: [compiler.ts](../../tools/symbol-compiler/src/compiler.ts), [cli.ts](../../tools/symbol-compiler/src/cli.ts). See the [symbol workflow](../guides/symbols.md) for build/run commands.

| Entry point | Contract |
|---|---|
| `parseColor(colorStr, opacity?)` | Returns `Color` or `undefined` |
| `normalizePath(value, transform?)` | Normalizes SVG path data through a transform |
| `compileSvg(svgContent, options?)` | Returns `SymbolPrimitive[]` |
| `compileLibrary(configPath, rootDir?, options?)` | Returns `DeclarativeLibraryDto`; file paths are resolved against the configuration |
| `CompilerError` | Coded compiler failure used by the CLI |

Configuration/output use `library_name`. Input `symbols` is a nonempty array of `{ id, svg_path, bbox, anchor }`; output `symbols` is a map keyed by identifier. Bounds have four finite numbers and anchors two; duplicate identifiers and reversed bounds are rejected. Compiled entries contain declarative primitives, bounds and anchors. Use the compiler's exported types for the exact serialized shape.

The CLI accepts required `--config`/`-c` and `--output`/`-o` paths, optional `--strict` (reject unsupported SVG elements), and `--verbose` (emit warnings). It writes a temporary output and renames it only after successful compilation. Failure sets a nonzero exit code.

Supported geometry, transforms, styles and color handling are covered by [compiler tests](../../tools/symbol-compiler/src/test.ts). Unsupported behavior should be surfaced through strict mode/warnings rather than assumed from a generic SVG renderer.
