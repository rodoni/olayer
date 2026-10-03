use crate::geodesy::coords::LatLon;
use crate::geodesy::spatial::GeodesicPolygon;
use crate::weather::errors::WeatherError;
use serde::{Deserialize, Serialize};

fn optional_property_f64(
    properties: &serde_json::Map<String, serde_json::Value>,
    names: &[&str],
) -> Result<Option<f64>, WeatherError> {
    let Some(value) = names.iter().find_map(|name| properties.get(*name)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_f64()
        .map(Some)
        .ok_or_else(|| WeatherError::ParseError("SIGMET numeric property has invalid type".into()))
}

fn optional_property_i64(
    properties: &serde_json::Map<String, serde_json::Value>,
    names: &[&str],
) -> Result<Option<i64>, WeatherError> {
    let Some(value) = names.iter().find_map(|name| properties.get(*name)) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value.as_i64().map(Some).ok_or_else(|| {
        WeatherError::ParseError("SIGMET timestamp property has invalid type".into())
    })
}

/// Type of meteorological warning / hazard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SigmetHazardType {
    Thunderstorm,
    Turbulence,
    Icing,
    VolcanicAsh,
    TropicalCyclone,
    MountainWave,
    Duststorm,
    Other,
}

impl SigmetHazardType {
    pub fn from_str_name(name: &str) -> Self {
        let name = name.trim();
        if ["TS", "THUNDERSTORM", "CONVECTIVE", "EMBD_TS"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::Thunderstorm
        } else if ["TURB", "TURBULENCE", "SEV_TURB", "CAT"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::Turbulence
        } else if ["ICE", "ICING", "SEV_ICE"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::Icing
        } else if ["VA", "VOLCANIC_ASH", "ASH"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::VolcanicAsh
        } else if ["TC", "TROPICAL_CYCLONE", "HURRICANE", "TYPHOON"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::TropicalCyclone
        } else if ["MTW", "MOUNTAIN_WAVE"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::MountainWave
        } else if ["DS", "DUSTSTORM", "SANDSTORM"]
            .iter()
            .any(|alias| name.eq_ignore_ascii_case(alias))
        {
            Self::Duststorm
        } else {
            Self::Other
        }
    }
}

/// Hazard severity rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SigmetSeverity {
    Moderate,
    Severe,
}

impl SigmetSeverity {
    pub fn from_str_name(name: &str) -> Self {
        let name = name.trim();
        if name.eq_ignore_ascii_case("SEV") || name.eq_ignore_ascii_case("SEVERE") {
            Self::Severe
        } else {
            Self::Moderate
        }
    }
}

/// An individual SIGMET / AIRMET meteorological warning polygon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SigmetFeature {
    pub id: String,
    pub name: String,
    pub hazard_type: SigmetHazardType,
    pub severity: SigmetSeverity,
    pub polygon: Vec<LatLon>,
    pub floor_m: Option<f64>,
    pub ceiling_m: Option<f64>,
    pub valid_from_epoch_s: Option<i64>,
    pub valid_until_epoch_s: Option<i64>,
}

impl SigmetFeature {
    /// Returns true if the geodetic 2D coordinate is within the hazard polygon footprint.
    pub fn contains_2d(&self, point: &LatLon) -> bool {
        if self.polygon.len() < 4
            || self.polygon.first() != self.polygon.last()
            || point.validate().is_err()
            || self.polygon.iter().any(|vertex| vertex.validate().is_err())
        {
            return false;
        }
        GeodesicPolygon::contains_point_vertices(&self.polygon, point)
    }

    /// Returns true if the 3D position (latitude, longitude, altitude) is inside the hazard volume.
    pub fn contains_3d(&self, point: &LatLon) -> bool {
        if !point.lat.is_finite()
            || !point.lon.is_finite()
            || !point.height.is_finite()
            || self.floor_m.is_some_and(|value| !value.is_finite())
            || self.ceiling_m.is_some_and(|value| !value.is_finite())
            || matches!((self.floor_m, self.ceiling_m), (Some(floor), Some(ceiling)) if floor > ceiling)
        {
            return false;
        }
        if !self.contains_2d(point) {
            return false;
        }
        if let Some(floor) = self.floor_m {
            if point.height < floor {
                return false;
            }
        }
        if let Some(ceil) = self.ceiling_m {
            if point.height > ceil {
                return false;
            }
        }
        true
    }
}

/// Container dataset for active SIGMET and AIRMET warning features.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SigmetDataset {
    pub features: Vec<SigmetFeature>,
}

impl SigmetDataset {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
        }
    }

    pub fn add_feature(&mut self, feature: SigmetFeature) {
        self.features.push(feature);
    }

    pub fn try_add_feature(&mut self, feature: SigmetFeature) -> Result<(), WeatherError> {
        validate_feature(&feature)?;
        self.features.push(feature);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    /// Finds all hazard features containing a 2D or 3D point.
    pub fn find_hazards_at_point(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        alt_m: Option<f64>,
    ) -> Vec<&SigmetFeature> {
        let pt = LatLon::new(lat_rad, lon_rad, alt_m.unwrap_or(0.0));
        self.features
            .iter()
            .filter(|f| {
                if alt_m.is_some() {
                    f.contains_3d(&pt)
                } else {
                    f.contains_2d(&pt)
                }
            })
            .collect()
    }

    /// Parses SIGMET features from a standard GeoJSON FeatureCollection string.
    ///
    /// # Errors
    /// Returns [`WeatherError::ParseError`] for malformed GeoJSON, unsupported
    /// geometry, missing typed properties, malformed coordinates, or invalid
    /// altitude/timestamp values.
    pub fn from_geojson(geojson_str: &str) -> Result<Self, WeatherError> {
        let parsed: serde_json::Value = serde_json::from_str(geojson_str)
            .map_err(|e| WeatherError::ParseError(format!("Invalid GeoJSON: {e}")))?;

        if parsed.get("type").and_then(|value| value.as_str()) != Some("FeatureCollection") {
            return Err(WeatherError::ParseError(
                "GeoJSON type must be FeatureCollection".to_string(),
            ));
        }

        let mut dataset = Self::new();
        let features = parsed
            .get("features")
            .and_then(|f| f.as_array())
            .ok_or_else(|| {
                WeatherError::ParseError("Missing 'features' array in GeoJSON".to_string())
            })?;

        for feat in features {
            if feat.get("type").and_then(|v| v.as_str()) != Some("Feature") {
                return Err(WeatherError::ParseError(
                    "SIGMET member must be a Feature".into(),
                ));
            }
            let props = feat.get("properties").and_then(|p| p.as_object());
            let geom = feat.get("geometry").and_then(|g| g.as_object());
            let props = props.ok_or_else(|| {
                WeatherError::ParseError("SIGMET feature is missing properties".into())
            })?;
            let geom = geom.ok_or_else(|| {
                WeatherError::ParseError("SIGMET feature is missing geometry".into())
            })?;

            {
                let geom_type = geom.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if geom_type != "Polygon" {
                    return Err(WeatherError::ParseError(format!(
                        "unsupported geometry type: {geom_type}"
                    )));
                }

                let id = props
                    .get("id")
                    .or_else(|| props.get("uid"))
                    .or_else(|| props.get("sigmet_id"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| WeatherError::ParseError("SIGMET id is required".into()))?
                    .to_string();

                let name = props
                    .get("name")
                    .or_else(|| props.get("hazard"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| WeatherError::ParseError("SIGMET name is required".into()))?
                    .to_string();

                let hazard_str = props
                    .get("hazard_type")
                    .or_else(|| props.get("type"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        WeatherError::ParseError("SIGMET hazard_type is required".into())
                    })?;
                let hazard_type = SigmetHazardType::from_str_name(hazard_str);

                let severity_str =
                    props
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            WeatherError::ParseError("SIGMET severity is required".into())
                        })?;
                let severity = SigmetSeverity::from_str_name(severity_str);
                if !matches!(
                    severity_str.trim().to_ascii_uppercase().as_str(),
                    "MODERATE" | "MOD" | "SEVERE" | "SEV"
                ) {
                    return Err(WeatherError::ParseError("invalid SIGMET severity".into()));
                }

                let floor_m = optional_property_f64(props, &["lower_limit_m", "floor_m"])?;
                let ceiling_m = optional_property_f64(props, &["upper_limit_m", "ceiling_m"])?;
                let valid_from_epoch_s =
                    optional_property_i64(props, &["valid_from_epoch_s", "valid_from"])?;
                let valid_until_epoch_s =
                    optional_property_i64(props, &["valid_until_epoch_s", "valid_until"])?;
                if floor_m.is_some_and(|v| !v.is_finite())
                    || ceiling_m.is_some_and(|v| !v.is_finite())
                    || matches!((floor_m, ceiling_m), (Some(floor), Some(ceiling)) if floor > ceiling)
                {
                    return Err(WeatherError::ParseError(
                        "invalid altitude bounds".to_string(),
                    ));
                }

                let coords_arr = geom
                    .get("coordinates")
                    .and_then(|c| c.as_array())
                    .and_then(|rings| rings.first())
                    .and_then(|outer| outer.as_array());

                let ring = coords_arr.ok_or_else(|| {
                    WeatherError::ParseError("SIGMET polygon is missing coordinates".into())
                })?;
                if ring.len() < 4 {
                    return Err(WeatherError::ParseError(
                        "SIGMET polygon requires a closed ring with at least four positions".into(),
                    ));
                }
                {
                    if geom
                        .get("coordinates")
                        .and_then(|c| c.as_array())
                        .is_some_and(|rings| rings.len() > 1)
                    {
                        return Err(WeatherError::ParseError(
                            "polygon holes are unsupported".to_string(),
                        ));
                    }
                    let mut polygon = Vec::with_capacity(ring.len());
                    for pt in ring {
                        let pt_arr = pt.as_array().ok_or_else(|| {
                            WeatherError::ParseError("invalid coordinate point".to_string())
                        })?;
                        if pt_arr.len() < 2 {
                            return Err(WeatherError::ParseError(
                                "coordinate requires longitude and latitude".to_string(),
                            ));
                        }
                        let lon_deg = pt_arr[0].as_f64().ok_or_else(|| {
                            WeatherError::ParseError("invalid longitude".to_string())
                        })?;
                        let lat_deg = pt_arr[1].as_f64().ok_or_else(|| {
                            WeatherError::ParseError("invalid latitude".to_string())
                        })?;
                        let alt_m = pt_arr.get(2).and_then(|a| a.as_f64()).unwrap_or(0.0);
                        if !lon_deg.is_finite()
                            || !lat_deg.is_finite()
                            || !alt_m.is_finite()
                            || !(-180.0..=180.0).contains(&lon_deg)
                            || !(-90.0..=90.0).contains(&lat_deg)
                        {
                            return Err(WeatherError::ParseError(
                                "invalid coordinate range".to_string(),
                            ));
                        }
                        polygon.push(LatLon::from_degrees(lat_deg, lon_deg, alt_m));
                    }

                    if polygon.first() != polygon.last() {
                        return Err(WeatherError::ParseError(
                            "SIGMET polygon ring must be closed".into(),
                        ));
                    }
                    let feature = SigmetFeature {
                        id,
                        name,
                        hazard_type,
                        severity,
                        polygon,
                        floor_m,
                        ceiling_m,
                        valid_from_epoch_s,
                        valid_until_epoch_s,
                    };
                    validate_feature(&feature)?;
                    dataset.add_feature(feature);
                }
            }
        }

        Ok(dataset)
    }

    /// Serializes dataset to a standard GeoJSON FeatureCollection string.
    ///
    /// # Errors
    /// Returns [`WeatherError::ParseError`] when a feature violates the SIGMET
    /// validation contract or JSON serialization fails.
    pub fn to_geojson(&self) -> Result<String, WeatherError> {
        let mut features_json = Vec::with_capacity(self.features.len());

        for feat in &self.features {
            validate_feature(feat)?;
            let coords: Vec<Vec<f64>> = feat
                .polygon
                .iter()
                .map(|p| vec![p.lon.to_degrees(), p.lat.to_degrees(), p.height])
                .collect();

            let f_val = serde_json::json!({
                "type": "Feature",
                "properties": {
                    "id": feat.id,
                    "name": feat.name,
                    "hazard_type": feat.hazard_type,
                    "severity": feat.severity,
                    "floor_m": feat.floor_m,
                    "ceiling_m": feat.ceiling_m
                    ,"valid_from_epoch_s": feat.valid_from_epoch_s,
                    "valid_until_epoch_s": feat.valid_until_epoch_s
                },
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [coords]
                }
            });
            features_json.push(f_val);
        }

        let fc = serde_json::json!({
            "type": "FeatureCollection",
            "features": features_json
        });

        serde_json::to_string(&fc)
            .map_err(|e| WeatherError::ParseError(format!("Failed to serialize GeoJSON: {e}")))
    }
}

fn validate_feature(feature: &SigmetFeature) -> Result<(), WeatherError> {
    if feature.id.trim().is_empty() || feature.name.trim().is_empty() {
        return Err(WeatherError::ParseError(
            "SIGMET id and name are required".into(),
        ));
    }
    if feature.polygon.len() < 4 || feature.polygon.first() != feature.polygon.last() {
        return Err(WeatherError::ParseError(
            "SIGMET polygon must be a closed ring with at least four positions".to_string(),
        ));
    }
    if feature
        .polygon
        .iter()
        .any(|point| point.validate().is_err())
    {
        return Err(WeatherError::ParseError(
            "SIGMET polygon contains invalid coordinates".to_string(),
        ));
    }
    if feature.floor_m.is_some_and(|value| !value.is_finite())
        || feature.ceiling_m.is_some_and(|value| !value.is_finite())
        || matches!((feature.floor_m, feature.ceiling_m), (Some(floor), Some(ceiling)) if floor > ceiling)
        || matches!((feature.valid_from_epoch_s, feature.valid_until_epoch_s), (Some(from), Some(until)) if from > until)
    {
        return Err(WeatherError::ParseError(
            "SIGMET feature contains invalid bounds".to_string(),
        ));
    }
    Ok(())
}
