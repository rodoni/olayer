use super::NativeMapDataError;
use olayer_map_core::{
    WcsGetCoverageRequest, WfsGetFeatureRequest, WmsGetMapRequest, WmtsGetTileRequest,
};
use std::io::Read;
use std::num::NonZeroUsize;

/// Transport boundary used by the native OGC client.
pub trait OgcTransport {
    /// Fetches a complete response using the transport's default size policy.
    ///
    /// # Errors
    /// Returns transport or body-read errors, including any default size limit.
    fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError>;

    /// Fetches a complete response of at most `max_bytes` bytes (zero is valid).
    ///
    /// For backward compatibility, the default calls [`Self::get`] and checks
    /// the completed body. Custom transports must override this method to bound
    /// consumption and allocation while reading, rather than only post-checking.
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::ResponseTooLarge`] for an oversized body,
    /// or propagates the error from [`Self::get`]. No partial body is returned.
    fn get_bounded(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, NativeMapDataError> {
        let bytes = self.get(url)?;
        if bytes.len() > max_bytes {
            return Err(NativeMapDataError::ResponseTooLarge { max_bytes });
        }
        Ok(bytes)
    }
}

/// Native `ureq` transport. Authentication and agent policy remain outside the map core.
///
/// Plain [`OgcTransport::get`] limits bodies to 16 MiB. Use
/// [`OgcTransport::get_bounded`] to select another limit. Limits apply to bytes
/// returned by ureq's body reader, including transparent decompression.
#[derive(Debug)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// Wraps an agent, preserving its authentication, timeout, and redirect policy.
    pub fn new(agent: ureq::Agent) -> Self {
        Self { agent }
    }
}

impl OgcTransport for UreqTransport {
    fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
        self.get_bounded(url, 16 * 1024 * 1024)
    }

    fn get_bounded(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, NativeMapDataError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| NativeMapDataError::OgcTransport(error.to_string()))?;
        read_bounded(response.into_reader(), max_bytes)
    }
}

// Keep partial results local. The separate sentinel avoids max_bytes + 1
// overflow and never becomes part of the returned allocation.
fn read_bounded(mut reader: impl Read, max_bytes: usize) -> Result<Vec<u8>, NativeMapDataError> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    while bytes.len() < max_bytes {
        let allowance = buffer.len().min(max_bytes - bytes.len());
        let count = read_retry(&mut reader, &mut buffer[..allowance])?;
        if count == 0 {
            return Ok(bytes);
        }
        bytes
            .try_reserve(count)
            .map_err(|error| NativeMapDataError::Io(std::io::Error::other(error)))?;
        bytes.extend_from_slice(&buffer[..count]);
    }
    if read_retry(&mut reader, &mut buffer[..1])? != 0 {
        return Err(NativeMapDataError::ResponseTooLarge { max_bytes });
    }
    Ok(bytes)
}

fn read_retry(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, NativeMapDataError> {
    loop {
        match reader.read(buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => return result.map_err(NativeMapDataError::Io),
        }
    }
}

/// Executes pure OGC requests through an injected native transport.
#[derive(Debug)]
pub struct NativeOgcClient<T> {
    transport: T,
    max_response_bytes: NonZeroUsize,
}

impl<T: OgcTransport> NativeOgcClient<T> {
    /// Creates a client with an explicit nonzero response limit.
    ///
    /// Custom transports using the default bounded method only post-check bodies.
    pub fn new(transport: T, max_response_bytes: NonZeroUsize) -> Self {
        Self {
            transport,
            max_response_bytes,
        }
    }

    /// Fetches a WMS map atomically.
    ///
    /// # Errors
    /// Returns transport, body-read, allocation, or response-size errors.
    pub fn fetch_wms(&self, request: &WmsGetMapRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    /// Fetches a WMTS tile atomically.
    ///
    /// # Errors
    /// Returns transport, body-read, allocation, or response-size errors.
    pub fn fetch_wmts(&self, request: &WmtsGetTileRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    /// Fetches WFS features atomically.
    ///
    /// # Errors
    /// Returns transport, body-read, allocation, or response-size errors.
    pub fn fetch_wfs(&self, request: &WfsGetFeatureRequest) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    /// Fetches a WCS coverage atomically.
    ///
    /// # Errors
    /// Returns transport, body-read, allocation, or response-size errors.
    pub fn fetch_wcs(
        &self,
        request: &WcsGetCoverageRequest,
    ) -> Result<Vec<u8>, NativeMapDataError> {
        self.execute(request.url())
    }

    fn execute(&self, url: String) -> Result<Vec<u8>, NativeMapDataError> {
        let bytes = self
            .transport
            .get_bounded(&url, self.max_response_bytes.get())?;
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

    #[test]
    fn bounded_reader_accepts_exact_and_short_bodies() {
        for length in [0, 1, 8191, 8192, 8193, 16384] {
            let body = vec![42; length];
            assert_eq!(read_bounded(body.as_slice(), length).unwrap(), body);
            assert_eq!(read_bounded(body.as_slice(), usize::MAX).unwrap(), body);
        }
    }

    #[test]
    fn bounded_reader_stops_after_one_sentinel_byte() {
        for limit in [0, 1, 8191, 8192, 8193, 16384] {
            let body = vec![42; limit + 100];
            let mut reader = std::io::Cursor::new(body);
            assert!(matches!(
                read_bounded(&mut reader, limit),
                Err(NativeMapDataError::ResponseTooLarge { max_bytes }) if max_bytes == limit
            ));
            assert_eq!(reader.position(), u64::try_from(limit).unwrap() + 1);
        }
    }

    struct FailingReader {
        remaining: usize,
        interrupt: bool,
    }

    impl Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if std::mem::take(&mut self.interrupt) {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if self.remaining == 0 {
                return Err(std::io::ErrorKind::ConnectionReset.into());
            }
            // Exercise short reads regardless of the requested buffer size.
            let count = self.remaining.min(buffer.len()).min(3);
            buffer[..count].fill(42);
            self.remaining -= count;
            Ok(count)
        }
    }

    #[test]
    fn bounded_reader_propagates_io_errors_without_partial_success() {
        for remaining in [0, 5, 8] {
            let mut reader = FailingReader {
                remaining,
                interrupt: true,
            };
            assert!(matches!(
                read_bounded(&mut reader, 8),
                Err(NativeMapDataError::Io(error))
                    if error.kind() == std::io::ErrorKind::ConnectionReset
            ));
            assert_eq!(reader.remaining, 0);
            assert!(!reader.interrupt);
        }
    }

    #[test]
    fn bounded_reader_retries_interrupted_sentinel_and_accepts_short_reads() {
        struct ShortReader {
            remaining: usize,
            interrupt_at_eof: bool,
        }
        impl Read for ShortReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.remaining == 0 && std::mem::take(&mut self.interrupt_at_eof) {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let count = self.remaining.min(buffer.len()).min(3);
                buffer[..count].fill(42);
                self.remaining -= count;
                Ok(count)
            }
        }
        for limit in [0, 8, usize::MAX] {
            let length = 8.min(limit);
            let reader = ShortReader {
                remaining: length,
                interrupt_at_eof: true,
            };
            assert_eq!(read_bounded(reader, limit).unwrap(), vec![42; length]);
        }
    }

    struct BoundedOnlyTransport;

    impl OgcTransport for BoundedOnlyTransport {
        fn get(&self, _url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            Err(NativeMapDataError::HttpRequestFailed)
        }

        fn get_bounded(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, NativeMapDataError> {
            assert_eq!(url, "https://example.test/bounded");
            assert_eq!(max_bytes, 8);
            Ok(vec![42; 8])
        }
    }

    #[test]
    fn client_dispatches_to_bounded_transport() {
        let client = NativeOgcClient::new(BoundedOnlyTransport, NonZeroUsize::new(8).unwrap());
        assert_eq!(
            client
                .execute("https://example.test/bounded".to_owned())
                .unwrap(),
            vec![42; 8]
        );
    }

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
