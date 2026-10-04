use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct MapError {
    pub code: ErrorCode,
    pub message: String,
}

impl MapError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub const INVALID_CRS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0001");
pub const UNSUPPORTED_TRANSFORMATION: ErrorCode = ErrorCode::new("OL-CORE-MAP-0002");
pub const INVALID_TILE_KEY: ErrorCode = ErrorCode::new("OL-CORE-MAP-0003");
pub const TILE_OUTSIDE_MATRIX: ErrorCode = ErrorCode::new("OL-CORE-MAP-0004");
pub const INVALID_BOUNDS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0005");
pub const INVALID_WMS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0006");
pub const INVALID_WMTS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0007");
pub const INVALID_WFS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0008");
pub const INVALID_WCS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0009");
pub const UNSUPPORTED_FORMAT: ErrorCode = ErrorCode::new("OL-CORE-MAP-0010");
pub const INVALID_COVERAGE_DIMENSIONS: ErrorCode = ErrorCode::new("OL-CORE-MAP-0011");
pub const INVALID_CAPABILITIES: ErrorCode = ErrorCode::new("OL-CORE-MAP-0012");
