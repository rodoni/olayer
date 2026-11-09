import init, { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { OlayerController } from "../src/controller";
import { VolumetricAirspaceLayer } from "../src/layers/volumetric_airspace";

async function run(): Promise<string> {
  await init();
  const canvas = document.querySelector<HTMLCanvasElement>("#map");
  if (!canvas) throw new Error("Missing volumetric canvas");
  const controller = new OlayerController({
    glCanvas: canvas,
    canvas2D: document.createElement("canvas"),
    projection: WasmProjection.new_stereographic(0, 0),
    initialCenterLatRad: 0,
    initialCenterLonRad: 0,
    viewportBaseMeters: 600_000,
  });
  const gl = controller.gl;
  const layer = new VolumetricAirspaceLayer("volume", {
    baseColor: "rgba(255, 0, 0, 0.5)",
    edgeColor: "#ff0000",
    opacity: 1,
    fresnelIntensity: 0,
  });
  try {
    controller.setViewMode("3D");
    controller.setZoom(20);
    controller.setPitch(2.5);
    layer.addAirspace("test-volume", [-0.2, -0.2, -0.2, 0.2, 0.2, 0.2, 0.2, -0.2], 100, 1000);
    const camera = controller.getCameraState();
    const matrix = new Float32Array(controller.projection.get_3d_view_proj_matrix(camera));
    camera.free();
    const centerEcef = lla_to_ecef(0, 0, 500);
    const clip = [
      matrix[0]! * centerEcef[0]! + matrix[4]! * centerEcef[1]! + matrix[8]! * centerEcef[2]! + matrix[12]!,
      matrix[1]! * centerEcef[0]! + matrix[5]! * centerEcef[1]! + matrix[9]! * centerEcef[2]! + matrix[13]!,
      matrix[2]! * centerEcef[0]! + matrix[6]! * centerEcef[1]! + matrix[10]! * centerEcef[2]! + matrix[14]!,
      matrix[3]! * centerEcef[0]! + matrix[7]! * centerEcef[1]! + matrix[11]! * centerEcef[2]! + matrix[15]!,
    ];
    const pixelX = Math.max(0, Math.min(canvas.width - 1, Math.floor((clip[0]! / clip[3]! + 1) * canvas.width / 2)));
    const pixelY = Math.max(0, Math.min(canvas.height - 1, Math.floor((clip[1]! / clip[3]! + 1) * canvas.height / 2)));

    gl.viewport(0, 0, canvas.width, canvas.height);
    const pixel = new Uint8Array(4);
    const drawWithHostState = (): void => {
      gl.disable(gl.DEPTH_TEST);
      gl.depthFunc(gl.GREATER);
      gl.depthMask(false);
      gl.disable(gl.BLEND);
      gl.blendEquation(gl.FUNC_REVERSE_SUBTRACT);
      layer.renderStatic(gl, matrix, { controller });
      if (gl.isEnabled(gl.DEPTH_TEST) || gl.getParameter(gl.DEPTH_FUNC) !== gl.GREATER
        || gl.getParameter(gl.DEPTH_WRITEMASK) !== false || gl.isEnabled(gl.BLEND)
        || gl.getParameter(gl.BLEND_EQUATION_RGB) !== gl.FUNC_REVERSE_SUBTRACT) {
        throw new Error("Volumetric layer leaked WebGL depth/blend state");
      }
    };
    gl.clearColor(0, 0, 0, 1);
    gl.depthMask(true);
    gl.clearDepth(0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    drawWithHostState();
    gl.readPixels(pixelX, pixelY, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    if (pixel[0] !== 0) throw new Error("Volumetric mesh ignored the existing opaque depth");

    gl.depthMask(true);
    gl.clearDepth(1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    drawWithHostState();
    gl.readPixels(pixelX, pixelY, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    if (pixel[0] === undefined || pixel[0] < 80 || pixel[1] !== 0 || pixel[2] !== 0) {
      throw new Error(`Expected translucent red volumetric pixel at projected center ${pixelX},${pixelY}, got ${Array.from(pixel)}`);
    }
    layer.opacity = 0;
    gl.depthMask(true);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    drawWithHostState();
    gl.readPixels(pixelX, pixelY, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    if (pixel[0] !== 0) throw new Error("Zero-opacity volumetric layer emitted pixels");
    return "3D volumetric alpha blend, depth state restoration, zero-opacity discard, and resource cleanup verified";
  } finally {
    layer.destroy(gl);
    controller.destroy();
  }
}

run().then((result) => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: true, result }); })
  .catch((error: unknown) => { document.querySelector("#result")!.textContent = JSON.stringify({ ok: false, error: String(error) }); });
