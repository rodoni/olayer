use crate::error_code::ErrorCode;
use crate::projections::ProjectionError;
use thiserror::Error;

/// Errors that can occur during camera operations.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum CameraError {
    /// The camera center is not a finite coordinate in the valid geodetic range.
    #[error("Invalid camera center")]
    InvalidCenter,
    /// One or more camera attitude angles is not finite.
    #[error("Invalid camera attitude")]
    InvalidAttitude,
    /// The camera zoom factor is invalid (must be greater than zero).
    #[error("Invalid camera state: zoom must be greater than zero")]
    InvalidZoom,
    /// The camera viewport aspect ratio is invalid (must be greater than zero).
    #[error("Invalid camera state: aspect ratio must be greater than zero")]
    InvalidAspectRatio,
    /// The camera viewport base meters value is invalid (must be greater than zero).
    #[error("Invalid camera state: viewport base meters must be greater than zero")]
    InvalidViewportBase,
    /// A value needed to build a projection matrix is not finite or positive.
    #[error("Invalid projection value: {name}")]
    InvalidProjectionValue { name: &'static str },
    /// An error occurred in the underlying cartographic projection.
    #[error("Projection error: {0}")]
    Projection(ProjectionError),
}

impl CameraError {
    /// Returns the stable machine-readable code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidCenter => ErrorCode::new("OL-CORE-CAM-0001"),
            Self::InvalidAttitude => ErrorCode::new("OL-CORE-CAM-0002"),
            Self::InvalidZoom => ErrorCode::new("OL-CORE-CAM-0003"),
            Self::InvalidAspectRatio => ErrorCode::new("OL-CORE-CAM-0004"),
            Self::InvalidViewportBase => ErrorCode::new("OL-CORE-CAM-0005"),
            Self::InvalidProjectionValue { .. } => ErrorCode::new("OL-CORE-CAM-0006"),
            Self::Projection(_) => ErrorCode::new("OL-CORE-CAM-0007"),
        }
    }
}

impl From<ProjectionError> for CameraError {
    #[inline]
    fn from(err: ProjectionError) -> Self {
        Self::Projection(err)
    }
}

#[cfg(test)]
mod tests {
    use super::CameraError;
    use crate::projections::ProjectionError;

    #[test]
    fn every_variant_has_a_unique_code() {
        let errors = [
            CameraError::InvalidCenter,
            CameraError::InvalidAttitude,
            CameraError::InvalidZoom,
            CameraError::InvalidAspectRatio,
            CameraError::InvalidViewportBase,
            CameraError::InvalidProjectionValue { name: "test" },
            CameraError::Projection(ProjectionError::InvalidInput),
        ];
        let codes = errors.map(|error| error.code());

        assert_eq!(
            codes.map(|code| code.as_str()),
            [
                "OL-CORE-CAM-0001",
                "OL-CORE-CAM-0002",
                "OL-CORE-CAM-0003",
                "OL-CORE-CAM-0004",
                "OL-CORE-CAM-0005",
                "OL-CORE-CAM-0006",
                "OL-CORE-CAM-0007",
            ]
        );
        for (index, code) in codes.iter().enumerate() {
            assert!(!codes[..index].contains(code));
        }
    }
}
