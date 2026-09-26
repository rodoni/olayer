use olayer_core::geodesy::LatLon;
use olayer_core::interpolator::InterpolationEngine;
use olayer_core::projections::{CameraState, Projection, Stereographic};
use olayer_core::terrain::{AltitudeMode, AltitudeUnknownPolicy, TerrainEngine};
use std::fmt;

/// Errors returned while constructing a native controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NativeControllerError {
    /// The requested center is outside the valid geodetic range or is not finite.
    InvalidCenter,
    /// The projection rejected its construction parameters.
    Projection(olayer_core::projections::ProjectionError),
}

impl fmt::Display for NativeControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCenter => write!(f, "invalid geodetic center"),
            Self::Projection(error) => write!(f, "failed to create controller projection: {error}"),
        }
    }
}

impl std::error::Error for NativeControllerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidCenter => None,
            Self::Projection(error) => Some(error),
        }
    }
}

impl From<olayer_core::projections::ProjectionError> for NativeControllerError {
    fn from(error: olayer_core::projections::ProjectionError) -> Self {
        Self::Projection(error)
    }
}

/// Controller wrapping WASM-equivalent engines for native environments.
///
/// Acts as a **Facade** (Fachada) and central orchestrator for the native SDK.
/// Unifies geodesy, camera attitude, map projection, and cinematic interpolation
/// provided by the Rust Core, and implements dynamic FPS throttling (60/15 FPS).
pub struct NativeController {
    pub terrain: TerrainEngine,
    pub interpolator: InterpolationEngine,
    pub projection: Box<dyn Projection + Send + Sync>,
    pub camera: CameraState,
    pub view_mode: String,

    // FPS Throttler
    is_active: bool,
    last_active_time: std::time::Instant,
    active_timeout: std::time::Duration,
}

impl NativeController {
    /// Creates a native controller centered on the supplied geodetic position.
    ///
    /// Coordinates are in radians. Latitude must be within `[-pi/2, pi/2]` and
    /// longitude within `[-pi, pi]`.
    ///
    /// # Errors
    /// Returns [`NativeControllerError::InvalidCenter`] if either coordinate is
    /// non-finite or outside its valid range. Returns
    /// [`NativeControllerError::Projection`] if the projection cannot be created.
    pub fn new(center_lat: f64, center_lon: f64) -> Result<Self, NativeControllerError> {
        if !center_lat.is_finite()
            || !center_lon.is_finite()
            || !(-std::f64::consts::FRAC_PI_2..=std::f64::consts::FRAC_PI_2).contains(&center_lat)
            || !(-std::f64::consts::PI..=std::f64::consts::PI).contains(&center_lon)
        {
            return Err(NativeControllerError::InvalidCenter);
        }

        let projection = Box::new(Stereographic::new(
            center_lat,
            center_lon,
            olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
        )?);

        let camera = CameraState::with_attitude(
            LatLon::new(center_lat, center_lon, 0.0),
            1.0,                  // zoom
            0.0,                  // rotation
            35.0f64.to_radians(), // default 2.5D pitch
            0.0,                  // roll
            1.0,                  // aspect ratio (updated dynamically)
            250000.0,             // viewport base meters
        );

        Ok(Self {
            terrain: TerrainEngine::new(),
            interpolator: InterpolationEngine::new(),
            projection,
            camera,
            view_mode: "2D".to_string(),
            is_active: true,
            last_active_time: std::time::Instant::now(),
            active_timeout: std::time::Duration::from_millis(1000),
        })
    }

    /// Helper to create and initialize a new `GeoserverWmtsSource`.
    #[inline]
    pub fn create_geoserver_source(
        &self,
        id: &str,
        base_url: &str,
        layer_name: &str,
    ) -> crate::native_map_data_stack::GeoserverWmtsSource {
        crate::native_map_data_stack::GeoserverWmtsSource::new(id, base_url, layer_name)
    }

    /// Resolves a geodetic object height using the native terrain engine.
    ///
    /// # Errors
    /// Returns the terrain engine's error if the coordinate or altitude cannot
    /// be resolved under the requested policies.
    #[inline]
    pub fn resolve_altitude(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        input_height: f64,
        mode: AltitudeMode,
        unknown_policy: AltitudeUnknownPolicy,
        mesh_height: Option<f64>,
    ) -> Result<f64, olayer_core::terrain::TerrainError> {
        self.terrain.resolve_altitude(
            lat_rad,
            lon_rad,
            input_height,
            mode,
            unknown_policy,
            mesh_height,
        )
    }

    #[inline]
    pub fn trigger_active(&mut self) {
        self.is_active = true;
        self.last_active_time = std::time::Instant::now();
    }

    #[inline]
    pub fn check_active(&mut self) -> bool {
        if self.is_active && self.last_active_time.elapsed() > self.active_timeout {
            self.is_active = false;
        }
        self.is_active
    }

    #[inline]
    pub fn get_target_fps(&mut self) -> u32 {
        if self.check_active() {
            60
        } else {
            15
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_controller_new() {
        let mut ctrl = NativeController::new(0.0, 0.0).unwrap();
        assert_eq!(ctrl.view_mode, "2D");
        assert!(ctrl.check_active());
        assert_eq!(ctrl.get_target_fps(), 60);
    }

    #[test]
    fn test_native_controller_rejects_non_finite_center() {
        assert!(matches!(
            NativeController::new(f64::NAN, 0.0),
            Err(NativeControllerError::InvalidCenter)
        ));
        assert!(matches!(
            NativeController::new(0.0, f64::NAN),
            Err(NativeControllerError::InvalidCenter)
        ));
    }

    #[test]
    fn test_native_controller_rejects_out_of_range_center() {
        assert!(matches!(
            NativeController::new(std::f64::consts::FRAC_PI_2 + 0.01, 0.0),
            Err(NativeControllerError::InvalidCenter)
        ));
        assert!(matches!(
            NativeController::new(0.0, std::f64::consts::PI + 0.01),
            Err(NativeControllerError::InvalidCenter)
        ));
    }

    #[test]
    fn test_fps_throttling_active() {
        let mut ctrl = NativeController::new(0.0, 0.0).unwrap();
        // Immediately after creation, should be active
        assert_eq!(ctrl.get_target_fps(), 60);
    }

    #[test]
    fn test_fps_throttling_idle() {
        let mut ctrl = NativeController::new(0.0, 0.0).unwrap();
        // Manually set to idle by backdating the last active time
        ctrl.last_active_time = std::time::Instant::now() - std::time::Duration::from_secs(2);
        ctrl.is_active = true; // reset flag so check_active evaluates
        assert_eq!(ctrl.get_target_fps(), 15);
    }

    #[test]
    fn test_trigger_active() {
        let mut ctrl = NativeController::new(0.0, 0.0).unwrap();
        // Force idle state
        ctrl.last_active_time = std::time::Instant::now() - std::time::Duration::from_secs(2);
        ctrl.is_active = false;
        assert!(!ctrl.check_active());
        assert_eq!(ctrl.get_target_fps(), 15);

        // Trigger active should restore 60 FPS
        ctrl.trigger_active();
        assert!(ctrl.check_active());
        assert_eq!(ctrl.get_target_fps(), 60);
    }

    #[test]
    fn test_check_active_resets_after_timeout() {
        let mut ctrl = NativeController::new(0.0, 0.0).unwrap();
        assert!(ctrl.is_active);
        // Backdate last active time beyond the 1-second timeout
        ctrl.last_active_time = std::time::Instant::now() - std::time::Duration::from_millis(1500);
        assert!(!ctrl.check_active());
        assert!(!ctrl.is_active);
    }
}
