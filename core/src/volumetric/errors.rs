use thiserror::Error;

/// Errors arising during 3D volumetric mesh generation and ribbon extrusion.
#[derive(Debug, PartialEq, Error)]
pub enum VolumetricError {
    #[error("Insufficient vertices: expected at least {expected}, found {actual}")]
    InsufficientVertices { expected: usize, actual: usize },
    #[error("Invalid altitude bounds: floor ({floor_m} m) must be below ceiling ({ceiling_m} m)")]
    InvalidAltitudeBounds { floor_m: f64, ceiling_m: f64 },
    #[error("Triangulation failed: {0}")]
    TriangulationFailed(String),
    #[error("Invalid ribbon parameters: {0}")]
    InvalidRibbonParameters(String),
    #[error("Degenerate polygon geometry: {0}")]
    DegenerateGeometry(String),
}
