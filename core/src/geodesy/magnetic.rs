use crate::geodesy::coords::LatLon;
use crate::geodesy::ellipsoid::Ellipsoid;
use crate::geodesy::errors::GeodesyError;
use crate::geodesy::math::normalize_bearing;
use serde::{Deserialize, Serialize};

/// Geomagnetic reference radius $a$ in meters (WMM standard: 6,371,200.0 m).
pub const WMM_EARTH_RADIUS_METERS: f64 = 6371200.0;

/// Base epoch for WMM-2025.
pub const WMM_BASE_EPOCH: f64 = 2025.0;

/// Maximum degree of the standard WMM-2025 spherical harmonic expansion.
pub const WMM_MAX_DEGREE: usize = 12;

/// Magnetic elements computed at a specific point and epoch.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MagneticElements {
    /// Magnetic declination (variation) in radians (positive East, negative West).
    pub declination_rad: f64,
    /// Magnetic inclination (dip angle) in radians (positive downward).
    pub inclination_rad: f64,
    /// Horizontal magnetic field intensity $H$ in nanoTesla (nT).
    pub horizontal_intensity_nt: f64,
    /// Total magnetic field intensity $F$ in nanoTesla (nT).
    pub total_intensity_nt: f64,
    /// North component of the magnetic field in nT.
    pub x_nt: f64,
    /// East component of the magnetic field in nT.
    pub y_nt: f64,
    /// Downward component of the magnetic field in nT.
    pub z_nt: f64,
}

impl MagneticElements {
    /// Returns magnetic declination in decimal degrees.
    #[inline]
    pub fn declination_deg(&self) -> f64 {
        self.declination_rad.to_degrees()
    }

    /// Returns magnetic inclination in decimal degrees.
    #[inline]
    pub fn inclination_deg(&self) -> f64 {
        self.inclination_rad.to_degrees()
    }
}

/// A single spherical harmonic coefficient entry ($g_n^m, h_n^m, \dot{g}_n^m, \dot{h}_n^m$).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MagneticCoeffEntry {
    /// Degree $n \ge 1$.
    pub n: usize,
    /// Order $0 \le m \le n$.
    pub m: usize,
    /// Gauss coefficient $g_n^m$ in nT at base epoch.
    pub g: f64,
    /// Gauss coefficient $h_n^m$ in nT at base epoch.
    pub h: f64,
    /// Secular variation $\dot{g}_n^m$ in nT/year.
    pub g_dot: f64,
    /// Secular variation $\dot{h}_n^m$ in nT/year.
    pub h_dot: f64,
}

/// Dynamic container for spherical harmonic magnetic model coefficients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MagneticCoefficients {
    /// Base epoch in decimal years (e.g. 2025.0).
    pub epoch: f64,
    /// Name or identifier of the magnetic model (e.g. "WMM-2025", "WMM-2030").
    pub model_name: String,
    /// Release date string (e.g. "11/20/2024").
    pub release_date: String,
    /// Maximum degree of the spherical harmonic expansion.
    pub max_degree: usize,
    /// Coefficient entries.
    pub entries: Vec<MagneticCoeffEntry>,
}

impl MagneticCoefficients {
    /// Parses a standard NOAA / BGS `WMM.COF` formatted string.
    ///
    /// # Format Specification
    /// - Line 1 (Header): `<epoch> <model_name> <release_date>` (e.g., `2025.0 WMM-2025 11/20/2024`)
    /// - Subsequent lines: `<n> <m> <g_nm> <h_nm> <g_dot_nm> <h_dot_nm>`
    /// - Blank lines and comment lines (starting with `#` or `//`) are ignored.
    /// - Trailing lines starting with `9999` are treated as End-Of-File.
    pub fn from_cof_str(cof_content: &str) -> Result<Self, GeodesyError> {
        let mut lines = cof_content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//"));

        let header = lines
            .next()
            .ok_or_else(|| GeodesyError::MagneticModelError("Empty COF content".to_string()))?;

        let header_tokens: Vec<&str> = header.split_whitespace().collect();
        if header_tokens.is_empty() {
            return Err(GeodesyError::MagneticModelError("Missing COF header".to_string()));
        }

        let epoch = header_tokens[0]
            .parse::<f64>()
            .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid COF epoch in header '{header}': {e}")))?;

        let model_name = header_tokens.get(1).copied().unwrap_or("CUSTOM").to_string();
        let release_date = header_tokens.get(2).copied().unwrap_or("").to_string();

        let mut entries = Vec::new();
        let mut max_degree = 0;

        for line in lines {
            if line.starts_with("9999") {
                break;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() || tokens[0].starts_with('#') {
                continue;
            }

            if tokens.len() < 6 {
                return Err(GeodesyError::MagneticModelError(format!(
                    "Invalid COF line (expected 6 fields, got {}): '{line}'",
                    tokens.len()
                )));
            }

            let n = tokens[0]
                .parse::<usize>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid degree n in line '{line}': {e}")))?;
            let m = tokens[1]
                .parse::<usize>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid order m in line '{line}': {e}")))?;
            let g = tokens[2]
                .parse::<f64>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid g_nm in line '{line}': {e}")))?;
            let h = tokens[3]
                .parse::<f64>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid h_nm in line '{line}': {e}")))?;
            let g_dot = tokens[4]
                .parse::<f64>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid g_dot_nm in line '{line}': {e}")))?;
            let h_dot = tokens[5]
                .parse::<f64>()
                .map_err(|e| GeodesyError::MagneticModelError(format!("Invalid h_dot_nm in line '{line}': {e}")))?;

            if m > n {
                return Err(GeodesyError::MagneticModelError(format!(
                    "Order m ({m}) cannot exceed degree n ({n}) in line '{line}'"
                )));
            }

            max_degree = max_degree.max(n);
            entries.push(MagneticCoeffEntry {
                n,
                m,
                g,
                h,
                g_dot,
                h_dot,
            });
        }

        if entries.is_empty() {
            return Err(GeodesyError::MagneticModelError(
                "No coefficient entries found in COF content".to_string(),
            ));
        }

        Ok(MagneticCoefficients {
            epoch,
            model_name,
            release_date,
            max_degree,
            entries,
        })
    }

    /// Loads magnetic coefficients from a `WMM.COF` formatted file on the filesystem.
    pub fn from_cof_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, GeodesyError> {
        let p = path.as_ref();
        let content = std::fs::read_to_string(p)
            .map_err(|e| GeodesyError::MagneticModelError(format!("Failed to read COF file '{}': {e}", p.display())))?;
        Self::from_cof_str(&content)
    }
}

/// Official built-in WMM-2025 spherical harmonic coefficients.
pub(crate) const WMM2025_COEFFS: [MagneticCoeffEntry; 90] = [
    // Degree 1
    MagneticCoeffEntry { n: 1, m: 0, g: -29396.6, h: 0.0, g_dot: 11.6, h_dot: 0.0 },
    MagneticCoeffEntry { n: 1, m: 1, g: -1404.9, h: 4589.6, g_dot: 12.3, h_dot: -23.4 },
    // Degree 2
    MagneticCoeffEntry { n: 2, m: 0, g: -2499.7, h: 0.0, g_dot: -12.4, h_dot: 0.0 },
    MagneticCoeffEntry { n: 2, m: 1, g: 2997.5, h: -3013.9, g_dot: 0.8, h_dot: -20.6 },
    MagneticCoeffEntry { n: 2, m: 2, g: 1686.2, h: -664.1, g_dot: -2.1, h_dot: -20.1 },
    // Degree 3
    MagneticCoeffEntry { n: 3, m: 0, g: 1362.4, h: 0.0, g_dot: 3.6, h_dot: 0.0 },
    MagneticCoeffEntry { n: 3, m: 1, g: -2382.4, h: -70.9, g_dot: -6.5, h_dot: -6.1 },
    MagneticCoeffEntry { n: 3, m: 2, g: 1237.9, h: 247.9, g_dot: -0.5, h_dot: -1.2 },
    MagneticCoeffEntry { n: 3, m: 3, g: 557.0, h: -572.7, g_dot: -10.3, h_dot: 2.1 },
    // Degree 4
    MagneticCoeffEntry { n: 4, m: 0, g: 947.2, h: 0.0, g_dot: -1.6, h_dot: 0.0 },
    MagneticCoeffEntry { n: 4, m: 1, g: 806.3, h: 296.3, g_dot: -0.3, h_dot: 1.4 },
    MagneticCoeffEntry { n: 4, m: 2, g: 457.9, h: -244.2, g_dot: -4.8, h_dot: 6.7 },
    MagneticCoeffEntry { n: 4, m: 3, g: -425.4, h: 90.7, g_dot: 1.7, h_dot: 3.3 },
    MagneticCoeffEntry { n: 4, m: 4, g: 147.2, h: -336.5, g_dot: -6.5, h_dot: -0.3 },
    // Degree 5
    MagneticCoeffEntry { n: 5, m: 0, g: -238.1, h: 0.0, g_dot: -0.8, h_dot: 0.0 },
    MagneticCoeffEntry { n: 5, m: 1, g: 367.6, h: 44.5, g_dot: 0.2, h_dot: 0.0 },
    MagneticCoeffEntry { n: 5, m: 2, g: 201.2, h: 187.8, g_dot: 1.5, h_dot: 1.8 },
    MagneticCoeffEntry { n: 5, m: 3, g: -152.0, h: -150.1, g_dot: -3.8, h_dot: 4.8 },
    MagneticCoeffEntry { n: 5, m: 4, g: -160.0, h: -85.7, g_dot: -1.2, h_dot: 2.7 },
    MagneticCoeffEntry { n: 5, m: 5, g: 97.4, h: 104.9, g_dot: 2.1, h_dot: 1.7 },
    // Degree 6
    MagneticCoeffEntry { n: 6, m: 0, g: 67.9, h: 0.0, g_dot: -0.3, h_dot: 0.0 },
    MagneticCoeffEntry { n: 6, m: 1, g: 65.5, h: -17.5, g_dot: -0.2, h_dot: -0.5 },
    MagneticCoeffEntry { n: 6, m: 2, g: 73.1, h: 63.8, g_dot: 1.0, h_dot: -0.5 },
    MagneticCoeffEntry { n: 6, m: 3, g: -142.4, h: 72.8, g_dot: 1.2, h_dot: -0.6 },
    MagneticCoeffEntry { n: 6, m: 4, g: -1.8, h: -67.5, g_dot: -0.3, h_dot: -1.5 },
    MagneticCoeffEntry { n: 6, m: 5, g: 15.6, h: -3.0, g_dot: -0.3, h_dot: -0.4 },
    MagneticCoeffEntry { n: 6, m: 6, g: -88.9, h: 22.8, g_dot: 1.0, h_dot: 2.8 },
    // Degree 7
    MagneticCoeffEntry { n: 7, m: 0, g: 79.9, h: 0.0, g_dot: -0.1, h_dot: 0.0 },
    MagneticCoeffEntry { n: 7, m: 1, g: -73.3, h: -63.7, g_dot: -0.4, h_dot: -0.2 },
    MagneticCoeffEntry { n: 7, m: 2, g: 2.2, h: -0.3, g_dot: 0.5, h_dot: -0.7 },
    MagneticCoeffEntry { n: 7, m: 3, g: 31.7, h: 18.0, g_dot: 1.2, h_dot: 0.4 },
    MagneticCoeffEntry { n: 7, m: 4, g: -12.4, h: -24.4, g_dot: 0.2, h_dot: 0.5 },
    MagneticCoeffEntry { n: 7, m: 5, g: -18.7, h: 6.9, g_dot: -0.7, h_dot: 0.9 },
    MagneticCoeffEntry { n: 7, m: 6, g: 7.2, h: 25.1, g_dot: 0.8, h_dot: 0.0 },
    MagneticCoeffEntry { n: 7, m: 7, g: 10.3, h: -10.9, g_dot: 0.7, h_dot: -0.2 },
    // Degree 8
    MagneticCoeffEntry { n: 8, m: 0, g: 24.3, h: 0.0, g_dot: -0.1, h_dot: 0.0 },
    MagneticCoeffEntry { n: 8, m: 1, g: 7.6, h: 9.9, g_dot: 0.1, h_dot: -0.3 },
    MagneticCoeffEntry { n: 8, m: 2, g: -8.8, h: -18.8, g_dot: -0.3, h_dot: 0.4 },
    MagneticCoeffEntry { n: 8, m: 3, g: -12.3, h: 8.5, g_dot: 0.3, h_dot: -0.3 },
    MagneticCoeffEntry { n: 8, m: 4, g: -17.2, h: -23.0, g_dot: -0.3, h_dot: 0.2 },
    MagneticCoeffEntry { n: 8, m: 5, g: 5.6, h: 14.5, g_dot: -0.1, h_dot: -0.6 },
    MagneticCoeffEntry { n: 8, m: 6, g: 1.9, h: -13.0, g_dot: 0.4, h_dot: 0.3 },
    MagneticCoeffEntry { n: 8, m: 7, g: 4.8, h: -14.6, g_dot: 0.0, h_dot: 0.4 },
    MagneticCoeffEntry { n: 8, m: 8, g: -8.9, h: 11.2, g_dot: -0.3, h_dot: 0.2 },
    // Degree 9
    MagneticCoeffEntry { n: 9, m: 0, g: 5.6, h: 0.0, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 9, m: 1, g: 9.7, h: -20.6, g_dot: -0.1, h_dot: 0.0 },
    MagneticCoeffEntry { n: 9, m: 2, g: 2.4, h: 14.7, g_dot: 0.0, h_dot: -0.2 },
    MagneticCoeffEntry { n: 9, m: 3, g: -10.4, h: 10.3, g_dot: -0.2, h_dot: -0.2 },
    MagneticCoeffEntry { n: 9, m: 4, g: 7.7, h: -4.3, g_dot: -0.1, h_dot: 0.1 },
    MagneticCoeffEntry { n: 9, m: 5, g: -0.6, h: -7.5, g_dot: -0.1, h_dot: 0.1 },
    MagneticCoeffEntry { n: 9, m: 6, g: -0.4, h: 0.3, g_dot: 0.1, h_dot: 0.0 },
    MagneticCoeffEntry { n: 9, m: 7, g: 3.3, h: 2.6, g_dot: 0.0, h_dot: -0.2 },
    MagneticCoeffEntry { n: 9, m: 8, g: 0.5, h: 5.2, g_dot: 0.0, h_dot: -0.1 },
    MagneticCoeffEntry { n: 9, m: 9, g: -2.3, h: -0.8, g_dot: -0.4, h_dot: 0.2 },
    // Degree 10
    MagneticCoeffEntry { n: 10, m: 0, g: -2.0, h: 0.0, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 10, m: 1, g: -5.7, h: 2.8, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 10, m: 2, g: 2.1, h: -0.2, g_dot: 0.0, h_dot: -0.1 },
    MagneticCoeffEntry { n: 10, m: 3, g: -6.0, h: 4.0, g_dot: 0.1, h_dot: -0.1 },
    MagneticCoeffEntry { n: 10, m: 4, g: -0.8, h: -0.5, g_dot: 0.0, h_dot: 0.1 },
    MagneticCoeffEntry { n: 10, m: 5, g: 3.9, h: 4.8, g_dot: 0.0, h_dot: -0.1 },
    MagneticCoeffEntry { n: 10, m: 6, g: 0.4, h: -1.7, g_dot: 0.1, h_dot: 0.0 },
    MagneticCoeffEntry { n: 10, m: 7, g: 2.5, h: -0.8, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 10, m: 8, g: 2.8, h: 2.7, g_dot: 0.0, h_dot: -0.1 },
    MagneticCoeffEntry { n: 10, m: 9, g: 2.4, h: -3.9, g_dot: 0.0, h_dot: -0.1 },
    MagneticCoeffEntry { n: 10, m: 10, g: -2.1, h: -3.8, g_dot: -0.1, h_dot: 0.0 },
    // Degree 11
    MagneticCoeffEntry { n: 11, m: 0, g: 3.0, h: 0.0, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 1, g: -1.4, h: -1.2, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 2, g: -2.4, h: 2.3, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 3, g: 2.0, h: -1.9, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 4, g: -1.0, h: -1.3, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 5, g: 0.2, h: 0.9, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 6, g: 0.8, h: -0.5, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 7, g: -0.2, h: -0.3, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 8, g: 1.4, h: -0.7, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 9, g: -0.5, h: 0.7, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 10, g: 0.3, h: -0.2, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 11, m: 11, g: -0.7, h: -1.1, g_dot: 0.0, h_dot: 0.0 },
    // Degree 12
    MagneticCoeffEntry { n: 12, m: 0, g: -2.2, h: 0.0, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 1, g: -0.3, h: -0.8, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 2, g: 0.4, h: 0.4, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 3, g: 1.1, h: 1.6, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 4, g: -0.4, h: -0.4, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 5, g: 0.8, h: 0.1, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 6, g: 0.0, h: -0.7, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 7, g: 0.3, h: 0.4, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 8, g: -0.2, h: 0.2, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 9, g: 0.1, h: 0.2, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 10, g: -0.7, h: -0.3, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 11, g: -0.2, h: 0.4, g_dot: 0.0, h_dot: 0.0 },
    MagneticCoeffEntry { n: 12, m: 12, g: 0.2, h: -0.9, g_dot: 0.0, h_dot: 0.0 },
];

/// Future-proof spherical harmonic Magnetic Model computational engine.
///
/// Supports built-in WMM-2025 as well as dynamic loading of future models
/// (e.g. WMM-2030, WMM-2035, custom regional models) from standard `WMM.COF` files or strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MagneticModel {
    coeffs: MagneticCoefficients,
}

impl Default for MagneticModel {
    fn default() -> Self {
        Self::wmm2025()
    }
}

impl MagneticModel {
    /// Creates a magnetic model from a `MagneticCoefficients` collection.
    pub fn new(coeffs: MagneticCoefficients) -> Self {
        Self { coeffs }
    }

    /// Creates a default magnetic model initialized with built-in WMM-2025 coefficients.
    pub fn wmm2025() -> Self {
        Self {
            coeffs: MagneticCoefficients {
                epoch: WMM_BASE_EPOCH,
                model_name: "WMM-2025".to_string(),
                release_date: "11/20/2024".to_string(),
                max_degree: WMM_MAX_DEGREE,
                entries: WMM2025_COEFFS.to_vec(),
            },
        }
    }

    /// Loads a magnetic model from a `WMM.COF` formatted string.
    pub fn from_cof_str(cof_content: &str) -> Result<Self, GeodesyError> {
        let coeffs = MagneticCoefficients::from_cof_str(cof_content)?;
        Ok(Self::new(coeffs))
    }

    /// Loads a magnetic model from a `WMM.COF` formatted file on the filesystem.
    pub fn from_cof_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, GeodesyError> {
        let coeffs = MagneticCoefficients::from_cof_file(path)?;
        Ok(Self::new(coeffs))
    }

    /// Returns the model base epoch in decimal years.
    #[inline]
    pub fn epoch(&self) -> f64 {
        self.coeffs.epoch
    }

    /// Returns the model identifier name.
    #[inline]
    pub fn model_name(&self) -> &str {
        &self.coeffs.model_name
    }

    /// Returns the release date string.
    #[inline]
    pub fn release_date(&self) -> &str {
        &self.coeffs.release_date
    }

    /// Returns the maximum degree of the spherical harmonic model.
    #[inline]
    pub fn max_degree(&self) -> usize {
        self.coeffs.max_degree
    }

    /// Returns a reference to the inner coefficients.
    #[inline]
    pub fn coefficients(&self) -> &MagneticCoefficients {
        &self.coeffs
    }

    /// Computes all magnetic field elements for a given WGS84 geodetic coordinate and decimal epoch.
    ///
    /// # Arguments
    /// * `point` - Geodetic coordinate (lat, lon in radians, height in meters above WGS84 ellipsoid).
    /// * `epoch_decimal_year` - Time in decimal years (e.g. 2025.0, 2026.5).
    pub fn get_magnetic_elements(&self, point: &LatLon, epoch_decimal_year: f64) -> MagneticElements {
        let ell = Ellipsoid::wgs84();
        let dt = epoch_decimal_year - self.coeffs.epoch;
        let max_degree = self.coeffs.max_degree;

        // Convert geodetic coordinates to geocentric spherical coordinates
        let sin_lat = point.lat.sin();
        let cos_lat = point.lat.cos();
        let sin_lat_sq = sin_lat * sin_lat;

        // Radius of curvature in prime vertical
        let n = ell.a / (1.0 - ell.e_sq * sin_lat_sq).sqrt();
        let p = (n + point.height) * cos_lat;
        let z = (n * (1.0 - ell.e_sq) + point.height) * sin_lat;
        let r = p.hypot(z);

        // Geocentric latitude and colatitude
        let lat_geocentric = (z / p).atan();
        let sin_theta = (p / r).clamp(0.0, 1.0);
        let cos_theta = (z / r).clamp(-1.0, 1.0);

        // Angle between geodetic and geocentric normal: Δϕ = ϕ_geodetic - ϕ_geocentric
        let delta_phi = point.lat - lat_geocentric;

        // Precompute powers of (a / r)
        let a_over_r = WMM_EARTH_RADIUS_METERS / r;
        let mut a_over_r_pow = vec![0.0; max_degree + 3];
        a_over_r_pow[0] = 1.0;
        a_over_r_pow[1] = a_over_r;
        for i in 2..=max_degree + 2 {
            a_over_r_pow[i] = a_over_r_pow[i - 1] * a_over_r;
        }

        // Precompute sin(m * lon) and cos(m * lon)
        let mut sin_m_lon = vec![0.0; max_degree + 1];
        let mut cos_m_lon = vec![0.0; max_degree + 1];
        for m in 0..=max_degree {
            let angle = m as f64 * point.lon;
            sin_m_lon[m] = angle.sin();
            cos_m_lon[m] = angle.cos();
        }

        // Compute Schmidt semi-normalized Associated Legendre Polynomials P_n^m and derivatives dP_n^m / dtheta
        // Indexing: idx(n, m) = n*(n+1)/2 + m
        let num_poly = (max_degree + 1) * (max_degree + 2) / 2;
        let mut p_nm = vec![0.0; num_poly];
        let mut dp_nm = vec![0.0; num_poly];

        // P_0^0 = 1
        p_nm[0] = 1.0;
        dp_nm[0] = 0.0;

        for n in 1..=max_degree {
            for m in 0..=n {
                let idx = n * (n + 1) / 2 + m;
                if n == m {
                    let prev_idx = (n - 1) * n / 2 + (n - 1);
                    if n == 1 {
                        p_nm[idx] = sin_theta;
                        dp_nm[idx] = cos_theta;
                    } else {
                        let factor = ((2 * n - 1) as f64 / (2 * n) as f64).sqrt();
                        p_nm[idx] = factor * sin_theta * p_nm[prev_idx];
                        dp_nm[idx] = factor * (sin_theta * dp_nm[prev_idx] + cos_theta * p_nm[prev_idx]);
                    }
                } else if n == 1 && m == 0 {
                    p_nm[idx] = cos_theta;
                    dp_nm[idx] = -sin_theta;
                } else if m == 0 {
                    let idx_n1 = (n - 1) * n / 2;
                    let idx_n2 = (n - 2) * (n - 1) / 2;
                    let f1 = (2 * n - 1) as f64 / n as f64;
                    let f2 = (n - 1) as f64 / n as f64;
                    p_nm[idx] = f1 * cos_theta * p_nm[idx_n1] - f2 * p_nm[idx_n2];
                    dp_nm[idx] = f1 * (cos_theta * dp_nm[idx_n1] - sin_theta * p_nm[idx_n1]) - f2 * dp_nm[idx_n2];
                } else {
                    let idx_n1 = (n - 1) * n / 2 + m;
                    let idx_n2 = (n - 2) * (n - 1) / 2 + m;

                    let k_nm = (2 * n - 1) as f64 / ((n * n - m * m) as f64).sqrt();
                    let l_nm = if n > m + 1 {
                        (((n - 1) * (n - 1) - m * m) as f64 / (n * n - m * m) as f64).sqrt()
                    } else {
                        0.0
                    };

                    p_nm[idx] = k_nm * cos_theta * p_nm[idx_n1] - l_nm * p_nm[idx_n2];
                    dp_nm[idx] = k_nm * (cos_theta * dp_nm[idx_n1] - sin_theta * p_nm[idx_n1]) - l_nm * dp_nm[idx_n2];
                }
            }
        }

        // Spherical harmonic field summation in geocentric frame (X', Y', Z')
        // X' = + 1/r dV/dtheta (North)
        // Y' = - 1/(r sin theta) dV/dlambda (East)
        // Z' = + dV/dr (Down)
        let mut x_prime = 0.0;
        let mut y_prime = 0.0;
        let mut z_prime = 0.0;

        for coeff in &self.coeffs.entries {
            let n = coeff.n;
            let m = coeff.m;
            if n > max_degree {
                continue;
            }
            let idx = n * (n + 1) / 2 + m;

            let g_val = coeff.g + dt * coeff.g_dot;
            let h_val = coeff.h + dt * coeff.h_dot;

            let gh_cos_sin = g_val * cos_m_lon[m] + h_val * sin_m_lon[m];
            let gh_sin_cos = g_val * sin_m_lon[m] - h_val * cos_m_lon[m];

            let ratio = a_over_r_pow[n + 2];

            // X' = + (1/r) dV/dtheta = ratio * (g cos(m lambda) + h sin(m lambda)) * dP/dtheta
            x_prime += ratio * gh_cos_sin * dp_nm[idx];

            // Z' = dV/dr = - (n+1) * ratio * (g cos(m lambda) + h sin(m lambda)) * P
            z_prime -= (n + 1) as f64 * ratio * gh_cos_sin * p_nm[idx];

            // Y' = - (1/(r sin theta)) dV/dlambda = (m / sin theta) * ratio * (g sin(m lambda) - h cos(m lambda)) * P
            if m > 0 {
                if sin_theta > 1e-10 {
                    y_prime += (m as f64 / sin_theta) * ratio * gh_sin_cos * p_nm[idx];
                } else {
                    // Polar limit: (P_n^m / sin theta) -> dP_n^m / dtheta * cos_theta
                    y_prime += (m as f64) * ratio * gh_sin_cos * dp_nm[idx] * cos_theta;
                }
            }
        }

        // Rotate from geocentric (X', Y', Z') to geodetic (X, Y, Z)
        let cos_dphi = delta_phi.cos();
        let sin_dphi = delta_phi.sin();

        let x = x_prime * cos_dphi - z_prime * sin_dphi;
        let y = y_prime;
        let z = x_prime * sin_dphi + z_prime * cos_dphi;

        let h = x.hypot(y);
        let f = h.hypot(z);
        let declination = y.atan2(x);
        let inclination = z.atan2(h);

        MagneticElements {
            declination_rad: declination,
            inclination_rad: inclination,
            horizontal_intensity_nt: h,
            total_intensity_nt: f,
            x_nt: x,
            y_nt: y,
            z_nt: z,
        }
    }

    /// Computes magnetic declination (variation) in radians for given WGS84 coordinate and epoch.
    #[inline]
    pub fn get_declination(&self, point: &LatLon, epoch_decimal_year: f64) -> f64 {
        self.get_magnetic_elements(point, epoch_decimal_year).declination_rad
    }

    /// Converts a True North bearing (radians) to a Magnetic North bearing (radians).
    pub fn true_to_magnetic(&self, true_bearing_rad: f64, point: &LatLon, epoch_decimal_year: f64) -> f64 {
        let declination = self.get_declination(point, epoch_decimal_year);
        normalize_bearing(true_bearing_rad - declination)
    }

    /// Converts a Magnetic North bearing (radians) to a True North bearing (radians).
    pub fn magnetic_to_true(&self, mag_bearing_rad: f64, point: &LatLon, epoch_decimal_year: f64) -> f64 {
        let declination = self.get_declination(point, epoch_decimal_year);
        normalize_bearing(mag_bearing_rad + declination)
    }

    // --- Static convenience delegates (using default WMM-2025 model) ---

    /// Static convenience method: computes magnetic elements using the built-in WMM-2025 model.
    #[inline]
    pub fn get_default_elements(point: &LatLon, epoch_decimal_year: f64) -> MagneticElements {
        Self::wmm2025().get_magnetic_elements(point, epoch_decimal_year)
    }

    /// Static convenience method: computes magnetic declination using the built-in WMM-2025 model.
    #[inline]
    pub fn get_default_declination(point: &LatLon, epoch_decimal_year: f64) -> f64 {
        Self::wmm2025().get_declination(point, epoch_decimal_year)
    }
}
