use crate::aeronautical::dataset::AeronauticalDataset;
use crate::aeronautical::errors::AeronauticalError;
use crate::aeronautical::types::{
    AeronauticalAirspace, AeronauticalAirway, AeronauticalNavaid, AirspaceType, AirwaySegment,
    AirwayType, AltitudeLimit, NavaidType, SegmentDirection,
};
use crate::geodesy::coords::LatLon;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

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
///
/// # Errors
/// Returns [`AeronauticalError`] when XML, coordinates, required fields, or
/// altitude limits are malformed.
///
/// # Panics
/// This function does not panic for valid Rust inputs.
///
/// # Safety
/// This function does not use unsafe operations and requires no caller-held
/// memory invariants.
pub fn parse_aixm_51_str(xml_str: &str) -> Result<AeronauticalDataset, AeronauticalError> {
    let mut reader = Reader::from_str(xml_str);
    reader.trim_text(true);

    let mut dataset = AeronauticalDataset::new();
    let mut tag_stack: Vec<String> = Vec::new();

    // Transient parsing state
    let mut current_airspace: Option<AeronauticalAirspaceBuilder> = None;
    let mut current_navaid: Option<AeronauticalNavaidBuilder> = None;
    let mut current_route: Option<AeronauticalAirwayBuilder> = None;
    let mut saw_root = false;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = strip_namespace(e.name().into_inner());
                if tag_stack.is_empty() {
                    if tag != "AIXMBasicMessage" {
                        return Err(AeronauticalError::XmlParseError(
                            "AIXM document root must be AIXMBasicMessage".into(),
                        ));
                    }
                    saw_root = true;
                }
                tag_stack.push(tag.clone());

                match tag.as_str() {
                    "Airspace" => {
                        let mut uid = String::new();
                        // Extract gml:id if available
                        for attr in e.attributes() {
                            let attr = attr.map_err(|error| {
                                AeronauticalError::XmlParseError(error.to_string())
                            })?;
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                uid = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_airspace = Some(AeronauticalAirspaceBuilder::new(uid));
                    }
                    "Navaid" | "DesignatedPoint" => {
                        let mut ident = String::new();
                        for attr in e.attributes() {
                            let attr = attr.map_err(|error| {
                                AeronauticalError::XmlParseError(error.to_string())
                            })?;
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                ident = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_navaid = Some(AeronauticalNavaidBuilder::new(ident));
                    }
                    "Route" => {
                        let mut ident = String::new();
                        for attr in e.attributes() {
                            let attr = attr.map_err(|error| {
                                AeronauticalError::XmlParseError(error.to_string())
                            })?;
                            let key = strip_namespace(attr.key.into_inner());
                            if key == "id" {
                                ident = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                        current_route = Some(AeronauticalAirwayBuilder::new(ident));
                    }
                    "lowerLimit" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_limit_attrs(&e)?;
                        }
                    }
                    "upperLimit" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_limit_attrs(&e)?;
                        }
                    }
                    "value" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_value_attrs(&e)?;
                        }
                    }
                    "posList" => {
                        if let Some(ref mut asp) = current_airspace {
                            asp.extract_dimension(&e)?;
                        }
                        if let Some(ref mut rte) = current_route {
                            rte.extract_dimension(&e)?;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map_err(|err| AeronauticalError::XmlParseError(err.to_string()))?
                    .to_string();
                if let Some(current_tag) = tag_stack.last() {
                    if let Some(ref mut asp) = current_airspace {
                        asp.handle_text(current_tag, &text)?;
                    }
                    if let Some(ref mut nav) = current_navaid {
                        nav.handle_text(current_tag, &text)?;
                    }
                    if let Some(ref mut rte) = current_route {
                        rte.handle_text(current_tag, &text)?;
                    }
                }
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref());
                if let Some(current_tag) = tag_stack.last() {
                    if let Some(ref mut asp) = current_airspace {
                        asp.handle_text(current_tag, &text)?;
                    }
                    if let Some(ref mut nav) = current_navaid {
                        nav.handle_text(current_tag, &text)?;
                    }
                    if let Some(ref mut rte) = current_route {
                        rte.handle_text(current_tag, &text)?;
                    }
                }
            }
            Ok(Event::End(e)) => {
                let tag = strip_namespace(e.name().into_inner());
                tag_stack.pop();

                match tag.as_str() {
                    "Airspace" => {
                        if let Some(builder) = current_airspace.take() {
                            if let Some(airspace) = builder.build()? {
                                dataset.add_airspace(airspace);
                            }
                        }
                    }
                    "Navaid" | "DesignatedPoint" => {
                        if let Some(builder) = current_navaid.take() {
                            if let Some(navaid) = builder.build()? {
                                dataset.add_navaid(navaid);
                            }
                        }
                    }
                    "Route" => {
                        if let Some(builder) = current_route.take() {
                            if let Some(route) = builder.build()? {
                                dataset.add_airway(route);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AeronauticalError::XmlParseError(format!(
                    "XML error at position {}: {e}",
                    reader.buffer_position()
                )))
            }
            _ => {}
        }
        buf.clear();
    }

    if !saw_root {
        return Err(AeronauticalError::XmlParseError(
            "AIXM document is missing its root element".into(),
        ));
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
    pos_dimension: usize,
    active_limit_is_upper: bool,
    has_lower_limit: bool,
    has_upper_limit: bool,
}

impl AeronauticalAirspaceBuilder {
    fn new(uid: String) -> Self {
        Self {
            uid,
            upper_limit_val: 10000.0,
            ..Default::default()
        }
    }

    fn extract_limit_attrs(&mut self, e: &BytesStart) -> Result<(), AeronauticalError> {
        let tag = strip_namespace(e.name().into_inner());
        self.active_limit_is_upper = tag == "upperLimit";
        for attr in e.attributes() {
            let attr = attr.map_err(|error| AeronauticalError::XmlParseError(error.to_string()))?;
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
        Ok(())
    }

    fn extract_value_attrs(&mut self, e: &BytesStart) -> Result<(), AeronauticalError> {
        for attr in e.attributes() {
            let attr = attr.map_err(|error| AeronauticalError::XmlParseError(error.to_string()))?;
            if strip_namespace(attr.key.into_inner()) == "uom" {
                let value = String::from_utf8_lossy(&attr.value).to_string();
                if self.active_limit_is_upper {
                    self.upper_limit_uom = value;
                } else {
                    self.lower_limit_uom = value;
                }
            }
        }
        Ok(())
    }

    fn extract_dimension(&mut self, e: &BytesStart) -> Result<(), AeronauticalError> {
        for attr in e.attributes() {
            let attr = attr.map_err(|error| AeronauticalError::XmlParseError(error.to_string()))?;
            if strip_namespace(attr.key.into_inner()) == "srsDimension" {
                self.pos_dimension =
                    String::from_utf8_lossy(&attr.value).parse().map_err(|_| {
                        AeronauticalError::InvalidCoordinateString(
                            "srsDimension must be numeric".into(),
                        )
                    })?;
            }
        }
        Ok(())
    }

    fn handle_text(&mut self, tag: &str, text: &str) -> Result<(), AeronauticalError> {
        match tag {
            "name" | "AirspaceName" => self.name = text.to_string(),
            "type" | "AirspaceType" => self.airspace_type_str = text.to_string(),
            "identifier" if self.uid.is_empty() => self.uid = text.to_string(),
            "lowerLimit" => {
                self.has_lower_limit = true;
                if let Ok(v) = text.trim().parse::<f64>() {
                    self.lower_limit_val = v;
                } else {
                    return Err(AeronauticalError::InvalidAltitude(text.to_owned()));
                }
            }
            "upperLimit" => {
                self.has_upper_limit = true;
                if let Ok(v) = text.trim().parse::<f64>() {
                    self.upper_limit_val = v;
                } else {
                    return Err(AeronauticalError::InvalidAltitude(text.to_owned()));
                }
            }
            "value" => {
                if self.active_limit_is_upper {
                    self.has_upper_limit = true;
                    self.upper_limit_val = text
                        .trim()
                        .parse()
                        .map_err(|_| AeronauticalError::InvalidAltitude(text.to_owned()))?;
                } else {
                    self.has_lower_limit = true;
                    self.lower_limit_val = text
                        .trim()
                        .parse()
                        .map_err(|_| AeronauticalError::InvalidAltitude(text.to_owned()))?;
                }
            }
            "lowerLimitReference" => self.lower_limit_ref = text.to_string(),
            "upperLimitReference" => self.upper_limit_ref = text.to_string(),
            "posList" | "coordinates" => self.pos_list_str.push_str(text),
            _ => {}
        }
        Ok(())
    }

    fn build(self) -> Result<Option<AeronauticalAirspace>, AeronauticalError> {
        let uid = if self.uid.is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airspace identifier".into(),
            ));
        } else {
            self.uid
        };
        let name = if self.name.is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airspace name".into(),
            ));
        } else {
            self.name
        };
        if self.airspace_type_str.is_empty() {
            return Err(AeronauticalError::MissingRequiredField(
                "airspace type".into(),
            ));
        }
        if !self.has_lower_limit {
            return Err(AeronauticalError::MissingRequiredField(
                "lower limit".into(),
            ));
        }
        if !self.has_upper_limit {
            return Err(AeronauticalError::MissingRequiredField(
                "upper limit".into(),
            ));
        }

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

        let lower_limit = parse_altitude_limit(
            self.lower_limit_val,
            &self.lower_limit_uom,
            &self.lower_limit_ref,
        )?;
        let upper_limit = parse_altitude_limit(
            self.upper_limit_val,
            &self.upper_limit_uom,
            &self.upper_limit_ref,
        )?;

        let boundary = parse_gml_pos_list(&self.pos_list_str, self.pos_dimension)?;
        if boundary.len() < 3 {
            return Err(AeronauticalError::MissingRequiredField(
                "airspace boundary".into(),
            ));
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
        Self {
            ident,
            ..Default::default()
        }
    }

    fn handle_text(&mut self, tag: &str, text: &str) -> Result<(), AeronauticalError> {
        match tag {
            "designator" | "ident" => self.ident = text.to_string(),
            "name" => self.name = text.to_string(),
            "type" => self.type_str = text.to_string(),
            "pos" | "coordinates" | "posList" => self.pos_str.push_str(text),
            "frequency" => self.freq_str = text.to_string(),
            "elevation" => self.elevation_str = text.to_string(),
            _ => {}
        }
        Ok(())
    }

    fn build(self) -> Result<Option<AeronauticalNavaid>, AeronauticalError> {
        let ident = if self.ident.is_empty() {
            "FIX".to_string()
        } else {
            self.ident
        };
        let name = if self.name.is_empty() {
            ident.clone()
        } else {
            self.name
        };

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

        let coords = parse_single_gml_pos(&self.pos_str)?
            .ok_or_else(|| AeronauticalError::MissingRequiredField("navaid position".into()))?;
        let frequency_mhz = parse_optional_f64(&self.freq_str, "frequency")?;
        let elevation_m = parse_optional_f64(&self.elevation_str, "elevation")?;

        let navaid = AeronauticalNavaid {
            ident,
            name,
            navaid_type,
            coords,
            frequency_mhz,
            channel: None,
            elevation_m,
            magnetic_variation_deg: None,
        };
        navaid.validate()?;
        Ok(Some(navaid))
    }
}

#[derive(Default)]
struct AeronauticalAirwayBuilder {
    ident: String,
    type_str: String,
    pos_list_str: String,
    pos_dimension: usize,
}

impl AeronauticalAirwayBuilder {
    fn new(ident: String) -> Self {
        Self {
            ident,
            ..Default::default()
        }
    }

    fn extract_dimension(&mut self, e: &BytesStart) -> Result<(), AeronauticalError> {
        for attr in e.attributes() {
            let attr = attr.map_err(|error| AeronauticalError::XmlParseError(error.to_string()))?;
            if strip_namespace(attr.key.into_inner()) == "srsDimension" {
                self.pos_dimension =
                    String::from_utf8_lossy(&attr.value).parse().map_err(|_| {
                        AeronauticalError::InvalidCoordinateString(
                            "srsDimension must be numeric".into(),
                        )
                    })?;
            }
        }
        Ok(())
    }

    fn handle_text(&mut self, tag: &str, text: &str) -> Result<(), AeronauticalError> {
        match tag {
            "designator" | "ident" => self.ident = text.to_string(),
            "designatorPrefix" => self.ident.insert_str(0, text),
            "designatorSecondLetter" | "designatorNumber" => self.ident.push_str(text),
            "type" => self.type_str = text.to_string(),
            "posList" | "coordinates" => self.pos_list_str.push_str(text),
            _ => {}
        }
        Ok(())
    }

    fn build(self) -> Result<Option<AeronauticalAirway>, AeronauticalError> {
        let ident = if self.ident.is_empty() {
            "AIRWAY".to_string()
        } else {
            self.ident
        };
        let route_type = match self.type_str.to_uppercase().as_str() {
            "RNAV5" => AirwayType::Rnav5,
            "RNAV1" => AirwayType::Rnav1,
            "RNP4" => AirwayType::Rnp4,
            "MILITARY" => AirwayType::Military,
            _ => AirwayType::Conventional,
        };

        let waypoints = parse_gml_pos_list(&self.pos_list_str, self.pos_dimension)?;
        if waypoints.len() < 2 {
            return Err(AeronauticalError::MissingRequiredField(
                "airway route geometry".into(),
            ));
        }

        let mut segments = Vec::with_capacity(waypoints.len() - 1);
        for i in 0..waypoints.len() - 1 {
            segments.push(AirwaySegment {
                from_ident: format!("{ident}_{i}"),
                to_ident: format!("{ident}_{}", i + 1),
                from_coords: waypoints[i],
                to_coords: waypoints[i + 1],
                mea_m: None,
                maa_m: None,
                inbound_bearing_deg: None,
                direction: SegmentDirection::Bidirectional,
            });
        }

        let airway = AeronauticalAirway {
            ident,
            route_type,
            segments,
        };
        airway.validate()?;
        Ok(Some(airway))
    }
}

/// Parses a GML posList into a list of [`LatLon`] points.
///
/// In standard AIXM 5.1 GML, posList entries are in `(lat, lon)` or `(lat, lon, height)` decimal degrees.
fn parse_gml_pos_list(pos_list: &str, dimension: usize) -> Result<Vec<LatLon>, AeronauticalError> {
    let tokens: Vec<f64> = pos_list
        .split_whitespace()
        .map(|s| {
            s.trim()
                .parse::<f64>()
                .map_err(|_| AeronauticalError::InvalidCoordinateString(s.to_string()))
        })
        .collect::<Result<_, _>>()?;

    let dimension = if dimension == 0 { 2 } else { dimension };
    if !matches!(dimension, 2 | 3) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "posList dimension must be 2 or 3".into(),
        ));
    }
    if !tokens.len().is_multiple_of(dimension) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "posList contains an incomplete coordinate".into(),
        ));
    }

    let mut points = Vec::with_capacity(tokens.len() / dimension);
    let mut i = 0;
    while i + 1 < tokens.len() {
        let lat_deg = tokens[i];
        let lon_deg = tokens[i + 1];
        let height_m = if dimension == 3 { tokens[i + 2] } else { 0.0 };
        validate_coordinate(lat_deg, lon_deg, height_m)?;
        points.push(LatLon::from_degrees(lat_deg, lon_deg, height_m));
        i += dimension;
    }
    Ok(points)
}

/// Parses a single GML pos coordinate `lat lon` or `lat lon height`.
fn parse_single_gml_pos(pos: &str) -> Result<Option<LatLon>, AeronauticalError> {
    let tokens: Vec<f64> = pos
        .split_whitespace()
        .map(|s| {
            s.trim()
                .parse::<f64>()
                .map_err(|_| AeronauticalError::InvalidCoordinateString(s.to_string()))
        })
        .collect::<Result<_, _>>()?;

    if !(2..=3).contains(&tokens.len()) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "single position must contain exactly 2 or 3 values".into(),
        ));
    }
    if tokens.len() >= 2 {
        let lat_deg = tokens[0];
        let lon_deg = tokens[1];
        let height_m = if tokens.len() >= 3 { tokens[2] } else { 0.0 };
        validate_coordinate(lat_deg, lon_deg, height_m)?;
        Ok(Some(LatLon::from_degrees(lat_deg, lon_deg, height_m)))
    } else {
        Ok(None)
    }
}

fn validate_coordinate(lat_deg: f64, lon_deg: f64, height_m: f64) -> Result<(), AeronauticalError> {
    if !lat_deg.is_finite() || !(-90.0..=90.0).contains(&lat_deg) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "latitude is outside [-90, 90]".into(),
        ));
    }
    if !lon_deg.is_finite() || !(-180.0..=180.0).contains(&lon_deg) {
        return Err(AeronauticalError::InvalidCoordinateString(
            "longitude is outside [-180, 180]".into(),
        ));
    }
    if !height_m.is_finite() {
        return Err(AeronauticalError::InvalidCoordinateString(
            "height must be finite".into(),
        ));
    }
    Ok(())
}

fn parse_optional_f64(value: &str, field: &str) -> Result<Option<f64>, AeronauticalError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|_| AeronauticalError::FormatError(format!("{field} must be numeric")))?;
    if !parsed.is_finite() {
        return Err(AeronauticalError::FormatError(format!(
            "{field} must be finite"
        )));
    }
    Ok(Some(parsed))
}

fn parse_altitude_limit(
    val: f64,
    uom: &str,
    reference: &str,
) -> Result<AltitudeLimit, AeronauticalError> {
    if !val.is_finite() || val < 0.0 {
        return Err(AeronauticalError::InvalidAltitude(format!(
            "invalid value: {val}"
        )));
    }
    let is_fl = uom.eq_ignore_ascii_case("FL") || reference.eq_ignore_ascii_case("STD");
    let is_gnd = reference.eq_ignore_ascii_case("GND") || reference.eq_ignore_ascii_case("SFC");

    if is_gnd && val == 0.0 {
        return Ok(AltitudeLimit::ground());
    }

    if is_fl {
        if val.fract() != 0.0 {
            return Err(AeronauticalError::InvalidAltitude(
                "flight level must be an integer".into(),
            ));
        }
        AltitudeLimit::from_flight_level(val)
    } else if uom.eq_ignore_ascii_case("FT") {
        AltitudeLimit::amsl(val * 0.3048)
    } else {
        if !uom.is_empty()
            && !uom.eq_ignore_ascii_case("M")
            && !uom.eq_ignore_ascii_case("FT")
            && !uom.eq_ignore_ascii_case("FL")
        {
            return Err(AeronauticalError::InvalidAltitude(format!(
                "unsupported unit: {uom}"
            )));
        }
        AltitudeLimit::amsl(val)
    }
}
