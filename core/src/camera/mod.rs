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
    ///
    /// # Errors
    /// Returns an error when the center, attitude, or scale parameters are invalid.
    ///
    /// # Panics
    /// This method does not panic.
    #[inline]
    pub fn validate(&self) -> Result<(), CameraError> {
        if !self.center.lat.is_finite()
            || !self.center.lon.is_finite()
            || !self.center.height.is_finite()
            || self.center.lat < -std::f64::consts::FRAC_PI_2
            || self.center.lat > std::f64::consts::FRAC_PI_2
            || self.center.lon < -std::f64::consts::PI
            || self.center.lon > std::f64::consts::PI
        {
            return Err(CameraError::InvalidCenter);
        }
        if !self.rotation.is_finite() || !self.pitch.is_finite() || !self.roll.is_finite() {
            return Err(CameraError::InvalidAttitude);
        }
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            return Err(CameraError::InvalidZoom);
        }
        if !self.aspect_ratio.is_finite() || self.aspect_ratio <= 0.0 {
            return Err(CameraError::InvalidAspectRatio);
        }
        if !self.viewport_base_meters.is_finite() || self.viewport_base_meters <= 0.0 {
            return Err(CameraError::InvalidViewportBase);
        }
        Ok(())
    }

    /// Generates a standard flat 2D orthographic View-Projection matrix.
    ///
    /// # Errors
    /// Returns an error when the camera, projection result, or derived matrix values are invalid.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn get_2d_view_proj_matrix(
        &self,
        projection: &dyn Projection,
    ) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let (cx, cy) = projection.project(&self.center)?;
        let cx = finite_f32(cx, "center x")?;
        let cy = finite_f32(cy, "center y")?;
        let rotation = finite_f32(self.rotation, "rotation")?;

        let view_trans = Matrix4::translation(-cx, -cy, 0.0);
        let view_rot = Matrix4::rotation_z(-rotation);
        let view = view_rot.multiply(&view_trans);

        let w = positive_f32(self.viewport_base_meters / self.zoom, "viewport width")?;
        let aspect = positive_f32(self.aspect_ratio, "aspect ratio")?;
        let h = positive_f32(f64::from(w) / f64::from(aspect), "viewport height")?;

        let proj = Matrix4::ortho(-w / 2.0, w / 2.0, -h / 2.0, h / 2.0, -1000.0, 1000.0)?;
        let vp = proj.multiply(&view);

        Ok(vp.into_array())
    }

    /// Generates a perspective 2.5D View-Projection matrix for a tilted flat map.
    ///
    /// # Errors
    /// Returns an error when the camera, projection result, or derived matrix values are invalid.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn get_25d_view_proj_matrix(
        &self,
        projection: &dyn Projection,
    ) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let (cx, cy) = projection.project(&self.center)?;
        let cx = finite_f32(cx, "center x")?;
        let cy = finite_f32(cy, "center y")?;

        // Calculate a camera distance that scales nicely with zoom
        let w = positive_f32(self.viewport_base_meters / self.zoom, "viewport width")?;
        let distance = positive_f32(f64::from(w) * 0.8, "camera distance")?;
        let rotation = finite_f32(self.rotation, "rotation")?;
        let pitch = finite_f32(self.pitch, "pitch")?;
        let roll = finite_f32(self.roll, "roll")?;

        // 1. Translate the map target center (cx, cy) to the origin
        let trans_target = Matrix4::translation(-cx, -cy, 0.0);

        // 2. Rotate around Z (camera heading bearing / yaw)
        let rot_z = Matrix4::rotation_z(-rotation);

        // 3. Tilt the camera (pitch) around the X axis
        let rot_x = Matrix4::rotation_x(pitch);

        // 4. Roll the camera around the Y axis
        let rot_y = Matrix4::rotation_y(roll);

        // 5. Translate back along Z by the view distance
        let trans_dist = Matrix4::translation(0.0, 0.0, -distance);

        // Combine view matrices: trans_dist * rot_y * rot_x * rot_z * trans_target
        let view = trans_dist
            .multiply(&rot_y)
            .multiply(&rot_x)
            .multiply(&rot_z)
            .multiply(&trans_target);

        // 6. Perspective projection matrix
        let fovy = positive_f32(45.0_f64.to_radians(), "field of view")?;
        let aspect = positive_f32(self.aspect_ratio, "aspect ratio")?;
        let near = positive_f32(f64::from(distance) * 0.01, "near plane")?;
        let far = positive_f32(f64::from(distance) * 10.0, "far plane")?;
        let proj_mat = Matrix4::perspective(fovy, aspect, near, far)?;

        let vp = proj_mat.multiply(&view);
        Ok(vp.into_array())
    }

    /// Generates a perspective View-Projection matrix for 3D globe visualization.
    ///
    /// # Errors
    /// Returns an error when the camera or derived matrix values are invalid.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn get_3d_view_proj_matrix(&self) -> Result<[f32; 16], CameraError> {
        self.validate()?;
        let earth_radius = crate::geodesy::ellipsoid::Ellipsoid::wgs84().a;
        let base_distance = 15000000.0_f64;
        let distance = positive_f32(
            earth_radius + (base_distance / self.zoom),
            "camera distance",
        )?;
        let lat = finite_f32(self.center.lat - std::f64::consts::FRAC_PI_2, "latitude")?;
        let lon = finite_f32(-self.center.lon - std::f64::consts::FRAC_PI_2, "longitude")?;
        let rotation = finite_f32(self.rotation, "rotation")?;
        let pitch = finite_f32(self.pitch, "pitch")?;
        let roll = finite_f32(self.roll, "roll")?;

        let trans = Matrix4::translation(0.0, 0.0, -distance);

        let lat_rot = Matrix4::rotation_x(lat);
        let lon_rot = Matrix4::rotation_z(lon);

        // Apply camera orientation: rotation (bearing/yaw), pitch (tilt), and roll
        let rot_z = Matrix4::rotation_z(-rotation);
        let rot_x = Matrix4::rotation_x(pitch);
        let rot_y = Matrix4::rotation_y(roll);

        let view = trans
            .multiply(&rot_y)
            .multiply(&rot_x)
            .multiply(&rot_z)
            .multiply(&lat_rot)
            .multiply(&lon_rot);

        let fovy = positive_f32(45.0_f64.to_radians(), "field of view")?;
        let aspect = positive_f32(self.aspect_ratio, "aspect ratio")?;
        let near = positive_f32(50000.0, "near plane")?;
        let far = positive_f32(40000000.0, "far plane")?;
        let proj = Matrix4::perspective(fovy, aspect, near, far)?;

        let vp = proj.multiply(&view);
        Ok(vp.into_array())
    }
}

#[inline]
fn finite_f32(value: f64, name: &'static str) -> Result<f32, CameraError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CameraError::InvalidProjectionValue { name })
    }
}

#[inline]
fn positive_f32(value: f64, name: &'static str) -> Result<f32, CameraError> {
    let value = finite_f32(value, name)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(CameraError::InvalidProjectionValue { name })
    }
}
