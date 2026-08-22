use crate::weather::errors::WeatherError;

/// Standard meteorological radar reflectivity (dBZ) color palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadarColorPalette {
    /// NWS/NOAA NEXRAD standard 15-level color scale.
    #[default]
    Nexrad,
    /// ICAO civil aviation weather radar palette (Green, Yellow, Red, Magenta).
    Icao,
    /// High-contrast palette tailored for high ambient light ATC cockpits and dark console backgrounds.
    HighContrast,
}

impl RadarColorPalette {
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "nexrad" | "nexrad_default" | "default" => Some(Self::Nexrad),
            "icao" | "icao_standard" => Some(Self::Icao),
            "highcontrast" | "high_contrast" => Some(Self::HighContrast),
            _ => None,
        }
    }
}

/// Maps a radar reflectivity value in decibels relative to $Z$ (dBZ) to an 8-bit RGBA color `[R, G, B, A]`.
#[inline]
pub fn dbz_to_rgba(dbz: f64, palette: RadarColorPalette) -> [u8; 4] {
    if !dbz.is_finite() || dbz < 5.0 {
        return [0, 0, 0, 0];
    }

    match palette {
        RadarColorPalette::Nexrad => {
            if dbz < 10.0 {
                [4, 233, 231, 160] // Light cyan
            } else if dbz < 15.0 {
                [1, 159, 244, 180] // Cyan-blue
            } else if dbz < 20.0 {
                [3, 0, 244, 200] // Blue
            } else if dbz < 25.0 {
                [2, 253, 2, 220] // Light green
            } else if dbz < 30.0 {
                [1, 197, 1, 230] // Medium green
            } else if dbz < 35.0 {
                [0, 142, 0, 240] // Dark green
            } else if dbz < 40.0 {
                [253, 248, 2, 245] // Yellow
            } else if dbz < 45.0 {
                [229, 188, 0, 250] // Mustard
            } else if dbz < 50.0 {
                [253, 149, 0, 255] // Orange
            } else if dbz < 55.0 {
                [253, 0, 0, 255] // Bright red
            } else if dbz < 60.0 {
                [212, 0, 0, 255] // Dark red
            } else if dbz < 65.0 {
                [248, 0, 253, 255] // Magenta / Pink
            } else if dbz < 70.0 {
                [153, 85, 204, 255] // Purple
            } else {
                [255, 255, 255, 255] // White (Extreme hail)
            }
        }
        RadarColorPalette::Icao => {
            if dbz < 20.0 {
                [0, 0, 0, 0] // Below threshold
            } else if dbz < 35.0 {
                [0, 230, 0, 210] // Level 1: Light to Moderate rain
            } else if dbz < 45.0 {
                [255, 230, 0, 230] // Level 2: Heavy precipitation
            } else if dbz < 55.0 {
                [255, 30, 0, 245] // Level 3: Very heavy / Thunderstorm
            } else {
                [255, 0, 255, 255] // Level 4: Extreme / Severe Convection & Hail
            }
        }
        RadarColorPalette::HighContrast => {
            if dbz < 15.0 {
                [0, 0, 0, 0]
            } else if dbz < 30.0 {
                [0, 255, 255, 200] // Electric cyan
            } else if dbz < 45.0 {
                [255, 255, 0, 230] // Neon yellow
            } else if dbz < 60.0 {
                [255, 50, 0, 255] // Neon red-orange
            } else {
                [255, 0, 255, 255] // Neon magenta
            }
        }
    }
}

/// Converts a 2D scalar grid of dBZ values into a flat RGBA pixel buffer `(width * height * 4)` bytes.
pub fn colorize_dbz_grid(
    grid: &[f64],
    width: usize,
    height: usize,
    palette: RadarColorPalette,
) -> Result<Vec<u8>, WeatherError> {
    if grid.len() != width * height {
        return Err(WeatherError::InvalidGridDimensions {
            width,
            height,
            actual_len: grid.len(),
        });
    }

    let mut rgba_buf = Vec::with_capacity(width * height * 4);
    for &dbz in grid {
        let pixel = dbz_to_rgba(dbz, palette);
        rgba_buf.extend_from_slice(&pixel);
    }

    Ok(rgba_buf)
}
