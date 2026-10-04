use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    GeoTiff,
}
impl ImageFormat {
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::GeoTiff => "image/tiff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorFormat {
    GeoJson,
    Gml,
}
impl VectorFormat {
    pub const fn mime(self) -> &'static str {
        match self {
            Self::GeoJson => "application/geo+json",
            Self::Gml => "application/gml+xml",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageSampleType {
    UInt8,
    Int16,
    UInt16,
    Float32,
    Float64,
}
