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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileAddress {
    pub matrix_identifier: String,
    pub tile: TileKey,
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
    pub fn new(
        identifier: impl Into<String>,
        crs: CoordinateReferenceSystem,
        matrix_width: NonZeroU32,
        matrix_height: NonZeroU32,
        tile_width: NonZeroU32,
        tile_height: NonZeroU32,
    ) -> Result<Self, MapError> {
        let identifier = identifier.into();
        if identifier.trim().is_empty() {
            return Err(MapError::new(
                INVALID_TILE_KEY,
                "matrix identifier is empty",
            ));
        }
        if tile_width.get() > 16_384 || tile_height.get() > 16_384 {
            return Err(MapError::new(
                INVALID_TILE_KEY,
                "tile dimensions exceed the supported limit",
            ));
        }
        Ok(Self {
            identifier,
            crs,
            matrix_width,
            matrix_height,
            tile_width,
            tile_height,
        })
    }

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

    pub fn address(&self, tile: TileKey) -> Result<TileAddress, MapError> {
        self.validate(tile)?;
        Ok(TileAddress {
            matrix_identifier: self.identifier.clone(),
            tile,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix() -> TileMatrix {
        TileMatrix::new(
            "0",
            CoordinateReferenceSystem::epsg(3857).unwrap(),
            NonZeroU32::new(8).unwrap(),
            NonZeroU32::new(4).unwrap(),
            NonZeroU32::new(256).unwrap(),
            NonZeroU32::new(256).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn addresses_all_matrix_edges_and_rejects_outside_tiles() {
        let matrix = matrix();
        for y in 0..matrix.matrix_height.get() {
            for x in 0..matrix.matrix_width.get() {
                let address = matrix.address(TileKey { x, y, z: 0 }).unwrap();
                assert_eq!(address.matrix_identifier, "0");
                assert_eq!(address.tile, TileKey { x, y, z: 0 });
            }
        }
        assert_eq!(
            matrix
                .address(TileKey { x: 8, y: 0, z: 0 })
                .unwrap_err()
                .code,
            TILE_OUTSIDE_MATRIX
        );
        assert_eq!(
            matrix
                .address(TileKey { x: 0, y: 4, z: 0 })
                .unwrap_err()
                .code,
            TILE_OUTSIDE_MATRIX
        );
    }

    #[test]
    fn constructor_rejects_blank_identifier_and_excessive_dimensions() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let result = TileMatrix::new(
            " ",
            crs,
            NonZeroU32::new(1).unwrap(),
            NonZeroU32::new(1).unwrap(),
            NonZeroU32::new(256).unwrap(),
            NonZeroU32::new(256).unwrap(),
        );
        assert_eq!(result.unwrap_err().code, INVALID_TILE_KEY);

        let result = TileMatrix::new(
            "0",
            crs,
            NonZeroU32::new(1).unwrap(),
            NonZeroU32::new(1).unwrap(),
            NonZeroU32::new(16_385).unwrap(),
            NonZeroU32::new(256).unwrap(),
        );
        assert_eq!(result.unwrap_err().code, INVALID_TILE_KEY);
    }
}
