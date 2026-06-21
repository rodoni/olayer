use std::sync::Arc;
use olayer_core::geodesy::LatLon;
use olayer_native::NativeController;

/// A simulated radar target for the desktop demo.
pub struct SimulatedTarget {
    pub id: Arc<str>,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub speed: f64,
    pub heading: f64,
}

/// Advances simulated targets by `dt` seconds and forwards their state to the
/// interpolation engine. Returns the number of targets updated.
pub fn update_simulated_targets(
    targets: &mut [SimulatedTarget],
    controller: &mut NativeController,
    current_time: f64,
    dt: f64,
) -> usize {
    let r_earth = 6378137.0;
    let mut updated = 0;

    for t in targets.iter_mut() {
        let lat_offset = (t.speed * dt * t.heading.cos()) / r_earth;
        let lon_offset = (t.speed * dt * t.heading.sin()) / (r_earth * t.lat.cos());
        t.lat += lat_offset;
        t.lon += lon_offset;

        let _ = controller.interpolator.update_target(olayer_core::interpolator::TargetState {
            id: t.id.clone(),
            last_position: LatLon::new(t.lat, t.lon, t.alt),
            speed_mps: t.speed,
            track_heading_rad: t.heading,
            vertical_rate_mps: 0.0,
            last_ping_time: current_time,
        });
        updated += 1;
    }

    if updated > 0 {
        controller.trigger_active();
    }
    updated
}
