use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use crate::aeronautical::dataset::AeronauticalDataset;
use crate::aeronautical::errors::AeronauticalError;
use crate::aeronautical::types::{
    AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid, AirspaceType,
    AirwaySegment, AirwayType, AltitudeLimit, AltitudeReference, NavaidType,
};
use crate::geodesy::coords::LatLon;

/// Strips namespace prefixes (e.g. `aixm:Airspace` -> `Airspace`, `gml:posList` -> `posList`).
fn strip_namespace(tag_name: &[u8]) -> String {
    let local = if let Some(pos) = tag_name.iter().position(|&b| b == b':') {
        &tag_name[pos + 1..]
    } else {
        tag_name
    };
    String::from_utf8_lossy(local).to_string()
}

/// Parses an AIXM 5.1 formatted XML string into an [`AeronauticalDataset`].
pub fn parse_aixm_51_str(xml_str: &str) -> Result<AeronauticalDataset, AeronauticalError> {
    let mut reader = Reader::from_str(xml_str);
    reader.trim_text(true);

    let mut dataset = AeronauticalDataset::new();
    let mut tag_stack: Vec<String> = Vec::new();

    // Transient parsing state
    let mut current_airspace: Option<AeronauticalAirspaceBuilder> = None;
    let mut current_navaid: Option<AeronauticalNavaidBuilder> = None;
    let mut current_route: Option<AeronauticalAirwayBuilder> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = strip_namespace(e.name().into_inner());
                tag_stack.push(tag.clone());

                match tag.as_str() {
                    "Airspace" => {
                        let mut uid = String::new();
                        // Extract gml:id if available
                        for attr in e.attributes().flatten() {
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                uid = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_airspace = Some(AeronauticalAirspaceBuilder::new(uid));
                    }
                    "Navaid" | "DesignatedPoint" => {
                        let mut ident = String::new();
                        for attr in e.attributes().flatten() {
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                ident = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_navaid = Some(AeronauticalNavaidBuilder::new(ident));
                    }
                    "Route" => {
                        let mut ident = String::new();
                        for attr in e.attributes().flatten() {
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                ident = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_route = Some(AeronauticalAirwayBuilder::new(ident));
                    }
                    "lowerLimit" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_limit_attrs(&e);
                        }
                    }
                    "upperLimit" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_limit_attrs(&e);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().map_err(|err| AeronauticalError::XmlParseError(err.to_string()))?.to_string();
                if let Some(current_tag) = tag_stack.last() {
                    if let Some(ref mut asp) = current_airspace {
                        asp.handle_text(current_tag, &text);
                    }
                    if let Some(ref mut nav) = current_navaid {
                        nav.handle_text(current_tag, &text);
                    }
                    if let Some(ref mut rte) = current_route {
                        rte.handle_text(current_tag, &text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag = strip_namespace(e.name().into_inner());
                tag_stack.pop();

                match tag.as_str() {
                    "Airspace" => {
                        if let Some(builder) = current_airspace.take() {
                            if let Some(airspace) = builder.build() {
                                dataset.add_airspace(airspace);
                            }
                        }
                    }
                    "Navaid" | "DesignatedPoint" => {
                        if let Some(builder) = current_navaid.take() {
                            if let Some(navaid) = builder.build() {
                                dataset.add_navaid(navaid);
                            }
                        }
                    }
                    "Route" => {
                        if let Some(builder) = current_route.take() {
                            if let Some(route) = builder.build() {
                                dataset.add_airway(route);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(AeronauticalError::XmlParseError(format!("XML error at position {}: {e}", reader.buffer_position()))),
            _ => {}
        }
        buf.clear();
    }

    Ok(dataset)
}

#[derive(Default)]
struct AeronauticalAirspaceBuilder {
    uid: String,
    name: String,
    airspace_type_str: String,
    lower_limit_val: f64,
    lower_limit_uom: String,
    lower_limit_ref: String,
    upper_limit_val: f64,
    upper_limit_uom: String,
    upper_limit_ref: String,
    pos_list_str: String,
    active_limit_is_upper: bool,
}

impl AeronauticalAirspaceBuilder {
    fn new(uid: String) -> Self {
        Self {
            uid,
            upper_limit_val: 10000.0,
            ..Default::default()
        }
    }

    fn extract_limit_attrs(&mut self, e: &BytesStart) {
        let tag = strip_namespace(e.name().into_inner());
        self.active_limit_is_upper = tag == "upperLimit";
        for attr in e.attributes().flatten() {
            let key = strip_namespace(attr.key.into_inner());
            let val = String::from_utf8_lossy(&attr.value).to_string();
            if key == "uom" {
                if self.active_limit_is_upper {
                    self.upper_limit_uom = val;
                } else {
                    self.lower_limit_uom = val;
                }
            }
        }
    }

    fn handle_text(&mut self, tag: &str, text: &str) {
        match tag {
            "name" | "AirspaceName" => self.name = text.to_string(),
            "type" | "AirspaceType" => self.airspace_type_str = text.to_string(),
            "identifier" if self.uid.is_empty() => self.uid = text.to_string(),
            "lowerLimit" => {
                if let Ok(v) = text.trim().parse::<f64>() {
                    self.lower_limit_val = v;
                }
            }
            "upperLimit" => {
                if let Ok(v) = text.trim().parse::<f64>() {
                    self.upper_limit_val = v;
                }
            }
            "lowerLimitReference" => self.lower_limit_ref = text.to_string(),
            "upperLimitReference" => self.upper_limit_ref = text.to_string(),
            "posList" | "coordinates" => self.pos_list_str.push_str(text),
            _ => {}
        }
    }

    fn build(self) -> Option<AeronauticalAirspace> {
        let uid = if self.uid.is_empty() { "AIRSPACE".to_string() } else { self.uid };
        let name = if self.name.is_empty() { uid.clone() } else { self.name };

        let airspace_type = match self.airspace_type_str.to_uppercase().as_str() {
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

        let lower_limit = parse_altitude_limit(self.lower_limit_val, &self.lower_limit_uom, &self.lower_limit_ref);
        let upper_limit = parse_altitude_limit(self.upper_limit_val, &self.upper_limit_uom, &self.upper_limit_ref);

        let boundary = parse_gml_pos_list(&self.pos_list_str);

        Some(AeronauticalAirspace {
            uid,
            name,
            airspace_type,
            lower_limit,
            upper_limit,
            boundary,
        })
    }
}

#[derive(Default)]
struct AeronauticalNavaidBuilder {
    ident: String,
    name: String,
    type_str: String,
    pos_str: String,
    freq_str: String,
    elevation_str: String,
}

impl AeronauticalNavaidBuilder {
    fn new(ident: String) -> Self {
        Self { ident, ..Default::default() }
    }

    fn handle_text(&mut self, tag: &str, text: &str) {
        match tag {
            "designator" | "ident" => self.ident = text.to_string(),
            "name" => self.name = text.to_string(),
            "type" => self.type_str = text.to_string(),
            "pos" | "coordinates" | "posList" => self.pos_str.push_str(text),
            "frequency" => self.freq_str = text.to_string(),
            "elevation" => self.elevation_str = text.to_string(),
            _ => {}
        }
    }

    fn build(self) -> Option<AeronauticalNavaid> {
        let ident = if self.ident.is_empty() { "FIX".to_string() } else { self.ident };
        let name = if self.name.is_empty() { ident.clone() } else { self.name };

        let navaid_type = match self.type_str.to_uppercase().as_str() {
            "VOR" => NavaidType::Vor,
            "DME" => NavaidType::Dme,
            "VOR_DME" | "VORDME" | "VOR-DME" => NavaidType::VorDme,
            "TACAN" => NavaidType::Tacan,
            "VORTAC" => NavaidType::Vortac,
            "NDB" => NavaidType::Ndb,
            "FIX" | "INTERSECTION" => NavaidType::Fix,
            _ => NavaidType::Waypoint,
        };

        let coords = parse_single_gml_pos(&self.pos_str)?;
        let frequency_mhz = self.freq_str.trim().parse::<f64>().ok();
        let elevation_m = self.elevation_str.trim().parse::<f64>().ok();

        Some(AeronauticalNavaid {
            ident,
            name,
            navaid_type,
            coords,
            frequency_mhz,
            channel: None,
            elevation_m,
            magnetic_variation_deg: None,
        })
    }
}

#[derive(Default)]
struct AeronauticalAirwayBuilder {
    ident: String,
    type_str: String,
    pos_list_str: String,
}

impl AeronauticalAirwayBuilder {
    fn new(ident: String) -> Self {
        Self { ident, ..Default::default() }
    }

    fn handle_text(&mut self, tag: &str, text: &str) {
        match tag {
            "designator" | "ident" => self.ident = text.to_string(),
            "designatorPrefix" => self.ident.insert_str(0, text),
            "designatorSecondLetter" | "designatorNumber" => self.ident.push_str(text),
            "type" => self.type_str = text.to_string(),
            "posList" | "coordinates" => self.pos_list_str.push_str(text),
            _ => {}
        }
    }

    fn build(self) -> Option<AeronauticalAirway> {
        let ident = if self.ident.is_empty() { "AIRWAY".to_string() } else { self.ident };
        let route_type = match self.type_str.to_uppercase().as_str() {
            "RNAV5" => AirwayType::Rnav5,
            "RNAV1" => AirwayType::Rnav1,
            "RNP4" => AirwayType::Rnp4,
            "MILITARY" => AirwayType::Military,
            _ => AirwayType::Conventional,
        };

        let waypoints = parse_gml_pos_list(&self.pos_list_str);
        if waypoints.len() < 2 {
            return None;
        }

        let mut segments = Vec::new();
        for i in 0..waypoints.len() - 1 {
            segments.push(AirwaySegment {
                from_ident: format!("{ident}_{i}"),
                to_ident: format!("{ident}_{}", i + 1),
                from_coords: waypoints[i],
                to_coords: waypoints[i + 1],
                mea_m: None,
                maa_m: None,
                inbound_bearing_deg: None,
                is_unidirectional: false,
            });
        }

        Some(AeronauticalAirway {
            ident,
            route_type,
            segments,
        })
    }
}

/// Parses a GML posList into a list of [`LatLon`] points.
///
/// In standard AIXM 5.1 GML, posList entries are in `(lat, lon)` or `(lat, lon, height)` decimal degrees.
fn parse_gml_pos_list(pos_list: &str) -> Vec<LatLon> {
    let tokens: Vec<f64> = pos_list
        .split_whitespace()
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();

    let mut points = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        let lat_deg = tokens[i];
        let lon_deg = tokens[i + 1];
        let height_m = 0.0;
        points.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
        i += 2;
    }
    points
}

/// Parses a single GML pos coordinate `lat lon` or `lat lon height`.
fn parse_single_gml_pos(pos: &str) -> Option<LatLon> {
    let tokens: Vec<f64> = pos
        .split_whitespace()
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();

    if tokens.len() >= 2 {
        let lat_deg = tokens[0];
        let lon_deg = tokens[1];
        let height_m = if tokens.len() >= 3 { tokens[2] } else { 0.0 };
        Some(LatLon::from_degrees(lat_deg, lon_deg, height_m))
    } else {
        None
    }
}

fn parse_altitude_limit(val: f64, uom: &str, reference: &str) -> AltitudeLimit {
    let is_fl = uom.eq_ignore_ascii_case("FL") || reference.eq_ignore_ascii_case("STD");
    let is_gnd = reference.eq_ignore_ascii_case("GND") || reference.eq_ignore_ascii_case("SFC");

    if is_gnd && val == 0.0 {
        return AltitudeLimit::ground();
    }

    if is_fl {
        let fl = val.round() as u32;
        AltitudeLimit::flight_level(fl)
    } else if uom.eq_ignore_ascii_case("FT") {
        AltitudeLimit {
            value_m: val * 0.3048,
            reference: AltitudeReference::Amsl,
            flight_level: None,
        }
    } else {
        AltitudeLimit {
            value_m: val,
            reference: AltitudeReference::Amsl,
            flight_level: None,
        }
    }
}
