import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Build first via test:package. Only the tarball crosses into this consumer;
// neither workspace aliases nor workspace node_modules participate in resolution.
const sdkRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temporaryRoot = await mkdtemp("/tmp/kilo/olayer-package-");
const run = (command, args, cwd = temporaryRoot) => execFileSync(command, args, {
  cwd,
  encoding: "utf8",
  env: { ...process.env, NODE_PATH: "" },
  stdio: ["ignore", "pipe", "pipe"],
});

try {
  const manifest = JSON.parse(run("npm", ["pack", "--json", "--pack-destination", temporaryRoot], sdkRoot));
  assert.equal(manifest.length, 1);
  const [packed] = manifest;
  assert.equal(packed.name, "olayer-sdk");
  assert.equal(typeof packed.filename, "string");
  assert.equal(packed.filename, packed.filename.split("/").at(-1));
  const paths = new Set(packed.files.map((file) => file.path));
  for (const path of paths) {
    if (path.startsWith("dist/")) {
      assert.ok(path.endsWith(".d.ts") || path === "dist/olayer.js" || path === "dist/olayer.umd.cjs", `Unexpected or stale build artifact: ${path}`);
    }
  }
  for (const artifact of [
    "dist/index.d.ts", "dist/olayer.js", "dist/olayer.umd.cjs",
    "node_modules/olayer-wasm/package.json",
    "node_modules/olayer-wasm/olayer_wasm.js",
    "node_modules/olayer-wasm/olayer_wasm.d.ts",
    "node_modules/olayer-wasm/olayer_wasm_bg.wasm",
    "wasm/pkg/olayer_wasm_bg.wasm",
  ]) {
    assert.ok(paths.has(artifact), `Missing packed artifact: ${artifact}`);
  }
  assert.ok(packed.bundled.includes("olayer-wasm"));
  const compiler = JSON.parse(await readFile(join(sdkRoot, "node_modules/typescript/package.json"), "utf8"));
  await writeFile(join(temporaryRoot, "package.json"), JSON.stringify({ private: true, type: "module" }));
  run("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", join(temporaryRoot, packed.filename), `typescript@${compiler.version}`]);
  const installedRoot = join(temporaryRoot, "node_modules/olayer-sdk");
  assert.equal(await realpath(installedRoot), installedRoot, "Consumer must contain an installed package, not a workspace symlink");
  const wasm = await readFile(join(installedRoot, "wasm/pkg/olayer_wasm_bg.wasm"));
  assert.deepEqual([...wasm.subarray(0, 4)], [0, 97, 115, 109]);
  assert.deepEqual(wasm, await readFile(join(installedRoot, "node_modules/olayer-wasm/olayer_wasm_bg.wasm")));

  await writeFile(join(temporaryRoot, "consumer.ts"), `
import init, { WasmLatLon, WasmTerrainEngine, OlayerController, WcsTerrainSource, buildWmsGetMapUrl } from "olayer-sdk";
import type { OlayerConfig, WmsMapRequestOptions } from "olayer-sdk";
const options = {
  endpoint: "https://example.test/wms", layers: "terrain", crs: "EPSG:4326",
  bounds: [-10, -20, 10, 20], width: 256, height: 256,
} satisfies WmsMapRequestOptions;
const url: string = buildWmsGetMapUrl(options);
const initialize: typeof init = init;
const coordinate: WasmLatLon = new WasmLatLon(0, 0, 0);
const engine: WasmTerrainEngine = new WasmTerrainEngine();
const createController = (config: OlayerConfig): OlayerController => new OlayerController(config);
const source: typeof WcsTerrainSource = WcsTerrainSource;
void [url, initialize, coordinate, engine, createController, source];
`);
  await writeFile(join(temporaryRoot, "tsconfig.json"), JSON.stringify({
    compilerOptions: {
      target: "ES2022", module: "ESNext", moduleResolution: "Bundler",
      // wasm-bindgen exposes Symbol.dispose on owned WASM handles.
      lib: ["ES2022", "ESNext.Disposable", "DOM", "DOM.Iterable"], strict: true,
      noUncheckedIndexedAccess: true, exactOptionalPropertyTypes: true,
      noImplicitReturns: true, noFallthroughCasesInSwitch: true,
      noImplicitOverride: true, noPropertyAccessFromIndexSignature: true,
      skipLibCheck: false, noEmit: true, types: [],
    },
    files: ["consumer.ts"],
  }));
  run(process.execPath, [join(temporaryRoot, "node_modules/typescript/bin/tsc"), "--project", "tsconfig.json"]);

  await writeFile(join(temporaryRoot, "consumer.mjs"), `
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import init, * as sdk from "olayer-sdk";
const bytes = await readFile(new URL("./node_modules/olayer-sdk/wasm/pkg/olayer_wasm_bg.wasm", import.meta.url));
async function verify(api) {
  assert.equal(typeof api.default, "function");
  for (const name of ["OlayerController", "WcsTerrainSource", "WmsLayer", "WfsLayer"]) assert.equal(typeof api[name], "function");
  await api.default({ module_or_path: bytes });
  const point = new api.WasmLatLon(0.1, 0.2, 123);
  try {
    assert.equal(point.lat, 0.1);
    assert.equal(point.lon, 0.2);
    assert.equal(point.height, 123);
  } finally { point.free(); }
  const url = new URL(api.buildWmsGetMapUrl({ endpoint: "https://example.test/wms", layers: "terrain", crs: "EPSG:4326", bounds: [-10, -20, 10, 20], width: 256, height: 256 }));
  assert.equal(url.searchParams.get("BBOX"), "-20,-10,20,10");
}
assert.equal(init, sdk.default);
await verify(sdk);
await verify(createRequire(import.meta.url)("olayer-sdk"));
`);
  run(process.execPath, ["consumer.mjs"]);
  // A fresh module instance also verifies the bundled default WASM URL, not
  // just initialization using an explicitly supplied packaged binary.
  run(process.execPath, ["--input-type=module", "-e", 'import init, { WasmLatLon } from "olayer-sdk"; await init(); const p = new WasmLatLon(0, 0, 0); p.free();']);
  process.stdout.write("Package smoke passed: tarball artifacts, isolated install, strict declarations, ESM/CJS runtime, explicit/default WASM initialization.\n");
} catch (error) {
  if (error instanceof Error && "stdout" in error && typeof error.stdout === "string") process.stderr.write(error.stdout);
  if (error instanceof Error && "stderr" in error && typeof error.stderr === "string") process.stderr.write(error.stderr);
  throw error;
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}
