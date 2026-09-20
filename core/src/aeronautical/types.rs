use crate::aeronautical::errors::AeronauticalError;
use crate::geodesy::coords::LatLon;
use serde::{Deserialize, Deserializer, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AltitudeLimit {
    /// Height in meters above the datum.
    value_m: f64,
    /// Vertical reference datum.
    reference: AltitudeReference,
    /// Flight Level number if defined (e.g. FL245 = 245).
    flight_level: Option<u32>,
}

impl AltitudeLimit {
    /// Creates an altitude limit in meters AMSL.
    ///
    /// # Errors
    /// Returns [`AeronauticalError::InvalidAltitude`] for non-finite or negative values.
    ///
    /// # Panics
    /// This function does not panic for valid Rust inputs.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn amsl(value_m: f64) -> Result<Self, AeronauticalError> {
        if !value_m.is_finite() || value_m < 0.0 {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "invalid AMSL value: {value_m}"
            )));
        }
        Ok(Self {
            value_m,
            reference: AltitudeReference::Amsl,
            flight_level: None,
        })
    }

    /// Creates an altitude limit in meters AGL.
    ///
    /// # Errors
    /// Returns [`AeronauticalError::InvalidAltitude`] for non-finite or negative values.
    ///
    /// # Panics
    /// This function does not panic for valid Rust inputs.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn agl(value_m: f64) -> Result<Self, AeronauticalError> {
        if !value_m.is_finite() || value_m < 0.0 {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "invalid AGL value: {value_m}"
            )));
        }
        Ok(Self {
            value_m,
            reference: AltitudeReference::Agl,
            flight_level: None,
        })
    }

    /// Creates an altitude limit from a Flight Level (e.g. FL195 = 19500 ft = 5943.6 m).
    ///
    /// # Errors
    /// Returns [`AeronauticalError::InvalidAltitude`] for invalid, non-finite,
    /// fractional, negative, or out-of-range flight levels.
    ///
    /// # Panics
    /// This function does not panic for valid Rust inputs.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn from_flight_level(fl: impl Into<f64>) -> Result<Self, AeronauticalError> {
        let fl = fl.into();
        if !fl.is_finite() || fl < 0.0 || fl > f64::from(u32::MAX) || fl.fract() != 0.0 {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "invalid flight level: {fl}"
            )));
        }
        let flight_level = fl.to_string().parse::<u32>().map_err(|_| {
            AeronauticalError::InvalidAltitude(format!("invalid flight level: {fl}"))
        })?;
        let value_m = fl * 100.0 * 0.3048;
        if !value_m.is_finite() {
            return Err(AeronauticalError::InvalidAltitude(
                "flight-level conversion is not finite".into(),
            ));
        }
        Ok(Self {
            value_m,
            reference: AltitudeReference::FlightLevel,
            flight_level: Some(flight_level),
        })
    }

    /// Creates a Ground/Surface limit (0.0 m GND).
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub const fn ground() -> Self {
        Self {
            value_m: 0.0,
            reference: AltitudeReference::Ground,
            flight_level: None,
        }
    }

    /// Creates an Uncapped/Unlimited limit.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub const fn uncapped() -> Self {
        Self {
            value_m: 100_000.0, // 100 km standard uncapped ceiling
            reference: AltitudeReference::Uncapped,
            flight_level: None,
        }
    }

    /// Returns the altitude in meters relative to the stored datum.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub const fn value_m(&self) -> f64 {
        self.value_m
    }

    /// Returns the vertical datum reference.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub const fn reference(&self) -> AltitudeReference {
        self.reference
    }
    /// Returns the flight level, when this limit uses a flight-level datum.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub const fn flight_level(&self) -> Option<u32> {
        self.flight_level
    }

    /// Validates a deserialized altitude limit.
    ///
    /// # Errors
    /// Returns [`AeronauticalError::InvalidAltitude`] when the stored value is invalid.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if !self.value_m.is_finite() || self.value_m < 0.0 {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "invalid value: {}",
                self.value_m
            )));
        }
        if self.reference == AltitudeReference::FlightLevel {
            let Some(fl) = self.flight_level else {
                return Err(AeronauticalError::InvalidAltitude(
                    "flight-level reference requires a flight level".into(),
                ));
            };
            let expected = f64::from(fl) * 100.0 * 0.3048;
            if !expected.is_finite() || self.value_m != expected {
                return Err(AeronauticalError::InvalidAltitude(
                    "flight-level value does not match flight level".into(),
                ));
            }
        } else if self.flight_level.is_some() {
            return Err(AeronauticalError::InvalidAltitude(
                "non-flight-level reference cannot contain a flight level".into(),
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for AltitudeLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawAltitudeLimit {
            value_m: f64,
            reference: AltitudeReference,
            flight_level: Option<u32>,
        }

        let raw = RawAltitudeLimit::deserialize(deserializer)?;
        let limit = Self {
            value_m: raw.value_m,
            reference: raw.reference,
            flight_level: raw.flight_level,
        };
        limit.validate().map_err(serde::de::Error::custom)?;
        Ok(limit)
    }
}

fn validate_coordinate(coordinate: &LatLon) -> Result<(), AeronauticalError> {
    coordinate
        .validate()
        .map_err(|error| AeronauticalError::InvalidCoordinateString(error.to_string()))
}

fn validate_bearing(value: f64, field: &str) -> Result<(), AeronauticalError> {
    if !value.is_finite() || !(0.0..360.0).contains(&value) {
        return Err(AeronauticalError::FormatError(format!(
            "{field} must be finite and in [0, 360)"
        )));
    }
    Ok(())
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

impl AeronauticalAirspace {
    /// Validates identifiers, vertical limits, and polygon coordinates.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.uid.trim().is_empty() || self.name.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airspace identifier and name".into(),
            ));
        }
        self.lower_limit.validate()?;
        self.upper_limit.validate()?;
        if self.lower_limit.reference() == AltitudeReference::Uncapped {
            return Err(AeronauticalError::InvalidAltitude(
                "uncapped cannot be an airspace lower limit".into(),
            ));
        }
        if self.upper_limit.reference() == AltitudeReference::Ground {
            return Err(AeronauticalError::InvalidAltitude(
                "ground cannot be an airspace upper limit".into(),
            ));
        }
        if self.boundary.len() < 4 || self.boundary.first() != self.boundary.last() {
            return Err(AeronauticalError::FormatError(
                "airspace boundary must be a closed ring".into(),
            ));
        }
        self.boundary.iter().try_for_each(validate_coordinate)
    }
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

impl AeronauticalNavaid {
    /// Validates the navaid identity, position, and optional numeric fields.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.ident.trim().is_empty() || self.name.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "navaid identifier and name".into(),
            ));
        }
        validate_coordinate(&self.coords)?;
        for (value, field) in [
            (self.frequency_mhz, "frequency_mhz"),
            (self.elevation_m, "elevation_m"),
            (self.magnetic_variation_deg, "magnetic_variation_deg"),
        ] {
            if let Some(value) = value {
                if !value.is_finite() {
                    return Err(AeronauticalError::FormatError(format!(
                        "{field} must be finite"
                    )));
                }
            }
        }
        Ok(())
    }
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

/// Directionality of an airway segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SegmentDirection {
    /// The segment may be flown in both directions.
    #[default]
    Bidirectional,
    /// The segment may be flown only in its declared direction.
    Unidirectional,
}

mod segment_direction_serde {
    use super::SegmentDirection;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(direction: &SegmentDirection, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(matches!(direction, SegmentDirection::Unidirectional))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SegmentDirection, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(if bool::deserialize(deserializer)? {
            SegmentDirection::Unidirectional
        } else {
            SegmentDirection::Bidirectional
        })
    }
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
    /// Directionality of the segment.
    #[serde(with = "segment_direction_serde")]
    pub direction: SegmentDirection,
}

impl AirwaySegment {
    /// Validates endpoints, altitude bands, and inbound bearing.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.from_ident.trim().is_empty() || self.to_ident.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airway segment identifiers".into(),
            ));
        }
        validate_coordinate(&self.from_coords)?;
        validate_coordinate(&self.to_coords)?;
        if let Some(value) = self.mea_m {
            if !value.is_finite() || value < 0.0 {
                return Err(AeronauticalError::InvalidAltitude("invalid MEA".into()));
            }
        }
        if let Some(value) = self.maa_m {
            if !value.is_finite() || value < 0.0 {
                return Err(AeronauticalError::InvalidAltitude("invalid MAA".into()));
            }
        }
        if let (Some(mea), Some(maa)) = (self.mea_m, self.maa_m) {
            if mea > maa {
                return Err(AeronauticalError::InvalidAltitude("MEA exceeds MAA".into()));
            }
        }
        if let Some(bearing) = self.inbound_bearing_deg {
            validate_bearing(bearing, "inbound bearing")?;
        }
        Ok(())
    }
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

impl AeronauticalAirway {
    /// Validates the airway identity and all of its segments.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.ident.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airway identifier".into(),
            ));
        }
        if self.segments.is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airway segments".into(),
            ));
        }
        self.segments.iter().try_for_each(AirwaySegment::validate)
    }
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

impl AeronauticalRunway {
    /// Validates runway dimensions, bearings, and thresholds.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.ident.trim().is_empty() || self.surface.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "runway identifier and surface".into(),
            ));
        }
        validate_bearing(self.true_bearing_deg, "true bearing")?;
        validate_bearing(self.magnetic_bearing_deg, "magnetic bearing")?;
        if !self.length_m.is_finite() || self.length_m <= 0.0 {
            return Err(AeronauticalError::FormatError(
                "runway length must be positive and finite".into(),
            ));
        }
        if !self.width_m.is_finite() || self.width_m <= 0.0 {
            return Err(AeronauticalError::FormatError(
                "runway width must be positive and finite".into(),
            ));
        }
        validate_coordinate(&self.threshold_primary)?;
        validate_coordinate(&self.threshold_secondary)
    }
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

impl AeronauticalAirport {
    /// Validates airport identity, position, elevation, and runways.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        if self.icao.trim().is_empty() || self.name.trim().is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airport ICAO and name".into(),
            ));
        }
        validate_coordinate(&self.coords)?;
        if !self.elevation_m.is_finite() {
            return Err(AeronauticalError::FormatError(
                "airport elevation must be finite".into(),
            ));
        }
        self.runways
            .iter()
            .try_for_each(AeronauticalRunway::validate)
    }
}
