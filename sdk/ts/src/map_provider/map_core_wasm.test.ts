import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { initSync, map_build_request_json, map_parse_capabilities_json, map_validate_model_json } from "olayer-wasm";

function hasCodedMapError(error: unknown, code: string): boolean {
  return error instanceof Error && "code" in error && error.code === code && "module" in error && error.module === "MAP";
}

beforeAll(() => {
  initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) });
});

describe("WASM Map Core v1 wire facade", () => {
  it("uses the shared Rust WMS axis/query fixture", () => {
    const input = readFileSync(resolve(__dirname, "../../../../tests/fixtures/map-core/wms-4326.json"), "utf8");
    const result: unknown = JSON.parse(map_build_request_json(input));
    expect(result).toEqual({
      version: 1,
      url: "https://example.test/ows?tenant=one&SERVICE=WMS&REQUEST=GetMap&VERSION=1.3.0&LAYERS=roads&STYLES=&CRS=EPSG%3A4326&BBOX=20%2C10%2C40%2C30&WIDTH=256&HEIGHT=128&FORMAT=image%2Fpng&TRANSPARENT=true#map",
    });
  });

  it("builds explicit WMTS/WFS/WCS and capabilities requests", () => {
    const requests = [
      { operation: { kind: "wmts", endpoint: { base: "https://example.test", path: "wmts" }, layer: "roads", style: "default", matrix_set: "custom", matrix: { identifier: "matrix:3", epsg: 3857, matrix_width: 2, matrix_height: 3, tile_width: 512, tile_height: 256 }, tile: { x: 1, y: 2, z: 3 }, format: "image/png" }, expected: "TILEMATRIX=matrix%3A3&TILEROW=2&TILECOL=1" },
      { operation: { kind: "wfs", endpoint: { base: "https://example.test", path: "wfs" }, type_name: "roads", epsg: 4326, format: "application/geo+json", count: 10 }, expected: "TYPENAMES=roads" },
      { operation: { kind: "wcs", endpoint: { base: "https://example.test", path: "wcs" }, coverage_id: "elevation", epsg: 4326, bounds: { epsg: 4326, min_x: 0, min_y: 1, max_x: 2, max_y: 3 }, width: 2, height: 1, format: "image/tiff" }, expected: "COVERAGEID=elevation" },
      { operation: { kind: "get_capabilities", endpoint: { base: "https://example.test", path: "ows" }, service: "WMTS" }, expected: "SERVICE=WMTS&REQUEST=GetCapabilities" },
    ];
    for (const entry of requests) {
      const input = JSON.stringify({ version: 1, operation: entry.operation });
      const result: unknown = JSON.parse(map_build_request_json(input));
      expect(result).toEqual(expect.objectContaining({ version: 1 }));
      expect(JSON.stringify(result)).toContain(entry.expected);
    }
  });

  it("returns a bounded capabilities summary and validates model envelopes", () => {
    const summary: unknown = JSON.parse(map_parse_capabilities_json(new TextEncoder().encode("<WMS_Capabilities version=\"1.3.0\"><Service><Title>Demo</Title></Service><Capability><Layer><Name>roads</Name></Layer></Capability></WMS_Capabilities>")));
    expect(summary).toEqual({ version: 1, service: "WMS", service_version: "1.3.0", title: "Demo", layers: ["roads"] });
    expect(map_validate_model_json(JSON.stringify({ version: 1, operation: { kind: "coverage_scalar", bounds: { epsg: 4326, min_x: 0, min_y: 0, max_x: 1, max_y: 1 }, width: 1, height: 1, sample_type: "Float64", samples: [4] } }))).toBe("{\"version\":1,\"valid\":true}");
  });

  it("surfaces stable error code and MAP module", () => {
    try {
      map_build_request_json("{\"version\":2,\"operation\":{}} ");
      throw new Error("Expected version error");
    } catch (error: unknown) {
      expect(hasCodedMapError(error, "OL-CORE-MAP-0012")).toBe(true);
    }
  });
});
