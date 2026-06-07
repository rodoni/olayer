import { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { SymbolUV } from "./atlas";

export interface InterpolatedTarget {
  id: string;
  position: {
    lat: number;
    lon: number;
    height: number;
  };
  heading_rad: number;
}

export class CPURenderer {
  private ctx: CanvasRenderingContext2D;
  private occupiedRects: { x: number; y: number; w: number; h: number }[] = [];

  constructor(ctx: CanvasRenderingContext2D) {
    this.ctx = ctx;
  }

  /**
   * Clears the occupied screen regions list for the current frame.
   */
  public beginFrame(): void {
    this.occupiedRects = [];
  }

  /**
   * Projects geodetic coordinates to pixel screen coordinates using the camera parameters.
   */
  public projectToScreen(
    projection: WasmProjection,
    latRad: number,
    lonRad: number,
    height: number,
    cx: number, // camera center projected X
    cy: number, // camera center projected Y
    zoom: number,
    rotation: number,
    viewportBaseMeters: number,
    canvasWidth: number,
    canvasHeight: number,
    viewMode: string = "2D",
    viewProjMatrix?: Float32Array,
    centerLat?: number,
    centerLon?: number
  ): { x: number; y: number } | null {
    if (viewMode === "3D" && viewProjMatrix && centerLat !== undefined && centerLon !== undefined) {
      try {
        // 1. Convert LLA to ECEF using WASM's lla_to_ecef
        const xyz = lla_to_ecef(latRad, lonRad, height);
        const X = xyz[0];
        const Y = xyz[1];
        const Z = xyz[2];

        // 2. Horizon occlusion culling
        // Calculate camera ECEF coordinates
        const R = 6378137.0;
        const baseDistance = 15000000.0;
        const distance = R + (baseDistance / zoom);
        const camXyz = lla_to_ecef(centerLat, centerLon, distance - R);
        
        // Dot product between camera ECEF and target ECEF
        const dot = camXyz[0] * X + camXyz[1] * Y + camXyz[2] * Z;
        if (dot < R * R) {
          // Culled (blocked by earth)
          return null;
        }

        // 3. Project using view-projection matrix (column-major)
        const m = viewProjMatrix;
        const wNdc = m[3] * X + m[7] * Y + m[11] * Z + m[15];
        if (wNdc <= 0.0) {
          return null; // Behind near plane
        }

        const xNdc = m[0] * X + m[4] * Y + m[8] * Z + m[12];
        const yNdc = m[1] * X + m[5] * Y + m[9] * Z + m[13];

        const screenX = (xNdc / wNdc + 1) * 0.5 * canvasWidth;
        const screenY = (1 - yNdc / wNdc) * 0.5 * canvasHeight;

        return { x: screenX, y: screenY };
      } catch (err) {
        return null;
      }
    } else if (viewMode === "2.5D" && viewProjMatrix) {
      try {
        // 1. Project target coordinates to flat map planar meters
        const xy = projection.project(latRad, lonRad, 0.0);
        const X = xy[0];
        const Y = xy[1];
        const Z = height; // Z-axis is aircraft altitude in meters

        // 2. Project using view-projection matrix (column-major)
        const m = viewProjMatrix;
        const wNdc = m[3] * X + m[7] * Y + m[11] * Z + m[15];
        if (wNdc <= 0.0) {
          return null; // Behind near plane
        }

        const xNdc = m[0] * X + m[4] * Y + m[8] * Z + m[12];
        const yNdc = m[1] * X + m[5] * Y + m[9] * Z + m[13];

        const screenX = (xNdc / wNdc + 1) * 0.5 * canvasWidth;
        const screenY = (1 - yNdc / wNdc) * 0.5 * canvasHeight;

        return { x: screenX, y: screenY };
      } catch (err) {
        return null;
      }
    }

    try {
      // 1. Project to planar meters
      const xy = projection.project(latRad, lonRad, height);
      const px = xy[0];
      const py = xy[1];

      // 2. Translate by camera center
      const tx = px - cx;
      const ty = py - cy;

      // 3. Rotate by negative camera rotation
      const rx = tx * Math.cos(-rotation) - ty * Math.sin(-rotation);
      const ry = tx * Math.sin(-rotation) + ty * Math.cos(-rotation);

      // 4. Map to Normalized Device Coordinates (NDC)
      const aspect = canvasWidth / canvasHeight;
      const w = viewportBaseMeters / zoom;
      const h = w / aspect;

      const ndcX = rx / (w / 2);
      const ndcY = ry / (h / 2);

      // 5. Convert to screen pixels
      const screenX = (ndcX + 1) * 0.5 * canvasWidth;
      const screenY = (1 - ndcY) * 0.5 * canvasHeight;

      return { x: screenX, y: screenY };
    } catch {
      return null;
    }
  }

  /**
   * Draws a dynamic target along with its predicted vector and anti-cluttered data block.
   */
  public drawTarget(
    target: InterpolatedTarget,
    screenPos: { x: number; y: number },
    projection: WasmProjection,
    cx: number,
    cy: number,
    zoom: number,
    rotation: number,
    viewportBaseMeters: number,
    canvasWidth: number,
    canvasHeight: number,
    speedMps: number,
    atlasTexture: HTMLImageElement | HTMLCanvasElement | null,
    symbolUv: SymbolUV | undefined,
    viewMode: string = "2D",
    viewProjMatrix?: Float32Array,
    centerLat?: number,
    centerLon?: number
  ): void {
    const ctx = this.ctx;
    
    // Draw target dot/icon
    ctx.save();
    ctx.translate(screenPos.x, screenPos.y);

    if (atlasTexture && symbolUv) {
      // Draw from Texture Atlas
      const sw = symbolUv.width;
      const sh = symbolUv.height;
      ctx.drawImage(
        atlasTexture,
        symbolUv.u0 * atlasTexture.width,
        symbolUv.v0 * atlasTexture.height,
        sw,
        sh,
        -sw / 2,
        -sh / 2,
        sw,
        sh
      );
    } else {
      // Fallback: draw standard ATC target symbol (square/circle)
      ctx.fillStyle = "#00e676"; // Bright green
      ctx.strokeStyle = "#00e676";
      ctx.lineWidth = 1.5;
      
      // Target dot
      ctx.beginPath();
      ctx.arc(0, 0, 4, 0, 2 * Math.PI);
      ctx.fill();

      // Outer square
      ctx.strokeRect(-6, -6, 12, 12);
    }
    ctx.restore();

    // Draw velocity vector (1-minute prediction)
    if (speedMps > 0.5) {
      const R = 6378137.0; // Earth radius
      const vectorTimeSec = 60; // 1 minute prediction

      // Approximation of displacement on sphere
      const latOffset = (speedMps * vectorTimeSec * Math.cos(target.heading_rad)) / R;
      const lonOffset = (speedMps * vectorTimeSec * Math.sin(target.heading_rad)) / (R * Math.cos(target.position.lat));

      const endLat = target.position.lat + latOffset;
      const endLon = target.position.lon + lonOffset;

      const screenEnd = this.projectToScreen(
        projection,
        endLat,
        endLon,
        target.position.height,
        cx,
        cy,
        zoom,
        rotation,
        viewportBaseMeters,
        canvasWidth,
        canvasHeight,
        viewMode,
        viewProjMatrix,
        centerLat,
        centerLon
      );

      if (screenEnd) {
        ctx.save();
        ctx.strokeStyle = "#00b0ff"; // Sleek blue vector line
        ctx.lineWidth = 1.5;
        ctx.setLineDash([2, 2]); // Dashed predictive line
        ctx.beginPath();
        ctx.moveTo(screenPos.x, screenPos.y);
        ctx.lineTo(screenEnd.x, screenEnd.y);
        ctx.stroke();
        ctx.restore();
      }
    }

    // 3. Draw Data Block with Anti-cluttering
    const altitudeFeet = Math.round(target.position.height * 3.28084);
    const fl = Math.round(altitudeFeet / 100);
    const speedKnots = Math.round(speedMps * 1.94384);

    const line1 = target.id;
    const line2 = `FL${fl.toString().padStart(3, "0")} ${speedKnots}KT`;

    // Measure label size
    ctx.font = "bold 11px 'Inter', 'Roboto', sans-serif";
    const w1 = ctx.measureText(line1).width;
    const w2 = ctx.measureText(line2).width;
    const labelW = Math.max(w1, w2) + 8;
    const labelH = 26;

    // Define 4 candidate offsets (TR, BR, BL, TL)
    const offsets = [
      { dx: 15, dy: -30 }, // Top-Right (default)
      { dx: 15, dy: 10 },  // Bottom-Right
      { dx: -labelW - 15, dy: 10 }, // Bottom-Left
      { dx: -labelW - 15, dy: -30 }, // Top-Left
    ];

    let chosenOffset = null;
    let labelRect = { x: 0, y: 0, w: labelW, h: labelH };

    for (const offset of offsets) {
      const rx = screenPos.x + offset.dx;
      const ry = screenPos.y + offset.dy;

      // Construct candidate bounding box with a small safety margin
      const candidate = {
        x: rx - 2,
        y: ry - 2,
        w: labelW + 4,
        h: labelH + 4,
      };

      if (!this.checkOverlap(candidate)) {
        chosenOffset = offset;
        labelRect = { x: rx, y: ry, w: labelW, h: labelH };
        break;
      }
    }

    // Draw label if we found a non-cluttered position
    if (chosenOffset) {
      this.occupiedRects.push(labelRect);

      ctx.save();
      
      // Draw leader line from target to label anchor
      let leaderEndX = labelRect.x;
      let leaderEndY = labelRect.y + labelH / 2;
      if (chosenOffset.dx < 0) {
        leaderEndX = labelRect.x + labelW;
      }
      
      ctx.strokeStyle = "rgba(0, 230, 118, 0.4)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(screenPos.x, screenPos.y);
      ctx.lineTo(leaderEndX, leaderEndY);
      ctx.stroke();

      // Draw label background box
      ctx.fillStyle = "rgba(16, 18, 24, 0.85)"; // Sleek semi-transparency
      ctx.strokeStyle = "rgba(0, 230, 118, 0.6)"; // Muted green border
      ctx.lineWidth = 1;
      ctx.fillRect(labelRect.x, labelRect.y, labelRect.w, labelRect.h);
      ctx.strokeRect(labelRect.x, labelRect.y, labelRect.w, labelRect.h);

      // Draw label text
      ctx.fillStyle = "#00e676"; // Bright radar green
      ctx.fillText(line1, labelRect.x + 4, labelRect.y + 11);
      ctx.fillStyle = "#b9f6ca"; // Soft green
      ctx.fillText(line2, labelRect.x + 4, labelRect.y + 22);

      ctx.restore();
    }
  }

  /**
   * Checks if the candidate bounding box overlaps with any already occupied screen regions.
   */
  private checkOverlap(rect: { x: number; y: number; w: number; h: number }): boolean {
    for (const r of this.occupiedRects) {
      if (
        rect.x < r.x + r.w &&
        rect.x + rect.w > r.x &&
        rect.y < r.y + r.h &&
        rect.y + rect.h > r.y
      ) {
        return true; // Overlap detected
      }
    }
    return false;
  }
}
