pub mod errors;

#[cfg(test)]
mod tests;

pub use errors::CameraError;

use crate::geodesy::coords::LatLon;
use crate::projections::matrix::Matrix4;
use crate::projections::Projection;

/// Represents the camera view state in 3D geospatial space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraState {
    /// The geodetic focal point of the camera on the Earth's surface.
    pub center: LatLon,
    /// The linear zoom scale factor.
    pub zoom: f64,
    /// The horizontal rotation bearing angle in radians (yaw / heading).
    pub rotation: f64,
    /// The vertical tilt angle in radians (pitch / tilt). Nadir (looking straight down) is 0.
    pub pitch: f64,
    /// The lateral roll angle in radians.
    pub roll: f64,
    /// The width-to-height aspect ratio of the viewport.
    pub aspect_ratio: f64,
    /// The base size scale of the viewport in meters (e.g., 100_000.0 meters).
    pub viewport_base_meters: f64,
}

impl CameraState {
    /// Creates a new camera state with pitch and roll initialized to 0.0.
    /// Preserves backward compatibility with the original constructor.
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
            pitch: 0.0,
            roll: 0.0,
            aspect_ratio,
            viewport_base_meters,
        }
    }

    /// Creates a new camera state with attitude orientation parameters (pitch and roll).
    #[inline]
    pub const fn with_attitude(
        center: LatLon,
        zoom: f64,
        rotation: f64,
        pitch: f64,
        roll: f64,
        aspect_ratio: f64,
        viewport_base_meters: f64,
    ) -> Self {
        Self {
            center,
            zoom,
            rotation,
            pitch,
            roll,
            aspect_ratio,
            viewport_base_meters,
        }
    }

    /// Validates camera parameters to prevent divisions by zero and projection singularities.
    #[inline]
    pub fn validate(&self) -> Result<(), CameraError> {
        if self.zoom <= 0.0 {
            return Err(CameraError::InvalidZoom);
        }
        if self.aspect_ratio <= 0.0 {
            return Err(CameraError::InvalidAspectRatio);
        }
        if self.viewport_base_meters <= 0.0 {
            return Err(CameraError::InvalidViewportBase);
        }
        Ok(())
    }

    /// Generates a standard flat 2D orthographic View-Projection matrix.
    pub fn get_2d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let (cx, cy) = projection.project(&self.center)?;

        let view_trans = Matrix4::translation(-cx as f32, -cy as f32, 0.0);
        let view_rot = Matrix4::rotation_z(-self.rotation as f32);
        let view = view_rot.multiply(&view_trans);

        let w = (self.viewport_base_meters / self.zoom) as f32;
        let h = w / self.aspect_ratio as f32;

        let proj = Matrix4::ortho(-w / 2.0, w / 2.0, -h / 2.0, h / 2.0, -1000.0, 1000.0);
        let vp = proj.multiply(&view);

        Ok(vp.into_array())
    }

    /// Generates a perspective 2.5D View-Projection matrix for a tilted flat map.
    pub fn get_25d_view_proj_matrix(&self, projection: &dyn Projection) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let (cx, cy) = projection.project(&self.center)?;

        // Calculate a camera distance that scales nicely with zoom
        let w = (self.viewport_base_meters / self.zoom) as f32;
        let distance = w * 0.8;

        // 1. Translate the map target center (cx, cy) to the origin
        let trans_target = Matrix4::translation(-cx as f32, -cy as f32, 0.0);
        
        // 2. Rotate around Z (camera heading bearing / yaw)
        let rot_z = Matrix4::rotation_z(-self.rotation as f32);
        
        // 3. Tilt the camera (pitch) around the X axis
        let rot_x = Matrix4::rotation_x(self.pitch as f32);

        // 4. Roll the camera around the Y axis
        let rot_y = Matrix4::rotation_y(self.roll as f32);
        
        // 5. Translate back along Z by the view distance
        let trans_dist = Matrix4::translation(0.0, 0.0, -distance);

        // Combine view matrices: trans_dist * rot_y * rot_x * rot_z * trans_target
        let view = trans_dist
            .multiply(&rot_y)
            .multiply(&rot_x)
            .multiply(&rot_z)
            .multiply(&trans_target);

        // 6. Perspective projection matrix
        let fovy = 45.0_f64.to_radians() as f32;
        let aspect = self.aspect_ratio as f32;
        let near = distance * 0.01;
        let far = distance * 10.0;
        let proj_mat = Matrix4::perspective(fovy, aspect, near, far);

        let vp = proj_mat.multiply(&view);
        Ok(vp.into_array())
    }

    /// Generates a perspective View-Projection matrix for 3D globe visualization.
    pub fn get_3d_view_proj_matrix(&self) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let earth_radius = crate::geodesy::ellipsoid::Ellipsoid::wgs84().a;
        let base_distance = 15000000.0_f64;
        let distance = earth_radius + (base_distance / self.zoom);

        let trans = Matrix4::translation(0.0, 0.0, -distance as f32);
        
        let lat_rot = Matrix4::rotation_x((self.center.lat - std::f64::consts::FRAC_PI_2) as f32);
        let lon_rot = Matrix4::rotation_z((-self.center.lon - std::f64::consts::FRAC_PI_2) as f32);

        // Apply camera orientation: rotation (bearing/yaw), pitch (tilt), and roll
        let rot_z = Matrix4::rotation_z(-self.rotation as f32);
        let rot_x = Matrix4::rotation_x(self.pitch as f32);
        let rot_y = Matrix4::rotation_y(self.roll as f32);

        let view = trans
            .multiply(&rot_y)
            .multiply(&rot_x)
            .multiply(&rot_z)
            .multiply(&lat_rot)
            .multiply(&lon_rot);

        let fovy = 45.0_f64.to_radians() as f32;
        let aspect = self.aspect_ratio as f32;
        let near = 50000.0_f32;
        let far = 40000000.0_f32;
        let proj = Matrix4::perspective(fovy, aspect, near, far);

        let vp = proj.multiply(&view);
        Ok(vp.into_array())
    }
}
