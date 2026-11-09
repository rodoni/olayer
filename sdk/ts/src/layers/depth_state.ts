/** Shared depth contract for map and overlay layers. `writeDepth=false` is for ordered translucent fragments that must test, but not occlude later draws. Restores host GL state. */
export function drawWithLayerDepth(
  gl: WebGL2RenderingContext, mode: "2D" | "2.5D" | "3D", draw: () => void, imagery = false, writeDepth = true,
): void {
  const depth = gl.isEnabled(gl.DEPTH_TEST), blend = gl.isEnabled(gl.BLEND);
  const offset = gl.isEnabled(gl.POLYGON_OFFSET_FILL);
  const depthFunc = gl.getParameter(gl.DEPTH_FUNC) as number;
  const depthMask = gl.getParameter(gl.DEPTH_WRITEMASK) as boolean;
  const range = gl.getParameter(gl.DEPTH_RANGE) as Float32Array;
  const srcRgb = gl.getParameter(gl.BLEND_SRC_RGB) as number, dstRgb = gl.getParameter(gl.BLEND_DST_RGB) as number;
  const srcAlpha = gl.getParameter(gl.BLEND_SRC_ALPHA) as number, dstAlpha = gl.getParameter(gl.BLEND_DST_ALPHA) as number;
  const rgbEquation = gl.getParameter(gl.BLEND_EQUATION_RGB) as number, alphaEquation = gl.getParameter(gl.BLEND_EQUATION_ALPHA) as number;
  const lineWidth = gl.getParameter(gl.LINE_WIDTH) as number;
  const factor = gl.getParameter(gl.POLYGON_OFFSET_FACTOR) as number, units = gl.getParameter(gl.POLYGON_OFFSET_UNITS) as number;
  try {
    gl.enable(gl.BLEND);
    gl.blendEquation(gl.FUNC_ADD);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.lineWidth(1);
    gl.depthRange(0, 1);
    gl.depthFunc(gl.LEQUAL);
    gl.depthMask(mode !== "2D" && writeDepth);
    if (mode === "2D") gl.disable(gl.DEPTH_TEST); else gl.enable(gl.DEPTH_TEST);
    if (imagery && mode !== "2D") {
      gl.enable(gl.POLYGON_OFFSET_FILL);
      gl.polygonOffset(0, -1);
    } else gl.disable(gl.POLYGON_OFFSET_FILL);
    draw();
  } finally {
    gl.depthFunc(depthFunc);
    gl.depthMask(depthMask);
    gl.depthRange(range[0]!, range[1]!);
    gl.blendFuncSeparate(srcRgb, dstRgb, srcAlpha, dstAlpha);
    gl.blendEquationSeparate(rgbEquation, alphaEquation);
    gl.lineWidth(lineWidth);
    gl.polygonOffset(factor, units);
    if (offset) gl.enable(gl.POLYGON_OFFSET_FILL); else gl.disable(gl.POLYGON_OFFSET_FILL);
    if (depth) gl.enable(gl.DEPTH_TEST); else gl.disable(gl.DEPTH_TEST);
    if (blend) gl.enable(gl.BLEND); else gl.disable(gl.BLEND);
  }
}
