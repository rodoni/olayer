import { beforeAll, describe, expect, it } from "vitest";
import { initSync } from "olayer-wasm";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { TrajectoryRibbonLayer } from "./trajectory_ribbon";
import { TerrainLayer } from "./terrain";

beforeAll(() => {
  initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) });
});

describe("altitude mode layer integration", () => {
  it("resolves trajectory waypoint heights before mesh generation", () => {
    const layer = new TrajectoryRibbonLayer("test", {
      altitudeMode: "relative-to-ground",
      altitudeResolver: (_lat, _lon, height) => 800 + height,
    });
    layer.addTrajectory("route", [-23.6, -46.6, 100, -23.7, -46.7, 200]);

    expect(layer.getRibbonMesh("route")?.waypointsDeg).toEqual([
      -23.6, -46.6, 900,
      -23.7, -46.7, 1000,
    ]);
  });

  it("keeps absolute waypoint heights unchanged without a resolver", () => {
    const layer = new TrajectoryRibbonLayer("test", { altitudeMode: "absolute" });
    layer.addTrajectory("route", [-23.6, -46.6, 100, -23.7, -46.7, 200]);
    expect(layer.getRibbonMesh("route")?.waypointsDeg).toEqual([-23.6, -46.6, 100, -23.7, -46.7, 200]);
  });

  it("switches terrain visualization modes without changing the terrain source", () => {
    const layer = new TerrainLayer("terrain", 16);
    expect(layer.getRenderMode()).toBe("hypsometric");
    layer.setRenderMode("slope");
    layer.setElevationRange(100, 1800);
    layer.setHillshade({ azimuthDeg: 180, altitudeDeg: 35 });
    expect(layer.getRenderMode()).toBe("slope");

    layer.setRenderMode("taws");
    expect(layer.getRenderMode()).toBe("taws");
    layer.setTawsReferenceAltitude(1500);
    expect(layer.getTawsReferenceAltitude()).toBe(1500);

    layer.setContours(true, 50);
    expect(layer.isContoursEnabled()).toBe(true);
    expect(layer.getContourInterval()).toBe(50);
  });
});
