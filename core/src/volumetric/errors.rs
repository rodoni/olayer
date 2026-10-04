use crate::error_code::ErrorCode;
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

impl VolumetricError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InsufficientVertices { .. } => ErrorCode::new("OL-CORE-VOL-0001"),
            Self::InvalidAltitudeBounds { .. } => ErrorCode::new("OL-CORE-VOL-0002"),
            Self::TriangulationFailed(_) => ErrorCode::new("OL-CORE-VOL-0003"),
            Self::InvalidRibbonParameters(_) => ErrorCode::new("OL-CORE-VOL-0004"),
            Self::DegenerateGeometry(_) => ErrorCode::new("OL-CORE-VOL-0005"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VolumetricError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            VolumetricError::InsufficientVertices {
                expected: 3,
                actual: 0,
            },
            VolumetricError::InvalidAltitudeBounds {
                floor_m: 1.0,
                ceiling_m: 0.0,
            },
            VolumetricError::TriangulationFailed(String::new()),
            VolumetricError::InvalidRibbonParameters(String::new()),
            VolumetricError::DegenerateGeometry(String::new()),
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            [
                "OL-CORE-VOL-0001",
                "OL-CORE-VOL-0002",
                "OL-CORE-VOL-0003",
                "OL-CORE-VOL-0004",
                "OL-CORE-VOL-0005",
            ]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
