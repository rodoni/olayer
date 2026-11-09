import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { initSync, WasmCameraState, WasmProjection } from "olayer-wasm";

type Triple = readonly [number, number, number];
type Mode = "2d" | "25d" | "3d";
type ProjectionKind = "mercator" | "stereographic";
interface Point {
  readonly lla: Triple;
  readonly xy: readonly [number, number];
}
interface CameraCase {
  readonly name: string;
  readonly projection: ProjectionKind;
  readonly mode: Mode;
  readonly center: Triple;
  readonly attitude: Triple;
  readonly zoom: number;
  readonly aspect: number;
  readonly width: number;
  readonly matrix: readonly number[];
  readonly points: readonly Point[];
}
interface Fixture {
  readonly reference: string;
  readonly matrixAbsolute: number;
  readonly matrixRelative: number;
  readonly projectionTolerance: number;
  readonly cases: readonly CameraCase[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isArray(value: unknown): value is readonly unknown[] {
  return Array.isArray(value);
}
function record(value: unknown): Record<string, unknown> {
  if (!isRecord(value)) throw new TypeError("Expected camera fixture object");
  return value;
}
function array(value: unknown): readonly unknown[] {
  if (!isArray(value)) throw new TypeError("Expected camera fixture array");
  return value;
}
function number(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError("Expected finite camera fixture number");
  }
  return value;
}
function positive(value: unknown): number {
  const result = number(value);
  if (result <= 0) throw new RangeError("Expected positive fixture parameter");
  return result;
}
function text(value: unknown): string {
  if (typeof value !== "string" || value.length === 0) throw new TypeError("Expected fixture text");
  return value;
}
function triple(value: unknown): Triple {
  const values = array(value);
  if (values.length !== 3) throw new TypeError("Expected coordinate/attitude triple");
  return [number(values[0]), number(values[1]), number(values[2])];
}
function point(value: unknown): Point {
  const fields = record(value);
  const xy = array(fields["xy"]);
  if (xy.length !== 2) throw new TypeError("Expected projected pair");
  return { lla: triple(fields["lla"]), xy: [number(xy[0]), number(xy[1])] };
}
function cameraCase(value: unknown): CameraCase {
  const fields = record(value);
  const mode = fields["mode"];
  const projection = fields["projection"];
  if (mode !== "2d" && mode !== "25d" && mode !== "3d") throw new TypeError("Invalid camera mode");
  if (projection !== "mercator" && projection !== "stereographic") throw new TypeError("Invalid projection");
  const matrix = array(fields["matrix"]).map(number);
  const points = array(fields["points"]).map(point);
  if (matrix.length !== 16 || points.length === 0) throw new TypeError("Incomplete camera reference");
  return {
    name: text(fields["name"]), projection, mode,
    center: triple(fields["center"]), attitude: triple(fields["attitude"]),
    zoom: positive(fields["zoom"]), aspect: positive(fields["aspect"]), width: positive(fields["width"]),
    matrix, points,
  };
}
function parseFixture(value: unknown): Fixture {
  const fields = record(value);
  const cases = array(fields["cases"]).map(cameraCase);
  if (cases.length < 7) throw new TypeError("Missing camera conformance cases");
  return {
    reference: text(fields["reference"]),
    matrixAbsolute: positive(fields["matrix_absolute_tolerance"]),
    matrixRelative: positive(fields["matrix_relative_tolerance"]),
    projectionTolerance: positive(fields["projection_tolerance_meters"]), cases,
  };
}

const fixture = parseFixture(JSON.parse(readFileSync(
  resolve(__dirname, "../../../../tests/fixtures/camera/conformance.json"), "utf8",
)));
const radians = (degrees: number): number => degrees * Math.PI / 180;

beforeAll(() => {
  initSync({ module: readFileSync(resolve(__dirname, "../../wasm/pkg/olayer_wasm_bg.wasm")) });
});

function close(actual: number, expected: number, absolute: number, relative: number): void {
  expect(Number.isFinite(actual)).toBe(true);
  expect(Math.abs(actual - expected)).toBeLessThanOrEqual(absolute + relative * Math.abs(expected));
}

describe("real WASM camera/projection shared analytical conformance", () => {
  for (const entry of fixture.cases) {
    it(entry.name, () => {
      const [lat, lon, height] = entry.center;
      const [heading, pitch, roll] = entry.attitude;
      const camera = new WasmCameraState(radians(lat), radians(lon), height,
        entry.zoom, radians(heading), radians(pitch), radians(roll), entry.aspect, entry.width);
      const projection = entry.projection === "mercator"
        ? WasmProjection.new_web_mercator()
        : WasmProjection.new_stereographic(radians(lat), radians(lon));
      try {
        const matrix = entry.mode === "2d" ? projection.get_view_proj_matrix(camera)
          : entry.mode === "25d" ? projection.get_25d_view_proj_matrix(camera)
            : projection.get_3d_view_proj_matrix(camera);
        expect(matrix.length).toBe(16);
        entry.matrix.forEach((expected, index) => {
          // Preserve sensitivity to tiny map scale coefficients while allowing f32 quarter-turn noise.
          const absolute = expected === 0 ? fixture.matrixAbsolute
            : fixture.matrixAbsolute * Math.min(1, Math.abs(expected));
          close(number(matrix[index]), expected, absolute, fixture.matrixRelative);
        });
        for (const sample of entry.points) {
          const [pointLat, pointLon, pointHeight] = sample.lla;
          const xy = projection.project(radians(pointLat), radians(pointLon), pointHeight);
          expect(xy.length).toBe(2);
          sample.xy.forEach((expected, index) => {
            close(number(xy[index]), expected, fixture.projectionTolerance, 0);
          });
        }
      } finally {
        projection.free();
        camera.free();
      }
    });
  }
});
