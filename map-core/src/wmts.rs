use crate::errors::INVALID_WMTS;
use crate::{ImageFormat, MapError, QueryParameter, TileKey, TileMatrix, UrlParts};

#[derive(Debug, Clone, PartialEq)]
pub struct WmtsGetTileRequest {
    pub endpoint: UrlParts,
    pub layer: String,
    pub style: String,
    pub matrix_set: String,
    pub matrix: TileMatrix,
    pub tile: TileKey,
    pub format: ImageFormat,
}
impl WmtsGetTileRequest {
    pub fn url(&self) -> String {
        self.endpoint.url(&[
            QueryParameter::new("SERVICE", "WMTS"),
            QueryParameter::new("REQUEST", "GetTile"),
            QueryParameter::new("VERSION", "1.0.0"),
            QueryParameter::new("LAYER", &self.layer),
            QueryParameter::new("STYLE", &self.style),
            QueryParameter::new("TILEMATRIXSET", &self.matrix_set),
            QueryParameter::new("TILEMATRIX", &self.matrix.identifier),
            QueryParameter::new("TILEROW", self.tile.y.to_string()),
            QueryParameter::new("TILECOL", self.tile.x.to_string()),
            QueryParameter::new("FORMAT", self.format.mime()),
        ])
    }
}
#[derive(Debug, Clone)]
pub struct WmtsRequestBuilder {
    endpoint: UrlParts,
    layer: String,
    style: String,
    matrix_set: String,
    matrix: Option<TileMatrix>,
    tile: Option<TileKey>,
    format: ImageFormat,
}
impl WmtsRequestBuilder {
    pub fn new(endpoint: UrlParts, layer: impl Into<String>) -> Self {
        Self {
            endpoint,
            layer: layer.into(),
            style: "default".into(),
            matrix_set: String::new(),
            matrix: None,
            tile: None,
            format: ImageFormat::Png,
        }
    }
    pub fn style(mut self, value: impl Into<String>) -> Self {
        self.style = value.into();
        self
    }
    pub fn matrix_set(mut self, value: impl Into<String>) -> Self {
        self.matrix_set = value.into();
        self
    }
    pub fn matrix(mut self, value: TileMatrix) -> Self {
        self.matrix = Some(value);
        self
    }
    pub fn tile(mut self, value: TileKey) -> Self {
        self.tile = Some(value);
        self
    }
    pub fn format(mut self, value: ImageFormat) -> Self {
        self.format = value;
        self
    }
    pub fn get_tile(self) -> Result<WmtsGetTileRequest, MapError> {
        let matrix = self
            .matrix
            .ok_or_else(|| MapError::new(INVALID_WMTS, "matrix is required"))?;
        let tile = self
            .tile
            .ok_or_else(|| MapError::new(INVALID_WMTS, "tile is required"))?;
        matrix.validate(tile)?;
        if self.layer.is_empty() || self.matrix_set.is_empty() {
            return Err(MapError::new(
                INVALID_WMTS,
                "layer and matrix set are required",
            ));
        }
        Ok(WmtsGetTileRequest {
            endpoint: self.endpoint,
            layer: self.layer,
            style: self.style,
            matrix_set: self.matrix_set,
            matrix,
            tile,
            format: self.format,
        })
    }
}
