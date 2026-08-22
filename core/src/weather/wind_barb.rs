use crate::geodesy::coords::LatLon;
use crate::geodesy::local_frame::{EnuPoint, LocalTangentFrame};
use crate::weather::errors::WeatherError;

/// Geometric elements composing an aviation standard wind barb symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct WindBarbGeometry {
    /// Origin coordinate of the observation / grid station.
    pub origin: LatLon,
    /// Wind staff start and end coordinates `(origin, tip)`.
    pub staff: (LatLon, LatLon),
    /// Line segments for 10-knot barbs and 5-knot half barbs: `Vec<(start, end)>`.
    pub barbs: Vec<(LatLon, LatLon)>,
    /// Closed triangle coordinates for 50-knot pennants: `Vec<[p0, p1, p2]>`.
    pub pennants: Vec<[LatLon; 3]>,
    /// If speed is calm (< 2.5 kt), represents the calm circle radius in meters.
    pub calm_circle_radius_m: Option<f64>,
}

/// Generates aviation-standard wind barb geometry given geographic coordinates and wind parameters.
///
/// # Arguments
/// * `origin` - Observation point `LatLon`.
/// * `speed_knots` - Wind speed in knots (kt).
/// * `direction_rad` - Wind direction in radians (direction the wind is blowing *from*, standard meteorological convention, $0 = \text{North}, \pi/2 = \text{East}$).
/// * `staff_length_meters` - Ground length of the main staff in meters.
/// * `is_southern_hemisphere` - If true, barbs deflect to the right of the staff; otherwise to the left.
pub fn generate_wind_barb(
    origin: &LatLon,
    speed_knots: f64,
    direction_rad: f64,
    staff_length_meters: f64,
    is_southern_hemisphere: bool,
) -> Result<WindBarbGeometry, WeatherError> {
    if speed_knots < 0.0 {
        return Err(WeatherError::InvalidWindParameters(format!(
            "Wind speed cannot be negative: {speed_knots} kt"
        )));
    }

    let frame = LocalTangentFrame::new(*origin);

    // Calm wind threshold (< 2.5 kt) -> concentric calm circle
    if speed_knots < 2.5 {
        return Ok(WindBarbGeometry {
            origin: *origin,
            staff: (*origin, *origin),
            barbs: Vec::new(),
            pennants: Vec::new(),
            calm_circle_radius_m: Some(staff_length_meters * 0.25),
        });
    }

    // Round speed to nearest 5 knots
    let rounded_speed = ((speed_knots + 2.5) / 5.0).floor() as u32 * 5;

    let num_pennants = (rounded_speed / 50) as usize;
    let remainder = rounded_speed % 50;
    let num_full_barbs = (remainder / 10) as usize;
    let num_half_barbs = ((remainder % 10) / 5) as usize;

    // Unit vector along the staff pointing towards the direction the wind is coming FROM
    // Meteorological direction: 0 = North (East=0, North=1), 90 = East (East=1, North=0)
    let dir_east = direction_rad.sin();
    let dir_north = direction_rad.cos();

    // Perpendicular vector for barbs (Left in NH, Right in SH)
    let barb_sign = if is_southern_hemisphere { 1.0 } else { -1.0 };
    // Normal vector pointing 90 deg left: (-dir_north, dir_east)
    let perp_east = -dir_north * barb_sign;
    let perp_north = dir_east * barb_sign;

    // Staff runs from origin (0, 0) to tip: (dir_east * staff_len, dir_north * staff_len)
    let tip_e = dir_east * staff_length_meters;
    let tip_n = dir_north * staff_length_meters;

    let tip_lla = frame.enu_to_lla(&EnuPoint::new(tip_e, tip_n, 0.0));

    let barb_length = staff_length_meters * 0.35;
    let half_barb_length = barb_length * 0.5;
    let slot_spacing = staff_length_meters * 0.12;

    let mut barbs = Vec::new();
    let mut pennants = Vec::new();

    let mut current_slot = 0.0;

    // Angle of barbs relative to staff (60 degrees backwards towards origin)
    let barb_angle_cos = (60.0_f64.to_radians()).cos();
    let barb_angle_sin = (60.0_f64.to_radians()).sin();

    let barb_vec_e = perp_east * barb_angle_sin - dir_east * barb_angle_cos;
    let barb_vec_n = perp_north * barb_angle_sin - dir_north * barb_angle_cos;

    // 1. Draw 50-knot pennants (flags)
    for _ in 0..num_pennants {
        let p_base1_e = tip_e - dir_east * (current_slot * slot_spacing);
        let p_base1_n = tip_n - dir_north * (current_slot * slot_spacing);

        let p_tip_e = p_base1_e + barb_vec_e * barb_length;
        let p_tip_n = p_base1_n + barb_vec_n * barb_length;

        current_slot += 1.0;
        let p_base2_e = tip_e - dir_east * (current_slot * slot_spacing);
        let p_base2_n = tip_n - dir_north * (current_slot * slot_spacing);

        let lla_b1 = frame.enu_to_lla(&EnuPoint::new(p_base1_e, p_base1_n, 0.0));
        let lla_tip = frame.enu_to_lla(&EnuPoint::new(p_tip_e, p_tip_n, 0.0));
        let lla_b2 = frame.enu_to_lla(&EnuPoint::new(p_base2_e, p_base2_n, 0.0));

        pennants.push([lla_b1, lla_tip, lla_b2]);
    }

    // 2. Draw 10-knot full barbs
    for _ in 0..num_full_barbs {
        let base_e = tip_e - dir_east * (current_slot * slot_spacing);
        let base_n = tip_n - dir_north * (current_slot * slot_spacing);

        let barb_end_e = base_e + barb_vec_e * barb_length;
        let barb_end_n = base_n + barb_vec_n * barb_length;

        let lla_base = frame.enu_to_lla(&EnuPoint::new(base_e, base_n, 0.0));
        let lla_end = frame.enu_to_lla(&EnuPoint::new(barb_end_e, barb_end_n, 0.0));

        barbs.push((lla_base, lla_end));
        current_slot += 1.0;
    }

    // 3. Draw 5-knot half barbs
    if num_half_barbs > 0 {
        // If no pennants or full barbs, offset half-barb 1 slot inward from tip so it's not confused with a full barb
        if num_pennants == 0 && num_full_barbs == 0 {
            current_slot += 1.0;
        }

        let base_e = tip_e - dir_east * (current_slot * slot_spacing);
        let base_n = tip_n - dir_north * (current_slot * slot_spacing);

        let barb_end_e = base_e + barb_vec_e * half_barb_length;
        let barb_end_n = base_n + barb_vec_n * half_barb_length;

        let lla_base = frame.enu_to_lla(&EnuPoint::new(base_e, base_n, 0.0));
        let lla_end = frame.enu_to_lla(&EnuPoint::new(barb_end_e, barb_end_n, 0.0));

        barbs.push((lla_base, lla_end));
    }

    Ok(WindBarbGeometry {
        origin: *origin,
        staff: (*origin, tip_lla),
        barbs,
        pennants,
        calm_circle_radius_m: None,
    })
}

/// Serializes a wind barb geometry into a flat array of lines: `[start_lat, start_lon, end_lat, end_lon, ...]` in degrees.
pub fn wind_barb_to_flat_lines_deg(geom: &WindBarbGeometry) -> Vec<f64> {
    let mut flat = vec![
        geom.staff.0.lat.to_degrees(),
        geom.staff.0.lon.to_degrees(),
        geom.staff.1.lat.to_degrees(),
        geom.staff.1.lon.to_degrees(),
    ];

    // Barbs
    for (start, end) in &geom.barbs {
        flat.push(start.lat.to_degrees());
        flat.push(start.lon.to_degrees());
        flat.push(end.lat.to_degrees());
        flat.push(end.lon.to_degrees());
    }

    // Pennants (3 boundary lines per triangle)
    for [p0, p1, p2] in &geom.pennants {
        // Line 1: p0 -> p1
        flat.push(p0.lat.to_degrees());
        flat.push(p0.lon.to_degrees());
        flat.push(p1.lat.to_degrees());
        flat.push(p1.lon.to_degrees());

        // Line 2: p1 -> p2
        flat.push(p1.lat.to_degrees());
        flat.push(p1.lon.to_degrees());
        flat.push(p2.lat.to_degrees());
        flat.push(p2.lon.to_degrees());

        // Line 3: p2 -> p0
        flat.push(p2.lat.to_degrees());
        flat.push(p2.lon.to_degrees());
        flat.push(p0.lat.to_degrees());
        flat.push(p0.lon.to_degrees());
    }

    flat
}
