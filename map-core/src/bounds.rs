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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_and_inverted_bounds() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        for values in [
            [f64::NAN, 0.0, 1.0, 1.0],
            [0.0, f64::INFINITY, 1.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0, 0.0],
        ] {
            assert_eq!(
                BoundingBox::new(values[0], values[1], values[2], values[3], crs)
                    .unwrap_err()
                    .code,
                INVALID_BOUNDS
            );
        }
    }
}
