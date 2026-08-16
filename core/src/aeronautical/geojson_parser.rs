use serde_json::{json, Value};
use crate::aeronautical::dataset::AeronauticalDataset;
use crate::aeronautical::errors::AeronauticalError;
use crate::aeronautical::types::{
    AeronauticalAirport, AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid,
    AirspaceType, AirwaySegment, AirwayType, AltitudeLimit,
    NavaidType,
};
use crate::geodesy::coords::LatLon;

/// Parses a GeoJSON-Aviation formatted string into an [`AeronauticalDataset`].
pub fn parse_geojson_aviation_str(json_str: &str) -> Result<AeronauticalDataset, AeronauticalError> {
    let root: Value = serde_json::from_str(json_str)
        .map_err(|e| AeronauticalError::JsonParseError(e.to_string()))?;

    let features = root.get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| AeronauticalError::JsonParseError("GeoJSON must contain a 'features' array".to_string()))?;

    let mut dataset = AeronauticalDataset::new();

    for feature in features {
        let properties = feature.get("properties").and_then(Value::as_object);
        let geometry = feature.get("geometry").and_then(Value::as_object);

        let geom_type = geometry.and_then(|g| g.get("type")).and_then(Value::as_str).unwrap_or("");
        let coords_val = geometry.and_then(|g| g.get("coordinates"));

        let aero_type = properties
            .and_then(|p| p.get("aero_type").or_else(|| p.get("feature_type")).or_else(|| p.get("type")))
            .and_then(Value::as_str)
            .unwrap_or("");

        match (geom_type, aero_type) {
            ("Polygon", _) | (_, "Airspace") => {
                if let Some(airspace) = parse_airspace_feature(properties, coords_val) {
                    dataset.add_airspace(airspace);
                }
            }
            ("Point", _) if aero_type.eq_ignore_ascii_case("Airport") => {
                if let Some(airport) = parse_airport_feature(properties, coords_val) {
                    dataset.add_airport(airport);
                }
            }
            ("Point", _) | (_, "Navaid") => {
                if let Some(navaid) = parse_navaid_feature(properties, coords_val) {
                    dataset.add_navaid(navaid);
                }
            }
            ("LineString", _) | (_, "Airway") => {
                if let Some(airway) = parse_airway_feature(properties, coords_val) {
                    dataset.add_airway(airway);
                }
            }
            _ => {}
        }
    }

    Ok(dataset)
}

fn parse_airspace_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Option<AeronauticalAirspace> {
    let props = properties?;
    let uid = props.get("uid").or_else(|| props.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("AIRSPACE")
        .to_string();
    let name = props.get("name")
        .and_then(Value::as_str)
        .unwrap_or(&uid)
        .to_string();

    let type_str = props.get("airspace_type").or_else(|| props.get("sub_type"))
        .and_then(Value::as_str)
        .unwrap_or("SECTOR");
    let airspace_type = match type_str.to_uppercase().as_str() {
        "FIR" => AirspaceType::Fir,
        "UIR" => AirspaceType::Uir,
        "TMA" => AirspaceType::Tma,
        "CTR" => AirspaceType::Ctr,
        "PROHIBITED" | "P" => AirspaceType::Prohibited,
        "RESTRICTED" | "R" => AirspaceType::Restricted,
        "DANGER" | "D" => AirspaceType::Danger,
        "MOA" => AirspaceType::Moa,
        "TRA" | "TSA" => AirspaceType::Tra,
        other => AirspaceType::Other(other.to_string()),
    };

    let lower_val = props.get("lower_limit_m").and_then(Value::as_f64)
        .or_else(|| props.get("lower_limit").and_then(Value::as_f64))
        .unwrap_or(0.0);
    let lower_fl = props.get("lower_limit_fl").and_then(Value::as_u64).map(|v| v as u32);
    let lower_limit = if let Some(fl) = lower_fl {
        AltitudeLimit::flight_level(fl)
    } else {
        AltitudeLimit::amsl(lower_val)
    };

    let upper_val = props.get("upper_limit_m").and_then(Value::as_f64)
        .or_else(|| props.get("upper_limit").and_then(Value::as_f64))
        .unwrap_or(10000.0);
    let upper_fl = props.get("upper_limit_fl").and_then(Value::as_u64).map(|v| v as u32);
    let upper_limit = if let Some(fl) = upper_fl {
        AltitudeLimit::flight_level(fl)
    } else {
        AltitudeLimit::amsl(upper_val)
    };

    let mut boundary = Vec::new();
    if let Some(rings) = coordinates.and_then(Value::as_array) {
        // First ring is exterior ring
        if let Some(exterior) = rings.first().and_then(Value::as_array) {
            for pt in exterior {
                if let Some(coords) = pt.as_array() {
                    if coords.len() >= 2 {
                        let lon_deg = coords[0].as_f64().unwrap_or(0.0);
                        let lat_deg = coords[1].as_f64().unwrap_or(0.0);
                        let height_m = if coords.len() >= 3 { coords[2].as_f64().unwrap_or(0.0) } else { 0.0 };
                        boundary.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
                    }
                }
            }
        }
    }

    Some(AeronauticalAirspace {
        uid,
        name,
        airspace_type,
        lower_limit,
        upper_limit,
        boundary,
    })
}

fn parse_navaid_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Option<AeronauticalNavaid> {
    let props = properties?;
    let ident = props.get("ident").or_else(|| props.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("FIX")
        .to_string();
    let name = props.get("name")
        .and_then(Value::as_str)
        .unwrap_or(&ident)
        .to_string();

    let type_str = props.get("navaid_type").or_else(|| props.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("WAYPOINT");
    let navaid_type = match type_str.to_uppercase().as_str() {
        "VOR" => NavaidType::Vor,
        "DME" => NavaidType::Dme,
        "VORDME" | "VOR_DME" | "VOR-DME" => NavaidType::VorDme,
        "TACAN" => NavaidType::Tacan,
        "VORTAC" => NavaidType::Vortac,
        "NDB" => NavaidType::Ndb,
        "FIX" | "INTERSECTION" => NavaidType::Fix,
        _ => NavaidType::Waypoint,
    };

    let coords_arr = coordinates.and_then(Value::as_array)?;
    if coords_arr.len() < 2 {
        return None;
    }
    let lon_deg = coords_arr[0].as_f64().unwrap_or(0.0);
    let lat_deg = coords_arr[1].as_f64().unwrap_or(0.0);
    let height_m = if coords_arr.len() >= 3 { coords_arr[2].as_f64().unwrap_or(0.0) } else { 0.0 };
    let coords = LatLon::from_degrees(lat_deg, lon_deg, height_m);

    let frequency_mhz = props.get("frequency_mhz").or_else(|| props.get("frequency")).and_then(Value::as_f64);
    let channel = props.get("channel").and_then(Value::as_str).map(String::from);
    let elevation_m = props.get("elevation_m").or_else(|| props.get("elevation")).and_then(Value::as_f64);
    let magnetic_variation_deg = props.get("magnetic_variation_deg").or_else(|| props.get("mag_var")).and_then(Value::as_f64);

    Some(AeronauticalNavaid {
        ident,
        name,
        navaid_type,
        coords,
        frequency_mhz,
        channel,
        elevation_m,
        magnetic_variation_deg,
    })
}

fn parse_airway_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Option<AeronauticalAirway> {
    let props = properties?;
    let ident = props.get("ident").or_else(|| props.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("AIRWAY")
        .to_string();

    let type_str = props.get("route_type").and_then(Value::as_str).unwrap_or("CONVENTIONAL");
    let route_type = match type_str.to_uppercase().as_str() {
        "RNAV5" => AirwayType::Rnav5,
        "RNAV1" => AirwayType::Rnav1,
        "RNP4" => AirwayType::Rnp4,
        "MILITARY" => AirwayType::Military,
        _ => AirwayType::Conventional,
    };

    let pts = coordinates.and_then(Value::as_array)?;
    let mut waypoints = Vec::new();
    for pt in pts {
        if let Some(coords) = pt.as_array() {
            if coords.len() >= 2 {
                let lon_deg = coords[0].as_f64().unwrap_or(0.0);
                let lat_deg = coords[1].as_f64().unwrap_or(0.0);
                let height_m = if coords.len() >= 3 { coords[2].as_f64().unwrap_or(0.0) } else { 0.0 };
                waypoints.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
            }
        }
    }

    let mut segments = Vec::new();
    for i in 0..waypoints.len().saturating_sub(1) {
        segments.push(AirwaySegment {
            from_ident: format!("{ident}_{i}"),
            to_ident: format!("{ident}_{}", i + 1),
            from_coords: waypoints[i],
            to_coords: waypoints[i + 1],
            mea_m: props.get("mea_m").and_then(Value::as_f64),
            maa_m: props.get("maa_m").and_then(Value::as_f64),
            inbound_bearing_deg: None,
            is_unidirectional: props.get("is_unidirectional").and_then(Value::as_bool).unwrap_or(false),
        });
    }

    Some(AeronauticalAirway {
        ident,
        route_type,
        segments,
    })
}

fn parse_airport_feature(
    properties: Option<&serde_json::Map<String, Value>>,
    coordinates: Option<&Value>,
) -> Option<AeronauticalAirport> {
    let props = properties?;
    let icao = props.get("icao").or_else(|| props.get("ident")).or_else(|| props.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    let iata = props.get("iata").and_then(Value::as_str).map(String::from);
    let name = props.get("name").and_then(Value::as_str).unwrap_or(&icao).to_string();

    let coords_arr = coordinates.and_then(Value::as_array)?;
    if coords_arr.len() < 2 {
        return None;
    }
    let lon_deg = coords_arr[0].as_f64().unwrap_or(0.0);
    let lat_deg = coords_arr[1].as_f64().unwrap_or(0.0);
    let height_m = if coords_arr.len() >= 3 { coords_arr[2].as_f64().unwrap_or(0.0) } else { 0.0 };
    let coords = LatLon::from_degrees(lat_deg, lon_deg, height_m);

    let elevation_m = props.get("elevation_m").and_then(Value::as_f64).unwrap_or(height_m);

    Some(AeronauticalAirport {
        icao,
        iata,
        name,
        coords,
        elevation_m,
        runways: Vec::new(),
    })
}

/// Exports an [`AeronauticalDataset`] to a standard GeoJSON FeatureCollection string.
pub fn export_dataset_to_geojson(dataset: &AeronauticalDataset) -> Result<String, AeronauticalError> {
    let mut features = Vec::new();

    // 1. Export Airspaces as Polygons
    for airspace in &dataset.airspaces {
        if airspace.boundary.is_empty() {
            continue;
        }
        let mut ring = Vec::new();
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
                "lower_limit_m": airspace.lower_limit.value_m,
                "lower_limit_fl": airspace.lower_limit.flight_level,
                "upper_limit_m": airspace.upper_limit.value_m,
                "upper_limit_fl": airspace.upper_limit.flight_level,
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
        let mut line_coords = Vec::new();
        if let Some(first_seg) = airway.segments.first() {
            line_coords.push(vec![first_seg.from_coords.lon.to_degrees(), first_seg.from_coords.lat.to_degrees(), first_seg.from_coords.height]);
        }
        for seg in &airway.segments {
            line_coords.push(vec![seg.to_coords.lon.to_degrees(), seg.to_coords.lat.to_degrees(), seg.to_coords.height]);
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

    serde_json::to_string(&root)
        .map_err(|e| AeronauticalError::JsonParseError(e.to_string()))
}
