import { describe, it, expect, beforeAll } from "vitest";
import { initSync } from "olayer-wasm";
import { readFileSync } from "fs";
import { resolve } from "path";
import { AeronauticalLayer } from "./aeronautical";

const wasmPath = resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm");

beforeAll(() => {
  const wasmBuffer = readFileSync(wasmPath);
  initSync({ module: wasmBuffer });
});

const SAMPLE_AIXM_51 = `<?xml version="1.0" encoding="UTF-8"?>
<aixm:AIXMBasicMessage xmlns:aixm="http://www.aixm.aero/schema/5.1"
                       xmlns:gml="http://www.opengis.net/gml/3.2">
    <aixm:hasMember>
        <aixm:Airspace gml:id="EGLL_TMA">
            <aixm:name>LONDON TMA SECTOR 1</aixm:name>
            <aixm:type>TMA</aixm:type>
            <aixm:AirspaceVolume>
                <aixm:upperLimit uom="FL">195</aixm:upperLimit>
                <aixm:lowerLimit uom="FT">2500</aixm:lowerLimit>
            </aixm:AirspaceVolume>
            <gml:Polygon gml:id="POLY_EGLL">
                <gml:exterior>
                    <gml:LinearRing>
                        <gml:posList>51.0 -0.5 51.5 -0.5 51.5 0.5 51.0 0.5 51.0 -0.5</gml:posList>
                    </gml:LinearRing>
                </gml:exterior>
            </gml:Polygon>
        </aixm:Airspace>
    </aixm:hasMember>
    <aixm:hasMember>
        <aixm:Navaid gml:id="NAV_BIG">
            <aixm:designator>BIG</aixm:designator>
            <aixm:name>BIGGIN VOR</aixm:name>
            <aixm:type>VOR</aixm:type>
            <aixm:frequency>115.10</aixm:frequency>
            <gml:Point gml:id="PT_BIG">
                <gml:pos>51.330 0.030</gml:pos>
            </gml:Point>
        </aixm:Navaid>
    </aixm:hasMember>
</aixm:AIXMBasicMessage>`;

const SAMPLE_GEOJSON = JSON.stringify({
  type: "FeatureCollection",
  features: [
    {
      type: "Feature",
      properties: {
        aero_type: "Airspace",
        uid: "TEST_CTR",
        name: "TEST CTR",
        airspace_type: "CTR",
        lower_limit_m: 0.0,
        upper_limit_fl: 50,
      },
      geometry: {
        type: "Polygon",
        coordinates: [
          [
            [0.0, 50.0, 0.0],
            [1.0, 50.0, 0.0],
            [1.0, 51.0, 0.0],
            [0.0, 51.0, 0.0],
            [0.0, 50.0, 0.0],
          ],
        ],
      },
    },
    {
      type: "Feature",
      properties: {
        aero_type: "Navaid",
        ident: "TST",
        name: "TEST VOR",
        navaid_type: "VOR",
        frequency_mhz: 112.5,
      },
      geometry: {
        type: "Point",
        coordinates: [0.5, 50.5, 50.0],
      },
    },
  ],
});

describe("AeronauticalLayer", () => {
  it("should initialize with default options", () => {
    const layer = new AeronauticalLayer("aero-1");
    expect(layer.id).toBe("aero-1");
    expect(layer.visible).toBe(true);
    expect(layer.options.showNavaids).toBe(true);
    expect(layer.options.airspaceStrokeColor).toBe("#ff9100");
    expect(layer.getFeatureCount()).toBe(0);
  });

  it("should load AIXM 5.1 XML dataset", () => {
    const layer = new AeronauticalLayer("aero-aixm");
    layer.loadAixm51(SAMPLE_AIXM_51);

    expect(layer.getFeatureCount()).toBe(2);
    const ds = layer.getDataset();
    expect(ds).not.toBeNull();
    expect(ds?.airspace_count()).toBe(1);
    expect(ds?.navaid_count()).toBe(1);

    const geojson = layer.getGeoJsonString();
    expect(geojson).not.toBeNull();
    expect(geojson).toContain("EGLL_TMA");
    expect(geojson).toContain("BIG");

    layer.destroy();
    expect(layer.getDataset()).toBeNull();
  });

  it("should load GeoJSON-Aviation dataset and execute spatial queries", () => {
    const layer = new AeronauticalLayer("aero-geojson");
    layer.loadGeoJson(SAMPLE_GEOJSON);

    expect(layer.getFeatureCount()).toBe(2);

    // Query navaids near center (lat 50.5 deg = 0.8814 rad, lon 0.5 deg = 0.0087 rad)
    const latRad = (50.5 * Math.PI) / 180;
    const lonRad = (0.5 * Math.PI) / 180;
    const navaids = layer.findNavaidsNear(latRad, lonRad, 100_000);
    expect(navaids.length).toBe(1);
    expect(navaids[0].ident).toBe("TST");

    // Query airspace containment
    const airspaces = layer.findAirspacesContaining(latRad, lonRad);
    expect(airspaces.length).toBe(1);
    expect(airspaces[0].uid).toBe("TEST_CTR");

    layer.destroy();
  });
});
