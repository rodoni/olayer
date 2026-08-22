use serde::{Deserialize, Serialize};

/// Vertex format for extruded 3D volumetric airspaces and prisms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumetricVertex {
    /// 3D ECEF Cartesian coordinates in meters `[X, Y, Z]`.
    pub position_ecef: [f64; 3],
    /// Normalized surface normal vector `[Nx, Ny, Nz]`.
    pub normal: [f32; 3],
    /// Height ratio ($0.0 = \text{floor}$, $1.0 = \text{ceiling}$).
    pub height_ratio: f32,
    /// Edge indicator flag ($1.0 = \text{sidewall / silhouette border}$, $0.0 = \text{cap interior}$).
    pub is_edge: f32,
}

/// A complete indexed triangular 3D mesh representing an extruded volumetric airspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VolumetricMesh {
    pub vertices: Vec<VolumetricVertex>,
    pub indices: Vec<u32>,
}

impl VolumetricMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Converts vertices to a flat array of 32-bit floats interleaved:
    /// `[x, y, z, nx, ny, nz, height_ratio, is_edge, ...]` (8 floats per vertex).
    pub fn to_flat_f32_vertices(&self) -> Vec<f32> {
        let mut flat = Vec::with_capacity(self.vertices.len() * 8);
        for v in &self.vertices {
            flat.push(v.position_ecef[0] as f32);
            flat.push(v.position_ecef[1] as f32);
            flat.push(v.position_ecef[2] as f32);
            flat.push(v.normal[0]);
            flat.push(v.normal[1]);
            flat.push(v.normal[2]);
            flat.push(v.height_ratio);
            flat.push(v.is_edge);
        }
        flat
    }
}

/// Vertex format for 3D flight path trajectory ribbons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RibbonVertex {
    /// 3D ECEF Cartesian coordinates in meters `[X, Y, Z]`.
    pub position_ecef: [f64; 3],
    /// Normal vector perpendicular to the ribbon strip `[Nx, Ny, Nz]`.
    pub normal: [f32; 3],
    /// Texture coordinates `[U, V]` ($U \in [0, 1]$ across width, $V$ along length).
    pub uv: [f32; 2],
    /// Normalized scalar value for altitude / vertical rate / speed shader gradient mapping.
    pub scalar: f32,
}

/// A complete indexed triangular mesh representing a 3D flight trajectory ribbon.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RibbonMesh {
    pub vertices: Vec<RibbonVertex>,
    pub indices: Vec<u32>,
}

impl RibbonMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Converts ribbon vertices to a flat array of 32-bit floats interleaved:
    /// `[x, y, z, nx, ny, nz, u, v, scalar, ...]` (9 floats per vertex).
    pub fn to_flat_f32_vertices(&self) -> Vec<f32> {
        let mut flat = Vec::with_capacity(self.vertices.len() * 9);
        for v in &self.vertices {
            flat.push(v.position_ecef[0] as f32);
            flat.push(v.position_ecef[1] as f32);
            flat.push(v.position_ecef[2] as f32);
            flat.push(v.normal[0]);
            flat.push(v.normal[1]);
            flat.push(v.normal[2]);
            flat.push(v.uv[0]);
            flat.push(v.uv[1]);
            flat.push(v.scalar);
        }
        flat
    }
}
