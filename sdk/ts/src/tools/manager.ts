import {
  compute_tactical_rbl,
  generate_tactical_ppl,
  generate_tactical_holding_pattern,
  generate_tactical_ils_cone,
  generate_tactical_range_rings,
} from "olayer-wasm";

export interface LatLonCoords {
  lat: number;
  lon: number;
}

export interface RblMeasurement {
  fromCoords: LatLonCoords;
  toCoords: LatLonCoords;
  distanceNauticalMiles: number;
  distanceKilometers: number;
  trueBearingDeg: number;
  magneticBearingDeg: number;
  reciprocalTrueBearingDeg: number;
  reciprocalMagneticBearingDeg: number;
  estimatedTimeEnrouteSec?: number;
}

export interface PplTick {
  timeMinutes: number;
  distanceNauticalMiles: number;
  position: LatLonCoords;
}

export interface PplLeader {
  origin: LatLonCoords;
  groundSpeedKnots: number;
  trackDeg: number;
  ticks: PplTick[];
  trajectoryPolyline: LatLonCoords[];
}

export type TurnDirection = "StandardRight" | "NonStandardLeft";

export interface HoldingPatternConfig {
  fixCoords: LatLonCoords;
  inboundBearingDeg: number;
  turnDirection: TurnDirection;
  legTimeMinutes: number;
  airspeedKnots: number;
  pointsPerTurn?: number;
}

export interface IlsConeConfig {
  thresholdCoords: LatLonCoords;
  runwayHeadingDeg: number;
  lengthNauticalMiles: number;
  fovDeg?: number;
  extendedCenterlineNm?: number;
  arcSteps?: number;
}

export interface IlsGeometry {
  conePolygon: LatLonCoords[];
  extendedCenterline: LatLonCoords[];
}

export interface RangeRingsConfig {
  center: LatLonCoords;
  radiiNauticalMiles: number[];
  pointsPerRing?: number;
}

export interface HistoryDot {
  position: LatLonCoords;
  altitudeFt: number;
  timestampSec: number;
  opacity: number;
}

/**
 * Tactical Snail Trail (History Dots) radar decay tracker.
 */
export class SnailTrailTracker {
  private tracks: Map<string, HistoryDot[]> = new Map();

  /**
   * Appends a new radar hit for the given track.
   */
  public pushHit(
    trackId: string,
    position: LatLonCoords,
    altitudeFt: number,
    timestampSec: number
  ): void {
    let dots = this.tracks.get(trackId);
    if (!dots) {
      dots = [];
      this.tracks.set(trackId, dots);
    }
    dots.push({
      position: { ...position },
      altitudeFt,
      timestampSec,
      opacity: 1.0,
    });
  }

  /**
   * Updates opacity decay and removes dots older than `maxAgeSec` or exceeding `maxDots`.
   */
  public updateDecay(
    currentTimeSec: number,
    maxAgeSec: number = 20.0,
    maxDots: number = 10
  ): void {
    const maxAge = Math.max(0.001, maxAgeSec);
    for (const [trackId, dots] of this.tracks.entries()) {
      const filtered = dots.filter(
        (dot) => currentTimeSec - dot.timestampSec <= maxAge
      );
      if (filtered.length > maxDots) {
        filtered.splice(0, filtered.length - maxDots);
      }
      const count = filtered.length;
      for (let i = 0; i < count; i++) {
        const dot = filtered[i];
        const age = Math.max(0, currentTimeSec - dot.timestampSec);
        const ageFactor = Math.max(0, Math.min(1, 1 - age / maxAge));
        const scanFactor = (i + 1) / count;
        dot.opacity = Math.max(0.05, Math.min(1, ageFactor * scanFactor));
      }
      if (filtered.length === 0) {
        this.tracks.delete(trackId);
      } else {
        this.tracks.set(trackId, filtered);
      }
    }
  }

  /**
   * Returns active history dots for a specific track.
   */
  public getTrackDots(trackId: string): HistoryDot[] | undefined {
    return this.tracks.get(trackId);
  }

  /**
   * Clears dots for a specific track.
   */
  public removeTrack(trackId: string): boolean {
    return this.tracks.delete(trackId);
  }

  /**
   * Clears all tracks.
   */
  public clear(): void {
    this.tracks.clear();
  }
}

/**
 * Unified Tactical Aeronautical Measurement Tools engine for Web SDK.
 */
export class TacticalToolsManager {
  private snailTrails: SnailTrailTracker = new SnailTrailTracker();

  /**
   * Computes Range and Bearing Line (RBL / CRSR) measurement between two coordinates.
   */
  public computeRbl(
    fromCoords: LatLonCoords,
    toCoords: LatLonCoords,
    speedKnots?: number,
    epochYear: number = 2025.0
  ): RblMeasurement {
    const wasmRes = compute_tactical_rbl(
      fromCoords.lat,
      fromCoords.lon,
      toCoords.lat,
      toCoords.lon,
      speedKnots ?? 0.0,
      epochYear
    );

    return {
      fromCoords: { lat: wasmRes.from_lat_deg, lon: wasmRes.from_lon_deg },
      toCoords: { lat: wasmRes.to_lat_deg, lon: wasmRes.to_lon_deg },
      distanceNauticalMiles: wasmRes.distance_nm,
      distanceKilometers: wasmRes.distance_km,
      trueBearingDeg: wasmRes.true_bearing_deg,
      magneticBearingDeg: wasmRes.magnetic_bearing_deg,
      reciprocalTrueBearingDeg: wasmRes.reciprocal_true_bearing_deg,
      reciprocalMagneticBearingDeg: wasmRes.reciprocal_magnetic_bearing_deg,
      estimatedTimeEnrouteSec:
        wasmRes.estimated_time_enroute_sec >= 0
          ? wasmRes.estimated_time_enroute_sec
          : undefined,
    };
  }

  /**
   * Generates Projected Position Leader (PPL) vectors and time tick marks.
   */
  public generatePpl(
    origin: LatLonCoords,
    groundSpeedKnots: number,
    trackDeg: number,
    intervalsMinutes: number[] = [1.0, 2.0, 5.0]
  ): PplLeader {
    const flat = generate_tactical_ppl(
      origin.lat,
      origin.lon,
      groundSpeedKnots,
      trackDeg,
      new Float64Array(intervalsMinutes)
    );

    const ticks: PplTick[] = [];
    const polyline: LatLonCoords[] = [{ ...origin }];

    for (let i = 0; i < flat.length; i += 4) {
      const timeMinutes = flat[i];
      const distanceNauticalMiles = flat[i + 1];
      const lat = flat[i + 2];
      const lon = flat[i + 3];
      ticks.push({
        timeMinutes,
        distanceNauticalMiles,
        position: { lat, lon },
      });
      polyline.push({ lat, lon });
    }

    return {
      origin: { ...origin },
      groundSpeedKnots,
      trackDeg,
      ticks,
      trajectoryPolyline: polyline,
    };
  }

  /**
   * Generates standard racetrack holding pattern polyline coordinates.
   */
  public generateHoldingPatternPolyline(
    config: HoldingPatternConfig
  ): LatLonCoords[] {
    const isStandardRight = config.turnDirection === "StandardRight";
    const flat = generate_tactical_holding_pattern(
      config.fixCoords.lat,
      config.fixCoords.lon,
      config.inboundBearingDeg,
      isStandardRight,
      config.legTimeMinutes,
      config.airspeedKnots,
      config.pointsPerTurn ?? 16
    );

    const polyline: LatLonCoords[] = [];
    for (let i = 0; i < flat.length; i += 2) {
      polyline.push({ lat: flat[i], lon: flat[i + 1] });
    }
    return polyline;
  }

  /**
   * Generates ILS approach cone funnel polygon and extended centerline.
   */
  public generateIlsCone(config: IlsConeConfig): IlsGeometry {
    const fov = config.fovDeg ?? 5.0;
    const arcSteps = config.arcSteps ?? 12;
    const flat = generate_tactical_ils_cone(
      config.thresholdCoords.lat,
      config.thresholdCoords.lon,
      config.runwayHeadingDeg,
      config.lengthNauticalMiles,
      fov,
      arcSteps
    );

    const conePolygon: LatLonCoords[] = [];
    for (let i = 0; i < flat.length; i += 2) {
      conePolygon.push({ lat: flat[i], lon: flat[i + 1] });
    }

    // Extended centerline back-azimuth calculation
    const rwyHeadingRad = (config.runwayHeadingDeg * Math.PI) / 180;
    const backHeadingRad = rwyHeadingRad + Math.PI;
    const extNm = config.extendedCenterlineNm ?? config.lengthNauticalMiles * 1.5;
    const extM = extNm * 1852.0;

    // Approximate far centerline point using spherical displacement
    const earthRadius = 6378137.0;
    const deltaLat = (extM / earthRadius) * Math.cos(backHeadingRad);
    const deltaLon =
      (extM / (earthRadius * Math.cos((config.thresholdCoords.lat * Math.PI) / 180))) *
      Math.sin(backHeadingRad);

    const farPt: LatLonCoords = {
      lat: config.thresholdCoords.lat + (deltaLat * 180) / Math.PI,
      lon: config.thresholdCoords.lon + (deltaLon * 180) / Math.PI,
    };

    return {
      conePolygon,
      extendedCenterline: [{ ...config.thresholdCoords }, farPt],
    };
  }

  /**
   * Generates concentric range ring coordinate circles.
   */
  public generateRangeRings(config: RangeRingsConfig): LatLonCoords[][] {
    const steps = config.pointsPerRing ?? 72;
    const flat = generate_tactical_range_rings(
      config.center.lat,
      config.center.lon,
      new Float64Array(config.radiiNauticalMiles),
      steps
    );

    const rings: LatLonCoords[][] = [];
    const pointsPerCircle = steps + 1;
    const floatsPerCircle = pointsPerCircle * 2;

    for (let r = 0; r < config.radiiNauticalMiles.length; r++) {
      const ring: LatLonCoords[] = [];
      const startIdx = r * floatsPerCircle;
      for (let i = 0; i < pointsPerCircle; i++) {
        const idx = startIdx + i * 2;
        if (idx + 1 < flat.length) {
          ring.push({ lat: flat[idx], lon: flat[idx + 1] });
        }
      }
      rings.push(ring);
    }

    return rings;
  }

  /**
   * Access the snail trail manager.
   */
  public getSnailTrails(): SnailTrailTracker {
    return this.snailTrails;
  }
}
