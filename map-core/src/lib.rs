//! Transport-independent map contracts and pure OGC request builders.

pub mod bounds;
pub mod capabilities;
pub mod crs;
pub mod dimensions;
pub mod errors;
pub mod format;
pub mod request;
pub mod tile;
pub mod wcs;
pub mod wfs;
pub mod wms;
pub mod wmts;

pub use bounds::BoundingBox;
pub use capabilities::{Capabilities, ServiceType};
pub use crs::{CoordinateReferenceSystem, CrsAuthority};
pub use dimensions::DisplayDimension;
pub use errors::{
    ErrorCode, MapError, INVALID_BOUNDS, INVALID_CAPABILITIES, INVALID_COVERAGE_DIMENSIONS,
    INVALID_CRS, INVALID_TILE_KEY, INVALID_WCS, INVALID_WFS, INVALID_WMS, INVALID_WMTS,
    TILE_OUTSIDE_MATRIX, UNSUPPORTED_FORMAT, UNSUPPORTED_TRANSFORMATION,
};
pub use format::{CoverageSampleType, ImageFormat, VectorFormat};
pub use request::{QueryParameter, UrlParts};
pub use tile::{TileKey, TileMatrix};
pub use wcs::{Coverage, WcsGetCoverageRequest, WcsRequestBuilder};
pub use wfs::{WfsGetFeatureRequest, WfsRequestBuilder};
pub use wms::{MapImage, WmsGetMapRequest, WmsRequestBuilder};
pub use wmts::{WmtsGetTileRequest, WmtsRequestBuilder};
