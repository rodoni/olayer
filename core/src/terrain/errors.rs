use crate::error_code::ErrorCode;
use thiserror::Error;

/// Errors that can occur during terrain processing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TerrainError {
    /// The DTED header is malformed or the buffer is too short.
    #[error("Invalid DTED header: {0}")]
    InvalidHeader(String),
    /// The DTED data records are corrupted or incomplete.
    #[error("Corrupted DTED data: {0}")]
    MalformedData(String),
    /// The requested tile has not been loaded into the engine.
    #[error("DTED tile not loaded for coordinate ({0}, {1})")]
    TileNotLoaded(i32, i32),
    /// An error occurred decoding an RGB/Terrarium elevation tile.
    #[error("RGB terrain decode error: {0}")]
    RgbDecodeError(String),
    /// An error occurred parsing a GeoTIFF / Cloud-Optimized GeoTIFF raster.
    #[error("GeoTIFF terrain error: {0}")]
    GeoTiffError(String),
    #[error("Invalid terrain input: {0}")]
    InvalidInput(String),
    #[error("Altitude resolution failed: {0}")]
    AltitudeError(String),
}

impl TerrainError {
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidHeader(_) => ErrorCode::new("OL-CORE-TRN-0001"),
            Self::MalformedData(_) => ErrorCode::new("OL-CORE-TRN-0002"),
            Self::TileNotLoaded(_, _) => ErrorCode::new("OL-CORE-TRN-0003"),
            Self::RgbDecodeError(_) => ErrorCode::new("OL-CORE-TRN-0004"),
            Self::GeoTiffError(_) => ErrorCode::new("OL-CORE-TRN-0005"),
            Self::InvalidInput(_) => ErrorCode::new("OL-CORE-TRN-0006"),
            Self::AltitudeError(_) => ErrorCode::new("OL-CORE-TRN-0007"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerrainError;
    use crate::error_code::ErrorCode;
    use std::collections::HashSet;

    #[test]
    fn variants_have_stable_unique_codes() {
        let cases = [
            (
                TerrainError::InvalidHeader(String::new()),
                "OL-CORE-TRN-0001",
            ),
            (
                TerrainError::MalformedData(String::new()),
                "OL-CORE-TRN-0002",
            ),
            (TerrainError::TileNotLoaded(0, 0), "OL-CORE-TRN-0003"),
            (
                TerrainError::RgbDecodeError(String::new()),
                "OL-CORE-TRN-0004",
            ),
            (
                TerrainError::GeoTiffError(String::new()),
                "OL-CORE-TRN-0005",
            ),
            (
                TerrainError::InvalidInput(String::new()),
                "OL-CORE-TRN-0006",
            ),
            (
                TerrainError::AltitudeError(String::new()),
                "OL-CORE-TRN-0007",
            ),
        ];
        let codes: Vec<ErrorCode> = cases.iter().map(|(error, _)| error.code()).collect();
        let unique_codes: HashSet<ErrorCode> = codes.iter().copied().collect();

        assert_eq!(codes.len(), cases.len());
        assert_eq!(unique_codes.len(), cases.len());
        for (error, expected) in cases {
            assert_eq!(error.code().as_str(), expected);
        }
    }
}
