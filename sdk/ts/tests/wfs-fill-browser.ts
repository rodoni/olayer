import init, { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { OlayerController } from "../src/controller";
import type { ViewMode } from "../src/controller";
import { WmsLayer } from "../src/layers/wms";
import { WmsMapSource } from "../src/map_provider/ogc_sources";
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
  const layer = new WfsLayer("features", source, { altitudeMode: "relative-to-ground", color: [1, 1, 1, 1], fillColor: [0, 1, 0, 1], fillPolygons: true });
  const terrain = new WcsTerrainSource("terrain", controller.terrainEngine, new WcsRasterSource(new OgcHttpClient({ maxRetries: 0 }), {
    endpoint: "/fixture-wcs", coverageId: "elevation", crs: "EPSG:4326", bounds: [-2, -1, 2, 1], width: 3, height: 3,
  }));
  const background = new WmsLayer("background", new WmsMapSource(new OgcHttpClient({ maxRetries: 0 }), {
    endpoint: "/fixture-wms", layers: "blue", crs: "EPSG:4326", bounds: [-2, -1, 2, 1], width: 1, height: 1,
  }), { drapeTerrain: true });
  controller.layerManager.addLayer(background);
  controller.layerManager.addLayer(layer);
  const results: string[] = [];
  try {
    await terrain.load();
    await background.load(controller.gl);
    await layer.load();
    source.load = async () => ({ features: [{ geometry: { type: "Polygon", coordinates: [[[0, 0], [1, 1], [0, 1], [1, 0], [0, 0]]] }, properties: {} }] });
    let rejected = false;
    try { await layer.load(); } catch { rejected = true; }
    if (!rejected) throw new Error("Malformed WFS replacement must fail");
    source.load = async () => loaded!;
    const transparent = new WfsLayer("transparent", source, { fillPolygons: true, fillColor: [0, 1, 0, 0], altitudeMode: "relative-to-ground" });
    await transparent.load();
    // The previous valid collection must still render in every mode below.
    for (const mode of ["2D", "2.5D", "3D"] as ViewMode[]) {
      controller.setViewMode(mode);
      controller.setPitch(mode === "2.5D" ? 0.4 : 0);
      controller.setZoom(mode === "3D" ? 20 : 1);
      // In perspective modes draw fills first, then the background, proving holes do not write depth.
      controller.layerManager.reorderLayer(background.id, mode === "2D" ? 0 : 1);
      const camera = controller.getCameraState();
      const matrix = mode === "3D" ? controller.projection.get_3d_view_proj_matrix(camera)
        : mode === "2.5D" ? controller.projection.get_25d_view_proj_matrix(camera) : controller.projection.get_view_proj_matrix(camera);
      camera.free();
      const gl = controller.gl;
      gl.viewport(0, 0, 256, 256);
      gl.clearColor(0, 0, 0, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      controller.layerManager.renderStaticLayers(gl, new Float32Array(matrix), { controller });
      for (const [lon, lat, filled] of [[0, -0.5, true], [-1, -0.5, false], [-0.2, 0.5, false], [1, 0.5, true], [-1.2, 0.6, true]] as const) {
        const height = mode === "2D" ? 0 : filled ? 750 : 500;
        const position = mode === "3D" ? lla_to_ecef(lat * Math.PI / 180, lon * Math.PI / 180, height)
          : [...controller.projection.project(lat * Math.PI / 180, lon * Math.PI / 180, 0), height];
        const [x, y, zValue] = position;
        const w = matrix[3]! * x! + matrix[7]! * y! + matrix[11]! * zValue! + matrix[15]!;
        const nx = (matrix[0]! * x! + matrix[4]! * y! + matrix[8]! * zValue! + matrix[12]!) / w;
        const ny = (matrix[1]! * x! + matrix[5]! * y! + matrix[9]! * zValue! + matrix[13]!) / w;
        const pixel = new Uint8Array(4);
        gl.readPixels(Math.floor((nx + 1) * 128), Math.floor((ny + 1) * 128), 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        const expected = filled ? [0, 255, 0] : [0, 0, 255];
        if (expected.some((v, i) => Math.abs(v - pixel[i]!) > 35)) throw new Error(`${mode}: incorrect polygon interior at ${lon},${lat}: ${pixel}`);
      }
      gl.depthMask(true);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      transparent.renderStatic(gl, new Float32Array(matrix), { controller });
      background.renderStatic(gl, new Float32Array(matrix), { controller });
      // Fully transparent fill must leave no depth barrier in front of the background.
      const centre = new Uint8Array(4);
      const lat = -0.5 * Math.PI / 180;
      const point = mode === "3D" ? lla_to_ecef(lat, 0, 500) : [...controller.projection.project(lat, 0, 0), mode === "2D" ? 0 : 500];
      const w = matrix[3]! * point[0]! + matrix[7]! * point[1]! + matrix[11]! * point[2]! + matrix[15]!;
      const nx = (matrix[0]! * point[0]! + matrix[4]! * point[1]! + matrix[8]! * point[2]! + matrix[12]!) / w;
      const ny = (matrix[1]! * point[0]! + matrix[5]! * point[1]! + matrix[9]! * point[2]! + matrix[13]!) / w;
      gl.readPixels(Math.floor((nx + 1) * 128), Math.floor((ny + 1) * 128), 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, centre);
      if (centre[0]! > 30 || centre[1]! > 30 || centre[2]! < 220) throw new Error(`${mode}: transparent fill wrote depth`);
      if (gl.getError() !== gl.NO_ERROR || errors.length) throw new Error(`${mode}: WebGL errors ${errors}`);
      results.push(`${mode}: concave fill and island render, holes and concavities preserve background depth`);
    }
    transparent.destroy(controller.gl);
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
