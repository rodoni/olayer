pub mod engine;
pub mod spatial_grid;
pub mod types;

#[cfg(test)]
mod tests;

pub use engine::DeclutterEngine;
pub use spatial_grid::SpatialHashGrid;
pub use types::{DeclutterConfig, LabelPlacement, LabelTarget, OctantDirection, Rect2D};
