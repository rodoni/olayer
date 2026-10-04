import { describe, expect, it } from "vitest";
import { buildWcsGetCoverageUrl, buildWfsGetFeatureUrl, buildWmsGetMapUrl } from "./ogc";

describe("OGC request builders", () => {
  it("builds an escaped WMS request", () => {
    const url = buildWmsGetMapUrl({ endpoint: "https://example.test/ows", layers: "roads & water", crs: "EPSG:4326", bounds: [1, 2, 3, 4], width: 256, height: 128 });
    expect(url).toContain("SERVICE=WMS&REQUEST=GetMap");
    expect(url).toContain("LAYERS=roads%20%26%20water");
  });

  it("builds WFS and WCS requests with repeated parameters", () => {
    const wfs = buildWfsGetFeatureUrl({ endpoint: "https://example.test/wfs", typeName: "roads", crs: "EPSG:3857", bbox: [0, 1, 2, 3], count: 10 });
    const wcs = buildWcsGetCoverageUrl({ endpoint: "https://example.test/wcs", coverageId: "elevation", crs: "EPSG:4326", bounds: [0, 1, 2, 3], width: 64, height: 32 });
    expect(wfs).toContain("BBOX=0%2C1%2C2%2C3&COUNT=10");
    expect(wcs).toContain("SUBSET=x(0%2C2)&SUBSET=y(1%2C3)");
  });

  it("rejects invalid dimensions and bounds", () => {
    expect(() => buildWmsGetMapUrl({ endpoint: "https://example.test", layers: "roads", crs: "EPSG:4326", bounds: [2, 1, 0, 3], width: 1, height: 1 })).toThrowError(RangeError);
    expect(() => buildWcsGetCoverageUrl({ endpoint: "https://example.test", coverageId: "elevation", crs: "EPSG:4326", bounds: [0, 1, 2, 3], width: 0, height: 1 })).toThrowError(RangeError);
  });
});
