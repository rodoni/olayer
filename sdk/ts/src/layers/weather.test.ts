import { describe, it, expect, beforeAll } from "vitest";
import { initSync } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";
import { WeatherRadarLayer } from "./weather_radar";
import { WindBarbsLayer } from "./wind_barbs";
import { SigmetLayer } from "./sigmet";

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync({ module: wasmBuffer });
});

describe("Meteorological GIS Overlays (GIS-PROP-006)", () => {
  describe("WeatherRadarLayer", () => {
    it("should initialize with default options", () => {
      const layer = new WeatherRadarLayer("radar-layer");
      expect(layer.palette).toBe("nexrad");
      expect(layer.opacity).toBe(0.85);
      expect(layer.getGridData()).toBeNull();
    });

    it("should colorize dBZ grid and support palette switching", () => {
      const layer = new WeatherRadarLayer("radar-layer");
      const grid = [0.0, 20.0, 45.0, 65.0];
      layer.setDbzGrid(grid, 2, 2, [50.0, -1.0, 52.0, 1.0]);

      const rgba = layer.getColorizedRgba();
      expect(rgba).not.toBeNull();
      expect(rgba!.length).toBe(16); // 2x2 * 4

      // Pixel 0 (0 dBZ) should be transparent (alpha 0)
      expect(rgba![3]).toBe(0);

      // Pixel 2 (45 dBZ) should be non-transparent Orange
      expect(rgba![8]).toBe(253);
      expect(rgba![11]).toBeGreaterThan(0);

      // Change palette to ICAO
      layer.setPalette("icao");
      expect(layer.palette).toBe("icao");
      const icaoRgba = layer.getColorizedRgba();
      expect(icaoRgba).not.toBeNull();
    });

    it("should generate Marching Squares contour isolines", () => {
      const layer = new WeatherRadarLayer("radar-layer");
      const grid = [
        10.0, 20.0, 10.0,
        20.0, 50.0, 20.0,
        10.0, 20.0, 10.0,
      ];
      layer.setDbzGrid(grid, 3, 3, [0.0, 0.0, 2.0, 2.0]);

      const isolines = layer.generateContourIsolines([30.0]);
      expect(isolines.length).toBeGreaterThan(0);
      expect(isolines.length % 5).toBe(0);
      expect(isolines[0]).toBe(30.0);
    });

    it("should query single dBZ color directly", () => {
      const layer = new WeatherRadarLayer("radar-layer");
      const c = layer.getDbzColor(55.0, "nexrad");
      expect(c.length).toBe(4);
      expect(c[0]).toBe(212); // Dark Red (55..60 dBZ)
    });
  });

  describe("WindBarbsLayer", () => {
    it("should initialize with default options", () => {
      const layer = new WindBarbsLayer("wind-layer");
      expect(layer.defaultStaffLengthMeters).toBe(25000);
      expect(layer.strokeColor).toBe("#00e5ff");
      expect(layer.getStationCount()).toBe(0);
    });

    it("should add and compute geometries for wind stations", () => {
      const layer = new WindBarbsLayer("wind-layer");

      layer.addStation({
        id: "EGLL",
        latDeg: 51.47,
        lonDeg: -0.46,
        speedKnots: 65,
        directionDeg: 270,
      });

      layer.addStation({
        id: "SBGR",
        latDeg: -23.43,
        lonDeg: -46.47,
        speedKnots: 15,
        directionDeg: 180,
      });

      expect(layer.getStationCount()).toBe(2);

      const geomEGLL = layer.getStationGeometry("EGLL");
      expect(geomEGLL).toBeDefined();
      expect(geomEGLL!.length).toBeGreaterThanOrEqual(12);

      const geomSBGR = layer.getStationGeometry("SBGR");
      expect(geomSBGR).toBeDefined();
      expect(geomSBGR!.length).toBeGreaterThan(0);

      layer.removeStation("EGLL");
      expect(layer.getStationCount()).toBe(1);
      expect(layer.getStationGeometry("EGLL")).toBeUndefined();

      layer.clear();
      expect(layer.getStationCount()).toBe(0);
    });
  });

  describe("SigmetLayer", () => {
    const SAMPLE_SIGMET_GEOJSON = JSON.stringify({
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: {
            id: "SIG_CONVECTIVE_01",
            hazard: "THUNDERSTORM",
            severity: "SEVERE",
            floor_m: 1500,
            ceiling_m: 11000,
          },
          geometry: {
            type: "Polygon",
            coordinates: [
              [
                [-1.0, 51.0, 0.0],
                [1.0, 51.0, 0.0],
                [1.0, 52.0, 0.0],
                [-1.0, 52.0, 0.0],
                [-1.0, 51.0, 0.0],
              ],
            ],
          },
        },
      ],
    });

    it("should load GeoJSON hazard polygons", () => {
      const layer = new SigmetLayer("sigmet-layer");
      layer.loadGeoJson(SAMPLE_SIGMET_GEOJSON);

      expect(layer.getWarningCount()).toBe(1);
      expect(layer.getGeoJsonString()).toContain("SIG_CONVECTIVE_01");
    });

    it("should query hazard containment at points and flight levels", () => {
      const layer = new SigmetLayer("sigmet-layer");
      layer.loadGeoJson(SAMPLE_SIGMET_GEOJSON);

      // Inside footprint & within altitude (5000m is between 1500m and 11000m)
      const hazardsInside = layer.findHazardsAt(51.5, 0.0, 5000.0);
      expect(hazardsInside.length).toBe(1);
      expect(hazardsInside[0].id).toBe("SIG_CONVECTIVE_01");

      // Inside footprint but below floor (500m < 1500m)
      const hazardsBelow = layer.findHazardsAt(51.5, 0.0, 500.0);
      expect(hazardsBelow.length).toBe(0);

      // Outside footprint
      const hazardsOutside = layer.findHazardsAt(48.8, 2.3, 5000.0);
      expect(hazardsOutside.length).toBe(0);

      layer.destroy();
      expect(layer.getWarningCount()).toBe(0);
    });
  });
});
