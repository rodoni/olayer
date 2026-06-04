use std::fmt;

/// Errors that can occur during terrain processing.
#[derive(Debug, Clone, PartialEq)]
pub enum TerrainError {
    /// The DTED header is malformed or the buffer is too short.
    InvalidHeader(String),
    /// The DTED data records are corrupted or incomplete.
    MalformedData(String),
    /// The requested tile has not been loaded into the engine.
    TileNotLoaded(i32, i32),
}

impl fmt::Display for TerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerrainError::InvalidHeader(err) => write!(f, "Invalid DTED header: {err}"),
            TerrainError::MalformedData(err) => write!(f, "Corrupted DTED data: {err}"),
            TerrainError::TileNotLoaded(lat, lon) => {
                write!(f, "DTED tile not loaded for coordinate ({lat}, {lon})")
            }
        }
    }
}

impl std::error::Error for TerrainError {}
