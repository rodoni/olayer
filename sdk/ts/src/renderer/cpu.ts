import { WasmProjection, lla_to_ecef } from "olayer-wasm";
import { SymbolUV } from "./atlas";
import { LabelAntiClutterEngine, DeclutterTargetInput, SolvedLabelPlacement } from "./declutter";

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
  public declutterEngine: LabelAntiClutterEngine;

  constructor(ctx: CanvasRenderingContext2D, leaderLengthPx = 28, safetyMarginPx = 3) {
    this.ctx = ctx;
    this.declutterEngine = new LabelAntiClutterEngine(leaderLengthPx, safetyMarginPx);
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
        const ecef = lla_to_ecef(latRad, lonRad, height);
        const centerEcef = lla_to_ecef(centerLat, centerLon, 0.0);

        const relX = ecef[0] - centerEcef[0];
        const relY = ecef[1] - centerEcef[1];
        const relZ = ecef[2] - centerEcef[2];

        const m = viewProjMatrix;
        const wNdc = m[3] * relX + m[7] * relY + m[11] * relZ + m[15];
        if (wNdc <= 0.0) {
          return null;
        }

        const xNdc = m[0] * relX + m[4] * relY + m[8] * relZ + m[12];
        const yNdc = m[1] * relX + m[5] * relY + m[9] * relZ + m[13];

        const screenX = (xNdc / wNdc + 1) * 0.5 * canvasWidth;
        const screenY = (1 - yNdc / wNdc) * 0.5 * canvasHeight;

        return { x: screenX, y: screenY };
      } catch {
        return null;
      }
    } else if (viewMode === "2.5D" && viewProjMatrix) {
      try {
        const xy = projection.project(latRad, lonRad, 0.0);
        const X = xy[0];
        const Y = xy[1];
        const Z = height;

        const m = viewProjMatrix;
        const wNdc = m[3] * X + m[7] * Y + m[11] * Z + m[15];
        if (wNdc <= 0.0) {
          return null;
        }

        const xNdc = m[0] * X + m[4] * Y + m[8] * Z + m[12];
        const yNdc = m[1] * X + m[5] * Y + m[9] * Z + m[13];

        const screenX = (xNdc / wNdc + 1) * 0.5 * canvasWidth;
        const screenY = (1 - yNdc / wNdc) * 0.5 * canvasHeight;

        return { x: screenX, y: screenY };
      } catch {
        return null;
      }
    }

    try {
      const xy = projection.project(latRad, lonRad, height);
      const px = xy[0];
      const py = xy[1];

      const tx = px - cx;
      const ty = py - cy;

      const rx = tx * Math.cos(-rotation) - ty * Math.sin(-rotation);
      const ry = tx * Math.sin(-rotation) + ty * Math.cos(-rotation);

      const aspect = canvasWidth / canvasHeight;
      const w = viewportBaseMeters / zoom;
      const h = w / aspect;

      const ndcX = rx / (w / 2);
      const ndcY = ry / (h / 2);

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
      ctx.fillStyle = "#00e676";
      ctx.strokeStyle = "#00e676";
      ctx.lineWidth = 1.5;
      
      ctx.beginPath();
      ctx.arc(0, 0, 4, 0, 2 * Math.PI);
      ctx.fill();
      ctx.strokeRect(-6, -6, 12, 12);
    }
    ctx.restore();

    // Draw velocity vector (1-minute prediction)
    if (speedMps > 0.5) {
      const R = 6378137.0;
      const vectorTimeSec = 60;

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
        ctx.strokeStyle = "#00b0ff";
        ctx.lineWidth = 1.5;
        ctx.setLineDash([2, 2]);
        ctx.beginPath();
        ctx.moveTo(screenPos.x, screenPos.y);
        ctx.lineTo(screenEnd.x, screenEnd.y);
        ctx.stroke();
        ctx.restore();
      }
    }

    // 3. Draw Data Block with 8-Octant Force-Directed Anti-cluttering
    const altitudeFeet = Math.round(target.position.height * 3.28084);
    const fl = Math.round(altitudeFeet / 100);
    const speedKnots = Math.round(speedMps * 1.94384);

    const line1 = target.id;
    const line2 = `FL${fl.toString().padStart(3, "0")} ${speedKnots}KT`;

    ctx.font = "bold 11px 'Inter', 'Roboto', sans-serif";
    const w1 = ctx.measureText(line1).width;
    const w2 = ctx.measureText(line2).width;
    const labelW = Math.max(w1, w2) + 8;
    const labelH = 26;

    const targetInput: DeclutterTargetInput = {
      id: target.id,
      x: screenPos.x,
      y: screenPos.y,
      headingRad: target.heading_rad,
      width: labelW,
      height: labelH,
      priority: 0,
    };

    const solved = this.declutterEngine.solve([targetInput]);
    if (solved.length > 0) {
      const placement = solved[0];
      this.occupiedRects.push({
        x: placement.rect.x,
        y: placement.rect.y,
        w: placement.rect.width,
        h: placement.rect.height,
      });

      this.renderLabelBlock(placement, line1, line2);
    }
  }

  /**
   * Renders a solved label block with leader line and text.
   */
  public renderLabelBlock(
    placement: SolvedLabelPlacement,
    line1: string,
    line2: string
  ): void {
    const ctx = this.ctx;
    ctx.save();

    // Draw leader line
    ctx.strokeStyle = "rgba(0, 230, 118, 0.5)";
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    ctx.moveTo(placement.leaderStart[0], placement.leaderStart[1]);
    ctx.lineTo(placement.leaderEnd[0], placement.leaderEnd[1]);
    ctx.stroke();

    // Draw background box
    ctx.fillStyle = "rgba(16, 18, 24, 0.88)";
    ctx.strokeStyle = "rgba(0, 230, 118, 0.65)";
    ctx.lineWidth = 1;
    ctx.fillRect(
      placement.rect.x,
      placement.rect.y,
      placement.rect.width,
      placement.rect.height
    );
    ctx.strokeRect(
      placement.rect.x,
      placement.rect.y,
      placement.rect.width,
      placement.rect.height
    );

    // Draw label text
    ctx.fillStyle = "#00e676";
    ctx.fillText(line1, placement.rect.x + 4, placement.rect.y + 11);
    ctx.fillStyle = "#b9f6ca";
    ctx.fillText(line2, placement.rect.x + 4, placement.rect.y + 22);

    ctx.restore();
  }

  /**
   * Checks if the candidate bounding box overlaps with any already occupied screen regions.
   */
  public checkOverlap(rect: { x: number; y: number; w: number; h: number }): boolean {
    for (const r of this.occupiedRects) {
      if (
        rect.x < r.x + r.w &&
        rect.x + rect.w > r.x &&
        rect.y < r.y + r.h &&
        rect.y + rect.h > r.y
      ) {
        return true;
      }
    }
    return false;
  }
}
