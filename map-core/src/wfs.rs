use crate::errors::INVALID_WFS;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCollection {
    pub crs: CoordinateReferenceSystem,
    pub features: Vec<Feature>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: Option<String>,
    pub geometry: Value,
    pub properties: BTreeMap<String, Value>,
}

impl FeatureCollection {
    pub fn validate(
        crs: CoordinateReferenceSystem,
        features: Vec<Feature>,
    ) -> Result<Self, MapError> {
        if features
            .iter()
            .any(|feature| !feature.geometry.is_object() && !feature.geometry.is_null())
        {
            return Err(MapError::new(
                INVALID_WFS,
                "feature geometry must be an object or null",
            ));
        }
        Ok(Self { crs, features })
    }
}
use crate::{CoordinateReferenceSystem, MapError, QueryParameter, UrlParts, VectorFormat};

#[derive(Debug, Clone, PartialEq)]
pub struct WfsGetFeatureRequest {
    pub endpoint: UrlParts,
    pub type_name: String,
    pub crs: CoordinateReferenceSystem,
    pub output_format: VectorFormat,
    pub bbox: Option<[f64; 4]>,
    pub count: Option<u32>,
}
impl WfsGetFeatureRequest {
    pub fn url(&self) -> String {
        let mut p = vec![
            QueryParameter::new("SERVICE", "WFS"),
            QueryParameter::new("REQUEST", "GetFeature"),
            QueryParameter::new("VERSION", "2.0.0"),
            QueryParameter::new("TYPENAMES", &self.type_name),
            QueryParameter::new("SRSNAME", self.crs.authority_code()),
            QueryParameter::new("OUTPUTFORMAT", self.output_format.mime()),
        ];
        if let Some(b) = self.bbox {
            p.push(QueryParameter::new(
                "BBOX",
                format!("{},{},{},{}", b[0], b[1], b[2], b[3]),
            ));
        }
        if let Some(c) = self.count {
            p.push(QueryParameter::new("COUNT", c.to_string()));
        }
        self.endpoint.url(&p)
    }
}
#[derive(Debug, Clone)]
pub struct WfsRequestBuilder {
    endpoint: UrlParts,
    type_name: String,
    crs: Option<CoordinateReferenceSystem>,
    output_format: VectorFormat,
    bbox: Option<[f64; 4]>,
    count: Option<u32>,
}
impl WfsRequestBuilder {
    pub fn new(endpoint: UrlParts, type_name: impl Into<String>) -> Self {
        Self {
            endpoint,
            type_name: type_name.into(),
            crs: None,
            output_format: VectorFormat::GeoJson,
            bbox: None,
            count: None,
        }
    }
    pub fn crs(mut self, value: CoordinateReferenceSystem) -> Self {
        self.crs = Some(value);
        self
    }
    pub fn bbox(mut self, value: [f64; 4]) -> Self {
        self.bbox = Some(value);
        self
    }
    pub fn count(mut self, value: u32) -> Self {
        self.count = Some(value);
        self
    }
    pub fn output_format(mut self, value: VectorFormat) -> Self {
        self.output_format = value;
        self
    }
    pub fn get_feature(self) -> Result<WfsGetFeatureRequest, MapError> {
        if self.type_name.is_empty() {
            return Err(MapError::new(INVALID_WFS, "type name is empty"));
        }
        let crs = self
            .crs
            .ok_or_else(|| MapError::new(INVALID_WFS, "CRS is required"))?;
        if let Some(b) = self.bbox {
            if !b.iter().all(|v| v.is_finite()) || b[0] >= b[2] || b[1] >= b[3] {
                return Err(MapError::new(INVALID_WFS, "invalid BBOX"));
            }
        }
        Ok(WfsGetFeatureRequest {
            endpoint: self.endpoint,
            type_name: self.type_name,
            crs,
            output_format: self.output_format,
            bbox: self.bbox,
            count: self.count,
        })
    }
}
