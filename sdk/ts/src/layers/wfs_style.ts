import type { WfsFeatureCollection } from "../map_provider/ogc_client";

export type WfsScalar = null | boolean | string | number;
export type WfsColor = readonly [number, number, number, number];
export type WfsMatchPriority = "first-match" | "last-match";
export type WfsPredicate =
  | { readonly kind: "exists"; readonly property: string }
  | { readonly kind: "equals"; readonly property: string; readonly value: WfsScalar };
export interface WfsPaint {
  readonly visibility: "visible" | "hidden";
  readonly stroke?: WfsColor;
  readonly fill?: WfsColor;
}
export interface WfsStyleRule {
  readonly predicate: WfsPredicate;
  readonly paint: WfsPaint;
}
export interface WfsStyleSheet {
  readonly defaultPaint: WfsPaint;
  readonly rules: readonly WfsStyleRule[];
  readonly priority?: WfsMatchPriority;
}

const ENCODER = new TextEncoder();
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
function boundedString(value: unknown, max: number): string {
  if (typeof value !== "string") throw new TypeError("WFS style requires a string");
  if (value.length > max || ENCODER.encode(value).length > max) throw new RangeError("WFS style string limit exceeded");
  return value;
}
function scalar(value: unknown): WfsScalar {
  if (value === null || typeof value === "boolean") return value;
  if (typeof value === "string") return boundedString(value, 4096);
  if (typeof value === "number") {
    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) throw new RangeError("Invalid WFS numeric style value");
    return value;
  }
  throw new TypeError("WFS equality requires a JSON scalar");
}
function isColorChannel(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
}
function color(value: unknown): WfsColor {
  if (!Array.isArray(value) || value.length !== 4) {
    throw new RangeError("Invalid WFS style color");
  }
  // Read accessor-backed arrays once, then validate and freeze exactly those values.
  const red = value[0];
  const green = value[1];
  const blue = value[2];
  const alpha = value[3];
  if (![red, green, blue, alpha].every(isColorChannel)) throw new RangeError("Invalid WFS style color");
  const result: WfsColor = [red, green, blue, alpha];
  return Object.freeze(result);
}
function paint(value: unknown): WfsPaint {
  if (!record(value) || (value["visibility"] !== "visible" && value["visibility"] !== "hidden")) throw new TypeError("Invalid WFS paint visibility");
  return Object.freeze({ visibility: value["visibility"],
    ...(value["stroke"] === undefined ? {} : { stroke: color(value["stroke"]) }),
    ...(value["fill"] === undefined ? {} : { fill: color(value["fill"]) }),
  });
}

/** Validate and snapshot a bounded stylesheet. Throws TypeError/RangeError without changing a layer. */
export function validateWfsStyleSheet(value: unknown): WfsStyleSheet {
  if (!record(value) || !Array.isArray(value["rules"])) throw new TypeError("Invalid WFS stylesheet");
  const rawRules = value["rules"];
  if (rawRules.length > 64) throw new RangeError("WFS style rule limit exceeded");
  const rawPriority = Object.prototype.hasOwnProperty.call(value, "priority") ? value["priority"] : undefined;
  const priority = rawPriority === undefined && !Object.prototype.hasOwnProperty.call(value, "priority") ? "first-match" : rawPriority;
  if (priority !== "first-match" && priority !== "last-match") throw new TypeError("Invalid WFS match priority");
  const rules = Array.from(rawRules, (rule: unknown): WfsStyleRule => {
    if (!record(rule) || !record(rule["predicate"])) throw new TypeError("Invalid WFS style rule");
    const raw = rule["predicate"];
    const property = boundedString(raw["property"], 256);
    let predicate: WfsPredicate;
    if (raw["kind"] === "exists") predicate = { kind: "exists", property };
    else if (raw["kind"] === "equals") predicate = { kind: "equals", property, value: scalar(raw["value"]) };
    else throw new TypeError("Invalid WFS predicate");
    return Object.freeze({ predicate: Object.freeze(predicate), paint: paint(rule["paint"]) });
  });
  return Object.freeze({ defaultPaint: paint(value["defaultPaint"]), rules: Object.freeze(rules), priority });
}

/** Evaluate once per collection/style update; IDs and inherited properties never participate. */
export function evaluateWfsStyle(collection: unknown, rawSheet: unknown): readonly WfsPaint[] {
  if (!record(collection) || !Array.isArray(collection["features"])) throw new TypeError("Invalid WFS collection");
  const features = collection["features"];
  if (features.length > 100_000) throw new RangeError("WFS style feature limit exceeded");
  const sheet = validateWfsStyleSheet(rawSheet);
  if (features.length * sheet.rules.length > 6_400_000) throw new RangeError("WFS style evaluation budget exceeded");
  return features.map((feature: unknown): WfsPaint => {
    if (!record(feature)) throw new TypeError("Invalid WFS feature");
    const properties = feature["properties"];
    if (!record(properties)) throw new TypeError("Invalid WFS feature properties");
    let selected = sheet.defaultPaint;
    let matched = false;
    // Resolve the own property once; `exists` deliberately accepts all JSON values,
    // while scalar equality rejects/does not match structured values.
    for (const rule of sheet.rules) {
      const { predicate } = rule;
      if (!Object.prototype.hasOwnProperty.call(properties, predicate.property)) continue;
      const value = properties[predicate.property];
      if (value === undefined) throw new TypeError("Undefined WFS property is not JSON");
      let predicateMatches: boolean;
      if (predicate.kind === "exists") {
        predicateMatches = true;
      } else if (value === null || typeof value === "boolean") {
        predicateMatches = value === predicate.value;
      } else if (typeof value === "string") {
        predicateMatches = boundedString(value, 4096) === predicate.value;
      } else if (typeof value === "number") {
        predicateMatches = scalar(value) === predicate.value;
      } else {
        predicateMatches = false;
      }
      if ((!matched || sheet.priority === "last-match") && predicateMatches) {
        selected = rule.paint;
        matched = true;
      }
    }
    return selected;
  });
}
