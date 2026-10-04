use crate::errors::{MapError, INVALID_CAPABILITIES};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceType {
    Wms,
    Wmts,
    Wfs,
    Wcs,
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
}
