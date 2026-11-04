import init, { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { OlayerController } from "../src/controller";
import type { ViewMode } from "../src/controller";
import { WmsLayer } from "../src/layers/wms";
import { WmsMapSource } from "../src/map_provider/ogc_sources";
import { OgcHttpClient } from "../src/map_provider/ogc_client";

async function run(): Promise<string[]> {
  await init();
  const fixture = await (await fetch("/fixture-scene.json")).json();
  const errors: string[] = [];
  const controller = new OlayerController({
    glCanvas: document.querySelector<HTMLCanvasElement>("#map")!,
    canvas2D: document.querySelector<HTMLCanvasElement>("#overlay")!,
    projection: WasmProjection.new_stereographic(0, 0),
    initialCenterLatRad: 0, initialCenterLonRad: 0,
    viewportBaseMeters: 600000,
    logger: { debug: () => {}, error: (message) => { errors.push(message); } },
  });
  const layer = new WmsLayer("fixture", new WmsMapSource(new OgcHttpClient({ maxRetries: 0 }), {
    endpoint: "/fixture-wms", layers: "quadrants", crs: "EPSG:4326", bounds: fixture.bounds, width: 2, height: 2,
  }), { drapeTerrain: true });
  controller.terrainEngine.load_rgb_tile(0, 0, 0, "mapbox", new Uint8Array(Array.from({ length: 4 }, () => fixture.rgbaTerrain).flat()), 2, 2);
  controller.layerManager.addLayer(layer);
  const results: string[] = [];
  try {
    await layer.load(controller.gl);
    for (const mode of ["2D", "2.5D", "3D"] as ViewMode[]) {
      controller.setViewMode(mode);
      controller.setPitch(mode === "2.5D" ? 0.4 : 0);
      controller.setZoom(mode === "3D" ? 20 : 1);
      const camera = controller.getCameraState();
      const matrix = mode === "3D" ? controller.projection.get_3d_view_proj_matrix(camera)
        : mode === "2.5D" ? controller.projection.get_25d_view_proj_matrix(camera)
        : controller.projection.get_view_proj_matrix(camera);
      camera.free();
      const gl = controller.gl;
      gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
      gl.clearColor(0, 0, 0, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      controller.layerManager.renderStaticLayers(gl, new Float32Array(matrix), { controller });
      if (errors.length) throw new Error(errors.join("; "));
      for (const [lonDeg, latDeg, expected] of [
        [-1.6, 0.8, [255, 0, 0]], [1.6, 0.8, [0, 255, 0]],
        [-1.6, -0.8, [0, 0, 255]], [1.6, -0.8, [255, 255, 0]],
      ] as const) {
        const lat = latDeg * Math.PI / 180, lon = lonDeg * Math.PI / 180;
        const position = mode === "3D" ? lla_to_ecef(lat, lon, fixture.elevation)
          : [...controller.projection.project(lat, lon, 0), mode === "2D" ? 0 : fixture.elevation];
        const [x, y, z] = position;
        const w = matrix[3]! * x! + matrix[7]! * y! + matrix[11]! * z! + matrix[15]!;
        const nx = (matrix[0]! * x! + matrix[4]! * y! + matrix[8]! * z! + matrix[12]!) / w;
        const ny = (matrix[1]! * x! + matrix[5]! * y! + matrix[9]! * z! + matrix[13]!) / w;
        const px = Math.floor((nx + 1) / 2 * gl.drawingBufferWidth);
        const py = Math.floor((ny + 1) / 2 * gl.drawingBufferHeight);
        const pixel = new Uint8Array(4);
        gl.readPixels(px, py, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        if (expected.some((value, i) => Math.abs(pixel[i]! - value) > 35)) {
          throw new Error(`${mode} corner ${lonDeg},${latDeg}: expected ${expected}, got ${pixel}`);
        }
      }
      if (gl.getError() !== gl.NO_ERROR) throw new Error(`${mode}: WebGL error`);
      results.push(`${mode}: all four image corners render in the correct geographic positions`);
    }
    if (errors.length) throw new Error(errors.join("; "));
    return results;
  } finally { controller.destroy(); }
}

run().then(results => {
  document.querySelector("#result")!.textContent = JSON.stringify({ ok: true, results });
}).catch(error => {
  document.querySelector("#result")!.textContent = JSON.stringify({ ok: false, error: String(error) });
});
