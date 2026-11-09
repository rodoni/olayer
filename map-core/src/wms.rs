use crate::errors::INVALID_WMS;
use crate::{
    BoundingBox, CoordinateReferenceSystem, ImageFormat, MapError, QueryParameter, UrlParts,
};
use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq)]
pub struct WmsGetMapRequest {
    pub endpoint: UrlParts,
    pub layers: String,
    pub styles: String,
    /// Canonical source-CRS x/y bounds (longitude/latitude degrees for EPSG:4326).
    /// Only URL serialization changes these to WMS 1.3.0 wire axis order.
    pub bounds: BoundingBox,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub format: ImageFormat,
    pub transparent: bool,
}
impl WmsGetMapRequest {
    /// Serializes WMS 1.3.0, swapping EPSG:4326 axes on the wire only.
    pub fn url(&self) -> String {
        // Bounds use x/y internally; WMS 1.3.0 follows EPSG axis order on the wire.
        let bounds = if self.bounds.crs.epsg_code() == 4326 {
            [
                self.bounds.min_y,
                self.bounds.min_x,
                self.bounds.max_y,
                self.bounds.max_x,
            ]
        } else {
            [
                self.bounds.min_x,
                self.bounds.min_y,
                self.bounds.max_x,
                self.bounds.max_y,
            ]
        };
        self.endpoint.url(&[
            QueryParameter::new("SERVICE", "WMS"),
            QueryParameter::new("REQUEST", "GetMap"),
            QueryParameter::new("VERSION", "1.3.0"),
            QueryParameter::new("LAYERS", &self.layers),
            QueryParameter::new("STYLES", &self.styles),
            QueryParameter::new("CRS", self.bounds.crs.authority_code()),
            QueryParameter::new("BBOX", bounds.map(|value| value.to_string()).join(",")),
            QueryParameter::new("WIDTH", self.width.to_string()),
            QueryParameter::new("HEIGHT", self.height.to_string()),
            QueryParameter::new("FORMAT", self.format.mime()),
            QueryParameter::new("TRANSPARENT", self.transparent.to_string()),
        ])
    }
}
#[derive(Debug, Clone)]
pub struct WmsRequestBuilder {
    endpoint: UrlParts,
    layers: String,
    styles: String,
    bounds: Option<BoundingBox>,
    width: Option<NonZeroU32>,
    height: Option<NonZeroU32>,
    format: ImageFormat,
    transparent: bool,
}
impl WmsRequestBuilder {
    pub fn new(endpoint: UrlParts, layers: impl Into<String>) -> Self {
        Self {
            endpoint,
            layers: layers.into(),
            styles: String::new(),
            bounds: None,
            width: None,
            height: None,
            format: ImageFormat::Png,
            transparent: true,
        }
    }
    pub fn styles(mut self, value: impl Into<String>) -> Self {
        self.styles = value.into();
        self
    }
    pub fn bounds(mut self, value: BoundingBox) -> Self {
        self.bounds = Some(value);
        self
    }
    pub fn size(mut self, width: NonZeroU32, height: NonZeroU32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
    pub fn format(mut self, value: ImageFormat) -> Self {
        self.format = value;
        self
    }
    pub fn transparent(mut self, value: bool) -> Self {
        self.transparent = value;
        self
    }
    pub fn get_map(self) -> Result<WmsGetMapRequest, MapError> {
        Ok(WmsGetMapRequest {
            endpoint: self.endpoint,
            layers: if self.layers.is_empty() {
                return Err(MapError::new(INVALID_WMS, "layer is empty"));
            } else {
                self.layers
            },
            styles: self.styles,
            bounds: self
                .bounds
                .ok_or_else(|| MapError::new(INVALID_WMS, "bounds are required"))?,
            width: self
                .width
                .ok_or_else(|| MapError::new(INVALID_WMS, "width is required"))?,
            height: self
                .height
                .ok_or_else(|| MapError::new(INVALID_WMS, "height is required"))?,
            format: self.format,
            transparent: self.transparent,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct MapImage {
    pub pixels: Box<[u8]>,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub format: ImageFormat,
    pub crs: CoordinateReferenceSystem,
    pub bounds: BoundingBox,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wms_13_serializes_epsg_axis_order_without_mutating_xy_bounds() {
        for (code, expected) in [(4326, "2%2C1%2C4%2C3"), (3857, "1%2C2%2C3%2C4")] {
            let bounds = BoundingBox::new(
                1.0,
                2.0,
                3.0,
                4.0,
                CoordinateReferenceSystem::epsg(code).unwrap(),
            )
            .unwrap();
            let request =
                WmsRequestBuilder::new(UrlParts::new("https://example.test", "wms"), "fixture")
                    .bounds(bounds)
                    .size(NonZeroU32::new(2).unwrap(), NonZeroU32::new(2).unwrap())
                    .get_map()
                    .unwrap();
            assert!(request.url().contains(&format!("BBOX={expected}")));
            assert_eq!(request.bounds, bounds);
        }
    }
}
