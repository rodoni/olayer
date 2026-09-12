export type AltitudeMode =
  | "absolute"
  | "clamp-to-ground"
  | "relative-to-ground"
  | "relative-to-mesh";

export type AltitudeUnknownPolicy = "reject" | "use-absolute" | "use-zero";

export type AltitudeResolver = (
  latRad: number,
  lonRad: number,
  inputHeightMeters: number,
  mode: AltitudeMode,
) => number;
