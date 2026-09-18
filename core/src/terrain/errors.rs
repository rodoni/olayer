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
