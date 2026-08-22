use serde::{Deserialize, Serialize};

/// 8-octant leader arm directions relative to the target symbol center.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum OctantDirection {
    North = 0,
    NorthEast = 1,
    East = 2,
    SouthEast = 3,
    South = 4,
    SouthWest = 5,
    West = 6,
    NorthWest = 7,
}

impl OctantDirection {
    /// Returns the angle in radians of the octant direction (0 = North, clockwise).
    pub fn angle_rad(self) -> f32 {
        match self {
            Self::North => 0.0,
            Self::NorthEast => std::f32::consts::FRAC_PI_4,
            Self::East => std::f32::consts::FRAC_PI_2,
            Self::SouthEast => 3.0 * std::f32::consts::FRAC_PI_4,
            Self::South => std::f32::consts::PI,
            Self::SouthWest => 5.0 * std::f32::consts::FRAC_PI_4,
            Self::West => 3.0 * std::f32::consts::FRAC_PI_2,
            Self::NorthWest => 7.0 * std::f32::consts::FRAC_PI_4,
        }
    }

    /// Returns default preference bias cost (lower is more preferred).
    /// Standard ATC preference: NE (0.0), SE (0.1), NW (0.15), SW (0.2), E (0.3), W (0.35), N (0.4), S (0.5).
    pub fn default_preference_cost(self) -> f32 {
        match self {
            Self::NorthEast => 0.0,
            Self::SouthEast => 0.1,
            Self::NorthWest => 0.15,
            Self::SouthWest => 0.2,
            Self::East => 0.3,
            Self::West => 0.35,
            Self::North => 0.4,
            Self::South => 0.5,
        }
    }

    pub fn all() -> [OctantDirection; 8] {
        [
            Self::NorthEast,
            Self::SouthEast,
            Self::NorthWest,
            Self::SouthWest,
            Self::East,
            Self::West,
            Self::North,
            Self::South,
        ]
    }
}

/// A 2D axis-aligned bounding box for label anti-cluttering collision detection.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rect2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect2D {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn intersects(&self, other: &Rect2D) -> bool {
        !(self.x + self.width <= other.x
            || other.x + other.width <= self.x
            || self.y + self.height <= other.y
            || other.y + other.height <= self.y)
    }

    pub fn intersection_area(&self, other: &Rect2D) -> f32 {
        let x_overlap = (self.x + self.width).min(other.x + other.width) - self.x.max(other.x);
        let y_overlap = (self.y + self.height).min(other.y + other.height) - self.y.max(other.y);
        if x_overlap > 0.0 && y_overlap > 0.0 {
            x_overlap * y_overlap
        } else {
            0.0
        }
    }

    pub fn expanded(&self, margin: f32) -> Rect2D {
        Rect2D {
            x: self.x - margin,
            y: self.y - margin,
            width: self.width + 2.0 * margin,
            height: self.height + 2.0 * margin,
        }
    }
}

/// Target input descriptor for label deconfliction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelTarget {
    /// Unique target identifier (callsign/track number).
    pub id: String,
    /// Screen coordinate X in pixels.
    pub x: f32,
    /// Screen coordinate Y in pixels.
    pub y: f32,
    /// Heading/track angle in radians (0 = North, clockwise).
    pub heading_rad: Option<f32>,
    /// Label box width in pixels.
    pub width: f32,
    /// Label box height in pixels.
    pub height: f32,
    /// Priority level (higher priority targets are placed first, 0 = highest).
    pub priority: u8,
}

/// Solved optimal placement for a target's label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelPlacement {
    pub id: String,
    pub octant: OctantDirection,
    pub rect: Rect2D,
    pub leader_start: [f32; 2],
    pub leader_end: [f32; 2],
    pub cost: f32,
    pub visible: bool,
}

/// Configuration settings for the 8-octant force-directed label anti-cluttering engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclutterConfig {
    /// Default nominal leader arm length in pixels.
    pub leader_length_px: f32,
    /// Safety margin around label bounding box in pixels.
    pub safety_margin_px: f32,
    /// Weight penalty for overlapping area with other labels/symbols.
    pub weight_overlap: f32,
    /// Weight penalty for placing leader arm along the aircraft heading vector.
    pub weight_heading: f32,
    /// Weight penalty for leader line intersections.
    pub weight_leader_crossing: f32,
    /// Weight penalty for non-preferred octant directions.
    pub weight_preference: f32,
    /// Maximum relaxation iterations for local search.
    pub max_iterations: usize,
}

impl Default for DeclutterConfig {
    fn default() -> Self {
        Self {
            leader_length_px: 28.0,
            safety_margin_px: 3.0,
            weight_overlap: 1000.0,
            weight_heading: 25.0,
            weight_leader_crossing: 80.0,
            weight_preference: 10.0,
            max_iterations: 3,
        }
    }
}
