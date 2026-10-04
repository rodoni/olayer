use serde::Serialize;
use std::fmt;

/// Stable, machine-readable identifier for a public Olayer Core error.
///
/// Codes are part of the public compatibility contract. Once published, a
/// code must never be reassigned to a different error condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Creates a code from a static catalog entry.
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    /// Returns the canonical code string, for example `OL-CORE-GEO-0001`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// Returns the module namespace encoded in this code.
    #[must_use]
    pub fn module(self) -> &'static str {
        const PREFIX: &str = "OL-CORE-";
        let Some(rest) = self.0.strip_prefix(PREFIX) else {
            return "unknown";
        };
        rest.split_once('-').map_or("unknown", |(module, _)| module)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;
    use crate::{
        aeronautical::AeronauticalError, camera::CameraError, geodesy::GeodesyError,
        interpolator::InterpolatorError, projections::ProjectionError, sld::SldError,
        symbol_registry::SymbologyError, terrain::TerrainError, volumetric::VolumetricError,
        weather::WeatherError,
    };
    use std::collections::HashSet;

    #[test]
    fn code_displays_and_exposes_module_namespace() {
        let code = ErrorCode::new("OL-CORE-GEO-0001");
        assert_eq!(code.to_string(), "OL-CORE-GEO-0001");
        assert_eq!(code.as_str(), "OL-CORE-GEO-0001");
        assert_eq!(code.module(), "GEO");
    }

    #[test]
    fn every_public_core_error_has_a_globally_unique_well_formed_code() {
        let codes = [
            GeodesyError::NonFiniteLatitude.code(),
            GeodesyError::NonFiniteLongitude.code(),
            GeodesyError::NonFiniteHeight.code(),
            GeodesyError::LatitudeOutOfRange(0.0).code(),
            GeodesyError::LongitudeOutOfRange(0.0).code(),
            GeodesyError::InvalidHeight(0.0).code(),
            GeodesyError::InvalidSemiMajorAxis(0.0).code(),
            GeodesyError::InvalidFlattening(0.0).code(),
            GeodesyError::InvalidEcef.code(),
            GeodesyError::NonFiniteBearing.code(),
            GeodesyError::NonFiniteDistance.code(),
            GeodesyError::MagneticModelError(String::new()).code(),
            ProjectionError::InvalidCameraState.code(),
            ProjectionError::Singularity.code(),
            ProjectionError::ConvergenceFailed.code(),
            ProjectionError::InvalidInput.code(),
            ProjectionError::InvalidParameters.code(),
            CameraError::InvalidCenter.code(),
            CameraError::InvalidAttitude.code(),
            CameraError::InvalidZoom.code(),
            CameraError::InvalidAspectRatio.code(),
            CameraError::InvalidViewportBase.code(),
            CameraError::InvalidProjectionValue { name: "test" }.code(),
            CameraError::Projection(ProjectionError::InvalidInput).code(),
            TerrainError::InvalidHeader(String::new()).code(),
            TerrainError::MalformedData(String::new()).code(),
            TerrainError::TileNotLoaded(0, 0).code(),
            TerrainError::RgbDecodeError(String::new()).code(),
            TerrainError::GeoTiffError(String::new()).code(),
            TerrainError::InvalidInput(String::new()).code(),
            TerrainError::AltitudeError(String::new()).code(),
            AeronauticalError::XmlParseError(String::new()).code(),
            AeronauticalError::JsonParseError(String::new()).code(),
            AeronauticalError::MissingRequiredField(String::new()).code(),
            AeronauticalError::InvalidCoordinateString(String::new()).code(),
            AeronauticalError::InvalidAltitude(String::new()).code(),
            AeronauticalError::EmptyDataset(String::new()).code(),
            AeronauticalError::FormatError(String::new()).code(),
            WeatherError::InvalidGridDimensions {
                width: 0,
                height: 0,
                actual_len: 0,
            }
            .code(),
            WeatherError::InvalidIsovalues(String::new()).code(),
            WeatherError::InvalidWindParameters(String::new()).code(),
            WeatherError::ParseError(String::new()).code(),
            SldError::XmlError(String::new()).code(),
            SldError::InvalidValue(String::new()).code(),
            SldError::DuplicateLayer(String::new()).code(),
            SymbologyError::ProviderNotFound.code(),
            SymbologyError::SymbolNotFound(String::new()).code(),
            SymbologyError::InvalidFormat(String::new()).code(),
            InterpolatorError::InvalidState(String::new()).code(),
            InterpolatorError::GeodesyFailure(GeodesyError::InvalidEcef).code(),
            VolumetricError::InsufficientVertices {
                expected: 3,
                actual: 0,
            }
            .code(),
            VolumetricError::InvalidAltitudeBounds {
                floor_m: 0.0,
                ceiling_m: 1.0,
            }
            .code(),
            VolumetricError::TriangulationFailed(String::new()).code(),
            VolumetricError::InvalidRibbonParameters(String::new()).code(),
            VolumetricError::DegenerateGeometry(String::new()).code(),
        ];

        let unique = codes
            .iter()
            .map(|code| code.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(
            unique.len(),
            codes.len(),
            "error codes must be globally unique"
        );
        for code in codes {
            let value = code.as_str();
            assert!(value.starts_with("OL-CORE-"), "invalid prefix: {value}");
            let Some((module, sequence)) = value[8..].split_once('-') else {
                panic!("missing module/sequence separator in {value}");
            };
            assert_eq!(module.len(), 3, "invalid module namespace in {value}");
            assert_eq!(sequence.len(), 4, "invalid sequence length in {value}");
            assert!(sequence.bytes().all(|digit| digit.is_ascii_digit()));
        }
    }

    #[test]
    fn code_serializes_as_its_canonical_string() {
        let code = ErrorCode::new("OL-CORE-TRN-0001");
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            "\"OL-CORE-TRN-0001\""
        );
    }
}
