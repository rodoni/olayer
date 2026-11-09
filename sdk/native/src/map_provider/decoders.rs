use super::NativeMapDataError;
use image::io::Reader as ImageReader;
use olayer_map_core::{
    BoundingBox, CoordinateReferenceSystem, Coverage, CoverageSampleType, Feature,
    FeatureCollection, ImageFormat, MapImage,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::io::Cursor;
use std::num::NonZeroU32;

const MAX_OGC_IMAGE_WIDTH: u32 = 8_192;
const MAX_OGC_IMAGE_HEIGHT: u32 = 8_192;
const MAX_OGC_IMAGE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;

/// Decodes a bounded WMS image response into the transport-independent image model.
pub fn decode_wms_image(
    bytes: &[u8],
    format: ImageFormat,
    crs: CoordinateReferenceSystem,
    bounds: BoundingBox,
) -> Result<MapImage, NativeMapDataError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let detected_format = reader
        .format()
        .ok_or_else(|| NativeMapDataError::OgcDecode("unrecognized image format".into()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    if width > MAX_OGC_IMAGE_WIDTH || height > MAX_OGC_IMAGE_HEIGHT {
        return Err(NativeMapDataError::OgcDecode(
            "image dimensions exceed the configured limit".into(),
        ));
    }
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(MAX_OGC_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_OGC_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_OGC_IMAGE_ALLOC_BYTES);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), detected_format);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?
        .to_rgba8()
        .into_raw()
        .into_boxed_slice();
    let width = NonZeroU32::new(width)
        .ok_or_else(|| NativeMapDataError::OgcDecode("image width is zero".into()))?;
    let height = NonZeroU32::new(height)
        .ok_or_else(|| NativeMapDataError::OgcDecode("image height is zero".into()))?;
    Ok(MapImage {
        pixels,
        width,
        height,
        format,
        crs,
        bounds,
    })
}

/// Validates a GeoJSON FeatureCollection while preserving every property value.
pub fn decode_wfs_feature_collection(
    bytes: &[u8],
    crs: CoordinateReferenceSystem,
) -> Result<FeatureCollection, NativeMapDataError> {
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let object = document
        .as_object()
        .ok_or_else(|| NativeMapDataError::OgcDecode("WFS document must be an object".into()))?;
    if object.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
        return Err(NativeMapDataError::OgcDecode(
            "WFS document must be a FeatureCollection".into(),
        ));
    }
    if let Some(metadata) = object.get("crs").filter(|value| !value.is_null()) {
        let name = metadata
            .get("properties")
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str);
        let code = name
            .and_then(|name| {
                [
                    "EPSG:",
                    "urn:ogc:def:crs:EPSG::",
                    "http://www.opengis.net/def/crs/EPSG/0/",
                    "https://www.opengis.net/def/crs/EPSG/0/",
                ]
                .iter()
                .find_map(|prefix| name.strip_prefix(*prefix))
            })
            .filter(|code| !code.starts_with('0') && code.chars().all(|c| c.is_ascii_digit()))
            .and_then(|code| code.parse::<u32>().ok());
        if metadata.get("type").and_then(Value::as_str) != Some("name")
            || code != Some(crs.epsg_code())
        {
            return Err(NativeMapDataError::OgcDecode(
                "WFS response CRS conflicts with request or is unsupported".into(),
            ));
        }
    }
    let raw_features = object
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeMapDataError::OgcDecode("WFS features must be an array".into()))?;
    let mut features = Vec::with_capacity(raw_features.len());
    for raw_feature in raw_features {
        let feature_object = raw_feature
            .as_object()
            .ok_or_else(|| NativeMapDataError::OgcDecode("WFS feature must be an object".into()))?;
        if feature_object.get("type").and_then(Value::as_str) != Some("Feature") {
            return Err(NativeMapDataError::OgcDecode(
                "WFS member must have type Feature".into(),
            ));
        }
        let geometry = feature_object
            .get("geometry")
            .cloned()
            .unwrap_or(Value::Null);
        let properties = feature_object
            .get("properties")
            .cloned()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        let properties = match properties {
            Value::Null => serde_json::Map::new(),
            Value::Object(properties) => properties,
            _ => {
                return Err(NativeMapDataError::OgcDecode(
                    "WFS feature properties must be an object or null".into(),
                ));
            }
        };
        let properties: BTreeMap<String, Value> = properties.into_iter().collect();
        let id = feature_object.get("id").and_then(|value| match value {
            Value::String(id) => Some(id.clone()),
            Value::Number(id) => Some(id.to_string()),
            _ => None,
        });
        features.push(Feature {
            id,
            geometry,
            properties,
        });
    }
    FeatureCollection::validate(crs, features)
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))
}

/// Decodes an image-backed WCS coverage into explicit numeric samples.
pub fn decode_wcs_raster_coverage(
    bytes: &[u8],
    bounds: BoundingBox,
    sample_type: CoverageSampleType,
    nodata: Option<f64>,
) -> Result<Coverage, NativeMapDataError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let detected_format = reader
        .format()
        .ok_or_else(|| NativeMapDataError::OgcDecode("unrecognized WCS raster format".into()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(MAX_OGC_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_OGC_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_OGC_IMAGE_ALLOC_BYTES);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), detected_format);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let samples: Vec<f64> = match (sample_type, image) {
        (CoverageSampleType::UInt8, image::DynamicImage::ImageLuma8(image)) => {
            image.into_raw().into_iter().map(f64::from).collect()
        }
        (CoverageSampleType::UInt16, image::DynamicImage::ImageLuma16(image)) => {
            image.into_raw().into_iter().map(f64::from).collect()
        }
        _ => {
            return Err(NativeMapDataError::OgcDecode(
                "WCS image samples must match a single-band UInt8 or UInt16 raster; signed, floating-point, multiband and converted samples are unsupported".into(),
            ));
        }
    };
    Coverage {
        bounds,
        width,
        height,
        sample_type,
        nodata,
        samples: samples.into_boxed_slice(),
    }
    .validate()
    .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))
}

/// Decodes a JSON scientific-grid payload: `{ "width", "height", "samples" }`.
pub fn decode_wcs_json_coverage(
    bytes: &[u8],
    bounds: BoundingBox,
    sample_type: CoverageSampleType,
    nodata: Option<f64>,
) -> Result<Coverage, NativeMapDataError> {
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(NativeMapDataError::OgcDecode(
            "WCS JSON exceeds the 64 MiB decode budget".into(),
        ));
    }
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))?;
    let object = document
        .as_object()
        .ok_or_else(|| NativeMapDataError::OgcDecode("WCS grid must be an object".into()))?;
    let width = object
        .get("width")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| NativeMapDataError::OgcDecode("WCS width is invalid".into()))?;
    let height = object
        .get("height")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| NativeMapDataError::OgcDecode("WCS height is invalid".into()))?;
    let cell_count = width
        .checked_mul(height)
        .filter(|count| *count > 0 && *count <= 16_777_216)
        .ok_or_else(|| {
            NativeMapDataError::OgcDecode("WCS grid exceeds the nonzero sample budget".into())
        })?;
    let metadata_nodata =
        match object.get("nodata") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| {
                NativeMapDataError::OgcDecode("WCS nodata must be finite".into())
            })?),
        };
    if nodata.zip(metadata_nodata).is_some_and(|(a, b)| a != b) {
        return Err(NativeMapDataError::OgcDecode(
            "conflicting WCS nodata metadata".into(),
        ));
    }
    let nodata = nodata.or(metadata_nodata);
    let samples = object
        .get("samples")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeMapDataError::OgcDecode("WCS samples must be an array".into()))?;
    if usize::try_from(cell_count).ok() != Some(samples.len()) {
        return Err(NativeMapDataError::OgcDecode(
            "WCS sample count does not match dimensions".into(),
        ));
    }
    let samples: Vec<f64> = samples
        .iter()
        .map(|value| {
            let sample = value
                .as_f64()
                .filter(|sample| sample.is_finite())
                .ok_or_else(|| {
                    NativeMapDataError::OgcDecode("WCS sample must be finite numeric data".into())
                })?;
            let valid = match sample_type {
                CoverageSampleType::UInt8 => {
                    sample.fract() == 0.0 && (0.0..=255.0).contains(&sample)
                }
                CoverageSampleType::UInt16 => {
                    sample.fract() == 0.0 && (0.0..=65_535.0).contains(&sample)
                }
                CoverageSampleType::Int16 => {
                    sample.fract() == 0.0 && (-32_768.0..=32_767.0).contains(&sample)
                }
                CoverageSampleType::Float32 => sample.abs() <= f64::from(f32::MAX),
                CoverageSampleType::Float64 => true,
            };
            if !valid {
                return Err(NativeMapDataError::OgcDecode(
                    "WCS sample does not fit the declared sample type".into(),
                ));
            }
            Ok(sample)
        })
        .collect::<Result<_, _>>()?;
    Coverage {
        bounds,
        width,
        height,
        sample_type,
        nodata,
        samples: samples.into_boxed_slice(),
    }
    .validate()
    .map_err(|error| NativeMapDataError::OgcDecode(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_wms_png_into_map_image() {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgba8(2, 1)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageOutputFormat::Png)
            .unwrap();
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let bounds = BoundingBox::new(0.0, 0.0, 1.0, 1.0, crs).unwrap();
        let image = decode_wms_image(&bytes, ImageFormat::Png, crs, bounds).unwrap();
        assert_eq!(image.width.get(), 2);
        assert_eq!(image.height.get(), 1);
        assert_eq!(image.pixels.len(), 8);
    }

    #[test]
    fn preserves_wfs_properties_and_numeric_ids() {
        let payload = br#"{"type":"FeatureCollection","features":[{"type":"Feature","id":7,"geometry":null,"properties":{"name":"road","metadata":{"priority":2}}}]}"#;
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let collection = decode_wfs_feature_collection(payload, crs).unwrap();
        assert_eq!(collection.features[0].id.as_deref(), Some("7"));
        assert_eq!(collection.features[0].properties["metadata"]["priority"], 2);
    }

    #[test]
    fn rejects_malformed_wfs_documents() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let result =
            decode_wfs_feature_collection(br#"{"type":"FeatureCollection","features":{}}"#, crs);
        assert!(matches!(result, Err(NativeMapDataError::OgcDecode(_))));
    }

    #[test]
    fn decodes_wcs_json_scientific_grid() {
        let payload = br#"{"width":2,"height":1,"samples":[1.5,2.5]}"#;
        let bounds = BoundingBox::new(
            0.0,
            0.0,
            1.0,
            1.0,
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        let coverage =
            decode_wcs_json_coverage(payload, bounds, CoverageSampleType::Float64, Some(-9999.0))
                .unwrap();
        assert_eq!(coverage.samples.as_ref(), &[1.5, 2.5]);
    }

    #[test]
    fn decodes_wcs_png_raster() {
        let mut bytes = Vec::new();
        image::DynamicImage::new_luma8(2, 1)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageOutputFormat::Png)
            .unwrap();
        let bounds = BoundingBox::new(
            0.0,
            0.0,
            1.0,
            1.0,
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        let coverage =
            decode_wcs_raster_coverage(&bytes, bounds, CoverageSampleType::UInt8, None).unwrap();
        assert_eq!(coverage.samples.len(), 2);
    }

    #[test]
    fn wcs_raster_rejects_signed_labels_and_multiband_conversion() {
        let bounds = BoundingBox::new(
            0.0,
            0.0,
            1.0,
            1.0,
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        for (image, sample_type) in [
            (
                image::DynamicImage::new_luma16(1, 1),
                CoverageSampleType::Int16,
            ),
            (
                image::DynamicImage::new_rgb8(1, 1),
                CoverageSampleType::UInt8,
            ),
            (
                image::DynamicImage::new_luma8(1, 1),
                CoverageSampleType::UInt16,
            ),
        ] {
            let mut bytes = Cursor::new(Vec::new());
            image
                .write_to(&mut bytes, image::ImageOutputFormat::Png)
                .unwrap();
            assert!(matches!(
                decode_wcs_raster_coverage(bytes.get_ref(), bounds, sample_type, None),
                Err(NativeMapDataError::OgcDecode(_))
            ));
        }
    }

    #[test]
    fn wcs_raster_preserves_unsigned_sixteen_bit_values() {
        let pixels = image::ImageBuffer::from_raw(2, 1, vec![0_u16, 65_535]).unwrap();
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma16(pixels)
            .write_to(&mut bytes, image::ImageOutputFormat::Png)
            .unwrap();
        let bounds = BoundingBox::new(
            0.0,
            0.0,
            1.0,
            1.0,
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        let coverage = decode_wcs_raster_coverage(
            bytes.get_ref(),
            bounds,
            CoverageSampleType::UInt16,
            Some(65_535.0),
        )
        .unwrap();
        assert_eq!(coverage.samples.as_ref(), &[0.0, 65_535.0]);
        assert_eq!(coverage.nodata, Some(65_535.0));
    }

    #[test]
    fn wcs_json_rejects_over_budget_dimensions_and_mislabeled_samples() {
        let bounds = BoundingBox::new(
            0.0,
            0.0,
            1.0,
            1.0,
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        for payload in [
            br#"{"width":4294967295,"height":4294967295,"samples":[]}"#.as_slice(),
            br#"{"width":1,"height":1,"samples":[256]}"#.as_slice(),
            br#"{"width":1,"height":1,"samples":[-1]}"#.as_slice(),
            br#"{"width":1,"height":1,"samples":[1.5]}"#.as_slice(),
        ] {
            assert!(matches!(
                decode_wcs_json_coverage(payload, bounds, CoverageSampleType::UInt8, None),
                Err(NativeMapDataError::OgcDecode(_))
            ));
        }
        let valid = decode_wcs_json_coverage(
            br#"{"width":2,"height":1,"samples":[-32768,32767]}"#,
            bounds,
            CoverageSampleType::Int16,
            None,
        )
        .unwrap();
        assert_eq!(valid.samples.as_ref(), &[-32_768.0, 32_767.0]);
    }
}
