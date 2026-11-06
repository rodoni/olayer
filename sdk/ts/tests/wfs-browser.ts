import init, { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { OlayerController } from "../src/controller";
import type { ViewMode } from "../src/controller";
import { WfsLayer } from "../src/layers/wfs";
import { WfsVectorSource, WcsRasterSource } from "../src/map_provider/ogc_sources";
import { WcsTerrainSource } from "../src/map_provider/wcs_terrain";
import type { WfsFeatureCollection } from "../src/map_provider/ogc_client";
import { OgcHttpClient } from "../src/map_provider/ogc_client";

async function run(): Promise<string[]> {
  await init();
  const errors: string[] = [];
  const controller = new OlayerController({
    glCanvas: document.querySelector<HTMLCanvasElement>("#map")!, canvas2D: document.querySelector<HTMLCanvasElement>("#overlay")!,
    projection: WasmProjection.new_stereographic(0, 0), initialCenterLatRad: 0, initialCenterLonRad: 0,
    viewportBaseMeters: 600000, logger: { debug: () => {}, error: message => { errors.push(message); } },
  });
  const source = new WfsVectorSource(new OgcHttpClient({ maxRetries: 0 }), { endpoint: "/fixture-wfs", typeName: "features", crs: "EPSG:4326" });
  const originalLoad = source.load.bind(source);
  let loaded: WfsFeatureCollection | undefined;
  source.load = async signal => { loaded = await originalLoad(signal); return loaded; };
  const layer = new WfsLayer("features", source, { altitudeMode: "relative-to-ground", color: [0, 1, 0, 1], pointRadius: 12000 });
  const terrain = new WcsTerrainSource("terrain", controller.terrainEngine, new WcsRasterSource(new OgcHttpClient({ maxRetries: 0 }), {
    endpoint: "/fixture-wcs", coverageId: "elevation", crs: "EPSG:4326", bounds: [-2, -1, 2, 1], width: 3, height: 3,
  }));
  controller.layerManager.addLayer(layer);
  const results: string[] = [];
  try {
    await terrain.load();
    await layer.load();
    source.load = async () => ({ features: [{ geometry: { type: "LineString", coordinates: [[0, 0]] }, properties: {} }] });
    let rejected = false;
    try { await layer.load(); } catch { rejected = true; }
    if (!rejected) throw new Error("Malformed WFS replacement must fail");
    // The previous valid collection must still render in every mode below.
    for (const mode of ["2D", "2.5D", "3D"] as ViewMode[]) {
      controller.setViewMode(mode);
      controller.setPitch(mode === "2.5D" ? 0.4 : 0);
      controller.setZoom(mode === "3D" ? 20 : 1);
      const camera = controller.getCameraState();
      const matrix = mode === "3D" ? controller.projection.get_3d_view_proj_matrix(camera)
        : mode === "2.5D" ? controller.projection.get_25d_view_proj_matrix(camera) : controller.projection.get_view_proj_matrix(camera);
      camera.free();
      const gl = controller.gl;
      gl.viewport(0, 0, 256, 256);
      gl.clearColor(0, 0, 0, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      controller.layerManager.renderStaticLayers(gl, new Float32Array(matrix), { controller });
      // Point centre, route endpoint, outer ring and hole boundary must each draw nearby pixels.
      for (const [lon, lat, z] of [[0, 0, 120], [-1, -0.5, 100], [-1.5, 0.8, 0], [-0.4, -0.3, 0]]) {
        const height = mode === "2D" ? 0 : 500 + z!;
        const position = mode === "3D" ? lla_to_ecef(lat! * Math.PI / 180, lon! * Math.PI / 180, height)
          : [...controller.projection.project(lat! * Math.PI / 180, lon! * Math.PI / 180, 0), height];
        const [x, y, zValue] = position;
        const w = matrix[3]! * x! + matrix[7]! * y! + matrix[11]! * zValue! + matrix[15]!;
        const nx = (matrix[0]! * x! + matrix[4]! * y! + matrix[8]! * zValue! + matrix[12]!) / w;
        const ny = (matrix[1]! * x! + matrix[5]! * y! + matrix[9]! * zValue! + matrix[13]!) / w;
        const px = Math.floor((nx + 1) * 128), py = Math.floor((ny + 1) * 128);
        const pixels = new Uint8Array(7 * 7 * 4);
        gl.readPixels(px - 3, py - 3, 7, 7, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        // One-pixel lines can cover half an antialiased framebuffer sample.
        if (!pixels.some((v, i) => i % 4 === 1 && v > 100 && pixels[i - 1]! < 30 && pixels[i + 1]! < 30)) {
          throw new Error(`${mode}: missing WFS feature at ${lon},${lat}`);
        }
      }
      if (gl.getError() !== gl.NO_ERROR || errors.length) throw new Error(`${mode}: WebGL errors ${errors}`);
      results.push(`${mode}: WFS point, route and separate polygon rings render at expected positions`);
    }
    layer.destroy(controller.gl);
    controller.layerManager.removeLayer(layer.id);
    let finish!: (collection: WfsFeatureCollection) => void;
    source.load = () => new Promise(resolve => { finish = resolve; });
    const stale = new WfsLayer("stale", source);
    const pending = stale.load();
    stale.destroy(controller.gl);
    finish(loaded!);
    await pending;
    const gl = controller.gl;
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    stale.renderStatic(gl, new Float32Array(16), { controller });
    const pixels = new Uint8Array(256 * 256 * 4);
    gl.readPixels(0, 0, 256, 256, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    if (pixels.some((v, i) => i % 4 !== 3 && v !== 0)) throw new Error("A destroyed WFS layer must not install a pending load");
    results.push("Failed replacement preserves features; destruction invalidates pending loads");
    return results;
  } finally { terrain.unload(); controller.destroy(); }
}
run().then(results => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: true, results }); })
  .catch(error => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: false, error: String(error) }); });
