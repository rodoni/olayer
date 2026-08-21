mod errors;
pub mod geotiff;
pub mod rgb_decoder;
pub mod rgb_tile;
mod tile;
pub mod engine;

#[cfg(test)]
mod tests;

pub use errors::TerrainError;
pub use geotiff::GeoTiffTile;
pub use rgb_decoder::{
    decode_mapbox_rgb, decode_rgb_buffer, decode_rgb_elevation, decode_rgba_buffer,
    decode_terrarium_rgb, RgbElevationEncoding,
};
pub use rgb_tile::{RgbElevationTile, SlippyTileKey};
pub use tile::DtedTile;
pub use engine::{
    ClearanceResult, ElevationSample, MsawState, ProfilePoint, ProfilePointStatus, TerrainEngine,
    TileKey, UnknownTerrainPolicy,
};
