pub mod aixm_parser;
pub mod dataset;
pub mod errors;
pub mod geojson_parser;
pub mod types;

#[cfg(test)]
mod tests;

pub use aixm_parser::parse_aixm_51_str;
pub use dataset::AeronauticalDataset;
pub use errors::AeronauticalError;
pub use geojson_parser::{export_dataset_to_geojson, parse_geojson_aviation_str};
pub use types::{
    AeronauticalAirport, AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid,
    AeronauticalRunway, AirspaceType, AirwaySegment, AirwayType, AltitudeLimit, AltitudeReference,
    NavaidType,
};
