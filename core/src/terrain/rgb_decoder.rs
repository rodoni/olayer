use serde::{Deserialize, Serialize};
use crate::terrain::errors::TerrainError;

/// Color encoding schema for RGB raster elevation tiles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RgbElevationEncoding {
    /// Mapbox Terrain-RGB format:
    /// `height = -10000 + (R * 256 * 256 + G * 256 + B) * 0.1` (in meters).
    #[default]
    MapboxRgb,
    /// Mapzen / Nextzen Terrarium format:
    /// `height = (R * 256 + G + B / 256) - 32768` (in meters).
    Terrarium,
    /// Custom affine RGB encoding with scale and base offset:
    /// `height = offset + R * r_scale + G * g_scale + B * b_scale`.
    Custom {
        offset: f64,
        r_scale: f64,
        g_scale: f64,
        b_scale: f64,
    },
}

impl RgbElevationEncoding {
    /// Parses an encoding name from a string ("mapbox", "terrarium", etc.).
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "mapbox" | "mapboxrgb" | "mapbox_rgb" | "mapbox-rgb" => Some(Self::MapboxRgb),
            "terrarium" | "mapzen" | "nextzen" => Some(Self::Terrarium),
            _ => None,
        }
    }
}

/// Decodes an elevation value in meters from RGB color channels according to Mapbox Terrain-RGB formula:
/// `height = -10000 + (R * 256^2 + G * 256 + B) * 0.1`.
#[inline]
pub fn decode_mapbox_rgb(r: u8, g: u8, b: u8) -> f64 {
    let r_val = (r as f64) * 65536.0;
    let g_val = (g as f64) * 256.0;
    let b_val = b as f64;
    -10000.0 + (r_val + g_val + b_val) * 0.1
}

/// Decodes an elevation value in meters from RGB color channels according to Mapzen/Nextzen Terrarium formula:
/// `height = (R * 256 + G + B / 256) - 32768`.
#[inline]
pub fn decode_terrarium_rgb(r: u8, g: u8, b: u8) -> f64 {
    let r_val = (r as f64) * 256.0;
    let g_val = g as f64;
    let b_val = (b as f64) / 256.0;
    (r_val + g_val + b_val) - 32768.0
}

/// Decodes an elevation value in meters for a single RGB pixel using the specified encoding.
#[inline]
pub fn decode_rgb_elevation(r: u8, g: u8, b: u8, encoding: RgbElevationEncoding) -> f64 {
    match encoding {
        RgbElevationEncoding::MapboxRgb => decode_mapbox_rgb(r, g, b),
        RgbElevationEncoding::Terrarium => decode_terrarium_rgb(r, g, b),
        RgbElevationEncoding::Custom {
            offset,
            r_scale,
            g_scale,
            b_scale,
        } => offset + (r as f64) * r_scale + (g as f64) * g_scale + (b as f64) * b_scale,
    }
}

/// Decodes a raw RGBA (4 bytes per pixel) image buffer of dimensions `width * height` into a flat `Vec<f32>`
/// elevation raster (row-major order).
pub fn decode_rgba_buffer(
    buffer: &[u8],
    width: usize,
    height: usize,
    encoding: RgbElevationEncoding,
) -> Result<Vec<f32>, TerrainError> {
    let expected_len = width * height * 4;
    if buffer.len() < expected_len {
        return Err(TerrainError::RgbDecodeError(format!(
            "RGBA buffer length ({}) is less than expected ({}) for {}x{} image",
            buffer.len(),
            expected_len,
            width,
            height
        )));
    }

    let mut elevations = Vec::with_capacity(width * height);
    for chunk in buffer[..expected_len].chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let elev = decode_rgb_elevation(r, g, b, encoding);
        elevations.push(elev as f32);
    }

    Ok(elevations)
}

/// Decodes a raw RGB (3 bytes per pixel) image buffer of dimensions `width * height` into a flat `Vec<f32>`
/// elevation raster (row-major order).
pub fn decode_rgb_buffer(
    buffer: &[u8],
    width: usize,
    height: usize,
    encoding: RgbElevationEncoding,
) -> Result<Vec<f32>, TerrainError> {
    let expected_len = width * height * 3;
    if buffer.len() < expected_len {
        return Err(TerrainError::RgbDecodeError(format!(
            "RGB buffer length ({}) is less than expected ({}) for {}x{} image",
            buffer.len(),
            expected_len,
            width,
            height
        )));
    }

    let mut elevations = Vec::with_capacity(width * height);
    for chunk in buffer[..expected_len].chunks_exact(3) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let elev = decode_rgb_elevation(r, g, b, encoding);
        elevations.push(elev as f32);
    }

    Ok(elevations)
}
