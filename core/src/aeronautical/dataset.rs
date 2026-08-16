use serde::{Deserialize, Serialize};
use crate::aeronautical::types::{
    AeronauticalAirport, AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid,
};
use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::solvers::{GeodeticSolver, HaversineSolver};
use crate::geodesy::spatial::GeodesicPolygon;

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
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an airspace to the dataset.
    pub fn add_airspace(&mut self, airspace: AeronauticalAirspace) {
        self.airspaces.push(airspace);
    }

    /// Adds a navaid to the dataset.
    pub fn add_navaid(&mut self, navaid: AeronauticalNavaid) {
        self.navaids.push(navaid);
    }

    /// Adds an airway to the dataset.
    pub fn add_airway(&mut self, airway: AeronauticalAirway) {
        self.airways.push(airway);
    }

    /// Adds an airport to the dataset.
    pub fn add_airport(&mut self, airport: AeronauticalAirport) {
        self.airports.push(airport);
    }

    /// Total count of all aeronautical features.
    pub fn total_feature_count(&self) -> usize {
        self.airspaces.len() + self.navaids.len() + self.airways.len() + self.airports.len()
    }

    /// Finds a navaid by its identification code (case-insensitive).
    pub fn find_navaid(&self, ident: &str) -> Option<&AeronauticalNavaid> {
        self.navaids.iter().find(|n| n.ident.eq_ignore_ascii_case(ident))
    }

    /// Finds an airport by its ICAO code (case-insensitive).
    pub fn find_airport(&self, icao: &str) -> Option<&AeronauticalAirport> {
        self.airports.iter().find(|a| a.icao.eq_ignore_ascii_case(icao))
    }

    /// Finds an airway route by its designator (case-insensitive).
    pub fn find_airway(&self, ident: &str) -> Option<&AeronauticalAirway> {
        self.airways.iter().find(|a| a.ident.eq_ignore_ascii_case(ident))
    }

    /// Finds all navaids within a given radius in meters from a geodetic coordinate.
    pub fn find_navaids_within_radius(&self, center: &LatLon, radius_meters: f64) -> Vec<&AeronauticalNavaid> {
        let solver = HaversineSolver;
        let ellipsoid = Ellipsoid::wgs84();
        self.navaids
            .iter()
            .filter(|n| {
                if let Ok(res) = solver.inverse(center, &n.coords, &ellipsoid) {
                    res.distance <= radius_meters
                } else {
                    false
                }
            })
            .collect()
    }

    /// Finds all airspaces whose horizontal 2D/3D polygon contains the given coordinate.
    pub fn find_airspaces_containing_point(&self, point: &LatLon) -> Vec<&AeronauticalAirspace> {
        self.airspaces
            .iter()
            .filter(|a| {
                if a.boundary.len() < 3 {
                    return false;
                }
                let poly = GeodesicPolygon::new(a.boundary.clone());
                poly.contains_point(point)
            })
            .collect()
    }
}
