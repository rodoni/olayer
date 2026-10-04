use crate::errors::{INVALID_TILE_KEY, TILE_OUTSIDE_MATRIX};
use crate::{CoordinateReferenceSystem, MapError};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileKey {
    pub x: u32,
    pub y: u32,
    pub z: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileMatrix {
    pub identifier: String,
    pub crs: CoordinateReferenceSystem,
    pub matrix_width: NonZeroU32,
    pub matrix_height: NonZeroU32,
    pub tile_width: NonZeroU32,
    pub tile_height: NonZeroU32,
}
impl TileMatrix {
    pub fn validate(&self, tile: TileKey) -> Result<(), MapError> {
        if self.identifier.is_empty() {
            return Err(MapError::new(
                INVALID_TILE_KEY,
                "matrix identifier is empty",
            ));
        }
        if tile.x >= self.matrix_width.get() || tile.y >= self.matrix_height.get() {
            return Err(MapError::new(TILE_OUTSIDE_MATRIX, "tile is outside matrix"));
        }
        Ok(())
    }
}
