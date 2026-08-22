use serde::{Deserialize, Serialize};
use crate::geodesy::coords::LatLon;
use crate::geodesy::spatial::GeodesicPolygon;
use crate::weather::errors::WeatherError;

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
        match name.trim().to_uppercase().as_str() {
            "TS" | "THUNDERSTORM" | "CONVECTIVE" | "EMBD_TS" => Self::Thunderstorm,
            "TURB" | "TURBULENCE" | "SEV_TURB" | "CAT" => Self::Turbulence,
            "ICE" | "ICING" | "SEV_ICE" => Self::Icing,
            "VA" | "VOLCANIC_ASH" | "ASH" => Self::VolcanicAsh,
            "TC" | "TROPICAL_CYCLONE" | "HURRICANE" | "TYPHOON" => Self::TropicalCyclone,
            "MTW" | "MOUNTAIN_WAVE" => Self::MountainWave,
            "DS" | "DUSTSTORM" | "SANDSTORM" => Self::Duststorm,
            _ => Self::Other,
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
        match name.trim().to_uppercase().as_str() {
            "SEV" | "SEVERE" => Self::Severe,
            _ => Self::Moderate,
        }
    }
}

/// An individual SIGMET / AIRMET meteorological warning polygon.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        if self.polygon.len() < 3 {
            return false;
        }
        GeodesicPolygon::new(self.polygon.clone()).contains_point(point)
    }

    /// Returns true if the 3D position (latitude, longitude, altitude) is inside the hazard volume.
    pub fn contains_3d(&self, point: &LatLon) -> bool {
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SigmetDataset {
    pub features: Vec<SigmetFeature>,
}

impl SigmetDataset {
    pub fn new() -> Self {
        Self { features: Vec::new() }
    }

    pub fn add_feature(&mut self, feature: SigmetFeature) {
        self.features.push(feature);
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    /// Finds all hazard features containing a 2D or 3D point.
    pub fn find_hazards_at_point(&self, lat_rad: f64, lon_rad: f64, alt_m: Option<f64>) -> Vec<&SigmetFeature> {
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
    pub fn from_geojson(geojson_str: &str) -> Result<Self, WeatherError> {
        let parsed: serde_json::Value = serde_json::from_str(geojson_str)
            .map_err(|e| WeatherError::ParseError(format!("Invalid GeoJSON: {e}")))?;

        let mut dataset = Self::new();
        let features = parsed.get("features")
            .and_then(|f| f.as_array())
            .ok_or_else(|| WeatherError::ParseError("Missing 'features' array in GeoJSON".to_string()))?;

        for feat in features {
            let props = feat.get("properties").and_then(|p| p.as_object());
            let geom = feat.get("geometry").and_then(|g| g.as_object());

            if let (Some(props), Some(geom)) = (props, geom) {
                let geom_type = geom.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if geom_type != "Polygon" {
                    continue;
                }

                let id = props.get("id")
                    .or_else(|| props.get("uid"))
                    .or_else(|| props.get("sigmet_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("SIGMET")
                    .to_string();

                let name = props.get("name")
                    .or_else(|| props.get("hazard"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string();

                let hazard_str = props.get("hazard_type")
                    .or_else(|| props.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("OTHER");
                let hazard_type = SigmetHazardType::from_str_name(hazard_str);

                let severity_str = props.get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MODERATE");
                let severity = SigmetSeverity::from_str_name(severity_str);

                let floor_m = props.get("lower_limit_m")
                    .or_else(|| props.get("floor_m"))
                    .and_then(|v| v.as_f64());

                let ceiling_m = props.get("upper_limit_m")
                    .or_else(|| props.get("ceiling_m"))
                    .and_then(|v| v.as_f64());

                let coords_arr = geom.get("coordinates")
                    .and_then(|c| c.as_array())
                    .and_then(|rings| rings.first())
                    .and_then(|outer| outer.as_array());

                if let Some(ring) = coords_arr {
                    let mut polygon = Vec::new();
                    for pt in ring {
                        if let Some(pt_arr) = pt.as_array() {
                            if pt_arr.len() >= 2 {
                                let lon_deg = pt_arr[0].as_f64().unwrap_or(0.0);
                                let lat_deg = pt_arr[1].as_f64().unwrap_or(0.0);
                                let alt_m = pt_arr.get(2).and_then(|a| a.as_f64()).unwrap_or(0.0);
                                polygon.push(LatLon::from_degrees(lat_deg, lon_deg, alt_m));
                            }
                        }
                    }

                    if polygon.len() >= 3 {
                        dataset.add_feature(SigmetFeature {
                            id,
                            name,
                            hazard_type,
                            severity,
                            polygon,
                            floor_m,
                            ceiling_m,
                            valid_from_epoch_s: None,
                            valid_until_epoch_s: None,
                        });
                    }
                }
            }
        }

        Ok(dataset)
    }

    /// Serializes dataset to a standard GeoJSON FeatureCollection string.
    pub fn to_geojson(&self) -> Result<String, WeatherError> {
        let mut features_json = Vec::new();

        for feat in &self.features {
            let coords: Vec<Vec<f64>> = feat.polygon
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
