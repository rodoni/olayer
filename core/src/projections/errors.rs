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
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCameraState => write!(f, "Invalid camera state"),
            Self::Singularity => write!(f, "Projection singularity encountered"),
            Self::ConvergenceFailed => write!(f, "Iterative unprojection failed to converge"),
        }
    }
}

impl std::error::Error for ProjectionError {}
