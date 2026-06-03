use super::coords::{Ecef, Enu, LatLon};
use super::ellipsoid::Ellipsoid;

/// Converts Geodetic coordinates (LLA) to Cartesian Earth-Centered, Earth-Fixed (ECEF) coordinates.
pub fn lla_to_ecef(lla: &LatLon, ellipsoid: &Ellipsoid) -> Ecef {
    debug_assert!(lla.validate().is_ok(), "Invalid LLA passed to lla_to_ecef: {:?}", lla);
    let lat = lla.lat;
    let lon = lla.lon;
    let h = lla.height;

    let n = ellipsoid.radius_of_curvature_prime_vertical(lat);
    let cos_lat = lat.cos();
    let sin_lat = lat.sin();
    let cos_lon = lon.cos();
    let sin_lon = lon.sin();

    let x = (n + h) * cos_lat * cos_lon;
    let y = (n + h) * cos_lat * sin_lon;
    let z = (n * (1.0 - ellipsoid.e_sq) + h) * sin_lat;

    Ecef::new(x, y, z)
}

/// Converts Cartesian Earth-Centered, Earth-Fixed (ECEF) coordinates to Geodetic coordinates (LLA)
/// using Bowring's closed-form method for millimetric precision and high performance.
pub fn ecef_to_lla(ecef: &Ecef, ellipsoid: &Ellipsoid) -> LatLon {
    let x = ecef.x;
    let y = ecef.y;
    let z = ecef.z;
    let a = ellipsoid.a;
    let b = ellipsoid.b;
    let e_sq = ellipsoid.e_sq;
    let e_prime_sq = ellipsoid.e_prime_sq;

    let p = x.hypot(y);

    // Handle polar axis to avoid division by zero
    // Threshold raised to 1e-8 because floating-point cos(pi/2) is not exactly zero,
    // yielding p ≈ 4e-10 for surface points at the poles.
    if p < 1e-8 {
        let lat = if z >= 0.0 {
            std::f64::consts::FRAC_PI_2
        } else {
            -std::f64::consts::FRAC_PI_2
        };
        let lon = 0.0;
        let height = z.abs() - b;
        return LatLon::new(lat, lon, height);
    }

    let theta = (z * a).atan2(p * b);
    let sin_theta = theta.sin();
    let cos_theta = theta.cos();

    let lat = (z + e_prime_sq * b * sin_theta * sin_theta * sin_theta)
        .atan2(p - e_sq * a * cos_theta * cos_theta * cos_theta);
    let lon = y.atan2(x);

    let cos_lat = lat.cos();
    let sin_lat = lat.sin();
    let n = ellipsoid.radius_of_curvature_prime_vertical(lat);

    // Calculate height (use alternative equation for polar regions where cos(lat) approaches zero)
    let height = if cos_lat.abs() > 1e-6 {
        p / cos_lat - n
    } else {
        z.abs() / sin_lat.abs() - n * (1.0 - e_sq)
    };

    LatLon::new(lat, lon, height)
}

/// Converts Cartesian Earth-Centered, Earth-Fixed (ECEF) coordinates to Local East-North-Up (ENU) coordinates
/// relative to a reference geodetic origin.
pub fn ecef_to_enu(ecef: &Ecef, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu {
    debug_assert!(origin.validate().is_ok(), "Invalid origin passed to ecef_to_enu: {:?}", origin);
    let origin_ecef = lla_to_ecef(origin, ellipsoid);
    let dx = ecef.x - origin_ecef.x;
    let dy = ecef.y - origin_ecef.y;
    let dz = ecef.z - origin_ecef.z;

    let sin_lat = origin.lat.sin();
    let cos_lat = origin.lat.cos();
    let sin_lon = origin.lon.sin();
    let cos_lon = origin.lon.cos();

    let east = -sin_lon * dx + cos_lon * dy;
    let north = -sin_lat * cos_lon * dx - sin_lat * sin_lon * dy + cos_lat * dz;
    let up = cos_lat * cos_lon * dx + cos_lat * sin_lon * dy + sin_lat * dz;

    Enu::new(east, north, up)
}

/// Converts Local East-North-Up (ENU) coordinates relative to a reference geodetic origin
/// to Cartesian Earth-Centered, Earth-Fixed (ECEF) coordinates.
pub fn enu_to_ecef(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> Ecef {
    debug_assert!(origin.validate().is_ok(), "Invalid origin passed to enu_to_ecef: {:?}", origin);
    let origin_ecef = lla_to_ecef(origin, ellipsoid);

    let sin_lat = origin.lat.sin();
    let cos_lat = origin.lat.cos();
    let sin_lon = origin.lon.sin();
    let cos_lon = origin.lon.cos();

    let dx = -sin_lon * enu.east - sin_lat * cos_lon * enu.north + cos_lat * cos_lon * enu.up;
    let dy = cos_lon * enu.east - sin_lat * sin_lon * enu.north + cos_lat * sin_lon * enu.up;
    let dz = cos_lat * enu.north + sin_lat * enu.up;

    Ecef::new(origin_ecef.x + dx, origin_ecef.y + dy, origin_ecef.z + dz)
}

/// A convenience wrapper to convert Geodetic coordinates (LLA) to Local ENU coordinates directly.
pub fn lla_to_enu(lla: &LatLon, origin: &LatLon, ellipsoid: &Ellipsoid) -> Enu {
    debug_assert!(lla.validate().is_ok(), "Invalid LLA passed to lla_to_enu: {:?}", lla);
    debug_assert!(origin.validate().is_ok(), "Invalid origin passed to lla_to_enu: {:?}", origin);
    let ecef = lla_to_ecef(lla, ellipsoid);
    ecef_to_enu(&ecef, origin, ellipsoid)
}

/// A convenience wrapper to convert Local ENU coordinates directly to Geodetic coordinates (LLA).
pub fn enu_to_lla(enu: &Enu, origin: &LatLon, ellipsoid: &Ellipsoid) -> LatLon {
    debug_assert!(origin.validate().is_ok(), "Invalid origin passed to enu_to_lla: {:?}", origin);
    let ecef = enu_to_ecef(enu, origin, ellipsoid);
    ecef_to_lla(&ecef, ellipsoid)
}
