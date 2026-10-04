import { describe, expect, it } from "vitest";
import { buildWmtsTileUrl } from "./wmts";

describe("buildWmtsTileUrl", () => {
  it("builds deterministic escaped WMTS requests", () => {
    const url = buildWmtsTileUrl(
      { endpoint: "https://example.test/wmts", layer: "roads & water", style: "" },
      { x: 4, y: 5, z: 3 },
    );

    expect(url).toBe(
      "https://example.test/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=roads%20%26%20water&STYLE=&TILEMATRIXSET=EPSG%3A900913&TILEMATRIX=EPSG%3A900913%3A3&TILEROW=5&TILECOL=4&FORMAT=image%2Fpng",
    );
  });

  it("preserves an existing endpoint query string", () => {
    const url = buildWmtsTileUrl(
      { endpoint: "https://example.test/wmts?token=abc", layer: "roads" },
      { x: 0, y: 0, z: 0 },
    );

    expect(url).toContain("token=abc&SERVICE=WMTS");
  });

  it("rejects tiles outside their matrix", () => {
    expect(() => buildWmtsTileUrl({ endpoint: "https://example.test", layer: "roads" }, { x: 2, y: 0, z: 1 })).toThrowError(RangeError);
  });
});
