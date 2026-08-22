use std::fmt;

/// Errors arising during 3D volumetric mesh generation and ribbon extrusion.
#[derive(Debug, PartialEq)]
pub enum VolumetricError {
    InsufficientVertices {
        expected: usize,
        actual: usize,
    },
    InvalidAltitudeBounds {
        floor_m: f64,
        ceiling_m: f64,
    },
    TriangulationFailed(String),
    InvalidRibbonParameters(String),
    DegenerateGeometry(String),
}

impl fmt::Display for VolumetricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientVertices { expected, actual } => {
                write!(f, "Insufficient vertices: expected at least {expected}, found {actual}")
            }
            Self::InvalidAltitudeBounds { floor_m, ceiling_m } => {
                write!(
                    f,
                    "Invalid altitude bounds: floor ({floor_m} m) must be strictly less than ceiling ({ceiling_m} m)"
                )
            }
            Self::TriangulationFailed(msg) => write!(f, "Triangulation failed: {msg}"),
            Self::InvalidRibbonParameters(msg) => write!(f, "Invalid ribbon parameters: {msg}"),
            Self::DegenerateGeometry(msg) => write!(f, "Degenerate polygon geometry: {msg}"),
        }
    }
}

impl std::error::Error for VolumetricError {}
