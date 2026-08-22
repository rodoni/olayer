pub mod errors;
pub mod isoline;
pub mod radar_palette;
pub mod sigmet;
pub mod wind_barb;

#[cfg(test)]
mod tests;

pub use errors::WeatherError;
pub use isoline::{generate_isolines_rad, isolines_to_flat_array_deg, IsolineSegment};
pub use radar_palette::{colorize_dbz_grid, dbz_to_rgba, RadarColorPalette};
pub use sigmet::{SigmetDataset, SigmetFeature, SigmetHazardType, SigmetSeverity};
pub use wind_barb::{generate_wind_barb, wind_barb_to_flat_lines_deg, WindBarbGeometry};
