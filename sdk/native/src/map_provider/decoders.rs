use super::NativeMapDataError;
use image::io::Reader as ImageReader;
use olayer_map_core::{
    BoundingBox, CoordinateReferenceSystem, Feature, FeatureCollection, ImageFormat, MapImage,
};
use serde_json::Value;
use std::collections::BTreeMap;
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
}
