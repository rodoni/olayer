pub mod lcc;
pub mod matrix;
pub mod mercator;
pub mod stereographic;

#[cfg(test)]
mod tests;

pub mod errors;

use crate::geodesy::coords::LatLon;

pub use lcc::LambertConformalConic;
pub use mercator::WebMercator;
pub use stereographic::Stereographic;
pub use errors::ProjectionError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraState {
    pub center: LatLon,
    pub zoom: f64,
    pub rotation: f64,             // In radians (bearing)
    pub aspect_ratio: f64,         // Width / Height of viewport
    pub viewport_base_meters: f64, // Base scale factor in meters (e.g., 100_000.0)
}

impl CameraState {
    /// Creates a new camera state.
    #[inline]
    pub const fn new(
        center: LatLon,
        zoom: f64,
        rotation: f64,
        aspect_ratio: f64,
        viewport_base_meters: f64,
    ) -> Self {
        Self {
            center,
            zoom,
            rotation,
            aspect_ratio,
            viewport_base_meters,
        }
    }

    /// Validates camera parameters.
    #[inline]
    pub fn validate(&self) -> Result<(), ProjectionError> {
        if self.zoom <= 0.0 {
            return Err(ProjectionError::InvalidCameraState);
        }
        if self.aspect_ratio <= 0.0 {
            return Err(ProjectionError::InvalidCameraState);
        }
        if self.viewport_base_meters <= 0.0 {
            return Err(ProjectionError::InvalidCameraState);
        }
        Ok(())
    }
}

/// Trait for cartographic projections.
pub trait Projection {
    /// Projects geodetic coordinates (LLA) to 2D plane coordinates (x, y) in meters.
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError>;

    /// Unprojects 2D plane coordinates (x, y) in meters back to geodetic coordinates (LLA).
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError>;

    /// Generates a View-Projection matrix 4x4 (column-major `[f32; 16]`) for the given [`CameraState`].
    ///
    /// This default implementation is valid for all planar projections. Individual
    /// projections may override it if they require a specialized matrix pipeline.
    #[inline]
    fn get_view_proj_matrix(&self, camera: &CameraState) -> Result<[f32; 16], ProjectionError> {
        camera.validate()?;
        let (cx, cy) = self.project(&camera.center)?;

        let view_trans = matrix::Matrix4::translation(-cx as f32, -cy as f32, 0.0);
        let view_rot = matrix::Matrix4::rotation_z(-camera.rotation as f32);
        let view = view_rot.multiply(&view_trans);

        let w = (camera.viewport_base_meters / camera.zoom) as f32;
        let h = w / camera.aspect_ratio as f32;

        let proj = matrix::Matrix4::ortho(-w / 2.0, w / 2.0, -h / 2.0, h / 2.0, -1000.0, 1000.0);
        let vp = proj.multiply(&view);

        Ok(vp.into_array())
    }
}
