use crate::error_code::ErrorCode;
use std::fmt;

/// Errors that can occur during projection operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProjectionError {
    /// Camera state contains invalid parameters (e.g., zoom <= 0).
    InvalidCameraState,
    /// The point maps to a singularity in the projection (e.g., antipodal to
    /// the center of a stereographic projection).
    Singularity,
    /// Iterative solver inside unproject did not converge.
    ConvergenceFailed,
    InvalidInput,
    InvalidParameters,
}

impl ProjectionError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidCameraState => ErrorCode::new("OL-CORE-PRJ-0001"),
            Self::Singularity => ErrorCode::new("OL-CORE-PRJ-0002"),
            Self::ConvergenceFailed => ErrorCode::new("OL-CORE-PRJ-0003"),
            Self::InvalidInput => ErrorCode::new("OL-CORE-PRJ-0004"),
            Self::InvalidParameters => ErrorCode::new("OL-CORE-PRJ-0005"),
        }
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCameraState => write!(f, "Invalid camera state"),
            Self::Singularity => write!(f, "Projection singularity encountered"),
            Self::ConvergenceFailed => write!(f, "Iterative unprojection failed to converge"),
            Self::InvalidInput => write!(f, "Invalid projection input"),
            Self::InvalidParameters => write!(f, "Invalid projection parameters"),
        }
    }
}

impl std::error::Error for ProjectionError {}

#[cfg(test)]
mod tests {
    use super::ProjectionError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            ProjectionError::InvalidCameraState,
            ProjectionError::Singularity,
            ProjectionError::ConvergenceFailed,
            ProjectionError::InvalidInput,
            ProjectionError::InvalidParameters,
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            [
                "OL-CORE-PRJ-0001",
                "OL-CORE-PRJ-0002",
                "OL-CORE-PRJ-0003",
                "OL-CORE-PRJ-0004",
                "OL-CORE-PRJ-0005",
            ]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
