use std::fmt;
use crate::projections::ProjectionError;

/// Errors that can occur during camera operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraError {
    /// The camera zoom factor is invalid (must be greater than zero).
    InvalidZoom,
    /// The camera viewport aspect ratio is invalid (must be greater than zero).
    InvalidAspectRatio,
    /// The camera viewport base meters value is invalid (must be greater than zero).
    InvalidViewportBase,
    /// An error occurred in the underlying cartographic projection.
    Projection(ProjectionError),
}

impl fmt::Display for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidZoom => write!(f, "Invalid camera state: zoom must be greater than zero"),
            Self::InvalidAspectRatio => write!(f, "Invalid camera state: aspect ratio must be greater than zero"),
            Self::InvalidViewportBase => write!(f, "Invalid camera state: viewport base meters must be greater than zero"),
            Self::Projection(err) => write!(f, "Projection error: {err}"),
        }
    }
}

impl std::error::Error for CameraError {}

impl From<ProjectionError> for CameraError {
    #[inline]
    fn from(err: ProjectionError) -> Self {
        Self::Projection(err)
    }
}
