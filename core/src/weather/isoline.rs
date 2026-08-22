use crate::geodesy::coords::LatLon;
use crate::weather::errors::WeatherError;

/// A line segment belonging to an extracted isoline contour.
#[derive(Debug, Clone, PartialEq)]
pub struct IsolineSegment {
    pub isovalue: f64,
    pub start: LatLon,
    pub end: LatLon,
}

/// Generates smooth 2D isolines / contour curves from a 2D scalar grid using the Marching Squares algorithm with sub-grid linear edge interpolation.
///
/// # Arguments
/// * `grid` - Row-major scalar values of length `width * height`.
/// * `width` - Grid column count (must be >= 2).
/// * `height` - Grid row count (must be >= 2).
/// * `bounds_rad` - Geographic bounding box `(min_lat_rad, min_lon_rad, max_lat_rad, max_lon_rad)`.
/// * `isovalues` - Slice of contour threshold values to extract.
pub fn generate_isolines_rad(
    grid: &[f64],
    width: usize,
    height: usize,
    bounds_rad: (f64, f64, f64, f64),
    isovalues: &[f64],
) -> Result<Vec<IsolineSegment>, WeatherError> {
    if width < 2 || height < 2 || grid.len() != width * height {
        return Err(WeatherError::InvalidGridDimensions {
            width,
            height,
            actual_len: grid.len(),
        });
    }

    if isovalues.is_empty() {
        return Err(WeatherError::InvalidIsovalues("Isovalues slice cannot be empty".to_string()));
    }

    for &iso in isovalues {
        if !iso.is_finite() {
            return Err(WeatherError::InvalidIsovalues(format!("Non-finite isovalue: {iso}")));
        }
    }

    let (min_lat, min_lon, max_lat, max_lon) = bounds_rad;
    let d_lat = max_lat - min_lat;
    let d_lon = max_lon - min_lon;

    let mut segments = Vec::new();

    // Helper to map continuous grid cell coordinate (c_f, r_f) to geodetic LatLon
    let cell_to_latlon = |c_f: f64, r_f: f64| -> LatLon {
        let lon = min_lon + (c_f / (width - 1) as f64) * d_lon;
        let lat = max_lat - (r_f / (height - 1) as f64) * d_lat; // Row 0 is North (max_lat)
        LatLon::new(lat, lon, 0.0)
    };

    // Helper for linear edge interpolation: returns fraction t in [0, 1] between val_a and val_b
    #[inline]
    fn interp(iso: f64, val_a: f64, val_b: f64) -> f64 {
        let diff = val_b - val_a;
        if diff.abs() < 1e-12 {
            0.5
        } else {
            ((iso - val_a) / diff).clamp(0.0, 1.0)
        }
    }

    for &iso in isovalues {
        for r in 0..(height - 1) {
            let r0 = r;
            let r1 = r + 1;
            let row0_idx = r0 * width;
            let row1_idx = r1 * width;

            for c in 0..(width - 1) {
                let c0 = c;
                let c1 = c + 1;

                // 4 corners of the square cell:
                // v0 (Top-Left), v1 (Top-Right), v2 (Bottom-Right), v3 (Bottom-Left)
                let v0 = grid[row0_idx + c0];
                let v1 = grid[row0_idx + c1];
                let v2 = grid[row1_idx + c1];
                let v3 = grid[row1_idx + c0];

                let mut bitmask = 0u8;
                if v0 >= iso { bitmask |= 8; }
                if v1 >= iso { bitmask |= 4; }
                if v2 >= iso { bitmask |= 2; }
                if v3 >= iso { bitmask |= 1; }

                if bitmask == 0 || bitmask == 15 {
                    continue;
                }

                // Sub-pixel edge crossings:
                // Top edge: between (c0, r0) and (c1, r0)
                let top_pt = || {
                    let t = interp(iso, v0, v1);
                    cell_to_latlon(c0 as f64 + t, r0 as f64)
                };
                // Right edge: between (c1, r0) and (c1, r1)
                let right_pt = || {
                    let t = interp(iso, v1, v2);
                    cell_to_latlon(c1 as f64, r0 as f64 + t)
                };
                // Bottom edge: between (c0, r1) and (c1, r1)
                let bottom_pt = || {
                    let t = interp(iso, v3, v2);
                    cell_to_latlon(c0 as f64 + t, r1 as f64)
                };
                // Left edge: between (c0, r0) and (c0, r1)
                let left_pt = || {
                    let t = interp(iso, v0, v3);
                    cell_to_latlon(c0 as f64, r0 as f64 + t)
                };

                match bitmask {
                    1 | 14 => {
                        // Bottom to Left
                        segments.push(IsolineSegment { isovalue: iso, start: bottom_pt(), end: left_pt() });
                    }
                    2 | 13 => {
                        // Right to Bottom
                        segments.push(IsolineSegment { isovalue: iso, start: right_pt(), end: bottom_pt() });
                    }
                    3 | 12 => {
                        // Right to Left
                        segments.push(IsolineSegment { isovalue: iso, start: right_pt(), end: left_pt() });
                    }
                    4 | 11 => {
                        // Top to Right
                        segments.push(IsolineSegment { isovalue: iso, start: top_pt(), end: right_pt() });
                    }
                    5 => {
                        // Saddle point: Top to Left & Bottom to Right
                        segments.push(IsolineSegment { isovalue: iso, start: top_pt(), end: left_pt() });
                        segments.push(IsolineSegment { isovalue: iso, start: bottom_pt(), end: right_pt() });
                    }
                    6 | 9 => {
                        // Top to Bottom
                        segments.push(IsolineSegment { isovalue: iso, start: top_pt(), end: bottom_pt() });
                    }
                    7 | 8 => {
                        // Top to Left
                        segments.push(IsolineSegment { isovalue: iso, start: top_pt(), end: left_pt() });
                    }
                    10 => {
                        // Saddle point: Top to Right & Bottom to Left
                        segments.push(IsolineSegment { isovalue: iso, start: top_pt(), end: right_pt() });
                        segments.push(IsolineSegment { isovalue: iso, start: bottom_pt(), end: left_pt() });
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(segments)
}

/// Serializes isoline segments into a flat array of lines: `[isovalue, lat0, lon0, lat1, lon1, ...]` in degrees.
pub fn isolines_to_flat_array_deg(segments: &[IsolineSegment]) -> Vec<f64> {
    let mut flat = Vec::with_capacity(segments.len() * 5);
    for s in segments {
        flat.push(s.isovalue);
        flat.push(s.start.lat.to_degrees());
        flat.push(s.start.lon.to_degrees());
        flat.push(s.end.lat.to_degrees());
        flat.push(s.end.lon.to_degrees());
    }
    flat
}
