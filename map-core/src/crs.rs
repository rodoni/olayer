use crate::errors::{MapError, INVALID_CRS};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrsAuthority {
    Epsg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CoordinateReferenceSystem {
    pub authority: CrsAuthority,
    pub code: NonZeroU32,
}

impl CoordinateReferenceSystem {
    pub fn epsg(code: u32) -> Result<Self, MapError> {
        let code = NonZeroU32::new(code)
            .ok_or_else(|| MapError::new(INVALID_CRS, "EPSG code must be non-zero"))?;
        if !matches!(code.get(), 4326 | 3857 | 4978 | 900913) {
            return Err(MapError::new(INVALID_CRS, "unsupported EPSG code"));
        }
        Ok(Self {
            authority: CrsAuthority::Epsg,
            code,
        })
    }
    pub fn epsg_code(self) -> u32 {
        self.code.get()
    }
    pub fn authority_code(self) -> String {
        format!("EPSG:{}", self.code)
    }
}
