use crate::crs::CoordinateReferenceSystem;
use crate::errors::{MapError, INVALID_BOUNDS};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub crs: CoordinateReferenceSystem,
}

impl BoundingBox {
    pub fn new(
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        crs: CoordinateReferenceSystem,
    ) -> Result<Self, MapError> {
        let valid = [min_x, min_y, max_x, max_y].iter().all(|v| v.is_finite())
            && min_x < max_x
            && min_y < max_y;
        if !valid {
            return Err(MapError::new(
                INVALID_BOUNDS,
                "bounds must be finite and ordered",
            ));
        }
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
            crs,
        })
    }
}
