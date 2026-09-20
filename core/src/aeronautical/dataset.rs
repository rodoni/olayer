use crate::aeronautical::errors::AeronauticalError;
use crate::aeronautical::types::{
    AeronauticalAirport, AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid,
};
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::solvers::{GeodeticSolver, HaversineSolver};
use crate::geodesy::spatial::GeodesicPolygon;
use serde::{Deserialize, Serialize};

/// Complete in-memory repository of aeronautical information.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AeronauticalDataset {
    /// Controlled and special-use airspaces.
    pub airspaces: Vec<AeronauticalAirspace>,
    /// Radio navigation aids and fixes.
    pub navaids: Vec<AeronauticalNavaid>,
    /// ATS Airway routes.
    pub airways: Vec<AeronauticalAirway>,
    /// Aerodromes and runways.
    pub airports: Vec<AeronauticalAirport>,
}

impl AeronauticalDataset {
    /// Creates a new empty aeronautical dataset.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an airspace to the dataset.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn add_airspace(&mut self, airspace: AeronauticalAirspace) {
        self.airspaces.push(airspace);
    }

    /// Adds a navaid to the dataset.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn add_navaid(&mut self, navaid: AeronauticalNavaid) {
        self.navaids.push(navaid);
    }

    /// Adds an airway to the dataset.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn add_airway(&mut self, airway: AeronauticalAirway) {
        self.airways.push(airway);
    }

    /// Adds an airport to the dataset.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn add_airport(&mut self, airport: AeronauticalAirport) {
        self.airports.push(airport);
    }

    /// Total count of all aeronautical features.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn total_feature_count(&self) -> usize {
        self.airspaces.len() + self.navaids.len() + self.airways.len() + self.airports.len()
    }

    /// Validates every feature currently stored in the dataset.
    pub fn validate(&self) -> Result<(), AeronauticalError> {
        self.airspaces
            .iter()
            .try_for_each(AeronauticalAirspace::validate)?;
        self.navaids
            .iter()
            .try_for_each(AeronauticalNavaid::validate)?;
        self.airways
            .iter()
            .try_for_each(AeronauticalAirway::validate)?;
        self.airports
            .iter()
            .try_for_each(AeronauticalAirport::validate)
    }

    /// Finds a navaid by its identification code (case-insensitive).
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn find_navaid(&self, ident: &str) -> Option<&AeronauticalNavaid> {
        self.navaids
            .iter()
            .find(|n| n.ident.eq_ignore_ascii_case(ident))
    }

    /// Finds an airport by its ICAO code (case-insensitive).
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn find_airport(&self, icao: &str) -> Option<&AeronauticalAirport> {
        self.airports
            .iter()
            .find(|a| a.icao.eq_ignore_ascii_case(icao))
    }

    /// Finds an airway route by its designator (case-insensitive).
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn find_airway(&self, ident: &str) -> Option<&AeronauticalAirway> {
        self.airways
            .iter()
            .find(|a| a.ident.eq_ignore_ascii_case(ident))
    }

    /// Finds all navaids within a given radius in meters from a geodetic coordinate.
    ///
    /// # Errors
    /// Returns [`AeronauticalError::FormatError`] when the radius is negative,
    /// non-finite, or the center contains invalid coordinates.
    ///
    /// # Panics
    /// This function does not panic for valid Rust inputs.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn find_navaids_within_radius(
        &self,
        center: &LatLon,
        radius_meters: f64,
    ) -> Result<Vec<&AeronauticalNavaid>, AeronauticalError> {
        if !radius_meters.is_finite() || radius_meters < 0.0 {
            return Err(AeronauticalError::FormatError(
                "radius must be finite and non-negative".into(),
            ));
        }
        if center.validate().is_err() {
            return Err(AeronauticalError::InvalidCoordinateString(
                "center coordinates are invalid".into(),
            ));
        }
        let solver = HaversineSolver;
        let ellipsoid = Ellipsoid::wgs84();
        Ok(self
            .navaids
            .iter()
            .filter(|n| {
                if n.coords.validate().is_ok() {
                    if let Ok(res) = solver.inverse(center, &n.coords, &ellipsoid) {
                        return res.distance <= radius_meters;
                    }
                }
                false
            })
            .collect())
    }

    /// Finds all airspaces whose horizontal 2D/3D polygon contains the given coordinate.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    ///
    /// # Safety
    /// This function does not use unsafe operations.
    pub fn find_airspaces_containing_point(&self, point: &LatLon) -> Vec<&AeronauticalAirspace> {
        if point.validate().is_err() {
            return Vec::new();
        }
        self.airspaces
            .iter()
            .filter(|a| {
                if a.boundary.len() < 3 {
                    return false;
                }
                GeodesicPolygon::contains_point_vertices(&a.boundary, point)
            })
            .collect()
    }
}
