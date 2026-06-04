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

        // Parse southwest origin coordinates
        let lon_str = String::from_utf8_lossy(&data[4..12]);
        let lat_str = String::from_utf8_lossy(&data[12..20]);

        let parsed_lon = parse_uhl_lon(&lon_str).map_err(|e| {
            TerrainError::InvalidHeader(format!("Failed to parse origin longitude '{}': {}", lon_str, e))
        })?;

        let parsed_lat = parse_uhl_lat(&lat_str).map_err(|e| {
            TerrainError::InvalidHeader(format!("Failed to parse origin latitude '{}': {}", lat_str, e))
        })?;

        let origin_lon = parsed_lon.floor() as i32;
        let origin_lat = parsed_lat.floor() as i32;

        // Parse grid dimensions
        let num_cols_str = String::from_utf8_lossy(&data[47..51]);
        let num_rows_str = String::from_utf8_lossy(&data[51..55]);

        let num_cols = num_cols_str.trim().parse::<usize>().map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid column count '{}': {}", num_cols_str, e))
        })?;

        let num_rows = num_rows_str.trim().parse::<usize>().map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid row count '{}': {}", num_rows_str, e))
        })?;

        // Parse grid spacing (arc-seconds)
        let lat_spacing_str = String::from_utf8_lossy(&data[20..24]);
        let lon_spacing_str = String::from_utf8_lossy(&data[24..28]);

        let lat_spacing_arcsec = lat_spacing_str.trim().parse::<u32>().map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid latitude spacing '{}': {}", lat_spacing_str, e))
        })?;

        let lon_spacing_arcsec = lon_spacing_str.trim().parse::<u32>().map_err(|e| {
            TerrainError::InvalidHeader(format!("Invalid longitude spacing '{}': {}", lon_spacing_str, e))
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
                    "Block sentinel 0xAA not found at column {}",
                    c
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

fn parse_uhl_lon(s: &str) -> Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty string".to_string());
    }
    let (num_part, dir_char) = s.split_at(s.len() - 1);
    let dir = dir_char.to_uppercase();
    let val = if num_part.contains('.') {
        num_part.parse::<f64>().map_err(|e| e.to_string())?
    } else if num_part.len() >= 7 {
        let deg = num_part[0..3].parse::<f64>().map_err(|e| e.to_string())?;
        let min = num_part[3..5].parse::<f64>().map_err(|e| e.to_string())?;
        let sec = num_part[5..7].parse::<f64>().map_err(|e| e.to_string())?;
        deg + min / 60.0 + sec / 3600.0
    } else if num_part.len() >= 3 {
        num_part.parse::<f64>().map_err(|e| e.to_string())?
    } else {
        return Err(format!("Invalid number format: {}", num_part));
    };
    if dir == "W" || dir == "S" {
        Ok(-val)
    } else {
        Ok(val)
    }
}

fn parse_uhl_lat(s: &str) -> Result<f64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty string".to_string());
    }
    let (num_part, dir_char) = s.split_at(s.len() - 1);
    let dir = dir_char.to_uppercase();
    let val = if num_part.contains('.') {
        num_part.parse::<f64>().map_err(|e| e.to_string())?
    } else if num_part.len() >= 6 {
        let deg = num_part[0..2].parse::<f64>().map_err(|e| e.to_string())?;
        let min = num_part[2..4].parse::<f64>().map_err(|e| e.to_string())?;
        let sec = num_part[4..6].parse::<f64>().map_err(|e| e.to_string())?;
        deg + min / 60.0 + sec / 3600.0
    } else if num_part.len() >= 2 {
        num_part.parse::<f64>().map_err(|e| e.to_string())?
    } else {
        return Err(format!("Invalid number format: {}", num_part));
    };
    if dir == "W" || dir == "S" {
        Ok(-val)
    } else {
        Ok(val)
    }
}
