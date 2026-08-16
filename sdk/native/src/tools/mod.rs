use olayer_core::geodesy::{
    coords::LatLon,
    ellipsoid::Ellipsoid,
    magnetic::MagneticModel,
    math::normalize_bearing,
    solvers::{GeodeticSolver, VincentySolver},
};
use std::collections::HashMap;
use std::f64::consts::PI;

/// Conversion constant: 1 Nautical Mile in meters (exact standard).
pub const METERS_PER_NAUTICAL_MILE: f64 = 1852.0;

/// Standard Rate-One turn rate in degrees per second ($3^\circ/\text{s}$, $180^\circ$ in 1 minute).
pub const STANDARD_RATE_ONE_TURN_DPS: f64 = 3.0;

/// Range and Bearing Line (RBL / CRSR) measurement result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RblMeasurement {
    /// Origin coordinate latitude in degrees.
    pub from_lat_deg: f64,
    /// Origin coordinate longitude in degrees.
    pub from_lon_deg: f64,
    /// Target coordinate latitude in degrees.
    pub to_lat_deg: f64,
    /// Target coordinate longitude in degrees.
    pub to_lon_deg: f64,
    /// Geodesic distance in nautical miles (NM).
    pub distance_nm: f64,
    /// Geodesic distance in kilometers (km).
    pub distance_km: f64,
    /// Initial True bearing from origin to target in degrees ($0^\circ \le \theta < 360^\circ$).
    pub true_bearing_deg: f64,
    /// Initial Magnetic bearing from origin to target in degrees ($0^\circ \le \theta < 360^\circ$).
    pub magnetic_bearing_deg: f64,
    /// Reciprocal True bearing (back azimuth $+180^\circ$) in degrees.
    pub reciprocal_true_bearing_deg: f64,
    /// Reciprocal Magnetic bearing in degrees.
    pub reciprocal_magnetic_bearing_deg: f64,
    /// Estimated time en route in seconds (if ground speed provided).
    pub estimated_time_enroute_sec: Option<f64>,
}

/// Projected Position Leader (PPL) tick mark at a specific time interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PplTick {
    /// Time projection in minutes (e.g. 1.0, 2.0, 5.0).
    pub time_minutes: f64,
    /// Projected distance along trajectory in nautical miles.
    pub distance_nm: f64,
    /// Latitude in degrees of the tick mark position.
    pub lat_deg: f64,
    /// Longitude in degrees of the tick mark position.
    pub lon_deg: f64,
}

/// Projected Position Leader (PPL) result containing vector ticks and polyline.
#[derive(Debug, Clone, PartialEq)]
pub struct PplLeader {
    /// Origin track latitude in degrees.
    pub origin_lat_deg: f64,
    /// Origin track longitude in degrees.
    pub origin_lon_deg: f64,
    /// Ground speed in knots.
    pub ground_speed_knots: f64,
    /// Track heading in degrees True.
    pub track_deg: f64,
    /// Tick mark positions at requested time intervals.
    pub ticks: Vec<PplTick>,
    /// Polyline coordinates `(lat_deg, lon_deg)` representing the vector trajectory.
    pub trajectory_polyline: Vec<(f64, f64)>,
}

/// Turn direction for holding patterns and procedure turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDirection {
    /// Standard right turns (ICAO / FAA standard).
    StandardRight,
    /// Non-standard left turns.
    NonStandardLeft,
}

/// Configuration parameters for generating a standard racetrack holding pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldingPatternConfig {
    /// Holding fix latitude in degrees.
    pub fix_lat_deg: f64,
    /// Holding fix longitude in degrees.
    pub fix_lon_deg: f64,
    /// Inbound course bearing towards the fix in degrees ($0..360^\circ$).
    pub inbound_bearing_deg: f64,
    /// Turn direction (`StandardRight` or `NonStandardLeft`).
    pub turn_direction: TurnDirection,
    /// Straight leg timing in minutes (standard: 1.0 min below 14,000 ft, 1.5 min above).
    pub leg_time_minutes: f64,
    /// Indicated / True airspeed in knots (e.g. 210.0 knots).
    pub airspeed_knots: f64,
    /// Number of sample points per $180^\circ$ turn arc (e.g. 16).
    pub points_per_turn: usize,
}

impl Default for HoldingPatternConfig {
    fn default() -> Self {
        Self {
            fix_lat_deg: 0.0,
            fix_lon_deg: 0.0,
            inbound_bearing_deg: 360.0,
            turn_direction: TurnDirection::StandardRight,
            leg_time_minutes: 1.0,
            airspeed_knots: 210.0,
            points_per_turn: 16,
        }
    }
}

/// Configuration parameters for generating an Instrument Landing System (ILS) approach cone.
#[derive(Debug, Clone, PartialEq)]
pub struct IlsConeConfig {
    /// Runway threshold touchdown point latitude in degrees.
    pub threshold_lat_deg: f64,
    /// Runway threshold touchdown point longitude in degrees.
    pub threshold_lon_deg: f64,
    /// Runway landing heading in degrees ($0..360^\circ$, e.g. 090 for RWY 09).
    pub runway_heading_deg: f64,
    /// Length of approach cone in nautical miles (e.g. 10.0 to 15.0 NM).
    pub length_nm: f64,
    /// Angular width (field of view) of the localizer capture cone in degrees (standard: $5.0^\circ$).
    pub fov_deg: f64,
    /// Extended centerline length in nautical miles (e.g. 15.0 NM).
    pub extended_centerline_nm: f64,
    /// Number of points along the outer cone arc.
    pub arc_steps: usize,
}

impl Default for IlsConeConfig {
    fn default() -> Self {
        Self {
            threshold_lat_deg: 0.0,
            threshold_lon_deg: 0.0,
            runway_heading_deg: 0.0,
            length_nm: 10.0,
            fov_deg: 5.0,
            extended_centerline_nm: 15.0,
            arc_steps: 12,
        }
    }
}

/// Tick mark along an extended ILS centerline (e.g. 5 NM, 10 NM).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IlsTickMark {
    /// Distance from runway threshold in nautical miles.
    pub distance_nm: f64,
    /// Centerline point latitude in degrees.
    pub center_lat_deg: f64,
    /// Centerline point longitude in degrees.
    pub center_lon_deg: f64,
    /// Left crossbar endpoint `(lat_deg, lon_deg)`.
    pub left_lat_deg: f64,
    pub left_lon_deg: f64,
    /// Right crossbar endpoint `(lat_deg, lon_deg)`.
    pub right_lat_deg: f64,
    pub right_lon_deg: f64,
}

/// Complete ILS approach geometry including outer funnel polygon, centerline, and distance ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct IlsGeometry {
    /// Funnel polygon vertices `(lat_deg, lon_deg)` forming the closed localizer capture cone.
    pub cone_polygon: Vec<(f64, f64)>,
    /// Extended centerline polyline `(lat_deg, lon_deg)` from threshold upstream.
    pub extended_centerline: Vec<(f64, f64)>,
    /// Distance tick marks along the centerline.
    pub tick_marks: Vec<IlsTickMark>,
}

/// Configuration parameters for concentric range rings.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeRingsConfig {
    /// Anchor center latitude in degrees.
    pub center_lat_deg: f64,
    /// Anchor center longitude in degrees.
    pub center_lon_deg: f64,
    /// Radii of concentric rings in nautical miles (e.g. `[5.0, 10.0, 20.0, 40.0]`).
    pub radii_nm: Vec<f64>,
    /// Number of sample points per circular ring (e.g. 72 for $5^\circ$ resolution).
    pub points_per_ring: usize,
}

/// Configuration for a radar compass rose overlay with radial azimuth spokes.
#[derive(Debug, Clone, PartialEq)]
pub struct CompassRoseConfig {
    /// Anchor center latitude in degrees.
    pub center_lat_deg: f64,
    /// Anchor center longitude in degrees.
    pub center_lon_deg: f64,
    /// Outer radius in nautical miles.
    pub radius_nm: f64,
    /// Spoke interval in degrees (e.g. $30.0^\circ$ or $45.0^\circ$).
    pub spoke_interval_deg: f64,
    /// Decimal epoch year for magnetic declination alignment (if magnetic rose requested).
    pub epoch_year: Option<f64>,
}

/// Single historical radar hit dot with opacity decay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryDot {
    /// Latitude in degrees.
    pub lat_deg: f64,
    /// Longitude in degrees.
    pub lon_deg: f64,
    /// Altitude in feet.
    pub altitude_ft: f64,
    /// Timestamp of hit in seconds.
    pub timestamp_sec: f64,
    /// Computed opacity from 1.0 (newest) to 0.0 (oldest/fading).
    pub opacity: f32,
}

/// Tactical radar track history / snail trail manager.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnailTrailManager {
    tracks: HashMap<String, Vec<HistoryDot>>,
}

impl SnailTrailManager {
    /// Creates a new empty snail trail manager.
    pub fn new() -> Self {
        Self {
            tracks: HashMap::new(),
        }
    }

    /// Appends a new radar scan hit for the specified track.
    pub fn push_hit(
        &mut self,
        track_id: &str,
        lat_deg: f64,
        lon_deg: f64,
        altitude_ft: f64,
        timestamp_sec: f64,
    ) {
        let entry = self.tracks.entry(track_id.to_string()).or_default();
        entry.push(HistoryDot {
            lat_deg,
            lon_deg,
            altitude_ft,
            timestamp_sec,
            opacity: 1.0,
        });
    }

    /// Updates opacity decay and prunes stale history dots older than `max_age_sec` or exceeding `max_dots`.
    pub fn update_decay(&mut self, current_time_sec: f64, max_age_sec: f64, max_dots: usize) {
        let max_age = max_age_sec.max(0.001);
        for dots in self.tracks.values_mut() {
            dots.retain(|dot| (current_time_sec - dot.timestamp_sec) <= max_age);
            if dots.len() > max_dots {
                let excess = dots.len() - max_dots;
                dots.drain(0..excess);
            }
            let count = dots.len();
            for (idx, dot) in dots.iter_mut().enumerate() {
                // Opacity is proportional to age/scan index
                let age = (current_time_sec - dot.timestamp_sec).max(0.0);
                let age_factor = (1.0 - (age / max_age)).clamp(0.0, 1.0) as f32;
                let scan_factor = (idx + 1) as f32 / count as f32;
                dot.opacity = (age_factor * scan_factor).clamp(0.05, 1.0);
            }
        }
    }

    /// Returns the active history dots for a specific track.
    pub fn get_track_dots(&self, track_id: &str) -> Option<&[HistoryDot]> {
        self.tracks.get(track_id).map(|v| v.as_slice())
    }

    /// Clears history dots for a specific track.
    pub fn remove_track(&mut self, track_id: &str) -> bool {
        self.tracks.remove(track_id).is_some()
    }

    /// Clears all tracks from the manager.
    pub fn clear(&mut self) {
        self.tracks.clear();
    }
}

/// Unified Tactical Aeronautical Measurement Tools engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TacticalToolsManager {
    snail_trails: SnailTrailManager,
}

impl TacticalToolsManager {
    /// Creates a new tactical tools manager instance.
    pub fn new() -> Self {
        Self {
            snail_trails: SnailTrailManager::new(),
        }
    }

    /// Computes Range and Bearing Line (RBL / CRSR) measurement between two geodetic coordinates.
    pub fn compute_rbl(
        &self,
        from_lat_deg: f64,
        from_lon_deg: f64,
        to_lat_deg: f64,
        to_lon_deg: f64,
        speed_knots: Option<f64>,
        epoch_year: Option<f64>,
    ) -> RblMeasurement {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let from_pt = LatLon::from_degrees(from_lat_deg, from_lon_deg, 0.0);
        let to_pt = LatLon::from_degrees(to_lat_deg, to_lon_deg, 0.0);

        let inv = solver.inverse(&from_pt, &to_pt, &ell).unwrap_or_else(|_| {
            let p1 = LatLon::from_degrees(from_lat_deg, from_lon_deg, 0.0);
            let p2 = LatLon::from_degrees(to_lat_deg, to_lon_deg, 0.0);
            olayer_core::geodesy::solvers::HaversineSolver.inverse(&p1, &p2, &ell).unwrap()
        });

        let dist_m = inv.distance;
        let dist_nm = dist_m / METERS_PER_NAUTICAL_MILE;
        let dist_km = dist_m / 1000.0;

        let true_bearing_rad = normalize_bearing(inv.initial_bearing);
        let true_bearing_deg = true_bearing_rad.to_degrees();

        let epoch = epoch_year.unwrap_or(2025.0);
        let mag_model = MagneticModel::wmm2025();
        let mag_bearing_rad = mag_model.true_to_magnetic(true_bearing_rad, &from_pt, epoch);
        let mag_bearing_deg = mag_bearing_rad.to_degrees();

        let recip_true_deg = normalize_bearing(true_bearing_rad + PI).to_degrees();
        let recip_mag_deg = normalize_bearing(mag_bearing_rad + PI).to_degrees();

        let ete_sec = speed_knots.and_then(|kts| {
            if kts > 0.001 {
                Some((dist_nm / kts) * 3600.0)
            } else {
                None
            }
        });

        RblMeasurement {
            from_lat_deg,
            from_lon_deg,
            to_lat_deg,
            to_lon_deg,
            distance_nm: dist_nm,
            distance_km: dist_km,
            true_bearing_deg,
            magnetic_bearing_deg: mag_bearing_deg,
            reciprocal_true_bearing_deg: recip_true_deg,
            reciprocal_magnetic_bearing_deg: recip_mag_deg,
            estimated_time_enroute_sec: ete_sec,
        }
    }

    /// Generates Projected Position Leader (PPL) vectors and time tick marks.
    pub fn generate_ppl(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        ground_speed_knots: f64,
        track_deg: f64,
        intervals_minutes: &[f64],
    ) -> PplLeader {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let origin = LatLon::from_degrees(lat_deg, lon_deg, 0.0);
        let track_rad = track_deg.to_radians();

        let mut ticks = Vec::with_capacity(intervals_minutes.len());
        let mut polyline = Vec::with_capacity(intervals_minutes.len() + 1);
        polyline.push((lat_deg, lon_deg));

        for &t_min in intervals_minutes {
            let dist_nm = ground_speed_knots * (t_min / 60.0);
            let dist_m = dist_nm * METERS_PER_NAUTICAL_MILE;
            let projected = solver.direct(&origin, track_rad, dist_m, &ell).unwrap_or(origin);
            let (p_lat, p_lon, _) = projected.to_degrees();
            ticks.push(PplTick {
                time_minutes: t_min,
                distance_nm: dist_nm,
                lat_deg: p_lat,
                lon_deg: p_lon,
            });
            polyline.push((p_lat, p_lon));
        }

        PplLeader {
            origin_lat_deg: lat_deg,
            origin_lon_deg: lon_deg,
            ground_speed_knots,
            track_deg,
            ticks,
            trajectory_polyline: polyline,
        }
    }

    /// Generates standard racetrack holding pattern polyline coordinates.
    pub fn generate_holding_pattern(&self, config: &HoldingPatternConfig) -> Vec<(f64, f64)> {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let fix = LatLon::from_degrees(config.fix_lat_deg, config.fix_lon_deg, 0.0);

        let speed_mps = config.airspeed_knots * (METERS_PER_NAUTICAL_MILE / 3600.0);
        let omega_rad_s = STANDARD_RATE_ONE_TURN_DPS.to_radians(); // 3 deg/s
        let turn_radius_m = speed_mps / omega_rad_s;
        let leg_dist_m = speed_mps * (config.leg_time_minutes * 60.0);

        let theta_inbound = config.inbound_bearing_deg.to_radians();
        let is_right = config.turn_direction == TurnDirection::StandardRight;

        // Turn 1 center (at Fix, 90 deg perpendicular to inbound course)
        let perp_sign = if is_right { 1.0 } else { -1.0 };
        let bearing_to_turn1_center = normalize_bearing(theta_inbound + perp_sign * (PI / 2.0));
        let turn1_center = solver.direct(&fix, bearing_to_turn1_center, turn_radius_m, &ell).unwrap_or(fix);

        let steps = config.points_per_turn.max(4);
        let mut polyline = Vec::with_capacity(steps * 2 + 6);

        // 1. Fix point
        polyline.push((config.fix_lat_deg, config.fix_lon_deg));

        // 2. Turn 1 (Outbound turn, 180 degree arc from Fix away to outbound leg)
        let start_angle_1 = normalize_bearing(bearing_to_turn1_center + PI);
        for i in 1..=steps {
            let frac = i as f64 / steps as f64;
            let sweep = if is_right { frac * PI } else { -frac * PI };
            let angle = normalize_bearing(start_angle_1 + sweep);
            let pt = solver.direct(&turn1_center, angle, turn_radius_m, &ell).unwrap_or(turn1_center);
            let (lat_d, lon_d, _) = pt.to_degrees();
            polyline.push((lat_d, lon_d));
        }

        // 3. Outbound leg
        let outbound_start = {
            let (lat_d, lon_d) = *polyline.last().unwrap();
            LatLon::from_degrees(lat_d, lon_d, 0.0)
        };
        let theta_outbound = normalize_bearing(theta_inbound + PI);
        let outbound_end = solver.direct(&outbound_start, theta_outbound, leg_dist_m, &ell).unwrap_or(outbound_start);
        let (ob_lat, ob_lon, _) = outbound_end.to_degrees();
        polyline.push((ob_lat, ob_lon));

        // 4. Turn 2 (Inbound turn, 180 degree arc back to inbound course)
        let bearing_to_turn2_center = normalize_bearing(theta_outbound + perp_sign * (PI / 2.0));
        let turn2_center = solver.direct(&outbound_end, bearing_to_turn2_center, turn_radius_m, &ell).unwrap_or(outbound_end);
        let start_angle_2 = normalize_bearing(bearing_to_turn2_center + PI);

        for i in 1..=steps {
            let frac = i as f64 / steps as f64;
            let sweep = if is_right { frac * PI } else { -frac * PI };
            let angle = normalize_bearing(start_angle_2 + sweep);
            let pt = solver.direct(&turn2_center, angle, turn_radius_m, &ell).unwrap_or(turn2_center);
            let (lat_d, lon_d, _) = pt.to_degrees();
            polyline.push((lat_d, lon_d));
        }

        // 5. Inbound leg closing back to Fix
        polyline.push((config.fix_lat_deg, config.fix_lon_deg));

        polyline
    }

    /// Generates complete ILS approach cone geometry including outer funnel, extended centerline, and distance ticks.
    pub fn generate_ils_cone(&self, config: &IlsConeConfig) -> IlsGeometry {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let threshold = LatLon::from_degrees(config.threshold_lat_deg, config.threshold_lon_deg, 0.0);

        // Approach corridor extends backwards from the runway threshold (heading - 180 deg)
        let rwy_heading_rad = config.runway_heading_deg.to_radians();
        let approach_back_rad = normalize_bearing(rwy_heading_rad + PI);
        let half_fov_rad = (config.fov_deg / 2.0).to_radians();
        let cone_dist_m = config.length_nm * METERS_PER_NAUTICAL_MILE;

        // 1. Funnel polygon vertices
        let mut cone_polygon = Vec::new();
        cone_polygon.push((config.threshold_lat_deg, config.threshold_lon_deg));

        let left_bearing = normalize_bearing(approach_back_rad - half_fov_rad);
        let right_bearing = normalize_bearing(approach_back_rad + half_fov_rad);

        let left_pt = solver.direct(&threshold, left_bearing, cone_dist_m, &ell).unwrap_or(threshold);
        let (left_lat, left_lon, _) = left_pt.to_degrees();
        cone_polygon.push((left_lat, left_lon));

        // Arc steps along the far end of the cone
        let arc_steps = config.arc_steps.max(2);
        for i in 1..arc_steps {
            let frac = i as f64 / arc_steps as f64;
            let angle = normalize_bearing(left_bearing + frac * (config.fov_deg.to_radians()));
            let arc_pt = solver.direct(&threshold, angle, cone_dist_m, &ell).unwrap_or(threshold);
            let (a_lat, a_lon, _) = arc_pt.to_degrees();
            cone_polygon.push((a_lat, a_lon));
        }

        let right_pt = solver.direct(&threshold, right_bearing, cone_dist_m, &ell).unwrap_or(threshold);
        let (right_lat, right_lon, _) = right_pt.to_degrees();
        cone_polygon.push((right_lat, right_lon));

        // Close funnel back to threshold
        cone_polygon.push((config.threshold_lat_deg, config.threshold_lon_deg));

        // 2. Extended centerline
        let centerline_dist_m = config.extended_centerline_nm * METERS_PER_NAUTICAL_MILE;
        let far_centerline_pt = solver.direct(&threshold, approach_back_rad, centerline_dist_m, &ell).unwrap_or(threshold);
        let (far_lat, far_lon, _) = far_centerline_pt.to_degrees();
        let extended_centerline = vec![
            (config.threshold_lat_deg, config.threshold_lon_deg),
            (far_lat, far_lon),
        ];

        // 3. Distance tick marks every 1 NM / 5 NM
        let mut tick_marks = Vec::new();
        let crossbar_half_len_m = 300.0; // 300m crossbar tick
        let perp_left = normalize_bearing(approach_back_rad - PI / 2.0);
        let perp_right = normalize_bearing(approach_back_rad + PI / 2.0);

        let max_nm = config.extended_centerline_nm.floor() as usize;
        for nm in 1..=max_nm {
            let d_m = nm as f64 * METERS_PER_NAUTICAL_MILE;
            let center_tick = solver.direct(&threshold, approach_back_rad, d_m, &ell).unwrap_or(threshold);
            let tick_len = if nm % 5 == 0 { crossbar_half_len_m * 2.0 } else { crossbar_half_len_m };

            let left_tick = solver.direct(&center_tick, perp_left, tick_len, &ell).unwrap_or(center_tick);
            let right_tick = solver.direct(&center_tick, perp_right, tick_len, &ell).unwrap_or(center_tick);

            let (c_lat, c_lon, _) = center_tick.to_degrees();
            let (l_lat, l_lon, _) = left_tick.to_degrees();
            let (r_lat, r_lon, _) = right_tick.to_degrees();

            tick_marks.push(IlsTickMark {
                distance_nm: nm as f64,
                center_lat_deg: c_lat,
                center_lon_deg: c_lon,
                left_lat_deg: l_lat,
                left_lon_deg: l_lon,
                right_lat_deg: r_lat,
                right_lon_deg: r_lon,
            });
        }

        IlsGeometry {
            cone_polygon,
            extended_centerline,
            tick_marks,
        }
    }

    /// Generates concentric range rings centered on a coordinate.
    pub fn generate_range_rings(&self, config: &RangeRingsConfig) -> Vec<Vec<(f64, f64)>> {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let center = LatLon::from_degrees(config.center_lat_deg, config.center_lon_deg, 0.0);
        let steps = config.points_per_ring.max(12);

        let mut rings = Vec::with_capacity(config.radii_nm.len());

        for &radius_nm in &config.radii_nm {
            let radius_m = radius_nm * METERS_PER_NAUTICAL_MILE;
            let mut ring = Vec::with_capacity(steps + 1);

            for i in 0..=steps {
                let bearing_rad = (i as f64 / steps as f64) * 2.0 * PI;
                let pt = solver.direct(&center, bearing_rad, radius_m, &ell).unwrap_or(center);
                let (lat_d, lon_d, _) = pt.to_degrees();
                ring.push((lat_d, lon_d));
            }
            rings.push(ring);
        }

        rings
    }

    /// Generates radial azimuth spoke lines for a radar compass rose overlay.
    pub fn generate_compass_rose_spokes(&self, config: &CompassRoseConfig) -> Vec<Vec<(f64, f64)>> {
        let ell = Ellipsoid::wgs84();
        let solver = VincentySolver;
        let center = LatLon::from_degrees(config.center_lat_deg, config.center_lon_deg, 0.0);

        let declination_rad = if let Some(epoch) = config.epoch_year {
            let mag_model = MagneticModel::wmm2025();
            mag_model.get_declination(&center, epoch)
        } else {
            0.0
        };

        let radius_m = config.radius_nm * METERS_PER_NAUTICAL_MILE;
        let interval_deg = config.spoke_interval_deg.clamp(5.0, 90.0);
        let num_spokes = (360.0 / interval_deg).floor() as usize;

        let mut spokes = Vec::with_capacity(num_spokes);

        for i in 0..num_spokes {
            let spoke_bearing_deg = i as f64 * interval_deg;
            let true_bearing_rad = normalize_bearing(spoke_bearing_deg.to_radians() + declination_rad);
            let outer_pt = solver.direct(&center, true_bearing_rad, radius_m, &ell).unwrap_or(center);
            let (out_lat, out_lon, _) = outer_pt.to_degrees();

            spokes.push(vec![
                (config.center_lat_deg, config.center_lon_deg),
                (out_lat, out_lon),
            ]);
        }

        spokes
    }

    /// Returns a reference to the inner snail trail manager.
    pub fn snail_trails(&self) -> &SnailTrailManager {
        &self.snail_trails
    }

    /// Returns a mutable reference to the inner snail trail manager.
    pub fn snail_trails_mut(&mut self) -> &mut SnailTrailManager {
        &mut self.snail_trails
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbl_measurement() {
        let manager = TacticalToolsManager::new();

        // JFK (40.64 N, -73.78 W) to Boston Logan (42.36 N, -71.01 W)
        let rbl = manager.compute_rbl(40.64, -73.78, 42.36, -71.01, Some(450.0), Some(2025.0));

        // Distance should be ~160 to 175 NM
        assert!(rbl.distance_nm > 150.0 && rbl.distance_nm < 185.0);
        assert!(rbl.distance_km > 280.0 && rbl.distance_km < 340.0);

        // Bearing from NY to Boston should be North-East (~50 to 60 deg True)
        assert!(rbl.true_bearing_deg > 45.0 && rbl.true_bearing_deg < 65.0);

        // Magnetic declination in NY is ~ -13 deg, so Mag bearing should be ~ 65-75 deg
        assert!(rbl.magnetic_bearing_deg > rbl.true_bearing_deg);

        // Reciprocal bearing check
        let diff = (rbl.reciprocal_true_bearing_deg - (rbl.true_bearing_deg + 180.0)).abs();
        assert!(diff < 1e-6 || (diff - 360.0).abs() < 1e-6);

        // ETE at 450 kts: ~165 NM / 450 = ~0.366 hr = ~1320 sec
        assert!(rbl.estimated_time_enroute_sec.is_some());
        let ete = rbl.estimated_time_enroute_sec.unwrap();
        assert!(ete > 1100.0 && ete < 1600.0);
    }

    #[test]
    fn test_ppl_leader_generation() {
        let manager = TacticalToolsManager::new();
        let intervals = [1.0, 2.0, 5.0];
        let ppl = manager.generate_ppl(40.0, -74.0, 480.0, 90.0, &intervals);

        assert_eq!(ppl.ticks.len(), 3);
        assert_eq!(ppl.trajectory_polyline.len(), 4);

        // 480 knots = 8 NM per minute
        assert!((ppl.ticks[0].distance_nm - 8.0).abs() < 1e-3);
        assert!((ppl.ticks[1].distance_nm - 16.0).abs() < 1e-3);
        assert!((ppl.ticks[2].distance_nm - 40.0).abs() < 1e-3);

        // Moving East (heading 90 deg) increases longitude
        assert!(ppl.ticks[0].lon_deg > ppl.origin_lon_deg);
        assert!(ppl.ticks[2].lon_deg > ppl.ticks[0].lon_deg);
    }

    #[test]
    fn test_holding_pattern_geometry() {
        let manager = TacticalToolsManager::new();
        let config = HoldingPatternConfig {
            fix_lat_deg: 51.5,
            fix_lon_deg: -0.1,
            inbound_bearing_deg: 270.0, // Inbound heading West
            turn_direction: TurnDirection::StandardRight,
            leg_time_minutes: 1.0,
            airspeed_knots: 210.0,
            points_per_turn: 16,
        };

        let polyline = manager.generate_holding_pattern(&config);

        // Minimum vertices = Fix + 16 (turn 1) + 1 (outbound) + 16 (turn 2) + Fix = 35
        assert!(polyline.len() >= 34);

        // Closed racetrack loop: start and end at Fix
        let (start_lat, start_lon) = polyline.first().copied().unwrap();
        let (end_lat, end_lon) = polyline.last().copied().unwrap();
        assert!((start_lat - config.fix_lat_deg).abs() < 1e-6);
        assert!((start_lon - config.fix_lon_deg).abs() < 1e-6);
        assert!((end_lat - config.fix_lat_deg).abs() < 1e-6);
        assert!((end_lon - config.fix_lon_deg).abs() < 1e-6);
    }

    #[test]
    fn test_ils_cone_geometry() {
        let manager = TacticalToolsManager::new();
        let config = IlsConeConfig {
            threshold_lat_deg: 51.4775,
            threshold_lon_deg: -0.4614,
            runway_heading_deg: 270.0, // RWY 27
            length_nm: 10.0,
            fov_deg: 5.0,
            extended_centerline_nm: 15.0,
            arc_steps: 12,
        };

        let ils = manager.generate_ils_cone(&config);

        // Funnel polygon starts and ends at threshold
        assert_eq!(ils.cone_polygon.first(), ils.cone_polygon.last());
        assert!(ils.cone_polygon.len() >= 14);

        // Extended centerline has 2 points
        assert_eq!(ils.extended_centerline.len(), 2);
        assert_eq!(ils.extended_centerline[0], (config.threshold_lat_deg, config.threshold_lon_deg));

        // 15 tick marks for 15 NM
        assert_eq!(ils.tick_marks.len(), 15);
        assert_eq!(ils.tick_marks[0].distance_nm, 1.0);
        assert_eq!(ils.tick_marks[14].distance_nm, 15.0);
    }

    #[test]
    fn test_range_rings_and_compass_rose() {
        let manager = TacticalToolsManager::new();
        let rings_cfg = RangeRingsConfig {
            center_lat_deg: 0.0,
            center_lon_deg: 0.0,
            radii_nm: vec![5.0, 10.0, 20.0],
            points_per_ring: 36,
        };

        let rings = manager.generate_range_rings(&rings_cfg);
        assert_eq!(rings.len(), 3);
        for ring in &rings {
            assert_eq!(ring.len(), 37); // 36 steps + 1 to close
        }

        let rose_cfg = CompassRoseConfig {
            center_lat_deg: 0.0,
            center_lon_deg: 0.0,
            radius_nm: 30.0,
            spoke_interval_deg: 45.0,
            epoch_year: Some(2025.0),
        };
        let spokes = manager.generate_compass_rose_spokes(&rose_cfg);
        assert_eq!(spokes.len(), 8); // 360 / 45 = 8 spokes
    }

    #[test]
    fn test_snail_trails_decay() {
        let mut manager = TacticalToolsManager::new();
        let track_id = "AAL123";

        // Add 5 hits over time
        for i in 0..5 {
            manager.snail_trails_mut().push_hit(
                track_id,
                40.0 + (i as f64 * 0.01),
                -74.0 + (i as f64 * 0.01),
                10000.0,
                100.0 + (i as f64 * 4.0), // 100s, 104s, 108s, 112s, 116s
            );
        }

        let dots = manager.snail_trails().get_track_dots(track_id).unwrap();
        assert_eq!(dots.len(), 5);

        // Update decay at time 120s with max age 20s
        manager.snail_trails_mut().update_decay(120.0, 20.0, 10);
        let active_dots = manager.snail_trails().get_track_dots(track_id).unwrap();

        // Hit at 100s is age 20s, hit at 116s is age 4s
        assert!(!active_dots.is_empty());
        // Newest dot should have higher opacity than oldest dot
        assert!(active_dots.last().unwrap().opacity > active_dots.first().unwrap().opacity);
    }
}
