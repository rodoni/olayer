import { describe, expect, it } from "vitest";
import { getGlobeTileCoordinates } from "./tile";

describe("globe raster tile coverage", () => {
  it("selects front-facing tiles and omits tiles on the far side", () => {
    const centerLat = -23.62 * Math.PI / 180;
    const centerLon = -46.65 * Math.PI / 180;
    const zoom = 3;
    const tiles = getGlobeTileCoordinates(centerLat, centerLon, zoom);
    const tileCount = 2 ** zoom;
    const centerX = Math.floor(((centerLon + Math.PI) / (2 * Math.PI)) * tileCount);
    const centerY = Math.floor(
      ((1 - Math.asinh(Math.tan(centerLat)) / Math.PI) / 2) * tileCount,
    );
    const farX = (centerX + tileCount / 2) % tileCount;
    const selected = new Set(tiles.map(({ x, y }) => `${x}/${y}`));

    expect(selected.has(`${centerX}/${centerY}`)).toBe(true);
    expect(tiles.length).toBeGreaterThan(0);
    expect(tiles.length).toBeLessThan(tileCount * tileCount);
    expect(selected.has(`${farX}/${centerY}`)).toBe(false);
  });

  it("keeps low zoom coverage bounded for a globe-sized view", () => {
    const tiles = getGlobeTileCoordinates(0, 0, 2);

    expect(tiles.length).toBeLessThanOrEqual(16);
    expect(tiles.length).toBeGreaterThanOrEqual(8);
  });

  it("rejects invalid globe tile inputs", () => {
    expect(() => getGlobeTileCoordinates(Number.NaN, 0, 2)).toThrow(RangeError);
    expect(() => getGlobeTileCoordinates(0, 0, 6)).toThrow(RangeError);
  });
});
