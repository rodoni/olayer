use crate::errors::{MapError, INVALID_CAPABILITIES};
use crate::{QueryParameter, UrlParts};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_XML_BYTES: usize = 1024 * 1024;
const MAX_XML_DEPTH: usize = 64;
const MAX_IDENTIFIERS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceType {
    Wms,
    Wmts,
    Wfs,
    Wcs,
}

impl ServiceType {
    pub const fn parameter(self) -> &'static str {
        match self {
            Self::Wms => "WMS",
            Self::Wmts => "WMTS",
            Self::Wfs => "WFS",
            Self::Wcs => "WCS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitiesRequest {
    pub endpoint: UrlParts,
    pub service: ServiceType,
}

impl CapabilitiesRequest {
    pub fn new(endpoint: UrlParts, service: ServiceType) -> Self {
        Self { endpoint, service }
    }

    pub fn url(&self) -> String {
        self.endpoint.url(&[
            QueryParameter::new("SERVICE", self.service.parameter()),
            QueryParameter::new("REQUEST", "GetCapabilities"),
        ])
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub service: ServiceType,
    pub version: String,
    pub title: String,
    pub layers: Vec<String>,
}
impl Capabilities {
    pub fn validate(self) -> Result<Self, MapError> {
        if self.version.is_empty() || self.title.is_empty() {
            Err(MapError::new(
                INVALID_CAPABILITIES,
                "capabilities version and title are required",
            ))
        } else {
            Ok(self)
        }
    }

    /// Parses a capabilities summary without resolving DTDs or external entities.
    ///
    /// # Errors
    /// Returns `INVALID_CAPABILITIES` for malformed or incomplete XML, DTDs,
    /// unsupported roots, missing title/version, or resource limits exceeding
    /// 1 MiB of UTF-8 input, 64 nested elements, or 4,096 identifiers.
    /// This is a summary parser, not OGC schema or namespace validation.
    pub fn parse_xml(xml: &[u8]) -> Result<Self, MapError> {
        if xml.len() > MAX_XML_BYTES {
            return Err(invalid("capabilities XML exceeds 1 MiB"));
        }
        std::str::from_utf8(xml).map_err(|_| invalid("capabilities XML must be UTF-8"))?;
        let mut reader = Reader::from_reader(xml);
        reader.check_end_names(true);
        reader.check_comments(true);
        let mut buffer = Vec::new();
        let mut stack: Vec<Element> = Vec::new();
        let mut service = None;
        let mut version = String::new();
        let mut title = String::new();
        let mut layers = Vec::new();
        let mut declaration_seen = false;

        loop {
            let position = reader.buffer_position();
            let event = reader
                .read_event_into(&mut buffer)
                .map_err(|error| invalid(error.to_string()))?;
            if matches!(event, Event::Start(_) | Event::Empty(_) | Event::End(_))
                && reader
                    .buffer_position()
                    .checked_sub(1)
                    .and_then(|last| xml.get(last))
                    != Some(&b'>')
            {
                return Err(invalid("unterminated XML element"));
            }
            let empty = matches!(event, Event::Empty(_));
            match event {
                Event::Start(event) | Event::Empty(event) => {
                    if stack.len() >= MAX_XML_DEPTH {
                        return Err(invalid("capabilities XML exceeds depth 64"));
                    }
                    let name = std::str::from_utf8(event.name().as_ref())
                        .map_err(|_| invalid("invalid XML element name"))?
                        .to_owned();
                    if stack.is_empty() {
                        if service.is_some() {
                            return Err(invalid("capabilities XML has multiple root elements"));
                        }
                        service = Some(
                            service_from_root(local_name(&name))
                                .ok_or_else(|| invalid("unsupported capabilities root element"))?,
                        );
                    }
                    // Validate every attribute, including empty/non-root elements.
                    // Do not discard malformed, duplicate, or unescaped attributes.
                    // quick-xml's default duplicate scan is quadratic in the
                    // number of attributes. Keep duplicate rejection bounded
                    // to ordered-set lookups even for attribute-heavy input.
                    let mut attribute_names = BTreeSet::new();
                    for attribute in event.attributes().with_checks(false) {
                        let attribute = attribute.map_err(|error| invalid(error.to_string()))?;
                        if !attribute_names.insert(attribute.key.as_ref().to_owned()) {
                            return Err(invalid("duplicate XML attribute"));
                        }
                        let value = attribute
                            .decode_and_unescape_value(&reader)
                            .map_err(|error| invalid(error.to_string()))?;
                        if stack.is_empty() && attribute.key.as_ref() == b"version" {
                            version = value.into_owned();
                        }
                    }
                    let element = Element {
                        name,
                        text: String::new(),
                    };
                    if empty {
                        finish_element(element, &stack, &mut title, &mut layers)?;
                    } else {
                        stack.push(element);
                    }
                }
                Event::Text(text) => {
                    let value = text
                        .unescape()
                        .map_err(|error| invalid(error.to_string()))?;
                    if let Some(element) = stack.last_mut() {
                        element.text.push_str(&value);
                    } else if !value.trim().is_empty() {
                        return Err(invalid("text outside capabilities root element"));
                    }
                }
                Event::CData(text) => {
                    let value = std::str::from_utf8(text.as_ref())
                        .map_err(|_| invalid("CDATA must be UTF-8"))?;
                    let element = stack
                        .last_mut()
                        .ok_or_else(|| invalid("CDATA outside capabilities root element"))?;
                    element.text.push_str(value);
                }
                Event::End(event) => {
                    let element = stack
                        .pop()
                        .ok_or_else(|| invalid("unexpected closing element"))?;
                    if element.name.as_bytes() != event.name().as_ref() {
                        return Err(invalid("mismatched closing element"));
                    }
                    finish_element(element, &stack, &mut title, &mut layers)?;
                }
                Event::DocType(_) => return Err(invalid("DTDs are not supported")),
                Event::Decl(declaration) => {
                    if service.is_some() || declaration_seen {
                        return Err(invalid("misplaced XML declaration"));
                    }
                    declaration
                        .version()
                        .map_err(|error| invalid(error.to_string()))?;
                    if let Some(encoding) = declaration.encoding() {
                        let encoding = encoding.map_err(|error| invalid(error.to_string()))?;
                        if !encoding.eq_ignore_ascii_case(b"UTF-8") {
                            return Err(invalid("capabilities XML must declare UTF-8"));
                        }
                    }
                    declaration_seen = true;
                }
                Event::Eof => {
                    if !stack.is_empty() {
                        return Err(invalid("truncated capabilities XML"));
                    }
                    // quick-xml 0.31 can report EOF after consuming a lone '<'.
                    // EOF must not silently discard a partial markup token.
                    let remaining = xml
                        .get(position..)
                        .ok_or_else(|| invalid("truncated XML markup"))?;
                    if remaining.iter().any(|byte| !byte.is_ascii_whitespace()) {
                        return Err(invalid("truncated XML markup"));
                    }
                    break;
                }
                _ => {}
            }
            buffer.clear();
        }

        let service = service.ok_or_else(|| {
            MapError::new(
                INVALID_CAPABILITIES,
                "unsupported capabilities root element",
            )
        })?;
        Self {
            service,
            version,
            title,
            layers,
        }
        .validate()
    }
}

struct Element {
    name: String,
    text: String,
}

fn invalid(message: impl Into<String>) -> MapError {
    MapError::new(INVALID_CAPABILITIES, message)
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or_default()
}

fn finish_element(
    element: Element,
    ancestors: &[Element],
    title: &mut String,
    identifiers: &mut Vec<String>,
) -> Result<(), MapError> {
    let value = element.text.trim();
    if value.is_empty() {
        return Ok(());
    }
    let name = local_name(&element.name);
    if name == "Title" && title.is_empty() {
        *title = value.to_owned();
    } else if ancestors.iter().any(|parent| {
        matches!(
            (local_name(&parent.name), name),
            ("Layer" | "FeatureType", "Name")
                | ("Layer" | "CoverageSummary", "Identifier")
                | ("CoverageSummary", "CoverageId")
        )
    }) {
        if identifiers.len() >= MAX_IDENTIFIERS {
            return Err(invalid("capabilities XML exceeds 4096 identifiers"));
        }
        identifiers.push(value.to_owned());
    }
    Ok(())
}

fn service_from_root(root: &str) -> Option<ServiceType> {
    match root {
        "WMS_Capabilities" | "WMT_MS_Capabilities" => Some(ServiceType::Wms),
        "Capabilities" => Some(ServiceType::Wmts),
        "WFS_Capabilities" => Some(ServiceType::Wfs),
        "WCS_Capabilities" => Some(ServiceType::Wcs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use std::num::NonZeroU32;
    #[test]
    fn wmts_query_is_deterministic_and_escaped() {
        let crs = CoordinateReferenceSystem::epsg(3857).unwrap();
        let matrix = TileMatrix {
            identifier: "0".into(),
            crs,
            matrix_width: NonZeroU32::new(1).unwrap(),
            matrix_height: NonZeroU32::new(1).unwrap(),
            tile_width: NonZeroU32::new(256).unwrap(),
            tile_height: NonZeroU32::new(256).unwrap(),
        };
        let req = WmtsRequestBuilder::new(
            UrlParts::new("https://example.test", "/ows"),
            "roads & water",
        )
        .matrix_set("GoogleMapsCompatible")
        .matrix(matrix)
        .tile(TileKey { x: 0, y: 0, z: 0 })
        .get_tile()
        .unwrap();
        assert!(req.url().contains("roads%20%26%20water"));
        assert!(req.url().contains("TILECOL=0"));
    }
    #[test]
    fn tile_outside_matrix_is_rejected() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let matrix = TileMatrix {
            identifier: "0".into(),
            crs,
            matrix_width: NonZeroU32::new(1).unwrap(),
            matrix_height: NonZeroU32::new(1).unwrap(),
            tile_width: NonZeroU32::new(256).unwrap(),
            tile_height: NonZeroU32::new(256).unwrap(),
        };
        let result = WmtsRequestBuilder::new(UrlParts::new("https://example.test", "ows"), "layer")
            .matrix_set("set")
            .matrix(matrix)
            .tile(TileKey { x: 1, y: 0, z: 0 })
            .get_tile();
        assert_eq!(result.unwrap_err().code.as_str(), "OL-CORE-MAP-0004");
    }
    #[test]
    fn wms_requires_finite_ordered_bounds() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        assert!(BoundingBox::new(0.0, 1.0, 0.0, 2.0, crs).is_err());
    }

    #[test]
    fn capabilities_request_is_deterministic() {
        let request = CapabilitiesRequest::new(
            UrlParts::new("https://example.test", "ows"),
            ServiceType::Wms,
        );
        assert_eq!(
            request.url(),
            "https://example.test/ows?SERVICE=WMS&REQUEST=GetCapabilities"
        );
    }

    #[test]
    fn parses_wms_capabilities_layers_and_title() {
        let xml = br#"<WMS_Capabilities version="1.3.0"><Service><Title>Demo &amp; Maps</Title></Service><Capability><Layer><Name>roads</Name><Layer><Name>water</Name></Layer></Layer></Capability></WMS_Capabilities>"#;
        let capabilities = Capabilities::parse_xml(xml).unwrap();
        assert_eq!(capabilities.service, ServiceType::Wms);
        assert_eq!(capabilities.version, "1.3.0");
        assert_eq!(capabilities.title, "Demo & Maps");
        assert_eq!(capabilities.layers, ["roads", "water"]);
    }

    #[test]
    fn rejects_malformed_capabilities_documents() {
        let result = Capabilities::parse_xml(br#"<Unknown version="1.0"/>"#);
        assert_eq!(result.unwrap_err().code, INVALID_CAPABILITIES);
    }

    #[test]
    fn parses_all_supported_capabilities_roots() {
        let cases = [
            (br#"<Capabilities version="1.0.0"><ServiceIdentification><Title>WMTS</Title></ServiceIdentification><Contents><Layer><Identifier>roads</Identifier></Layer></Contents></Capabilities>"#.as_slice(), ServiceType::Wmts),
            (br#"<WFS_Capabilities version="2.0.0"><ServiceIdentification><Title>WFS</Title></ServiceIdentification><FeatureTypeList><FeatureType><Name>roads</Name></FeatureType></FeatureTypeList></WFS_Capabilities>"#.as_slice(), ServiceType::Wfs),
            (br#"<WCS_Capabilities version="2.0.1"><ServiceIdentification><Title>WCS</Title></ServiceIdentification><Contents><CoverageSummary><Identifier>elevation</Identifier></CoverageSummary></Contents></WCS_Capabilities>"#.as_slice(), ServiceType::Wcs),
        ];
        for (xml, expected_service) in cases {
            assert_eq!(
                Capabilities::parse_xml(xml).unwrap().service,
                expected_service
            );
        }
    }
}
