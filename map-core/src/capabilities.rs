use crate::errors::{MapError, INVALID_CAPABILITIES};
use crate::{QueryParameter, UrlParts};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

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

    pub fn parse_xml(xml: &[u8]) -> Result<Self, MapError> {
        let mut reader = Reader::from_reader(xml);
        reader.trim_text(true);
        let mut buffer = Vec::new();
        let mut stack: Vec<String> = Vec::new();
        let mut service = None;
        let mut version = String::new();
        let mut title = String::new();
        let mut layers = Vec::new();

        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(event)) => {
                    let name = local_name(event.name().as_ref());
                    if stack.is_empty() {
                        service = service_from_root(&name);
                        version = event
                            .attributes()
                            .flatten()
                            .find(|attribute| local_name(attribute.key.as_ref()) == "version")
                            .and_then(|attribute| attribute.decode_and_unescape_value(&reader).ok())
                            .map(|value| value.into_owned())
                            .unwrap_or_default();
                    }
                    stack.push(name);
                }
                Ok(Event::Text(text)) => {
                    let value = text
                        .unescape()
                        .map_err(|error| MapError::new(INVALID_CAPABILITIES, error.to_string()))?
                        .trim()
                        .to_owned();
                    if value.is_empty() {
                        buffer.clear();
                        continue;
                    }
                    if stack.last().map(String::as_str) == Some("Title") && title.is_empty() {
                        title = value;
                    } else if (stack.last().map(String::as_str) == Some("Name")
                        && stack
                            .iter()
                            .any(|name| name == "Layer" || name == "FeatureType"))
                        || (stack.last().map(String::as_str) == Some("Identifier")
                            && stack.iter().any(|name| name == "Layer"))
                    {
                        layers.push(value);
                    }
                }
                Ok(Event::End(_)) => {
                    stack.pop();
                }
                Ok(Event::Eof) => break,
                Err(error) => {
                    return Err(MapError::new(INVALID_CAPABILITIES, error.to_string()));
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

fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    name.rsplit(':').next().unwrap_or_default().to_owned()
}

fn service_from_root(root: &str) -> Option<ServiceType> {
    match root {
        "WMS_Capabilities" => Some(ServiceType::Wms),
        "WMT_MS_Capabilities" | "Capabilities" => Some(ServiceType::Wmts),
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
