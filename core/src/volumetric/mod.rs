pub mod airspace_mesh;
pub mod errors;
pub mod ribbon_mesh;
pub mod triangulation;
pub mod types;

#[cfg(test)]
mod tests;

pub use airspace_mesh::generate_airspace_volume_mesh;
pub use errors::VolumetricError;
pub use ribbon_mesh::generate_trajectory_ribbon_mesh;
pub use triangulation::{signed_area_2d, triangulate_polygon_2d};
pub use types::{RibbonMesh, RibbonVertex, VolumetricMesh, VolumetricVertex};
