use serde::{Deserialize, Serialize};
use crate::geodesy::coords::LatLon;

/// Standard classification of controlled, uncontrolled, or special-use airspace.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AirspaceType {
    /// Flight Information Region (FIR).
    Fir,
    /// Upper Flight Information Region (UIR).
    Uir,
    /// Terminal Maneuvering Area (TMA).
    Tma,
    /// Control Zone (CTR).
    Ctr,
    /// ATC Sector.
    #[default]
    Sector,
    /// Prohibited Area (P).
    Prohibited,
    /// Restricted Area (R).
    Restricted,
    /// Danger Area (D).
    Danger,
    /// Military Operations Area (MOA).
    Moa,
    /// Temporary Reserved / Segregated Area (TRA / TSA).
    Tra,
    /// Other airspace category.
    Other(String),
}

/// Vertical datum reference for airspace limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AltitudeReference {
    /// Above Mean Sea Level (AMSL).
    #[default]
    Amsl,
    /// Above Ground Level (AGL).
    Agl,
    /// Standard pressure altitude / Flight Level (FL).
    FlightLevel,
    /// Earth surface / Ground (GND / SFC).
    Ground,
    /// Uncapped / Unlimited vertical extent.
    Uncapped,
}

/// Vertical boundary definition for an airspace volume.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AltitudeLimit {
    /// Height in meters above the datum.
    pub value_m: f64,
    /// Vertical reference datum.
    pub reference: AltitudeReference,
    /// Flight Level number if defined (e.g. FL245 = 245).
    pub flight_level: Option<u32>,
}

impl AltitudeLimit {
    /// Creates an altitude limit in meters AMSL.
    pub const fn amsl(value_m: f64) -> Self {
        Self {
            value_m,
            reference: AltitudeReference::Amsl,
            flight_level: None,
        }
    }

    /// Creates an altitude limit from a Flight Level (e.g. FL195 = 19500 ft = 5943.6 m).
    pub fn flight_level(fl: u32) -> Self {
        let value_m = f64::from(fl) * 100.0 * 0.3048;
        Self {
            value_m,
            reference: AltitudeReference::FlightLevel,
            flight_level: Some(fl),
        }
    }

    /// Creates a Ground/Surface limit (0.0 m GND).
    pub const fn ground() -> Self {
        Self {
            value_m: 0.0,
            reference: AltitudeReference::Ground,
            flight_level: None,
        }
    }

    /// Creates an Uncapped/Unlimited limit.
    pub const fn uncapped() -> Self {
        Self {
            value_m: 100_000.0, // 100 km standard uncapped ceiling
            reference: AltitudeReference::Uncapped,
            flight_level: None,
        }
    }
}

/// 3D Airspace boundary with vertical ceiling/floor and geodetic vertices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalAirspace {
    /// Unique identifier (e.g. "LFFF_FIR", "TMA_PARIS_1").
    pub uid: String,
    /// Human-readable airspace name.
    pub name: String,
    /// Type of airspace.
    pub airspace_type: AirspaceType,
    /// Lower vertical limit.
    pub lower_limit: AltitudeLimit,
    /// Upper vertical limit.
    pub upper_limit: AltitudeLimit,
    /// Boundary vertices on WGS84 ellipsoid.
    pub boundary: Vec<LatLon>,
}

/// Type of ground-based or space-based civil/military radio navigation aid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NavaidType {
    /// VHF Omnidirectional Range.
    Vor,
    /// Distance Measuring Equipment.
    Dme,
    /// Collocated VOR and DME.
    VorDme,
    /// Tactical Air Navigation (military).
    Tacan,
    /// Collocated VOR and TACAN.
    Vortac,
    /// Non-Directional Radio Beacon.
    Ndb,
    /// Navigation Intersection / Reporting Point.
    Fix,
    /// Area Navigation (RNAV) Waypoint.
    #[default]
    Waypoint,
}

/// Navigation aid or reporting point feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalNavaid {
    /// Identification code (e.g. "EPM", "BOU", "CLM").
    pub ident: String,
    /// Full navaid name.
    pub name: String,
    /// Navaid equipment type.
    pub navaid_type: NavaidType,
    /// Geodetic position (WGS84).
    pub coords: LatLon,
    /// Operating frequency in MHz (e.g. 115.65 MHz).
    pub frequency_mhz: Option<f64>,
    /// TACAN or DME channel code (e.g. "103X").
    pub channel: Option<String>,
    /// Elevation above mean sea level in meters.
    pub elevation_m: Option<f64>,
    /// Local magnetic declination / variation in degrees.
    pub magnetic_variation_deg: Option<f64>,
}

/// Category of Air Traffic Service (ATS) airway route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AirwayType {
    /// Conventional ground-based VOR/NDB route.
    #[default]
    Conventional,
    /// Area Navigation RNAV-5 (B-RNAV).
    Rnav5,
    /// Area Navigation RNAV-1 (P-RNAV).
    Rnav1,
    /// Required Navigation Performance RNP-4.
    Rnp4,
    /// Military tactical jet corridor.
    Military,
}

/// A single segment linking two fixes in an airway route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwaySegment {
    /// Origin fix identifier.
    pub from_ident: String,
    /// Destination fix identifier.
    pub to_ident: String,
    /// Origin geodetic position.
    pub from_coords: LatLon,
    /// Destination geodetic position.
    pub to_coords: LatLon,
    /// Minimum En-route Altitude (MEA) in meters.
    pub mea_m: Option<f64>,
    /// Maximum Authorized Altitude (MAA) in meters.
    pub maa_m: Option<f64>,
    /// Inbound magnetic track in degrees.
    pub inbound_bearing_deg: Option<f64>,
    /// Whether the segment is one-way only.
    pub is_unidirectional: bool,
}

/// ATS Airway route composed of sequenced segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalAirway {
    /// Airway designator (e.g. "UM616", "J50", "UN858").
    pub ident: String,
    /// Airway specification.
    pub route_type: AirwayType,
    /// Route segments.
    pub segments: Vec<AirwaySegment>,
}

/// Runway surface alignment and physical dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalRunway {
    /// Runway designator (e.g. "09L/27R", "14/32").
    pub ident: String,
    /// True heading in degrees.
    pub true_bearing_deg: f64,
    /// Magnetic heading in degrees.
    pub magnetic_bearing_deg: f64,
    /// Physical length in meters.
    pub length_m: f64,
    /// Physical width in meters.
    pub width_m: f64,
    /// Primary threshold coordinates.
    pub threshold_primary: LatLon,
    /// Secondary reciprocal threshold coordinates.
    pub threshold_secondary: LatLon,
    /// Surface type (e.g. "Asphalt", "Concrete", "Grass").
    pub surface: String,
}

/// Aerodrome or Heliport facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalAirport {
    /// ICAO 4-letter location indicator (e.g. "EGLL", "LFPG", "KJFK").
    pub icao: String,
    /// IATA 3-letter code if assigned (e.g. "LHR", "CDG", "JFK").
    pub iata: Option<String>,
    /// Aerodrome name.
    pub name: String,
    /// Airport reference point coordinates.
    pub coords: LatLon,
    /// Field elevation above mean sea level in meters.
    pub elevation_m: f64,
    /// Operational runways.
    pub runways: Vec<AeronauticalRunway>,
}
