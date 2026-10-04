use crate::errors::{INVALID_COVERAGE_DIMENSIONS, INVALID_WCS};
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub bounds: BoundingBox,
    pub width: u32,
    pub height: u32,
    pub sample_type: crate::CoverageSampleType,
    pub nodata: Option<f64>,
    pub samples: Box<[f64]>,
}

impl Coverage {
    pub fn validate(self) -> Result<Self, MapError> {
        let expected = self.width.checked_mul(self.height).ok_or_else(|| {
            MapError::new(INVALID_COVERAGE_DIMENSIONS, "coverage dimensions overflow")
        })?;
        let expected = usize::try_from(expected).map_err(|_| {
            MapError::new(
                INVALID_COVERAGE_DIMENSIONS,
                "coverage dimensions exceed addressable memory",
            )
        })?;
        if expected == 0 || self.samples.len() != expected {
            return Err(MapError::new(
                INVALID_COVERAGE_DIMENSIONS,
                "sample count does not match dimensions",
            ));
        }
        Ok(self)
    }
}
use crate::{BoundingBox, CoordinateReferenceSystem, MapError, QueryParameter, UrlParts};

#[derive(Debug, Clone, PartialEq)]
pub struct WcsGetCoverageRequest {
    pub endpoint: UrlParts,
    pub coverage_id: String,
    pub crs: CoordinateReferenceSystem,
    pub bounds: BoundingBox,
    pub width: u32,
    pub height: u32,
    pub format: String,
}
impl WcsGetCoverageRequest {
    pub fn url(&self) -> String {
        self.endpoint.url(&[
            QueryParameter::new("SERVICE", "WCS"),
            QueryParameter::new("REQUEST", "GetCoverage"),
            QueryParameter::new("VERSION", "2.0.1"),
            QueryParameter::new("COVERAGEID", &self.coverage_id),
            QueryParameter::new(
                "SUBSET",
                format!("x({},{})", self.bounds.min_x, self.bounds.max_x),
            ),
            QueryParameter::new(
                "SUBSET",
                format!("y({},{})", self.bounds.min_y, self.bounds.max_y),
            ),
            QueryParameter::new("SUBSETTINGCRS", self.crs.authority_code()),
            QueryParameter::new("FORMAT", &self.format),
            QueryParameter::new("SIZE", format!("x({})", self.width)),
            QueryParameter::new("SIZE", format!("y({})", self.height)),
        ])
    }
}
#[derive(Debug, Clone)]
pub struct WcsRequestBuilder {
    endpoint: UrlParts,
    coverage_id: String,
    crs: Option<CoordinateReferenceSystem>,
    bounds: Option<BoundingBox>,
    width: u32,
    height: u32,
    format: String,
}
impl WcsRequestBuilder {
    pub fn new(endpoint: UrlParts, coverage_id: impl Into<String>) -> Self {
        Self {
            endpoint,
            coverage_id: coverage_id.into(),
            crs: None,
            bounds: None,
            width: 0,
            height: 0,
            format: "image/tiff".into(),
        }
    }
    pub fn crs(mut self, value: CoordinateReferenceSystem) -> Self {
        self.crs = Some(value);
        self
    }
    pub fn bounds(mut self, value: BoundingBox) -> Self {
        self.bounds = Some(value);
        self
    }
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    pub fn format(mut self, value: impl Into<String>) -> Self {
        self.format = value.into();
        self
    }
    pub fn get_coverage(self) -> Result<WcsGetCoverageRequest, MapError> {
        if self.coverage_id.is_empty() {
            return Err(MapError::new(INVALID_WCS, "coverage identifier is empty"));
        }
        if self.width == 0 || self.height == 0 {
            return Err(MapError::new(
                INVALID_COVERAGE_DIMENSIONS,
                "coverage dimensions must be non-zero",
            ));
        }
        let crs = self
            .crs
            .ok_or_else(|| MapError::new(INVALID_WCS, "CRS is required"))?;
        let bounds = self
            .bounds
            .ok_or_else(|| MapError::new(INVALID_WCS, "bounds are required"))?;
        Ok(WcsGetCoverageRequest {
            endpoint: self.endpoint,
            coverage_id: self.coverage_id,
            crs,
            bounds,
            width: self.width,
            height: self.height,
            format: self.format,
        })
    }
}
