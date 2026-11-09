//! Bounded, stateless version-1 JSON wire translation (no transport or codecs).
//!
//! All inputs use `{"version":1,"operation":{"kind":"…",…}}`. Unknown and
//! duplicate fields are errors, including within endpoints, bounds and matrices.
//! No aliases or implicit protocol versions are accepted. Optional fields may
//! be omitted or null. Bounds are canonical source-CRS x/y (degrees for 4326).
//!
//! # Request schema
//! Each request has `endpoint: {base: string, path: string}`. Operation kinds:
//! - `wms`: `layers`, `styles` (strings), `bounds`, `width`, `height` (u32),
//!   `format` (MIME), `transparency` (`"transparent"` or `"opaque"`).
//! - `wmts`: `layer`, `style`, `matrix_set` (strings), `matrix`, `tile`.
//!   Matrix: `{identifier,epsg,matrix_width,matrix_height,tile_width,tile_height}`;
//!   tile: `{x:u32,y:u32,z:u8}`. `format` is a MIME string. Matrix identifiers
//!   are explicit, not inferred from z; z is metadata, not a matrix constraint.
//! - `wfs`: `type_name`, `epsg`, `format`, optional `bbox: [min_x,min_y,max_x,max_y]`
//!   and `count: u32`.
//! - `wcs`: `coverage_id`, `epsg`, `bounds`, `width`, `height`, `format`.
//! - `get_capabilities`: `service` (`"WMS"`, `"WMTS"`, `"WFS"`, `"WCS"`).
//!
//! Bounds: `{epsg:u32,min_x:f64,min_y:f64,max_x:f64,max_y:f64}`. Image MIME
//! formats: `image/png`, `image/jpeg`, `image/webp`, `image/tiff`; WCS accepts
//! only `image/tiff`; WFS accepts `application/geo+json`, `application/gml+xml`.
//! Successful requests return `{"version":1,"url":"…"}`. Capabilities return
//! `{version:1,service,title,service_version,layers:[string,…]}` (not full OGC).
//!
//! # Model schema
//! [`validate_model_json`] accepts `coverage_scalar` with `bounds`, `width`,
//! `height`, `sample_type` (`UInt8`, `Int16`, `UInt16`, `Float32`, `Float64`),
//! optional `nodata`, and `samples: [f64,…]`; or `feature_envelope` with `epsg`
//! and `features: [{id?:string,geometry:object|null,properties:object},…]`.
//! It returns `{"version":1,"valid":true}`. This checks existing core contracts,
//! not scalar representability, geometry topology, or GeoJSON conformance.
//! RGBA8/image validation is intentionally absent: core has no image validator.
//!
//! # Limits and errors
//! Inputs and outputs are limited to 1 MiB; endpoint components to 8192 bytes;
//! identifiers to 1024 bytes. Output is serialized through a capped writer.
//! JSON syntax/schema/version/limit failures use `INVALID_CAPABILITIES`
//! (`OL-CORE-MAP-0012`); unknown MIME uses `UNSUPPORTED_FORMAT` (`0010`).
//! Constructor/builder errors retain their existing codes unchanged. Error
//! messages are diagnostic, not a stable wire contract. No error JSON is returned.
//!
//! ```
//! use olayer_map_core::interop::build_request_json;
//! let result = build_request_json(r#"{"version":1,"operation":{
//!   "kind":"get_capabilities","endpoint":{"base":"https://example.test","path":"ogc"},
//!   "service":"WMS"}}"#)?;
//! assert!(result.contains("GetCapabilities"));
//! # Ok::<(), olayer_map_core::MapError>(())
//! ```

use crate::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, num::NonZeroU32};

const LIMIT: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    version: u32,
    operation: T,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    base: String,
    path: String,
}
impl Endpoint {
    fn into_domain(self) -> Result<UrlParts, MapError> {
        bounded(&self.base, 8192)?;
        bounded(&self.path, 8192)?;
        Ok(UrlParts::new(self.base, self.path))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    epsg: u32,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}
impl Bounds {
    fn into_domain(self) -> Result<BoundingBox, MapError> {
        BoundingBox::new(
            self.min_x,
            self.min_y,
            self.max_x,
            self.max_y,
            CoordinateReferenceSystem::epsg(self.epsg)?,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Matrix {
    identifier: String,
    epsg: u32,
    matrix_width: u32,
    matrix_height: u32,
    tile_width: u32,
    tile_height: u32,
}
impl Matrix {
    fn into_domain(self) -> Result<TileMatrix, MapError> {
        bounded(&self.identifier, 1024)?;
        TileMatrix::new(
            self.identifier,
            CoordinateReferenceSystem::epsg(self.epsg)?,
            dimension(self.matrix_width, INVALID_TILE_KEY)?,
            dimension(self.matrix_height, INVALID_TILE_KEY)?,
            dimension(self.tile_width, INVALID_TILE_KEY)?,
            dimension(self.tile_height, INVALID_TILE_KEY)?,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tile {
    x: u32,
    y: u32,
    z: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Transparency {
    Transparent,
    Opaque,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Service {
    Wms,
    Wmts,
    Wfs,
    Wcs,
}
impl Service {
    fn into_domain(self) -> ServiceType {
        match self {
            Self::Wms => ServiceType::Wms,
            Self::Wmts => ServiceType::Wmts,
            Self::Wfs => ServiceType::Wfs,
            Self::Wcs => ServiceType::Wcs,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Wms {
        endpoint: Endpoint,
        layers: String,
        styles: String,
        bounds: Bounds,
        width: u32,
        height: u32,
        format: String,
        transparency: Transparency,
    },
    Wmts {
        endpoint: Endpoint,
        layer: String,
        style: String,
        matrix_set: String,
        matrix: Matrix,
        tile: Tile,
        format: String,
    },
    Wfs {
        endpoint: Endpoint,
        type_name: String,
        epsg: u32,
        format: String,
        bbox: Option<[f64; 4]>,
        count: Option<u32>,
    },
    Wcs {
        endpoint: Endpoint,
        coverage_id: String,
        epsg: u32,
        bounds: Bounds,
        width: u32,
        height: u32,
        format: String,
    },
    GetCapabilities {
        endpoint: Endpoint,
        service: Service,
    },
}

/// Builds a URL using version-1 wire DTOs and the existing pure request builders.
///
/// # Errors
/// Returns schema/limit errors or the unchanged constructor/builder error codes
/// documented in this module. No network requests are performed.
pub fn build_request_json(input: &str) -> Result<String, MapError> {
    let request: Request = decode(input)?;
    let url = match request {
        Request::Wms {
            endpoint,
            layers,
            styles,
            bounds,
            width,
            height,
            format,
            transparency,
        } => {
            bounded(&layers, 1024)?;
            bounded(&styles, 1024)?;
            WmsRequestBuilder::new(endpoint.into_domain()?, layers)
                .styles(styles)
                .bounds(bounds.into_domain()?)
                .size(
                    dimension(width, INVALID_WMS)?,
                    dimension(height, INVALID_WMS)?,
                )
                .format(image_format(&format)?)
                .transparent(matches!(transparency, Transparency::Transparent))
                .get_map()?
                .url()
        }
        Request::Wmts {
            endpoint,
            layer,
            style,
            matrix_set,
            matrix,
            tile,
            format,
        } => {
            for identifier in [&layer, &style, &matrix_set] {
                bounded(identifier, 1024)?;
            }
            WmtsRequestBuilder::new(endpoint.into_domain()?, layer)
                .style(style)
                .matrix_set(matrix_set)
                .matrix(matrix.into_domain()?)
                .tile(TileKey {
                    x: tile.x,
                    y: tile.y,
                    z: tile.z,
                })
                .format(image_format(&format)?)
                .get_tile()?
                .url()
        }
        Request::Wfs {
            endpoint,
            type_name,
            epsg,
            format,
            bbox,
            count,
        } => {
            bounded(&type_name, 1024)?;
            let format = match format.as_str() {
                "application/geo+json" => VectorFormat::GeoJson,
                "application/gml+xml" => VectorFormat::Gml,
                _ => return Err(MapError::new(UNSUPPORTED_FORMAT, "unsupported vector MIME")),
            };
            let mut builder = WfsRequestBuilder::new(endpoint.into_domain()?, type_name)
                .crs(CoordinateReferenceSystem::epsg(epsg)?)
                .output_format(format);
            if let Some(bbox) = bbox {
                builder = builder.bbox(bbox);
            }
            if let Some(count) = count {
                builder = builder.count(count);
            }
            builder.get_feature()?.url()
        }
        Request::Wcs {
            endpoint,
            coverage_id,
            epsg,
            bounds,
            width,
            height,
            format,
        } => {
            bounded(&coverage_id, 1024)?;
            if format != "image/tiff" {
                return Err(MapError::new(
                    UNSUPPORTED_FORMAT,
                    "unsupported coverage MIME",
                ));
            }
            WcsRequestBuilder::new(endpoint.into_domain()?, coverage_id)
                .crs(CoordinateReferenceSystem::epsg(epsg)?)
                .bounds(bounds.into_domain()?)
                .size(width, height)
                .format(format)
                .get_coverage()?
                .url()
        }
        Request::GetCapabilities { endpoint, service } => {
            CapabilitiesRequest::new(endpoint.into_domain()?, service.into_domain()).url()
        }
    };
    #[derive(Serialize)]
    struct Output {
        version: u32,
        url: String,
    }
    encode(&Output { version: 1, url })
}

/// Parses a bounded capabilities XML summary into version-1 JSON.
///
/// # Errors
/// Returns `INVALID_CAPABILITIES` for invalid XML or exceeded input/output limits.
pub fn parse_capabilities_json(xml: &[u8]) -> Result<String, MapError> {
    if xml.len() > LIMIT {
        return Err(invalid("XML exceeds 1 MiB"));
    }
    let summary = Capabilities::parse_xml(xml)?;
    for layer in &summary.layers {
        bounded(layer, 1024)?;
    }
    bounded(&summary.version, 1024)?;
    #[derive(Serialize)]
    struct Output {
        version: u32,
        service: &'static str,
        service_version: String,
        title: String,
        layers: Vec<String>,
    }
    encode(&Output {
        version: 1,
        service: summary.service.parameter(),
        service_version: summary.version,
        title: summary.title,
        layers: summary.layers,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeatureEnvelope {
    id: Option<String>,
    geometry: serde_json::Value,
    properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Model {
    CoverageScalar {
        bounds: Bounds,
        width: u32,
        height: u32,
        sample_type: CoverageSampleType,
        nodata: Option<f64>,
        samples: Vec<f64>,
    },
    FeatureEnvelope {
        epsg: u32,
        features: Vec<FeatureEnvelope>,
    },
}

/// Validates a scalar coverage or feature envelope without decoding media.
///
/// # Errors
/// Returns schema/limit errors, or existing CRS/coverage/feature validation codes.
/// Geometry objects are opaque; no topology or full GeoJSON validation is promised.
pub fn validate_model_json(input: &str) -> Result<String, MapError> {
    match decode::<Model>(input)? {
        Model::CoverageScalar {
            bounds,
            width,
            height,
            sample_type,
            nodata,
            samples,
        } => {
            Coverage {
                bounds: bounds.into_domain()?,
                width,
                height,
                sample_type,
                nodata,
                samples: samples.into_boxed_slice(),
            }
            .validate()?;
        }
        Model::FeatureEnvelope { epsg, features } => {
            let crs = CoordinateReferenceSystem::epsg(epsg)?;
            let features = features
                .into_iter()
                .map(|feature| {
                    if let Some(id) = &feature.id {
                        bounded(id, 1024)?;
                    }
                    for key in feature.properties.keys() {
                        bounded(key, 1024)?;
                    }
                    Ok(Feature {
                        id: feature.id,
                        geometry: feature.geometry,
                        properties: feature.properties,
                    })
                })
                .collect::<Result<Vec<_>, MapError>>()?;
            FeatureCollection::validate(crs, features)?;
        }
    }
    Ok("{\"version\":1,\"valid\":true}".to_owned())
}

fn decode<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, MapError> {
    if input.len() > LIMIT {
        return Err(invalid("JSON exceeds 1 MiB"));
    }
    let envelope: Envelope<T> =
        serde_json::from_str(input).map_err(|_| invalid("invalid versioned JSON schema"))?;
    if envelope.version != 1 {
        return Err(invalid("unsupported wire version"));
    }
    Ok(envelope.operation)
}

fn image_format(format: &str) -> Result<ImageFormat, MapError> {
    match format {
        "image/png" => Ok(ImageFormat::Png),
        "image/jpeg" => Ok(ImageFormat::Jpeg),
        "image/webp" => Ok(ImageFormat::Webp),
        "image/tiff" => Ok(ImageFormat::GeoTiff),
        _ => Err(MapError::new(UNSUPPORTED_FORMAT, "unsupported image MIME")),
    }
}
fn dimension(value: u32, code: ErrorCode) -> Result<NonZeroU32, MapError> {
    NonZeroU32::new(value).ok_or_else(|| MapError::new(code, "dimension must be non-zero"))
}
fn bounded(value: &str, limit: usize) -> Result<(), MapError> {
    if value.len() > limit {
        Err(invalid("wire string exceeds byte limit"))
    } else {
        Ok(())
    }
}
fn invalid(message: &str) -> MapError {
    MapError::new(INVALID_CAPABILITIES, message)
}

struct CappedWriter(Vec<u8>);
impl Write for CappedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > LIMIT.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("JSON output exceeds 1 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize) -> Result<String, MapError> {
    let mut writer = CappedWriter(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| invalid("JSON output exceeds limit"))?;
    String::from_utf8(writer.0).map_err(|_| invalid("JSON output is not UTF-8"))
}
