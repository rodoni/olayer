//! General mathematical utilities for geodesy and projection modules.

/// Normalizes an angle in radians to the range `[0, 2π)`.
#[inline]
pub fn normalize_bearing(rad: f64) -> f64 {
    rad.rem_euclid(std::f64::consts::TAU)
}

/// Normalizes a longitude in radians to the range `[-π, π)`.
#[inline]
pub fn normalize_longitude(rad: f64) -> f64 {
    (rad + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
