use super::NativeMapDataError;
use olayer_map_core::{
    WcsGetCoverageRequest, WfsGetFeatureRequest, WmsGetMapRequest, WmtsGetTileRequest,
};
use std::io::Read;
use std::num::NonZeroUsize;

/// Transport boundary used by the native OGC client.
pub trait OgcTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError>;
}

/// Native `ureq` transport. Authentication and agent policy remain outside the map core.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(agent: ureq::Agent) -> Self {
        Self { agent }
    }
}

impl OgcTransport for UreqTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| NativeMapDataError::OgcTransport(error.to_string()))?;
        let mut body = response.into_reader();
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .map_err(NativeMapDataError::Io)?;
        Ok(bytes)
    }
}

/// Executes pure OGC requests through an injected native transport.
pub struct NativeOgcClient<T> {
    transport: T,
    max_response_bytes: NonZeroUsize,
}

impl<T: OgcTransport> NativeOgcClient<T> {
    pub fn new(transport: T, max_response_bytes: NonZeroUsize) -> Self {
        Self {
            transport,
            max_response_bytes,
        }
    }

    pub fn fetch_wms(&self, request: &WmsGetMapRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    pub fn fetch_wmts(&self, request: &WmtsGetTileRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    pub fn fetch_wfs(&self, request: &WfsGetFeatureRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    pub fn fetch_wcs(
        &self,
        request: &WcsGetCoverageRequest,
    ) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    fn execute(&self, url: String) -> Result<Vec<u8>, NativeMapDataError> {
        let bytes = self.transport.get(&url)?;
        if bytes.len() > self.max_response_bytes.get() {
            return Err(NativeMapDataError::ResponseTooLarge {
                max_bytes: self.max_response_bytes.get(),
            });
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use olayer_map_core::{BoundingBox, CoordinateReferenceSystem, UrlParts, WmsRequestBuilder};

    struct FakeTransport {
        response: Vec<u8>,
        requested_url: std::sync::Mutex<Option<String>>,
    }

    impl OgcTransport for FakeTransport {
        fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            *self.requested_url.lock().unwrap() = Some(url.to_owned());
            Ok(self.response.clone())
        }
    }

    #[test]
    fn executes_shared_wms_request_through_fake_transport() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let bounds = BoundingBox::new(0.0, 1.0, 2.0, 3.0, crs).unwrap();
        let request = WmsRequestBuilder::new(UrlParts::new("https://example.test", "ows"), "roads")
            .bounds(bounds)
            .size(
                std::num::NonZeroU32::new(256).unwrap(),
                std::num::NonZeroU32::new(256).unwrap(),
            )
            .get_map()
            .unwrap();
        let transport = FakeTransport {
            response: vec![1, 2, 3],
            requested_url: std::sync::Mutex::new(None),
        };
        let client = NativeOgcClient::new(transport, NonZeroUsize::new(8).unwrap());
        assert_eq!(client.fetch_wms(&request).unwrap(), vec![1, 2, 3]);
        assert!(client
            .transport
            .requested_url
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("REQUEST=GetMap"));
    }

    #[test]
    fn rejects_responses_over_configured_limit() {
        let transport = FakeTransport {
            response: vec![0; 9],
            requested_url: std::sync::Mutex::new(None),
        };
        let client = NativeOgcClient::new(transport, NonZeroUsize::new(8).unwrap());
        let result = client.execute("https://example.test/oversized".to_owned());
        assert!(matches!(
            result,
            Err(NativeMapDataError::ResponseTooLarge { max_bytes: 8 })
        ));
    }
}
