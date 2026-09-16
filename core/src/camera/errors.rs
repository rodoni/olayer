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

impl From<ProjectionError> for CameraError {
    #[inline]
    fn from(err: ProjectionError) -> Self {
        Self::Projection(err)
    }
}
