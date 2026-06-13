use olayer_core::geodesy::LatLon;
use olayer_core::terrain::TerrainEngine;
use olayer_core::interpolator::InterpolationEngine;
use olayer_core::projections::{Projection, CameraState, Stereographic};

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
    pub fn new(center_lat: f64, center_lon: f64) -> Self {
        let projection = Box::new(Stereographic::new(
            center_lat,
            center_lon,
            olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
        ));

        let camera = CameraState::with_attitude(
            LatLon::new(center_lat, center_lon, 0.0),
            1.0,  // zoom
            0.0,  // rotation
            35.0f64.to_radians(), // default 2.5D pitch
            0.0,  // roll
            1.0,  // aspect ratio (updated dynamically)
            250000.0, // viewport base meters
        );

        Self {
            terrain: TerrainEngine::new(),
            interpolator: InterpolationEngine::new(),
            projection,
            camera,
            view_mode: "2D".to_string(),
            is_active: true,
            last_active_time: std::time::Instant::now(),
            active_timeout: std::time::Duration::from_millis(1000),
        }
    }

    pub fn trigger_active(&mut self) {
        self.is_active = true;
        self.last_active_time = std::time::Instant::now();
    }

    pub fn check_active(&mut self) -> bool {
        if self.is_active && self.last_active_time.elapsed() > self.active_timeout {
            self.is_active = false;
        }
        self.is_active
    }

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
        let mut ctrl = NativeController::new(0.0, 0.0);
        assert_eq!(ctrl.view_mode, "2D");
        assert!(ctrl.check_active());
        assert_eq!(ctrl.get_target_fps(), 60);
    }

    #[test]
    fn test_fps_throttling_active() {
        let mut ctrl = NativeController::new(0.0, 0.0);
        // Immediately after creation, should be active
        assert_eq!(ctrl.get_target_fps(), 60);
    }

    #[test]
    fn test_fps_throttling_idle() {
        let mut ctrl = NativeController::new(0.0, 0.0);
        // Manually set to idle by backdating the last active time
        ctrl.last_active_time = std::time::Instant::now() - std::time::Duration::from_secs(2);
        ctrl.is_active = true; // reset flag so check_active evaluates
        assert_eq!(ctrl.get_target_fps(), 15);
    }

    #[test]
    fn test_trigger_active() {
        let mut ctrl = NativeController::new(0.0, 0.0);
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
        let mut ctrl = NativeController::new(0.0, 0.0);
        assert!(ctrl.is_active);
        // Backdate last active time beyond the 1-second timeout
        ctrl.last_active_time = std::time::Instant::now() - std::time::Duration::from_millis(1500);
        assert!(!ctrl.check_active());
        assert!(!ctrl.is_active);
    }
}
