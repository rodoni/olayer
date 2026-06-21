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

pub use crate::camera::CameraState;

/// Trait for cartographic projections.
pub trait Projection {
    /// Projects geodetic coordinates (LLA) to 2D plane coordinates (x, y) in meters.
    fn project(&self, lla: &LatLon) -> Result<(f64, f64), ProjectionError>;

    /// Unprojects 2D plane coordinates (x, y) in meters back to geodetic coordinates (LLA).
    fn unproject(&self, x: f64, y: f64) -> Result<LatLon, ProjectionError>;

    /// Dynamically updates the center of projection (point of tangency) if supported.
    #[inline]
    fn update_center(&mut self, _center_lat_rad: f64, _center_lon_rad: f64) {}

    /// Generates a View-Projection matrix 4x4 (column-major `[f32; 16]`) for the given [`CameraState`].
    ///
    /// This default implementation is valid for all planar projections. Individual
    /// projections may override it if they require a specialized matrix pipeline.
    #[inline]
    fn get_view_proj_matrix(&self, camera: &CameraState) -> Result<[f32; 16], ProjectionError> {
        camera.validate().map_err(|_| ProjectionError::InvalidCameraState)?;
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
