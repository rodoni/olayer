import { solve_label_placements_flat } from "olayer-wasm";

export enum OctantDirection {
  North = 0,
  NorthEast = 1,
  East = 2,
  SouthEast = 3,
  South = 4,
  SouthWest = 5,
  West = 6,
  NorthWest = 7,
}

export interface DeclutterTargetInput {
  id: string;
  x: number;
  y: number;
  headingRad?: number;
  width: number;
  height: number;
  priority?: number;
}

export interface SolvedLabelPlacement {
  id: string;
  rect: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  leaderStart: [number, number];
  leaderEnd: [number, number];
  octant: OctantDirection;
  cost: number;
}

/**
 * High-performance 8-Octant Force-Directed Label Anti-Cluttering Engine.
 * Deconflicts radar target data blocks across 8 radial directions with velocity vector
 * deconfliction and leader line crossing avoidance.
 */
export class LabelAntiClutterEngine {
  public defaultLeaderLengthPx: number;
  public defaultSafetyMarginPx: number;

  constructor(leaderLengthPx = 28, safetyMarginPx = 3) {
    this.defaultLeaderLengthPx = leaderLengthPx;
    this.defaultSafetyMarginPx = safetyMarginPx;
  }

  /**
   * Solves non-overlapping 8-octant placements for a batch of radar targets.
   *
   * @param targets Array of targets with screen positions, label dimensions, and headings.
   * @param leaderLengthPx Optional custom leader arm length in pixels.
   * @param safetyMarginPx Optional safety margin around label boxes in pixels.
   */
  public solve(
    targets: DeclutterTargetInput[],
    leaderLengthPx?: number,
    safetyMarginPx?: number
  ): SolvedLabelPlacement[] {
    const n = targets.length;
    if (n === 0) {
      return [];
    }

    const leaderLen = leaderLengthPx ?? this.defaultLeaderLengthPx;
    const margin = safetyMarginPx ?? this.defaultSafetyMarginPx;

    // Pack into flat float32 array: [x, y, heading_or_neg1, width, height, priority] (6 floats per target)
    const flatInput = new Float32Array(n * 6);
    for (let i = 0; i < n; i++) {
      const t = targets[i];
      flatInput[i * 6] = t.x;
      flatInput[i * 6 + 1] = t.y;
      flatInput[i * 6 + 2] = t.headingRad !== undefined ? t.headingRad : -1.0;
      flatInput[i * 6 + 3] = t.width;
      flatInput[i * 6 + 4] = t.height;
      flatInput[i * 6 + 5] = t.priority ?? 0;
    }

    const flatOutput = solve_label_placements_flat(flatInput, leaderLen, margin);

    const placements: SolvedLabelPlacement[] = new Array(n);
    for (let i = 0; i < n; i++) {
      const offset = i * 10;
      placements[i] = {
        id: targets[i].id,
        rect: {
          x: flatOutput[offset],
          y: flatOutput[offset + 1],
          width: flatOutput[offset + 2],
          height: flatOutput[offset + 3],
        },
        leaderStart: [flatOutput[offset + 4], flatOutput[offset + 5]],
        leaderEnd: [flatOutput[offset + 6], flatOutput[offset + 7]],
        octant: flatOutput[offset + 8] as OctantDirection,
        cost: flatOutput[offset + 9],
      };
    }

    return placements;
  }
}
