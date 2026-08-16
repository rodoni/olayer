import { describe, it, expect, beforeAll } from "vitest";
import { TacticalToolsManager, SnailTrailTracker } from "./index";
import { initSync } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync({ module: wasmBuffer });
});

describe("TacticalToolsManager", () => {
  it("should compute Range and Bearing Line (RBL) measurements", () => {
    const tools = new TacticalToolsManager();

    // JFK to Boston Logan
    const rbl = tools.computeRbl(
      { lat: 40.64, lon: -73.78 },
      { lat: 42.36, lon: -71.01 },
      450.0,
      2025.0
    );

    expect(rbl.distanceNauticalMiles).toBeGreaterThan(150.0);
    expect(rbl.distanceNauticalMiles).toBeLessThan(185.0);
    expect(rbl.distanceKilometers).toBeGreaterThan(280.0);
    expect(rbl.trueBearingDeg).toBeGreaterThan(45.0);
    expect(rbl.trueBearingDeg).toBeLessThan(65.0);
    expect(rbl.magneticBearingDeg).toBeGreaterThan(rbl.trueBearingDeg);
    expect(rbl.estimatedTimeEnrouteSec).toBeDefined();
    expect(rbl.estimatedTimeEnrouteSec!).toBeGreaterThan(1100.0);
  });

  it("should generate Projected Position Leader (PPL) ticks and polyline", () => {
    const tools = new TacticalToolsManager();
    const ppl = tools.generatePpl({ lat: 40.0, lon: -74.0 }, 480.0, 90.0, [
      1.0, 2.0, 5.0,
    ]);

    expect(ppl.ticks).toHaveLength(3);
    expect(ppl.trajectoryPolyline).toHaveLength(4);

    expect(ppl.ticks[0].timeMinutes).toBe(1.0);
    expect(Math.abs(ppl.ticks[0].distanceNauticalMiles - 8.0)).toBeLessThan(1e-3);
    expect(Math.abs(ppl.ticks[1].distanceNauticalMiles - 16.0)).toBeLessThan(1e-3);
    expect(Math.abs(ppl.ticks[2].distanceNauticalMiles - 40.0)).toBeLessThan(1e-3);

    // Eastward track
    expect(ppl.ticks[0].position.lon).toBeGreaterThan(ppl.origin.lon);
  });

  it("should generate standard racetrack holding pattern polyline", () => {
    const tools = new TacticalToolsManager();
    const poly = tools.generateHoldingPatternPolyline({
      fixCoords: { lat: 51.5, lon: -0.1 },
      inboundBearingDeg: 270.0,
      turnDirection: "StandardRight",
      legTimeMinutes: 1.0,
      airspeedKnots: 210.0,
      pointsPerTurn: 16,
    });

    expect(poly.length).toBeGreaterThanOrEqual(34);
    // Closed loop at fix
    expect(Math.abs(poly[0].lat - 51.5)).toBeLessThan(1e-5);
    expect(Math.abs(poly[poly.length - 1].lat - 51.5)).toBeLessThan(1e-5);
  });

  it("should generate ILS approach funnel cone and centerline", () => {
    const tools = new TacticalToolsManager();
    const ils = tools.generateIlsCone({
      thresholdCoords: { lat: 51.4775, lon: -0.4614 },
      runwayHeadingDeg: 270.0,
      lengthNauticalMiles: 10.0,
      fovDeg: 5.0,
      extendedCenterlineNm: 15.0,
      arcSteps: 12,
    });

    expect(ils.conePolygon.length).toBeGreaterThanOrEqual(14);
    expect(ils.extendedCenterline).toHaveLength(2);
    expect(ils.conePolygon[0]).toEqual(ils.conePolygon[ils.conePolygon.length - 1]);
  });

  it("should generate concentric range rings", () => {
    const tools = new TacticalToolsManager();
    const rings = tools.generateRangeRings({
      center: { lat: 0.0, lon: 0.0 },
      radiiNauticalMiles: [5.0, 10.0, 20.0],
      pointsPerRing: 36,
    });

    expect(rings).toHaveLength(3);
    for (const ring of rings) {
      expect(ring.length).toBe(37); // 36 steps + 1 to close
    }
  });

  it("should manage snail trail history dots and opacity decay", () => {
    const tracker = new SnailTrailTracker();
    const trackId = "AFR456";

    tracker.pushHit(trackId, { lat: 48.85, lon: 2.35 }, 12000, 100);
    tracker.pushHit(trackId, { lat: 48.86, lon: 2.36 }, 12000, 104);
    tracker.pushHit(trackId, { lat: 48.87, lon: 2.37 }, 12000, 108);

    expect(tracker.getTrackDots(trackId)).toHaveLength(3);

    tracker.updateDecay(112, 10, 5);
    const dots = tracker.getTrackDots(trackId)!;
    expect(dots.length).toBeLessThanOrEqual(3);

    // Oldest dot should have lower opacity than latest dot
    expect(dots[dots.length - 1].opacity).toBeGreaterThan(dots[0].opacity);

    tracker.removeTrack(trackId);
    expect(tracker.getTrackDots(trackId)).toBeUndefined();
  });
});
