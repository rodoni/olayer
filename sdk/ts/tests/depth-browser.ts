import init, { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { OlayerController } from "../src/controller";
import type { ViewMode } from "../src/controller";
import { TerrainLayer } from "../src/layers/terrain";
import { WfsLayer } from "../src/layers/wfs";
import { WmsLayer } from "../src/layers/wms";
import { OgcHttpClient } from "../src/map_provider/ogc_client";
import { WcsRasterSource, WfsVectorSource, WmsMapSource } from "../src/map_provider/ogc_sources";
import { WcsTerrainSource } from "../src/map_provider/wcs_terrain";

async function run(): Promise<string[]> {
  await init();
  const scene = await (await fetch("/fixture-scene.json")).json();
  const controller = new OlayerController({ glCanvas: document.querySelector<HTMLCanvasElement>("#map")!, canvas2D: document.querySelector<HTMLCanvasElement>("#overlay")!,
    projection: WasmProjection.new_stereographic(0, 0), initialCenterLatRad: 0, initialCenterLonRad: 0, viewportBaseMeters: scene.viewportBaseMeters });
  const terrain = new TerrainLayer("terrain", 32);
  terrain.setLod({ targetErrorMeters: 5, maxSubdivisions: 64, maxSourceSamples: 4_225 });
  const client = new OgcHttpClient({ maxRetries: 0 });
  const grid = (height: number) => new WcsTerrainSource("terrain", controller.terrainEngine, new WcsRasterSource(client, {
    endpoint: `/fixture-wcs?height=${height}`, coverageId: "elevation", crs: "EPSG:4326", bounds: scene.bounds, width: 2, height: 2,
  }));
  const known = grid(scene.terrainHeight), missing = grid(-9999);
  const features = [scene.belowHeight, scene.aboveHeight].map((height: number) => new WfsLayer(`feature-${height}`, new WfsVectorSource(client, {
    endpoint: `/fixture-wfs?height=${height}`, typeName: "point", crs: "EPSG:4326",
  }), { color: [1, 0, 0, 1], pointRadius: scene.pointRadius }));
  const imagery = [0, 255].map(alpha => new WmsLayer(`image-${alpha}`, new WmsMapSource(client, {
    endpoint: `/fixture-wms?alpha=${alpha}`, layers: "blue", crs: "EPSG:4326", bounds: scene.bounds, width: 1, height: 1,
  }), { drapeTerrain: true }));
  const gl = controller.gl;
  const results: string[] = [];
  try {
    await known.load();
    const sourceSample = controller.terrainEngine.get_elevation_status(0, 0).elevation_meters;
    if (sourceSample !== scene.terrainHeight) throw new Error(`Unexpected WCS installed sample: ${sourceSample}`);
    for (const layer of features) await layer.load();
    for (const layer of imagery) await layer.load(gl);
    for (const mode of ["2D", "2.5D", "3D"] as ViewMode[]) {
      controller.setViewMode(mode);
      controller.setPitch(mode === "2.5D" ? 0.4 : 0);
      controller.setZoom(mode === "3D" ? 20 : 1);
      await known.load();
      const modeSample = controller.terrainEngine.get_elevation_status(0, 0).elevation_meters;
      if (modeSample !== scene.terrainHeight) throw new Error(`${mode}: WCS reload did not restore center terrain: ${modeSample}`);
      const camera = controller.getCameraState();
      const raw = mode === "3D" ? controller.projection.get_3d_view_proj_matrix(camera)
        : mode === "2.5D" ? controller.projection.get_25d_view_proj_matrix(camera) : controller.projection.get_view_proj_matrix(camera);
      camera.free();
      const matrix = new Float32Array(raw);
      const draw = (feature: WfsLayer, image: TerrainLayer | WmsLayer, reverse: boolean): void => {
        gl.viewport(0, 0, 256, 256); gl.clearColor(0, 0, 0, 1); gl.depthMask(true); gl.clearDepth(1);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
        // Deliberately unusual host state; each layer must establish and restore its own contract.
        gl.disable(gl.DEPTH_TEST); gl.depthFunc(gl.GREATER); gl.depthMask(false);
        gl.disable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ZERO); gl.blendEquation(gl.FUNC_REVERSE_SUBTRACT);
        const layers = reverse ? [feature, image] : [image, feature];
        for (const layer of layers) layer.renderStatic(gl, matrix, { controller });
        if (image === terrain && mode !== "2D") {
          const lod = terrain.getLodReport();
          if (!lod || (!lod.unknownSamples && lod.triangleCount === 0) || lod.sourceSamples > 4_225 || lod.gridSize < 32 || lod.gridSize > 64) {
            throw new Error(`TerrainLayer did not expose bounded triangles: ${JSON.stringify(lod)}`);
          }
          const meshHeight = terrain.getMeshHeightAt(0, 0);
          if (lod.triangleCount === 0 && meshHeight !== null) {
            throw new Error(`${mode}: empty terrain LOD reported a mesh height`);
          }
          if (lod.triangleCount > 0 && (meshHeight === null || Math.abs(meshHeight - scene.terrainHeight) > 20)) {
            throw new Error(`${mode}: rendered-mesh altitude sample is invalid: ${meshHeight}; LOD=${JSON.stringify(terrain.getLodReport())}`);
          }
          if (meshHeight !== null) {
            const resolved = terrain.resolveAltitudeAt(controller, 0, 0, 100, "relative-to-mesh", "reject");
            if (Math.abs(resolved - (meshHeight + 100)) > 1e-6) throw new Error(`${mode}: mesh-relative placement ignored rendered terrain`);
          }
        }
        if (gl.isEnabled(gl.DEPTH_TEST) || gl.getParameter(gl.DEPTH_FUNC) !== gl.GREATER || gl.getParameter(gl.DEPTH_WRITEMASK)
          || gl.isEnabled(gl.BLEND) || gl.getParameter(gl.BLEND_EQUATION_RGB) !== gl.FUNC_REVERSE_SUBTRACT) throw new Error("Layer leaked host GL state");
      };
      const visible = (height: number): boolean => {
        const p = mode === "3D" ? lla_to_ecef(0, 0, height) : [0, 0, mode === "2D" ? 0 : height];
        const [x, y, z] = p;
        const w = raw[3]! * x! + raw[7]! * y! + raw[11]! * z! + raw[15]!;
        const nx = (raw[0]! * x! + raw[4]! * y! + raw[8]! * z! + raw[12]!) / w;
        const ny = (raw[1]! * x! + raw[5]! * y! + raw[9]! * z! + raw[13]!) / w;
        const pixels = new Uint8Array(7 * 7 * 4);
        gl.readPixels(Math.floor((nx + 1) * 128) - 3, Math.floor((ny + 1) * 128) - 3, 7, 7, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        return pixels.some((v, i) => i % 4 === 0 && v > 100 && v > pixels[i + 1]! * 1.5 + 30 && v > pixels[i + 2]! * 1.5 + 30);
      };
      for (const [index, height] of [scene.belowHeight, scene.aboveHeight].entries()) {
        for (const reverse of [false, true]) {
          draw(features[index]!, terrain, reverse);
          const expected = mode === "2D" || height > scene.terrainHeight;
          if (visible(height) !== expected) throw new Error(`${mode}: terrain occlusion height=${height}, reverse=${reverse}`);
          for (let image = 0; image < 2; image++) {
            draw(features[index]!, imagery[image]!, reverse);
            const expected = image === 0 || (mode === "2D" ? !reverse : height > scene.terrainHeight);
            if (visible(height) !== expected) throw new Error(`${mode}: imagery occlusion height=${height}, image=${image}, reverse=${reverse}`);
          }
        }
        // Same camera: the terrain revision must replace the cached known surface with holes.
        await missing.load();
        draw(features[index]!, terrain, true);
        if (mode !== "2D" && terrain.getMeshHeightAt(0, 0) !== null) {
          throw new Error(`${mode}: rendered-mesh altitude provider bridged nodata`);
        }
        if (!visible(height)) throw new Error(`${mode}: nodata replacement left an occluding terrain mesh`);
        draw(features[index]!, imagery[1]!, true);
        if (visible(height) !== (mode !== "2D")) throw new Error(`${mode}: incorrect nodata imagery behavior`);
        await known.load();
        const restoredSample = controller.terrainEngine.get_elevation_status(0, 0).elevation_meters;
        if (restoredSample !== scene.terrainHeight) throw new Error(`WCS failed to restore terrain after nodata: ${restoredSample}`);
      }
      if (gl.getError() !== gl.NO_ERROR) throw new Error(`${mode}: WebGL error`);
      results.push(`${mode}: shared terrain/WMS/WFS depth, both draw orders, transparency, nodata and GL state verified`);
    }
    return results;
  } finally {
    terrain.destroy(gl); for (const layer of features) layer.destroy(gl); for (const layer of imagery) layer.destroy(gl);
    known.unload(); controller.destroy();
  }
}
run().then(results => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: true, results }); })
  .catch(error => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: false, error: String(error) }); });
