use crate::terrain::errors::TerrainError;

/// A single DTED elevation tile.
///
/// Stores elevations in a flat column-major array indexed by
/// `col * num_rows + row`.
pub struct DtedTile {
    pub origin_lat: i32,           // Southwest corner latitude (whole degrees)
    pub origin_lon: i32,           // Southwest corner longitude (whole degrees)
    pub num_rows: usize,           // Number of latitude points
    pub num_cols: usize,           // Number of longitude points
    pub lat_spacing_arcsec: u32,   // Grid spacing in arc-seconds (latitude)
    pub lon_spacing_arcsec: u32,   // Grid spacing in arc-seconds (longitude)
    pub elevations: Vec<i16>,       // Altitudes in metres (flat col-major)
}

impl DtedTile {
    /// Parses a DTED Level 0/1/2 buffer from raw bytes.
    #[inline]
    pub fn from_bytes(data: &[u8]) -> Result<Self, TerrainError> {
        if data.len() < 3428 {
            return Err(TerrainError::InvalidHeader(
                "Buffer too short for DTED header (3428 bytes)".to_string(),
            ));
        }

        // Verify UHL signature
        if &data[0..4] != b"UHL1" {
            return Err(TerrainError::InvalidHeader(
                "Invalid UHL signature: expected 'UHL1'".to_string(),
            ));
        }

        // Parse southwest origin coordinates (avoid allocations by parsing byte slices)
        let parsed_lon = parse_uhl_lon(&data[4..12]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Failed to parse origin longitude: {e}"))
        })?;

        let parsed_lat = parse_uhl_lat(&data[12..20]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Failed to parse origin latitude: {e}"))
        })?;

        let origin_lon = parsed_lon.floor() as i32;
        let origin_lat = parsed_lat.floor() as i32;

        // Parse grid dimensions
        let num_cols = parse_ascii_usize(&data[47..51]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid column count: {e}"))
        })?;

        let num_rows = parse_ascii_usize(&data[51..55]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid row count: {e}"))
        })?;

        // Parse grid spacing (arc-seconds)
        let lat_spacing_arcsec = parse_ascii_u32(&data[20..24]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid latitude spacing: {e}"))
        })?;

        let lon_spacing_arcsec = parse_ascii_u32(&data[24..28]).map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid longitude spacing: {e}"))
        })?;

        // Each data column: 1 sentinel + 3 lon idx + 3 lat idx + num_rows * 2 bytes + 4 checksum
        let col_size = 11 + num_rows * 2;
        let expected_size = 3428 + num_cols * col_size;

        if data.len() < expected_size {
            return Err(TerrainError::MalformedData(format!(
                "Buffer length ({}) is smaller than expected ({}) for a {}x{} grid",
                data.len(),
                expected_size,
                num_cols,
                num_rows
            )));
        }

        // Read elevation columns sequentially
        let mut elevations = vec![0; num_cols * num_rows];
        let mut offset = 3428;

        for c in 0..num_cols {
            // Block sentinel
            if data[offset] != 0xAA {
                return Err(TerrainError::MalformedData(format!(
                    "Block sentinel 0xAA not found at column {c}"
                )));
            }

            // Skip sentinel (1) + sequence counters (6 bytes)
            let val_offset = offset + 7;
            for r in 0..num_rows {
                let idx = val_offset + r * 2;
                let val_bytes = [data[idx], data[idx + 1]];
                let elev = i16::from_be_bytes(val_bytes);
                elevations[c * num_rows + r] = elev;
            }

            offset += col_size;
        }

        Ok(Self {
            origin_lat,
            origin_lon,
            num_rows,
            num_cols,
            lat_spacing_arcsec,
            lon_spacing_arcsec,
            elevations,
        })
    }

    /// Returns the elevation at a specific grid cell.
    #[inline]
    pub fn get_cell_elevation(&self, row: usize, col: usize) -> i16 {
        self.elevations[col * self.num_rows + row]
    }
}

/// Parses an ASCII unsigned integer from a byte slice, ignoring leading/trailing spaces.
fn parse_ascii_u32(bytes: &[u8]) -> Result<u32, String> {
    let mut val: u32 = 0;
    let mut started = false;
    for &b in bytes {
        if b == b' ' {
            if started {
                break;
            }
            continue;
        }
        if b.is_ascii_digit() {
            started = true;
            val = val.checked_mul(10).ok_or("u32 overflow")?;
            val = val.checked_add((b - b'0') as u32).ok_or("u32 overflow")?;
        } else {
            return Err(format!("Invalid digit: {}", b as char));
        }
    }
    if started {
        Ok(val)
    } else {
        Err("Empty number".to_string())
    }
}

/// Parses an ASCII usize from a byte slice, ignoring leading/trailing spaces.
fn parse_ascii_usize(bytes: &[u8]) -> Result<usize, String> {
    parse_ascii_u32(bytes).map(|v| v as usize)
}

/// Parses a floating-point value from an ASCII byte slice, ignoring spaces.
fn parse_ascii_f64(bytes: &[u8]) -> Result<f64, String> {
    let s = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    s.trim().parse::<f64>().map_err(|e| e.to_string())
}

/// Parses a DTED UHL longitude field from raw bytes.
///
/// Expected formats: `DDDMMSSH` or decimal degrees followed by `E`/`W`.
fn parse_uhl_lon(bytes: &[u8]) -> Result<f64, String> {
    let bytes = trim_ascii(bytes);
    if bytes.is_empty() {
        return Err("Empty string".to_string());
    }
    let dir = bytes[bytes.len() - 1];
    let num_part = &bytes[..bytes.len() - 1];

    let val = if num_part.contains(&b'.') {
        parse_ascii_f64(num_part)?
    } else if num_part.len() >= 7 {
        let deg = parse_ascii_f64(&num_part[0..3])?;
        let min = parse_ascii_f64(&num_part[3..5])?;
        let sec = parse_ascii_f64(&num_part[5..7])?;
        deg + min / 60.0 + sec / 3600.0
    } else if num_part.len() >= 3 {
        parse_ascii_f64(num_part)?
    } else {
        return Err("Invalid number format".to_string());
    };

    if dir == b'W' || dir == b'w' || dir == b'S' || dir == b's' {
        Ok(-val)
    } else {
        Ok(val)
    }
}

/// Parses a DTED UHL latitude field from raw bytes.
///
/// Expected formats: `DDMMSSH` or decimal degrees followed by `N`/`S`.
fn parse_uhl_lat(bytes: &[u8]) -> Result<f64, String> {
    let bytes = trim_ascii(bytes);
    if bytes.is_empty() {
        return Err("Empty string".to_string());
    }
    let dir = bytes[bytes.len() - 1];
    let num_part = &bytes[..bytes.len() - 1];

    let val = if num_part.contains(&b'.') {
        parse_ascii_f64(num_part)?
    } else if num_part.len() >= 6 {
        let deg = parse_ascii_f64(&num_part[0..2])?;
        let min = parse_ascii_f64(&num_part[2..4])?;
        let sec = parse_ascii_f64(&num_part[4..6])?;
        deg + min / 60.0 + sec / 3600.0
    } else if num_part.len() >= 2 {
        parse_ascii_f64(num_part)?
    } else {
        return Err("Invalid number format".to_string());
    };

    if dir == b'W' || dir == b'w' || dir == b'S' || dir == b's' {
        Ok(-val)
    } else {
        Ok(val)
    }
}

/// Trims leading and trailing ASCII whitespace from a byte slice.
fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().position(|&b| b != b' ').unwrap_or(bytes.len());
    let end = bytes.iter().rposition(|&b| b != b' ').map(|i| i + 1).unwrap_or(bytes.len());
    &bytes[start..end]
}
