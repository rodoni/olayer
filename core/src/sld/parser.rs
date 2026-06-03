use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use crate::sld::errors::SldError;
use crate::sld::styles::{FillStyle, PointStyle, RuleStyle, StrokeStyle, StyleRegistry, TextStyle};

/// Extract the local tag name from a `BytesStart`, stripping any XML namespace prefix.
fn local_name_start(e: &BytesStart) -> String {
    let name_bytes = e.name().into_inner();
    let local_bytes = if let Some(pos) = name_bytes.iter().position(|&x| x == b':') {
        &name_bytes[pos + 1..]
    } else {
        name_bytes
    };
    String::from_utf8_lossy(local_bytes).into_owned()
}

/// Extract the local tag name from a `BytesEnd`, stripping any XML namespace prefix.
fn local_name_end(e: &quick_xml::events::BytesEnd) -> String {
    let name_bytes = e.name().into_inner();
    let local_bytes = if let Some(pos) = name_bytes.iter().position(|&x| x == b':') {
        &name_bytes[pos + 1..]
    } else {
        name_bytes
    };
    String::from_utf8_lossy(local_bytes).into_owned()
}

/// Check whether the current XML path matches a given prefix.
///
/// `last_is_param` adds an extra slot at the end that must be either
/// `"CssParameter"` or `"SvgParameter"`.
fn path_matches(path: &[String], prefix: &[&str], last_is_param: bool) -> bool {
    if path.is_empty() {
        return false;
    }
    let expected_len = prefix.len() + if last_is_param { 1 } else { 0 };
    if path.len() < expected_len {
        return false;
    }
    let start_idx = path.len() - expected_len;
    for (i, &elem) in prefix.iter().enumerate() {
        if path[start_idx + i] != elem {
            return false;
        }
    }
    if last_is_param {
        let last_tag = &path[path.len() - 1];
        last_tag == "CssParameter" || last_tag == "SvgParameter"
    } else {
        true
    }
}

fn parse_dash_array(val: &str) -> Result<Vec<f32>, SldError> {
    let mut result = Vec::new();
    for part in val.split([' ', ',']) {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            let num = trimmed.parse::<f32>().map_err(|e| {
                SldError::InvalidValue(format!("Invalid dash_array value '{}': {}", trimmed, e))
            })?;
            result.push(num);
        }
    }
    if result.is_empty() {
        return Err(SldError::InvalidValue("Empty dash_array".to_string()));
    }
    Ok(result)
}

/// State-machine parser for OGC SLD XML documents.
struct SldParser {
    registry: StyleRegistry,
    path: Vec<String>,
    current_layer_name: Option<String>,
    current_rules: Vec<RuleStyle>,
    current_rule: Option<RuleStyle>,
    current_param_name: Option<String>,
}

impl SldParser {
    fn new() -> Self {
        Self {
            registry: StyleRegistry::default(),
            path: Vec::new(),
            current_layer_name: None,
            current_rules: Vec::new(),
            current_rule: None,
            current_param_name: None,
        }
    }

    /// Parse the `name` attribute from a `CssParameter` or `SvgParameter` start tag.
    fn extract_param_name(&mut self, e: &BytesStart) -> Result<(), SldError> {
        self.current_param_name = None;
        for attr in e.attributes() {
            let attr = attr.map_err(|err| SldError::XmlError(err.to_string()))?;
            let key = attr.key.into_inner();
            let local_key = if let Some(pos) = key.iter().position(|&x| x == b':') {
                &key[pos + 1..]
            } else {
                key
            };
            if local_key == b"name" {
                self.current_param_name = Some(String::from_utf8_lossy(&attr.value).into_owned());
                break;
            }
        }
        Ok(())
    }

    /// Create a fresh `RuleStyle` or, if a rule already exists, initialise one of the
    /// four symbolizer defaults.  This logic is shared between `Start` and `Empty` events.
    fn init_rule_or_symbolizer(&mut self, tag: &str) {
        if tag == "Rule" {
            self.current_rule = Some(RuleStyle {
                name: String::new(),
                min_scale: None,
                max_scale: None,
                stroke: None,
                fill: None,
                text: None,
                point: None,
            });
        } else if let Some(ref mut rule) = self.current_rule {
            match tag {
                "LineSymbolizer" => {
                    rule.stroke = Some(StrokeStyle {
                        color: "#000000".to_string(),
                        width: 1.0,
                        dash_array: None,
                    });
                }
                "PolygonSymbolizer" => {
                    rule.fill = Some(FillStyle {
                        color: "#000000".to_string(),
                        opacity: 1.0,
                    });
                }
                "TextSymbolizer" => {
                    rule.text = Some(TextStyle {
                        label_expression: String::new(),
                        font_family: "sans-serif".to_string(),
                        font_size: 10.0,
                        fill_color: "#000000".to_string(),
                    });
                }
                "PointSymbolizer" => {
                    rule.point = Some(PointStyle {
                        well_known_name: "circle".to_string(),
                        size: 6.0,
                        fill_color: None,
                        stroke_color: None,
                        stroke_width: None,
                    });
                }
                _ => {}
            }
        }
    }

    /// Finalise a `<Rule>` or `<NamedLayer>` element (used for both Start+End and Empty).
    fn finalise_element(&mut self, tag: &str) {
        if tag == "Rule" {
            if let Some(rule) = self.current_rule.take() {
                self.current_rules.push(rule);
            }
        } else if tag == "NamedLayer" {
            if let Some(layer_name) = self.current_layer_name.take() {
                let rules = std::mem::take(&mut self.current_rules);
                self.registry.layers.insert(layer_name, rules);
            } else {
                self.current_rules.clear();
            }
        }
    }

    /// Dispatch text content based on the current XML path.
    fn apply_text(&mut self, text: &str) -> Result<(), SldError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let text_val = trimmed.to_string();

        if path_matches(&self.path, &["NamedLayer", "Name"], false) {
            self.current_layer_name = Some(text_val);
        } else if path_matches(&self.path, &["Rule", "Name"], false) {
            if let Some(ref mut rule) = self.current_rule {
                rule.name = text_val;
            }
        } else if path_matches(&self.path, &["Rule", "MinScaleDenominator"], false) {
            if let Some(ref mut rule) = self.current_rule {
                let val = text_val.parse::<f64>().map_err(|err| {
                    SldError::InvalidValue(format!("Invalid MinScaleDenominator '{}': {}", text_val, err))
                })?;
                rule.min_scale = Some(val);
            }
        } else if path_matches(&self.path, &["Rule", "MaxScaleDenominator"], false) {
            if let Some(ref mut rule) = self.current_rule {
                let val = text_val.parse::<f64>().map_err(|err| {
                    SldError::InvalidValue(format!("Invalid MaxScaleDenominator '{}': {}", text_val, err))
                })?;
                rule.max_scale = Some(val);
            }
        } else if path_matches(&self.path, &["LineSymbolizer", "Stroke"], true) {
            self.apply_stroke_param(&text_val)?;
        } else if path_matches(&self.path, &["PolygonSymbolizer", "Fill"], true) {
            self.apply_fill_param(&text_val)?;
        } else if path_matches(&self.path, &["TextSymbolizer", "Label", "PropertyName"], false) {
            if let Some(ref mut rule) = self.current_rule {
                if let Some(ref mut text) = rule.text {
                    text.label_expression = text_val;
                }
            }
        } else if path_matches(&self.path, &["TextSymbolizer", "Font"], true) {
            self.apply_font_param(&text_val)?;
        } else if path_matches(&self.path, &["TextSymbolizer", "Fill"], true) {
            self.apply_text_fill_param(&text_val)?;
        } else if path_matches(&self.path, &["PointSymbolizer", "Graphic", "Mark", "WellKnownName"], false) {
            if let Some(ref mut rule) = self.current_rule {
                if let Some(ref mut point) = rule.point {
                    point.well_known_name = text_val;
                }
            }
        } else if path_matches(&self.path, &["PointSymbolizer", "Graphic", "Mark", "Fill"], true) {
            self.apply_point_fill_param(&text_val)?;
        } else if path_matches(&self.path, &["PointSymbolizer", "Graphic", "Mark", "Stroke"], true) {
            self.apply_point_stroke_param(&text_val)?;
        } else if path_matches(&self.path, &["PointSymbolizer", "Graphic", "Size"], false) {
            if let Some(ref mut rule) = self.current_rule {
                if let Some(ref mut point) = rule.point {
                    let s = text_val.parse::<f32>().map_err(|err| {
                        SldError::InvalidValue(format!("Invalid Point size '{}': {}", text_val, err))
                    })?;
                    point.size = s;
                }
            }
        }

        Ok(())
    }

    fn apply_stroke_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut stroke) = rule.stroke {
                if let Some(ref param) = self.current_param_name {
                    match param.as_str() {
                        "stroke" => stroke.color = text_val.to_string(),
                        "stroke-width" => {
                            stroke.width = text_val.parse::<f32>().map_err(|err| {
                                SldError::InvalidValue(format!("Invalid stroke-width '{}': {}", text_val, err))
                            })?;
                        }
                        "stroke-dasharray" => {
                            stroke.dash_array = Some(parse_dash_array(text_val)?);
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_fill_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut fill) = rule.fill {
                if let Some(ref param) = self.current_param_name {
                    match param.as_str() {
                        "fill" => fill.color = text_val.to_string(),
                        "fill-opacity" => {
                            fill.opacity = text_val.parse::<f32>().map_err(|err| {
                                SldError::InvalidValue(format!("Invalid fill-opacity '{}': {}", text_val, err))
                            })?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_font_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut text) = rule.text {
                if let Some(ref param) = self.current_param_name {
                    match param.as_str() {
                        "font-family" => text.font_family = text_val.to_string(),
                        "font-size" => {
                            text.font_size = text_val.parse::<f32>().map_err(|err| {
                                SldError::InvalidValue(format!("Invalid font-size '{}': {}", text_val, err))
                            })?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_text_fill_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut text) = rule.text {
                if let Some(ref param) = self.current_param_name {
                    if param == "fill" {
                        text.fill_color = text_val.to_string();
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_point_fill_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut point) = rule.point {
                if let Some(ref param) = self.current_param_name {
                    if param == "fill" {
                        point.fill_color = Some(text_val.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_point_stroke_param(&mut self, text_val: &str) -> Result<(), SldError> {
        if let Some(ref mut rule) = self.current_rule {
            if let Some(ref mut point) = rule.point {
                if let Some(ref param) = self.current_param_name {
                    match param.as_str() {
                        "stroke" => point.stroke_color = Some(text_val.to_string()),
                        "stroke-width" => {
                            let w = text_val.parse::<f32>().map_err(|err| {
                                SldError::InvalidValue(format!("Invalid Point stroke-width '{}': {}", text_val, err))
                            })?;
                            point.stroke_width = Some(w);
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

/// Parse an SLD XML document into a [`StyleRegistry`].
pub fn parse(xml_content: &str) -> Result<StyleRegistry, SldError> {
    let mut reader = Reader::from_str(xml_content);
    reader.trim_text(true);

    let mut parser = SldParser::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = local_name_start(e);
                if tag == "CssParameter" || tag == "SvgParameter" {
                    parser.extract_param_name(e)?;
                }
                parser.init_rule_or_symbolizer(&tag);
                parser.path.push(tag);
            }
            Ok(Event::End(ref e)) => {
                let tag = local_name_end(e);
                if tag == "CssParameter" || tag == "SvgParameter" {
                    parser.current_param_name = None;
                }
                parser.finalise_element(&tag);
                if !parser.path.is_empty() && parser.path.last().unwrap() == &tag {
                    parser.path.pop();
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = local_name_start(e);
                if tag == "CssParameter" || tag == "SvgParameter" {
                    parser.extract_param_name(e)?;
                }
                parser.init_rule_or_symbolizer(&tag);
                parser.finalise_element(&tag);
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().map_err(|err| SldError::XmlError(err.to_string()))?.into_owned();
                parser.apply_text(&text)?;
            }
            Ok(Event::Eof) => {
                if !parser.path.is_empty() {
                    return Err(SldError::XmlError(format!(
                        "Malformed XML: unclosed tags: {:?}",
                        parser.path
                    )));
                }
                break;
            }
            Err(e) => return Err(SldError::XmlError(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(parser.registry)
}
