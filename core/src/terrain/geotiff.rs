use serde::{Deserialize, Serialize};
use crate::terrain::errors::TerrainError;

/// Decoded Cloud-Optimized GeoTIFF or standard GeoTIFF elevation raster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoTiffTile {
    /// Raster width in pixels.
    pub width: usize,
    /// Raster height in pixels.
    pub height: usize,
    /// Decoded elevation grid in meters (row-major: row 0 is North, row H-1 is South).
    pub elevations: Vec<Option<f32>>,
    /// Geographic bounding box `(min_lat_rad, min_lon_rad, max_lat_rad, max_lon_rad)`.
    pub bounds_rad: (f64, f64, f64, f64),
    /// NoData sentinel value, if specified.
    pub nodata: Option<f64>,
}

impl GeoTiffTile {
    /// Parses a GeoTIFF / Cloud-Optimized GeoTIFF buffer from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TerrainError> {
        if data.len() < 8 {
            return Err(TerrainError::GeoTiffError(
                "Buffer too short for TIFF header".to_string(),
            ));
        }

        let is_le = match &data[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => {
                return Err(TerrainError::GeoTiffError(
                    "Invalid TIFF endianness marker (expected 'II' or 'MM')".to_string(),
                ))
            }
        };

        let magic = read_u16(&data[2..4], is_le);
        if magic != 42 {
            return Err(TerrainError::GeoTiffError(format!(
                "Invalid TIFF magic number (expected 42, got {magic})"
            )));
        }

        let first_ifd_offset = read_u32(&data[4..8], is_le) as usize;
        if first_ifd_offset >= data.len() {
            return Err(TerrainError::GeoTiffError(
                "IFD offset out of bounds".to_string(),
            ));
        }

        // Parse IFD entries
        let num_entries = read_u16(&data[first_ifd_offset..first_ifd_offset + 2], is_le) as usize;
        let mut width: usize = 0;
        let mut height: usize = 0;
        let mut bits_per_sample: usize = 32;
        let mut sample_format: u16 = 3; // default: IEEE float
        let mut strip_offsets: Vec<usize> = Vec::new();
        let mut pixel_scale: Option<[f64; 3]> = None;
        let mut tiepoints: Vec<[f64; 6]> = Vec::new();
        let mut nodata_val: Option<f64> = None;

        let entry_base = first_ifd_offset + 2;
        for i in 0..num_entries {
            let offset = entry_base + i * 12;
            if offset + 12 > data.len() {
                break;
            }

            let tag = read_u16(&data[offset..offset + 2], is_le);
            let field_type = read_u16(&data[offset + 2..offset + 4], is_le);
            let count = read_u32(&data[offset + 4..offset + 8], is_le) as usize;
            let val_or_offset = offset + 8;

            match tag {
                256 => {
                    // ImageWidth
                    width = read_field_val_u32(data, field_type, count, val_or_offset, is_le)? as usize;
                }
                257 => {
                    // ImageLength (height)
                    height = read_field_val_u32(data, field_type, count, val_or_offset, is_le)? as usize;
                }
                258 => {
                    // BitsPerSample
                    bits_per_sample = read_field_val_u32(data, field_type, count, val_or_offset, is_le)? as usize;
                }
                273 | 324 => {
                    // StripOffsets or TileOffsets
                    strip_offsets = read_offset_list(data, field_type, count, val_or_offset, is_le)?;
                }
                339 => {
                    // SampleFormat
                    sample_format = read_field_val_u32(data, field_type, count, val_or_offset, is_le)? as u16;
                }
                33550 => {
                    // ModelPixelScaleTag [ScaleX, ScaleY, ScaleZ]
                    if count >= 3 {
                        let doubles = read_double_list(data, field_type, count, val_or_offset, is_le)?;
                        if doubles.len() >= 3 {
                            pixel_scale = Some([doubles[0], doubles[1], doubles[2]]);
                        }
                    }
                }
                33922 => {
                    // ModelTiepointTag [I, J, K, X, Y, Z]
                    let doubles = read_double_list(data, field_type, count, val_or_offset, is_le)?;
                    for chunk in doubles.chunks_exact(6) {
                        tiepoints.push([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5]]);
                    }
                }
                42113 => {
                    // GDAL_NODATA ASCII
                    if let Ok(s) = read_ascii_string(data, count, val_or_offset, is_le) {
                        if let Ok(v) = s.trim().parse::<f64>() {
                            nodata_val = Some(v);
                        }
                    }
                }
                _ => {}
            }
        }

        if width == 0 || height == 0 {
            return Err(TerrainError::GeoTiffError(
                "Invalid or missing GeoTIFF width/height dimensions".to_string(),
            ));
        }

        if strip_offsets.is_empty() {
            return Err(TerrainError::GeoTiffError(
                "No raster data strip/tile offsets found".to_string(),
            ));
        }

        // Calculate geographic bounding box in radians
        let bounds_rad = if let (Some(scale), Some(tp)) = (pixel_scale, tiepoints.first()) {
            let i = tp[0];
            let j = tp[1];
            let x0 = tp[3]; // lon or easting in degrees
            let y0 = tp[4]; // lat or northing in degrees

            let min_lon_deg = x0 - i * scale[0];
            let max_lon_deg = min_lon_deg + (width as f64) * scale[0];

            let max_lat_deg = y0 + j * scale[1];
            let min_lat_deg = max_lat_deg - (height as f64) * scale[1];

            (
                min_lat_deg.to_radians(),
                min_lon_deg.to_radians(),
                max_lat_deg.to_radians(),
                max_lon_deg.to_radians(),
            )
        } else {
            // Default global or unit box if unreferenced
            (-std::f64::consts::FRAC_PI_2, -std::f64::consts::PI, std::f64::consts::FRAC_PI_2, std::f64::consts::PI)
        };

        // Extract elevation values from raster strips
        let mut elevations = Vec::with_capacity(width * height);
        for &strip_offset in &strip_offsets {
            if strip_offset >= data.len() {
                continue;
            }
            let strip_data = &data[strip_offset..];
            match (bits_per_sample, sample_format) {
                (32, 3) => {
                    // Float32
                    for chunk in strip_data.chunks_exact(4) {
                        if elevations.len() >= width * height {
                            break;
                        }
                        let val = if is_le {
                            f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                        } else {
                            f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                        };
                        let is_nodata = nodata_val.is_some_and(|nd| (val as f64 - nd).abs() < 1e-3) || val.is_nan();
                        elevations.push(if is_nodata { None } else { Some(val) });
                    }
                }
                (64, 3) => {
                    // Float64
                    for chunk in strip_data.chunks_exact(8) {
                        if elevations.len() >= width * height {
                            break;
                        }
                        let val = if is_le {
                            f64::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]])
                        } else {
                            f64::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7]])
                        };
                        let is_nodata = nodata_val.is_some_and(|nd| (val - nd).abs() < 1e-3) || val.is_nan();
                        elevations.push(if is_nodata { None } else { Some(val as f32) });
                    }
                }
                (16, 2) | (16, 1) => {
                    // Int16 / UInt16
                    for chunk in strip_data.chunks_exact(2) {
                        if elevations.len() >= width * height {
                            break;
                        }
                        let val = if is_le {
                            i16::from_le_bytes([chunk[0], chunk[1]])
                        } else {
                            i16::from_be_bytes([chunk[0], chunk[1]])
                        };
                        let is_nodata = nodata_val.is_some_and(|nd| (val as f64 - nd).abs() < 1e-3) || val == -32767 || val == -9999;
                        elevations.push(if is_nodata { None } else { Some(val as f32) });
                    }
                }
                _ => {
                    return Err(TerrainError::GeoTiffError(format!(
                        "Unsupported GeoTIFF sample format (bits: {bits_per_sample}, format: {sample_format})"
                    )));
                }
            }
        }

        // Pad if partial
        while elevations.len() < width * height {
            elevations.push(None);
        }

        Ok(Self {
            width,
            height,
            elevations,
            bounds_rad,
            nodata: nodata_val,
        })
    }

    /// Checks if a geodetic point (lat/lon in radians) falls within the GeoTIFF bounding box.
    #[inline]
    pub fn contains_point_rad(&self, lat_rad: f64, lon_rad: f64) -> bool {
        let (min_lat, min_lon, max_lat, max_lon) = self.bounds_rad;
        lat_rad >= min_lat && lat_rad <= max_lat && lon_rad >= min_lon && lon_rad <= max_lon
    }

    /// Samples elevation in meters at a geodetic coordinate (lat/lon in radians) using bilinear interpolation.
    /// Returns `None` if the coordinate is out of bounds or falls on NoData pixels.
    pub fn get_elevation_rad(&self, lat_rad: f64, lon_rad: f64) -> Option<f64> {
        if !self.contains_point_rad(lat_rad, lon_rad) {
            return None;
        }

        let (min_lat, min_lon, max_lat, max_lon) = self.bounds_rad;
        let u = if (max_lon - min_lon).abs() > 1e-12 {
            ((lon_rad - min_lon) / (max_lon - min_lon)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Row 0 is North (max_lat), Row H-1 is South (min_lat)
        let v = if (max_lat - min_lat).abs() > 1e-12 {
            ((max_lat - lat_rad) / (max_lat - min_lat)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let col_f = u * ((self.width - 1) as f64);
        let row_f = v * ((self.height - 1) as f64);

        let col0 = (col_f.floor() as usize).min(self.width - 1);
        let col1 = (col0 + 1).min(self.width - 1);
        let row0 = (row_f.floor() as usize).min(self.height - 1);
        let row1 = (row0 + 1).min(self.height - 1);

        let tx = (col_f - col0 as f64).clamp(0.0, 1.0);
        let ty = (row_f - row0 as f64).clamp(0.0, 1.0);

        let z00 = self.elevations[row0 * self.width + col0]?;
        let z01 = self.elevations[row0 * self.width + col1]?;
        let z10 = self.elevations[row1 * self.width + col0]?;
        let z11 = self.elevations[row1 * self.width + col1]?;

        let z_top = (z00 as f64) * (1.0 - tx) + (z01 as f64) * tx;
        let z_bottom = (z10 as f64) * (1.0 - tx) + (z11 as f64) * tx;
        let z_final = z_top * (1.0 - ty) + z_bottom * ty;

        Some(z_final)
    }
}

#[inline]
fn read_u16(bytes: &[u8], is_le: bool) -> u16 {
    if is_le {
        u16::from_le_bytes([bytes[0], bytes[1]])
    } else {
        u16::from_be_bytes([bytes[0], bytes[1]])
    }
}

#[inline]
fn read_u32(bytes: &[u8], is_le: bool) -> u32 {
    if is_le {
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    } else {
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

fn read_field_val_u32(data: &[u8], field_type: u16, _count: usize, val_offset: usize, is_le: bool) -> Result<u32, TerrainError> {
    match field_type {
        3 => {
            // SHORT (u16) stored directly in first 2 bytes of value field
            Ok(read_u16(&data[val_offset..val_offset + 2], is_le) as u32)
        }
        4 => {
            // LONG (u32) stored directly
            Ok(read_u32(&data[val_offset..val_offset + 4], is_le))
        }
        _ => Ok(read_u32(&data[val_offset..val_offset + 4], is_le)),
    }
}

fn read_offset_list(data: &[u8], field_type: u16, count: usize, val_offset: usize, is_le: bool) -> Result<Vec<usize>, TerrainError> {
    let mut list = Vec::with_capacity(count);
    if count == 1 {
        let val = read_field_val_u32(data, field_type, count, val_offset, is_le)? as usize;
        list.push(val);
        return Ok(list);
    }

    let ptr = read_u32(&data[val_offset..val_offset + 4], is_le) as usize;
    if ptr >= data.len() {
        return Ok(list);
    }

    match field_type {
        3 => {
            for i in 0..count {
                let off = ptr + i * 2;
                if off + 2 <= data.len() {
                    list.push(read_u16(&data[off..off + 2], is_le) as usize);
                }
            }
        }
        4 => {
            for i in 0..count {
                let off = ptr + i * 4;
                if off + 4 <= data.len() {
                    list.push(read_u32(&data[off..off + 4], is_le) as usize);
                }
            }
        }
        _ => {}
    }
    Ok(list)
}

fn read_double_list(data: &[u8], _field_type: u16, count: usize, val_offset: usize, is_le: bool) -> Result<Vec<f64>, TerrainError> {
    let mut list = Vec::with_capacity(count);
    let ptr = read_u32(&data[val_offset..val_offset + 4], is_le) as usize;
    if ptr + count * 8 > data.len() {
        return Ok(list);
    }

    for i in 0..count {
        let off = ptr + i * 8;
        let val = if is_le {
            f64::from_le_bytes([
                data[off], data[off + 1], data[off + 2], data[off + 3],
                data[off + 4], data[off + 5], data[off + 6], data[off + 7],
            ])
        } else {
            f64::from_be_bytes([
                data[off], data[off + 1], data[off + 2], data[off + 3],
                data[off + 4], data[off + 5], data[off + 6], data[off + 7],
            ])
        };
        list.push(val);
    }
    Ok(list)
}

fn read_ascii_string(data: &[u8], count: usize, val_offset: usize, is_le: bool) -> Result<String, TerrainError> {
    if count <= 4 {
        let s = std::str::from_utf8(&data[val_offset..val_offset + count])
            .map_err(|e| TerrainError::GeoTiffError(e.to_string()))?;
        Ok(s.to_string())
    } else {
        let ptr = read_u32(&data[val_offset..val_offset + 4], is_le) as usize;
        if ptr + count <= data.len() {
            let s = std::str::from_utf8(&data[ptr..ptr + count])
                .map_err(|e| TerrainError::GeoTiffError(e.to_string()))?;
            Ok(s.to_string())
        } else {
            Err(TerrainError::GeoTiffError("String offset out of bounds".to_string()))
        }
    }
}
