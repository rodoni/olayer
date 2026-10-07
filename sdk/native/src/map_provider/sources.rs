use super::decoders::{
    decode_wcs_json_coverage, decode_wcs_raster_coverage, decode_wfs_feature_collection,
    decode_wms_image,
};
use super::ogc::NativeOgcClient;
use super::NativeMapDataError;
use olayer_map_core::{
    BoundingBox, CoordinateReferenceSystem, Coverage, CoverageSampleType, FeatureCollection,
    ImageFormat, MapImage, UrlParts, VectorFormat, WcsRequestBuilder, WfsRequestBuilder,
    WmsRequestBuilder,
};
use std::num::NonZeroU32;

/// Native WMS source backed by an injected OGC client.
pub struct WmsMapSource<T> {
    client: NativeOgcClient<T>,
    endpoint: UrlParts,
    layer: String,
    crs: CoordinateReferenceSystem,
    bounds: BoundingBox,
    width: NonZeroU32,
    height: NonZeroU32,
    format: ImageFormat,
}

impl<T: super::ogc::OgcTransport> WmsMapSource<T> {
    pub fn new(
        client: NativeOgcClient<T>,
        endpoint: UrlParts,
        layer: impl Into<String>,
        crs: CoordinateReferenceSystem,
        bounds: BoundingBox,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Self {
        Self {
            client,
            endpoint,
            layer: layer.into(),
            crs,
            bounds,
            width,
            height,
            format: ImageFormat::Png,
        }
    }

    pub fn format(mut self, format: ImageFormat) -> Self {
        self.format = format;
        self
    }

    pub fn fetch(&self) -> Result<MapImage, NativeMapDataError> {
        let request = WmsRequestBuilder::new(self.endpoint.clone(), self.layer.clone())
            .bounds(self.bounds)
            .size(self.width, self.height)
            .format(self.format)
            .get_map()
            .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
        let bytes = self.client.fetch_wms(&request)?;
        decode_wms_image(&bytes, self.format, self.crs, self.bounds)
    }
}

/// Native WFS source backed by an injected OGC client.
pub struct WfsFeatureSource<T> {
    client: NativeOgcClient<T>,
    endpoint: UrlParts,
    type_name: String,
    crs: CoordinateReferenceSystem,
    bbox: Option<[f64; 4]>,
    count: Option<u32>,
}

impl<T: super::ogc::OgcTransport> WfsFeatureSource<T> {
    pub fn new(
        client: NativeOgcClient<T>,
        endpoint: UrlParts,
        type_name: impl Into<String>,
        crs: CoordinateReferenceSystem,
    ) -> Self {
        Self {
            client,
            endpoint,
            type_name: type_name.into(),
            crs,
            bbox: None,
            count: None,
        }
    }

    pub fn bbox(mut self, bbox: [f64; 4]) -> Self {
        self.bbox = Some(bbox);
        self
    }

    pub fn count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    pub fn fetch(&self) -> Result<FeatureCollection, NativeMapDataError> {
        let mut builder = WfsRequestBuilder::new(self.endpoint.clone(), self.type_name.clone())
            .crs(self.crs)
            .output_format(VectorFormat::GeoJson);
        if let Some(bbox) = self.bbox {
            builder = builder.bbox(bbox);
        }
        if let Some(count) = self.count {
            builder = builder.count(count);
        }
        let request = builder
            .get_feature()
            .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
        let bytes = self.client.fetch_wfs(&request)?;
        decode_wfs_feature_collection(&bytes, self.crs)
    }
}

/// Native WCS source backed by an injected OGC client.
pub struct WcsCoverageSourceConfig {
    pub endpoint: UrlParts,
    pub coverage_id: String,
    pub crs: CoordinateReferenceSystem,
    pub bounds: BoundingBox,
    pub width: u32,
    pub height: u32,
    pub sample_type: CoverageSampleType,
}

pub struct WcsCoverageSource<T> {
    client: NativeOgcClient<T>,
    endpoint: UrlParts,
    coverage_id: String,
    crs: CoordinateReferenceSystem,
    bounds: BoundingBox,
    width: u32,
    height: u32,
    sample_type: CoverageSampleType,
    nodata: Option<f64>,
}

impl<T: super::ogc::OgcTransport> WcsCoverageSource<T> {
    pub fn new(client: NativeOgcClient<T>, config: WcsCoverageSourceConfig) -> Self {
        Self {
            client,
            endpoint: config.endpoint,
            coverage_id: config.coverage_id,
            crs: config.crs,
            bounds: config.bounds,
            width: config.width,
            height: config.height,
            sample_type: config.sample_type,
            nodata: None,
        }
    }

    pub fn nodata(mut self, value: f64) -> Self {
        self.nodata = Some(value);
        self
    }

    fn request(&self) -> Result<olayer_map_core::WcsGetCoverageRequest, NativeMapDataError> {
        WcsRequestBuilder::new(self.endpoint.clone(), self.coverage_id.clone())
            .crs(self.crs)
            .bounds(self.bounds)
            .size(self.width, self.height)
            .format("image/tiff")
            .get_coverage()
            .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))
    }

    pub fn fetch_raster(&self) -> Result<Coverage, NativeMapDataError> {
        let bytes = self.client.fetch_wcs(&self.request()?)?;
        decode_wcs_raster_coverage(&bytes, self.bounds, self.sample_type, self.nodata)
    }

    pub fn fetch_json(&self) -> Result<Coverage, NativeMapDataError> {
        let bytes = self.client.fetch_wcs(&self.request()?)?;
        decode_wcs_json_coverage(&bytes, self.bounds, self.sample_type, self.nodata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_provider::ogc::OgcTransport;
    use std::num::{NonZeroU32, NonZeroUsize};
    use std::sync::{Arc, Mutex};

    struct FakeTransport {
        response: Vec<u8>,
        url: Arc<Mutex<Option<String>>>,
    }

    impl OgcTransport for FakeTransport {
        fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            *self.url.lock().unwrap() = Some(url.to_owned());
            Ok(self.response.clone())
        }
    }

    #[test]
    fn wfs_source_builds_and_decodes_feature_request() {
        let requested_url = Arc::new(Mutex::new(None));
        let transport = FakeTransport {
            response: br#"{"type":"FeatureCollection","features":[]}"#.to_vec(),
            url: requested_url.clone(),
        };
        let client = NativeOgcClient::new(transport, NonZeroUsize::new(1024).unwrap());
        let source = WfsFeatureSource::new(
            client,
            UrlParts::new("https://example.test", "wfs"),
            "roads",
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .count(5);
        assert!(source.fetch().unwrap().features.is_empty());
        assert!(requested_url
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("REQUEST=GetFeature"));
    }

    #[test]
    fn wms_source_decodes_image_response() {
        let mut png = Vec::new();
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageOutputFormat::Png,
            )
            .unwrap();
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let bounds = BoundingBox::new(0.0, 0.0, 1.0, 1.0, crs).unwrap();
        let source = WmsMapSource::new(
            NativeOgcClient::new(
                FakeTransport {
                    response: png,
                    url: Arc::new(Mutex::new(None)),
                },
                NonZeroUsize::new(1024).unwrap(),
            ),
            UrlParts::new("https://example.test", "wms"),
            "roads",
            crs,
            bounds,
            NonZeroU32::new(1).unwrap(),
            NonZeroU32::new(1).unwrap(),
        );
        assert_eq!(source.fetch().unwrap().width.get(), 1);
    }

    #[test]
    fn wcs_source_decodes_json_coverage() {
        let requested_url = Arc::new(Mutex::new(None));
        let source = WcsCoverageSource::new(
            NativeOgcClient::new(
                FakeTransport {
                    response: br#"{"width":2,"height":1,"samples":[10,20]}"#.to_vec(),
                    url: requested_url,
                },
                NonZeroUsize::new(1024).unwrap(),
            ),
            WcsCoverageSourceConfig {
                endpoint: UrlParts::new("https://example.test", "wcs"),
                coverage_id: "elevation".into(),
                crs: CoordinateReferenceSystem::epsg(4326).unwrap(),
                bounds: BoundingBox::new(
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    CoordinateReferenceSystem::epsg(4326).unwrap(),
                )
                .unwrap(),
                width: 2,
                height: 1,
                sample_type: CoverageSampleType::Float64,
            },
        )
        .nodata(-9999.0);
        assert_eq!(source.fetch_json().unwrap().samples.as_ref(), &[10.0, 20.0]);
    }
}
