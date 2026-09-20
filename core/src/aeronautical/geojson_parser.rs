use crate::aeronautical::dataset::AeronauticalDataset;
use crate::aeronautical::errors::AeronauticalError;
use crate::aeronautical::types::{
    AeronauticalAirport, AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid,
    AirspaceType, AirwaySegment, AirwayType, AltitudeLimit, NavaidType, SegmentDirection,
};
use crate::geodesy::coords::LatLon;
use serde_json::{json, Value};

/// Parses a GeoJSON-Aviation formatted string into an [`AeronauticalDataset`].
///
/// # Errors
/// Returns [`AeronauticalError`] when the JSON is malformed, is not a
/// `FeatureCollection`, or contains invalid coordinates for a recognized
/// aeronautical feature.
///
/// # Panics
/// This function does not panic for valid Rust inputs.
///
/// # Safety
/// This function does not use unsafe operations and requires no caller-held
/// memory invariants.
pub fn parse_geojson_aviation_str(
    json_str: &str,
) -> Result<AeronauticalDataset, AeronauticalError> {
    let root: Value = serde_json::from_str(json_str)
        .map_err(|e| AeronauticalError::JsonParseError(e.to_string()))?;

    let features = root
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AeronauticalError::JsonParseError("GeoJSON must contain a 'features' array".to_string())
        })?;

    if root.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
        return Err(AeronauticalError::JsonParseError(
            "GeoJSON root must have type 'FeatureCollection'".to_string(),
        ));
    }

    let mut dataset = AeronauticalDataset::new();

    for feature in features {
        if feature.get("type").and_then(Value::as_str) != Some("Feature") {
            return Err(AeronauticalError::JsonParseError(
                "each FeatureCollection item must have type 'Feature'".into(),
            ));
        }
        let properties = feature.get("properties").and_then(Value::as_object);
        let geometry = feature.get("geometry").and_then(Value::as_object);

        let geom_type = geometry
            .and_then(|g| g.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let coords_val = geometry.and_then(|g| g.get("coordinates"));

        let aero_type = properties
            .and_then(|p| {
                p.get("aero_type")
                    .or_else(|| p.get("feature_type"))
                    .or_else(|| p.get("type"))
            })
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    AeronauticalError::FormatError("aeronautical type must be a string".into())
                })
            })
            .transpose()?
            .unwrap_or("");

        let declared_kind = if aero_type.is_empty() {
            None
        } else if aero_type.eq_ignore_ascii_case("Airspace") {
            Some("Airspace")
        } else if aero_type.eq_ignore_ascii_case("Navaid") {
            Some("Navaid")
        } else if aero_type.eq_ignore_ascii_case("Airway") {
            Some("Airway")
        } else if aero_type.eq_ignore_ascii_case("Airport") {
            Some("Airport")
        } else {
            return Err(AeronauticalError::FormatError(format!(
                "unsupported aeronautical type: {aero_type}"
            )));
        };
        if let Some(kind) = declared_kind {
            let expected_geometry = match kind {
                "Airspace" => "Polygon",
                "Navaid" | "Airport" => "Point",
                "Airway" => "LineString",
                _ => unreachable!("declared aeronautical kind was validated above"),
            };
            if geom_type != expected_geometry {
                return Err(AeronauticalError::FormatError(format!(
                    "{kind} features require {expected_geometry} geometry"
                )));
            }
        }

        let recognized =
            declared_kind.is_some() || matches!(geom_type, "Point" | "LineString" | "Polygon");
        if recognized && properties.is_none() {
            return Err(AeronauticalError::MissingRequiredField("properties".into()));
        }

        if recognized {
            validate_feature_coordinates(geom_type, coords_val)?;
        }

        match declared_kind.unwrap_or(geom_type) {
            "Airspace" | "Polygon" => {
                if let Some(airspace) = parse_airspace_feature(properties, coords_val)? {
                    dataset.add_airspace(airspace);
                }
            }
            "Airport" => {
                dataset.add_airport(parse_airport_feature(properties, coords_val)?);
            }
            "Navaid" | "Point" => {
                dataset.add_navaid(parse_navaid_feature(properties, coords_val)?);
            }
            "Airway" | "LineString" => {
                dataset.add_airway(parse_airway_feature(properties, coords_val)?);
            }
            _ => {}
        }
    }

    Ok(dataset)
}

fn validate_feature_coordinates(
    geometry_type: &str,
    coordinates: Option<&Value>,
) -> Result<(), AeronauticalError> {
    let coordinates = coordinates.ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString(format!(
            "missing coordinates for {geometry_type}"
        ))
    })?;
    match geometry_type {
        "Point" => validate_position(coordinates),
        "LineString" => coordinates
            .as_array()
            .ok_or_else(|| {
                AeronauticalError::InvalidCoordinateString(
                    "LineString coordinates must be an array".into(),
                )
            })?
            .iter()
            .try_fold(0usize, |count, position| {
                validate_position(position)?;
                Ok(count + 1)
            })
            .and_then(|count| {
                if count < 2 {
                    Err(AeronauticalError::InvalidCoordinateString(
                        "LineString must contain at least two positions".into(),
                    ))
                } else {
                    Ok(())
                }
            }),
        "Polygon" => coordinates
            .as_array()
            .ok_or_else(|| {
                AeronauticalError::InvalidCoordinateString(
                    "Polygon coordinates must be an array".into(),
                )
            })?
            .iter()
            .try_for_each(|ring| {
                let ring = ring.as_array().ok_or_else(|| {
                    AeronauticalError::InvalidCoordinateString(
                        "Polygon ring must be an array".into(),
                    )
                })?;
                if ring.len() < 4 {
                    return Err(AeronauticalError::InvalidCoordinateString(
                        "Polygon ring must contain at least four positions".into(),
                    ));
                }
                if ring.first() != ring.last() {
                    return Err(AeronauticalError::InvalidCoordinateString(
                        "Polygon ring must be closed".into(),
                    ));
                }
                ring.iter().try_for_each(validate_position)
            }),
        _ => Err(AeronauticalError::FormatError(format!(
            "unsupported aeronautical geometry: {geometry_type}"
        ))),
    }
}

fn validate_position(position: &Value) -> Result<(), AeronauticalError> {
    let values = position.as_array().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("position must be an array".into())
    })?;
    if values.len() < 2 {
        return Err(AeronauticalError::InvalidCoordinateString(
            "position must contain longitude and latitude".into(),
        ));
    }
    let longitude = values[0].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("longitude must be numeric".into())
    })?;
    let latitude = values[1].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("latitude must be numeric".into())
    })?;
    if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "longitude is outside [-180, 180]".into(),
        ));
    }
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "latitude is outside [-90, 90]".into(),
        ));
    }
    if let Some(height) = values.get(2).and_then(Value::as_f64) {
        if !height.is_finite() {
            return Err(AeronauticalError::InvalidCoordinateString(
                "height must be finite".into(),
            ));
        }
    }
    Ok(())
}

fn parse_airspace_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Result<Option<AeronauticalAirspace>, AeronauticalError> {
    let Some(props) = properties else {
        return Ok(None);
    };
    let uid = typed_string(props, &["uid", "id"])?
        .unwrap_or("AIRSPACE")
        .to_string();
    let name = typed_string(props, &["name"])?.unwrap_or(&uid).to_string();

    let type_str = props
        .get("airspace_type")
        .or_else(|| props.get("sub_type"))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                AeronauticalError::FormatError("airspace type must be a string".into())
            })
        })
        .transpose()?
        .unwrap_or("SECTOR");
    let airspace_type = match type_str.to_uppercase().as_str() {
        "FIR" => AirspaceType::Fir,
        "UIR" => AirspaceType::Uir,
        "TMA" => AirspaceType::Tma,
        "CTR" => AirspaceType::Ctr,
        "SECTOR" => AirspaceType::Sector,
        "PROHIBITED" | "P" => AirspaceType::Prohibited,
        "RESTRICTED" | "R" => AirspaceType::Restricted,
        "DANGER" | "D" => AirspaceType::Danger,
        "MOA" => AirspaceType::Moa,
        "TRA" | "TSA" => AirspaceType::Tra,
        other => AirspaceType::Other(other.to_string()),
    };

    let lower_reference = optional_reference(props, "lower_limit_reference")?;
    let lower_value =
        optional_altitude(props, "lower_limit_m")?.or(optional_altitude(props, "lower_limit")?);
    let lower_fl = optional_flight_level(props, "lower_limit_fl")?;
    let lower_limit = if lower_reference
        == Some(crate::aeronautical::types::AltitudeReference::Ground)
    {
        AltitudeLimit::ground()
    } else if lower_reference == Some(crate::aeronautical::types::AltitudeReference::Uncapped) {
        AltitudeLimit::uncapped()
    } else if lower_reference == Some(crate::aeronautical::types::AltitudeReference::Agl) {
        AltitudeLimit::agl(lower_value.unwrap_or(0.0))?
    } else if let Some(fl) = lower_fl {
        AltitudeLimit::from_flight_level(fl)?
    } else if lower_reference == Some(crate::aeronautical::types::AltitudeReference::FlightLevel) {
        return Err(AeronauticalError::InvalidAltitude(
            "lower flight-level reference requires lower_limit_fl".into(),
        ));
    } else {
        AltitudeLimit::amsl(lower_value.unwrap_or(0.0))?
    };

    let upper_reference = optional_reference(props, "upper_limit_reference")?;
    let upper_value =
        optional_altitude(props, "upper_limit_m")?.or(optional_altitude(props, "upper_limit")?);
    let upper_fl = optional_flight_level(props, "upper_limit_fl")?;
    let upper_limit = if upper_reference
        == Some(crate::aeronautical::types::AltitudeReference::Ground)
    {
        AltitudeLimit::ground()
    } else if upper_reference == Some(crate::aeronautical::types::AltitudeReference::Uncapped) {
        AltitudeLimit::uncapped()
    } else if upper_reference == Some(crate::aeronautical::types::AltitudeReference::Agl) {
        AltitudeLimit::agl(upper_value.unwrap_or(0.0))?
    } else if let Some(fl) = upper_fl {
        AltitudeLimit::from_flight_level(fl)?
    } else if upper_reference == Some(crate::aeronautical::types::AltitudeReference::FlightLevel) {
        return Err(AeronauticalError::InvalidAltitude(
            "upper flight-level reference requires upper_limit_fl".into(),
        ));
    } else {
        AltitudeLimit::amsl(upper_value.unwrap_or(10000.0))?
    };

    let mut boundary = Vec::with_capacity(
        coordinates
            .and_then(Value::as_array)
            .and_then(|rings| rings.first())
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    );
    if let Some(rings) = coordinates.and_then(Value::as_array) {
        if rings.len() > 1 {
            return Err(AeronauticalError::FormatError(
                "Polygon holes are not supported".into(),
            ));
        }
        // First ring is exterior ring
        if let Some(exterior) = rings.first().and_then(Value::as_array) {
            for pt in exterior {
                if let Some(coords) = pt.as_array() {
                    if coords.len() >= 2 {
                        let lon_deg = coords[0].as_f64().ok_or_else(|| {
                            AeronauticalError::InvalidCoordinateString("longitude".into())
                        })?;
                        let lat_deg = coords[1].as_f64().ok_or_else(|| {
                            AeronauticalError::InvalidCoordinateString("latitude".into())
                        })?;
                        let height_m = match coords.get(2) {
                            Some(value) => value.as_f64().ok_or_else(|| {
                                AeronauticalError::InvalidCoordinateString("height".into())
                            })?,
                            None => 0.0,
                        };
                        boundary.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
                    }
                }
            }
        }
    }

    let airspace = AeronauticalAirspace {
        uid,
        name,
        airspace_type,
        lower_limit,
        upper_limit,
        boundary,
    };
    airspace.validate()?;
    Ok(Some(airspace))
}

fn optional_altitude(
    props: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<f64>, AeronauticalError> {
    let Some(value) = props.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let altitude = value
        .as_f64()
        .ok_or_else(|| AeronauticalError::InvalidAltitude(format!("{key} must be numeric")))?;
    if !altitude.is_finite() || altitude < 0.0 {
        return Err(AeronauticalError::InvalidAltitude(format!(
            "{key} must be finite and non-negative"
        )));
    }
    Ok(Some(altitude))
}

fn optional_flight_level(
    props: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<u32>, AeronauticalError> {
    let Some(value) = props.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let level = value.as_u64().ok_or_else(|| {
        AeronauticalError::InvalidAltitude(format!("{key} must be an unsigned integer"))
    })?;
    let level = u32::try_from(level)
        .map_err(|_| AeronauticalError::InvalidAltitude(format!("{key} is too large")))?;
    Ok(Some(level))
}

fn optional_reference(
    props: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<crate::aeronautical::types::AltitudeReference>, AeronauticalError> {
    let Some(value) = props.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let reference = value
        .as_str()
        .ok_or_else(|| AeronauticalError::InvalidAltitude(format!("{key} must be a string")))?;
    let reference = match reference.to_ascii_uppercase().as_str() {
        "AMSL" | "MSL" => crate::aeronautical::types::AltitudeReference::Amsl,
        "AGL" => crate::aeronautical::types::AltitudeReference::Agl,
        "FL" | "STD" | "FLIGHTLEVEL" => crate::aeronautical::types::AltitudeReference::FlightLevel,
        "GND" | "SFC" | "GROUND" => crate::aeronautical::types::AltitudeReference::Ground,
        "UNCAPPED" | "UNLIMITED" => crate::aeronautical::types::AltitudeReference::Uncapped,
        other => {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "unsupported {key}: {other}"
            )))
        }
    };
    Ok(Some(reference))
}

fn typed_string<'a>(
    props: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Result<Option<&'a str>, AeronauticalError> {
    for key in keys {
        if let Some(value) = props.get(*key).filter(|value| !value.is_null()) {
            return value
                .as_str()
                .map(Some)
                .ok_or_else(|| AeronauticalError::FormatError(format!("{key} must be a string")));
        }
    }
    Ok(None)
}

fn typed_f64(
    props: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Result<Option<f64>, AeronauticalError> {
    for key in keys {
        if let Some(value) = props.get(*key).filter(|value| !value.is_null()) {
            let number = value
                .as_f64()
                .ok_or_else(|| AeronauticalError::FormatError(format!("{key} must be numeric")))?;
            if !number.is_finite() {
                return Err(AeronauticalError::FormatError(format!(
                    "{key} must be finite"
                )));
            }
            return Ok(Some(number));
        }
    }
    Ok(None)
}

fn parse_navaid_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Result<AeronauticalNavaid, AeronauticalError> {
    let props =
        properties.ok_or_else(|| AeronauticalError::MissingRequiredField("properties".into()))?;
    let ident = typed_string(props, &["ident", "id"])?
        .unwrap_or("FIX")
        .to_string();
    let name = typed_string(props, &["name"])?
        .unwrap_or(&ident)
        .to_string();

    let type_str = props
        .get("navaid_type")
        .or_else(|| props.get("type"))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                AeronauticalError::FormatError("navaid type must be a string".into())
            })
        })
        .transpose()?
        .unwrap_or("WAYPOINT");
    let navaid_type = match type_str.to_uppercase().as_str() {
        "VOR" => NavaidType::Vor,
        "DME" => NavaidType::Dme,
        "VORDME" | "VOR_DME" | "VOR-DME" => NavaidType::VorDme,
        "TACAN" => NavaidType::Tacan,
        "VORTAC" => NavaidType::Vortac,
        "NDB" => NavaidType::Ndb,
        "FIX" | "INTERSECTION" => NavaidType::Fix,
        "WAYPOINT" => NavaidType::Waypoint,
        other => {
            return Err(AeronauticalError::FormatError(format!(
                "unsupported navaid type: {other}"
            )))
        }
    };

    let coords_arr = coordinates.and_then(Value::as_array).ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("Point coordinates must be an array".into())
    })?;
    if coords_arr.len() < 2 {
        return Err(AeronauticalError::InvalidCoordinateString(
            "Point must contain longitude and latitude".into(),
        ));
    }
    let lon_deg = coords_arr[0].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("longitude must be numeric".into())
    })?;
    let lat_deg = coords_arr[1].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("latitude must be numeric".into())
    })?;
    let height_m = if coords_arr.len() >= 3 {
        coords_arr[2].as_f64().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("height must be numeric".into())
        })?
    } else {
        0.0
    };
    let coords = LatLon::from_degrees(lat_deg, lon_deg, height_m);

    let frequency_mhz = typed_f64(props, &["frequency_mhz", "frequency"])?;
    let channel = typed_string(props, &["channel"])?.map(String::from);
    let elevation_m = typed_f64(props, &["elevation_m", "elevation"])?;
    let magnetic_variation_deg = typed_f64(props, &["magnetic_variation_deg", "mag_var"])?;

    let navaid = AeronauticalNavaid {
        ident,
        name,
        navaid_type,
        coords,
        frequency_mhz,
        channel,
        elevation_m,
        magnetic_variation_deg,
    };
    navaid.validate()?;
    Ok(navaid)
}

fn parse_airway_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Result<AeronauticalAirway, AeronauticalError> {
    let props =
        properties.ok_or_else(|| AeronauticalError::MissingRequiredField("properties".into()))?;
    let ident = typed_string(props, &["ident", "id"])?
        .unwrap_or("AIRWAY")
        .to_string();

    let type_str = props
        .get("route_type")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| AeronauticalError::FormatError("route type must be a string".into()))
        })
        .transpose()?
        .unwrap_or("CONVENTIONAL");
    let route_type = match type_str.to_uppercase().as_str() {
        "RNAV5" => AirwayType::Rnav5,
        "RNAV1" => AirwayType::Rnav1,
        "RNP4" => AirwayType::Rnp4,
        "MILITARY" => AirwayType::Military,
        "CONVENTIONAL" => AirwayType::Conventional,
        other => {
            return Err(AeronauticalError::FormatError(format!(
                "unsupported route type: {other}"
            )))
        }
    };

    let pts = coordinates.and_then(Value::as_array).ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("LineString coordinates must be an array".into())
    })?;
    let mut waypoints = Vec::with_capacity(pts.len());
    for pt in pts {
        let coords = pt.as_array().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("position must be an array".into())
        })?;
        let lon_deg = coords[0].as_f64().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("longitude must be numeric".into())
        })?;
        let lat_deg = coords[1].as_f64().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("latitude must be numeric".into())
        })?;
        let height_m = coords.get(2).map_or(Ok(0.0), |value| {
            value.as_f64().ok_or_else(|| {
                AeronauticalError::InvalidCoordinateString("height must be numeric".into())
            })
        })?;
        waypoints.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
    }

    let mut segments = Vec::with_capacity(waypoints.len().saturating_sub(1));
    let segment_properties = props.get("segments").and_then(Value::as_array);
    for i in 0..waypoints.len().saturating_sub(1) {
        let segment = segment_properties
            .and_then(|items| items.get(i))
            .and_then(Value::as_object);
        segments.push(AirwaySegment {
            from_ident: format!("{ident}_{i}"),
            to_ident: format!("{ident}_{}", i + 1),
            from_coords: waypoints[i],
            to_coords: waypoints[i + 1],
            mea_m: segment_f64(segment, "mea_m")?.or(typed_f64(props, &["mea_m"])?),
            maa_m: segment_f64(segment, "maa_m")?.or(typed_f64(props, &["maa_m"])?),
            inbound_bearing_deg: segment_f64(segment, "inbound_bearing_deg")?,
            direction: if segment_bool(segment, "is_unidirectional")?
                .or_else(|| props.get("is_unidirectional").and_then(Value::as_bool))
                .unwrap_or(false)
            {
                SegmentDirection::Unidirectional
            } else {
                SegmentDirection::Bidirectional
            },
        });
    }

    let airway = AeronauticalAirway {
        ident,
        route_type,
        segments,
    };
    airway.validate()?;
    Ok(airway)
}

fn segment_f64(
    segment: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Result<Option<f64>, AeronauticalError> {
    let Some(value) = segment
        .and_then(|item| item.get(key))
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| AeronauticalError::FormatError(format!("segment {key} must be numeric")))?;
    if !number.is_finite() {
        return Err(AeronauticalError::FormatError(format!(
            "segment {key} must be finite"
        )));
    }
    Ok(Some(number))
}

fn segment_bool(
    segment: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Result<Option<bool>, AeronauticalError> {
    let Some(value) = segment
        .and_then(|item| item.get(key))
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| AeronauticalError::FormatError(format!("segment {key} must be boolean")))
}

fn parse_airport_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Result<AeronauticalAirport, AeronauticalError> {
    let props =
        properties.ok_or_else(|| AeronauticalError::MissingRequiredField("properties".into()))?;
    let icao = typed_string(props, &["icao", "ident", "id"])?
        .ok_or_else(|| AeronauticalError::MissingRequiredField("icao".into()))?
        .to_string();
    let iata = typed_string(props, &["iata"])?.map(String::from);
    let name = typed_string(props, &["name"])?.unwrap_or(&icao).to_string();

    let coords_arr = coordinates.and_then(Value::as_array).ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("Point coordinates must be an array".into())
    })?;
    if coords_arr.len() < 2 {
        return Err(AeronauticalError::InvalidCoordinateString(
            "Point must contain longitude and latitude".into(),
        ));
    }
    let lon_deg = coords_arr[0].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("longitude must be numeric".into())
    })?;
    let lat_deg = coords_arr[1].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("latitude must be numeric".into())
    })?;
    let height_m = if coords_arr.len() >= 3 {
        coords_arr[2].as_f64().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("height must be numeric".into())
        })?
    } else {
        0.0
    };
    let coords = LatLon::from_degrees(lat_deg, lon_deg, height_m);

    let elevation_m = typed_f64(props, &["elevation_m"])?.unwrap_or(height_m);
    let runways = parse_runways(props.get("runways"))?;

    let airport = AeronauticalAirport {
        icao,
        iata,
        name,
        coords,
        elevation_m,
        runways,
    };
    airport.validate()?;
    Ok(airport)
}

fn parse_runways(
    value: Option<&Value>,
) -> Result<Vec<crate::aeronautical::types::AeronauticalRunway>, AeronauticalError> {
    let Some(items) = value else {
        return Ok(Vec::new());
    };
    let items = items
        .as_array()
        .ok_or_else(|| AeronauticalError::FormatError("runways must be an array".into()))?;
    let mut runways = Vec::with_capacity(items.len());
    for item in items {
        let runway = item
            .as_object()
            .ok_or_else(|| AeronauticalError::FormatError("runway must be an object".into()))?;
        let ident = object_string(runway, "ident")?;
        let surface = object_string(runway, "surface")?;
        let true_bearing_deg = object_f64(runway, "true_bearing_deg")?;
        let magnetic_bearing_deg = object_f64(runway, "magnetic_bearing_deg")?;
        let length_m = object_non_negative_f64(runway, "length_m")?;
        let width_m = object_non_negative_f64(runway, "width_m")?;
        let threshold_primary = parse_exported_position(runway.get("threshold_primary"))?;
        let threshold_secondary = parse_exported_position(runway.get("threshold_secondary"))?;
        runways.push(crate::aeronautical::types::AeronauticalRunway {
            ident,
            true_bearing_deg,
            magnetic_bearing_deg,
            length_m,
            width_m,
            threshold_primary,
            threshold_secondary,
            surface,
        });
    }
    Ok(runways)
}

fn object_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, AeronauticalError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AeronauticalError::FormatError(format!("runway {key} must be a string")))
}

fn object_f64(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<f64, AeronauticalError> {
    let value = object
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| AeronauticalError::FormatError(format!("runway {key} must be numeric")))?;
    if !value.is_finite() {
        return Err(AeronauticalError::FormatError(format!(
            "runway {key} must be finite"
        )));
    }
    Ok(value)
}

fn object_non_negative_f64(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<f64, AeronauticalError> {
    let value = object_f64(object, key)?;
    if value < 0.0 {
        return Err(AeronauticalError::FormatError(format!(
            "runway {key} must be non-negative"
        )));
    }
    Ok(value)
}

fn parse_exported_position(value: Option<&Value>) -> Result<LatLon, AeronauticalError> {
    let values = value.and_then(Value::as_array).ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("runway threshold must be an array".into())
    })?;
    if values.len() < 2 {
        return Err(AeronauticalError::InvalidCoordinateString(
            "runway threshold must contain longitude and latitude".into(),
        ));
    }
    let lon = values[0].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("runway longitude must be numeric".into())
    })?;
    let lat = values[1].as_f64().ok_or_else(|| {
        AeronauticalError::InvalidCoordinateString("runway latitude must be numeric".into())
    })?;
    let height = match values.get(2) {
        Some(value) => value.as_f64().ok_or_else(|| {
            AeronauticalError::InvalidCoordinateString("runway height must be numeric".into())
        })?,
        None => 0.0,
    };
    if !lon.is_finite()
        || !lat.is_finite()
        || !height.is_finite()
        || !(-180.0..=180.0).contains(&lon)
        || !(-90.0..=90.0).contains(&lat)
    {
        return Err(AeronauticalError::InvalidCoordinateString(
            "runway threshold coordinate is invalid".into(),
        ));
    }
    Ok(LatLon::from_degrees(lat, lon, height))
}

/// Exports an [`AeronauticalDataset`] to a standard GeoJSON FeatureCollection string.
///
/// # Errors
/// Returns [`AeronauticalError::JsonParseError`] if serialization fails.
///
/// # Panics
/// This function does not panic for valid Rust inputs.
///
/// # Safety
/// This function does not use unsafe operations and requires no caller-held
/// memory invariants.
pub fn export_dataset_to_geojson(
    dataset: &AeronauticalDataset,
) -> Result<String, AeronauticalError> {
    let mut features = Vec::with_capacity(
        dataset.airspaces.len()
            + dataset.navaids.len()
            + dataset.airways.len()
            + dataset.airports.len(),
    );

    // 1. Export Airspaces as Polygons
    for airspace in &dataset.airspaces {
        if airspace.boundary.is_empty() {
            continue;
        }
        let mut ring = Vec::with_capacity(airspace.boundary.len() + 1);
        for v in &airspace.boundary {
            ring.push(vec![v.lon.to_degrees(), v.lat.to_degrees(), v.height]);
        }
        // Ensure polygon ring is closed
        if let (Some(first), Some(last)) = (ring.first(), ring.last()) {
            if first != last {
                ring.push(first.clone());
            }
        }

        let type_str = match &airspace.airspace_type {
            AirspaceType::Fir => "FIR",
            AirspaceType::Uir => "UIR",
            AirspaceType::Tma => "TMA",
            AirspaceType::Ctr => "CTR",
            AirspaceType::Sector => "SECTOR",
            AirspaceType::Prohibited => "PROHIBITED",
            AirspaceType::Restricted => "RESTRICTED",
            AirspaceType::Danger => "DANGER",
            AirspaceType::Moa => "MOA",
            AirspaceType::Tra => "TRA",
            AirspaceType::Other(s) => s.as_str(),
        };

        features.push(json!({
            "type": "Feature",
            "properties": {
                "aero_type": "Airspace",
                "uid": airspace.uid,
                "name": airspace.name,
                "airspace_type": type_str,
                "lower_limit_m": airspace.lower_limit.value_m(),
                "lower_limit_fl": airspace.lower_limit.flight_level(),
                "lower_limit_reference": format!("{:?}", airspace.lower_limit.reference()),
                "upper_limit_m": airspace.upper_limit.value_m(),
                "upper_limit_fl": airspace.upper_limit.flight_level(),
                "upper_limit_reference": format!("{:?}", airspace.upper_limit.reference()),
            },
            "geometry": {
                "type": "Polygon",
                "coordinates": [ring]
            }
        }));
    }

    // 2. Export Navaids as Points
    for navaid in &dataset.navaids {
        let type_str = match navaid.navaid_type {
            NavaidType::Vor => "VOR",
            NavaidType::Dme => "DME",
            NavaidType::VorDme => "VORDME",
            NavaidType::Tacan => "TACAN",
            NavaidType::Vortac => "VORTAC",
            NavaidType::Ndb => "NDB",
            NavaidType::Fix => "FIX",
            NavaidType::Waypoint => "WAYPOINT",
        };

        features.push(json!({
            "type": "Feature",
            "properties": {
                "aero_type": "Navaid",
                "ident": navaid.ident,
                "name": navaid.name,
                "navaid_type": type_str,
                "frequency_mhz": navaid.frequency_mhz,
                "channel": navaid.channel,
                "elevation_m": navaid.elevation_m,
                "magnetic_variation_deg": navaid.magnetic_variation_deg,
            },
            "geometry": {
                "type": "Point",
                "coordinates": [navaid.coords.lon.to_degrees(), navaid.coords.lat.to_degrees(), navaid.coords.height]
            }
        }));
    }

    // 3. Export Airways as LineStrings
    for airway in &dataset.airways {
        if airway.segments.is_empty() {
            continue;
        }
        let mut line_coords = Vec::with_capacity(airway.segments.len() + 1);
        if let Some(first_seg) = airway.segments.first() {
            line_coords.push(vec![
                first_seg.from_coords.lon.to_degrees(),
                first_seg.from_coords.lat.to_degrees(),
                first_seg.from_coords.height,
            ]);
        }
        for seg in &airway.segments {
            line_coords.push(vec![
                seg.to_coords.lon.to_degrees(),
                seg.to_coords.lat.to_degrees(),
                seg.to_coords.height,
            ]);
        }

        let type_str = match airway.route_type {
            AirwayType::Conventional => "CONVENTIONAL",
            AirwayType::Rnav5 => "RNAV5",
            AirwayType::Rnav1 => "RNAV1",
            AirwayType::Rnp4 => "RNP4",
            AirwayType::Military => "MILITARY",
        };

        features.push(json!({
            "type": "Feature",
            "properties": {
                "aero_type": "Airway",
                "ident": airway.ident,
                "route_type": type_str,
                "segment_count": airway.segments.len(),
                "segments": airway.segments.iter().map(|segment| json!({
                    "mea_m": segment.mea_m,
                    "maa_m": segment.maa_m,
                    "inbound_bearing_deg": segment.inbound_bearing_deg,
                    "is_unidirectional": matches!(
                        segment.direction,
                        SegmentDirection::Unidirectional
                    ),
                })).collect::<Vec<_>>(),
            },
            "geometry": {
                "type": "LineString",
                "coordinates": line_coords
            }
        }));
    }

    // 4. Export Airports as Points
    for airport in &dataset.airports {
        features.push(json!({
            "type": "Feature",
            "properties": {
                "aero_type": "Airport",
                "icao": airport.icao,
                "iata": airport.iata,
                "name": airport.name,
                "elevation_m": airport.elevation_m,
                "runway_count": airport.runways.len(),
                "runways": airport.runways.iter().map(|runway| json!({
                    "ident": runway.ident,
                    "true_bearing_deg": runway.true_bearing_deg,
                    "magnetic_bearing_deg": runway.magnetic_bearing_deg,
                    "length_m": runway.length_m,
                    "width_m": runway.width_m,
                    "threshold_primary": [
                        runway.threshold_primary.lon.to_degrees(),
                        runway.threshold_primary.lat.to_degrees(),
                        runway.threshold_primary.height,
                    ],
                    "threshold_secondary": [
                        runway.threshold_secondary.lon.to_degrees(),
                        runway.threshold_secondary.lat.to_degrees(),
                        runway.threshold_secondary.height,
                    ],
                    "surface": runway.surface,
                })).collect::<Vec<_>>(),
            },
            "geometry": {
                "type": "Point",
                "coordinates": [airport.coords.lon.to_degrees(), airport.coords.lat.to_degrees(), airport.coords.height]
            }
        }));
    }

    let root = json!({
        "type": "FeatureCollection",
        "features": features
    });

    serde_json::to_string(&root).map_err(|e| AeronauticalError::JsonParseError(e.to_string()))
}
