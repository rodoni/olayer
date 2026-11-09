use crate::native_controller::NativeController;
use olayer_core::{geodesy::LatLon, terrain::TerrainError};
use wgpu::util::DeviceExt;

pub mod wms;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RasterVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub elevation: f32,
    pub slope: f32,
    pub terrain_known: f32,
}

/// Opt-in limits for sampled, whole-grid terrain LOD. Subdivisions select powers of two up to 64.
/// Bounds the source lattice and sampled geometric error used by opt-in terrain LOD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainLodOptions {
    /// Nonnegative sampled 3D position residual threshold in metres.
    pub target_error_meters: f64,
    /// Maximum subdivision level; must be a power of two in `1..=64`.
    pub max_subdivisions: u32,
    /// Maximum source-lattice samples, bounded to 4,225.
    pub max_source_samples: u32,
}

/// Diagnostics for the sampled (not globally guaranteed) terrain LOD decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainLodReport {
    pub chosen_subdivisions: u32,
    pub triangle_count: u32,
    pub samples: u32,
    /// Largest residual across all tested candidate levels; `None` means no valid probes.
    pub max_observed_error: Option<f64>,
    pub threshold_satisfied: bool,
    pub has_unknown: bool,
    pub budget_limited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainLodError {
    InvalidOptions,
    TerrainQueryFailed,
    ProjectionFailed,
}

pub struct WgpuRasterTile {
    pub key: String,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

const RASTER_TILE_SIZE: u32 = 256;
const RASTER_TILE_BYTES: usize = 256 * 256 * 4;
const MAX_GPU_RASTER_TILES: usize = 128;

#[derive(Debug, PartialEq, Eq)]
pub enum RasterTileUploadError {
    InvalidRgbaLength { expected: usize, actual: usize },
    InvalidTileCoordinates { x: u32, y: u32, z: u32 },
    ProjectionFailed,
}

#[derive(Debug, PartialEq, Eq)]
struct TerrainCacheKey {
    terrain_revision: u32,
    view_mode: String,
    center_lat: u64,
    center_lon: u64,
    zoom: u64,
    rotation: u64,
    pitch: u64,
    roll: u64,
    aspect_ratio: u64,
    viewport_base_meters: u64,
    exaggeration: u32,
    lod: Option<(u64, u32, u32)>,
    projection_reference: [u64; 6],
}

#[derive(Clone, Copy, Debug)]
struct TerrainMeshFootprint {
    center_lat: f64,
    center_lon: f64,
    lat_span: f64,
    lon_span: f64,
    subdivisions: u32,
    view_mode: &'static str,
}

fn sample_terrain_mesh_height(
    vertices: &[TerrainVertex],
    footprint: TerrainMeshFootprint,
    lat_rad: f64,
    lon_rad: f64,
) -> Option<f64> {
    if footprint.view_mode == "2D"
        || !lat_rad.is_finite()
        || !lon_rad.is_finite()
        || lat_rad.abs() > std::f64::consts::FRAC_PI_2
        || footprint.subdivisions == 0
        || footprint.lat_span <= 0.0
        || footprint.lon_span <= 0.0
    {
        return None;
    }
    let lon_delta = (lon_rad - footprint.center_lon + std::f64::consts::PI)
        .rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    let row_position = (0.5 - (lat_rad - footprint.center_lat) / footprint.lat_span)
        * f64::from(footprint.subdivisions);
    let column_position =
        (0.5 + lon_delta / footprint.lon_span) * f64::from(footprint.subdivisions);
    let grid = f64::from(footprint.subdivisions);
    if row_position < 0.0 || row_position > grid || column_position < 0.0 || column_position > grid
    {
        return None;
    }
    // Both floored coordinates were checked above to lie in [0, subdivisions], with a hard max of 64.
    let row =
        usize::try_from((row_position.floor() as u32).min(footprint.subdivisions - 1)).ok()?;
    let column =
        usize::try_from((column_position.floor() as u32).min(footprint.subdivisions - 1)).ok()?;
    let subdivisions = usize::try_from(footprint.subdivisions).ok()?;
    let cell_offset = row
        .checked_mul(subdivisions)?
        .checked_add(column)?
        .checked_mul(6)?;
    let cell = vertices.get(cell_offset..cell_offset.checked_add(6)?)?;
    let u = column_position - f64::from(u32::try_from(column).ok()?);
    let v = row_position - f64::from(u32::try_from(row).ok()?);
    let (triangle, weights) = if u + v <= 1.0 {
        ([0, 1, 2], [1.0 - u - v, u, v])
    } else {
        ([3, 4, 5], [1.0 - v, u + v - 1.0, 1.0 - u])
    };
    let triangle = triangle.map(|index| cell[index]);
    if triangle.iter().any(|vertex| vertex.terrain_known < 0.5) {
        return None;
    }
    if footprint.view_mode != "3D" {
        let height = triangle
            .iter()
            .zip(weights)
            .map(|(vertex, weight)| f64::from(vertex.position[2]) * weight)
            .sum::<f64>();
        return height.is_finite().then_some(height);
    }
    let ecef: [f64; 3] = std::array::from_fn(|axis| {
        triangle
            .iter()
            .zip(weights)
            .map(|(vertex, weight)| f64::from(vertex.position[axis]) * weight)
            .sum()
    });
    let geodetic = olayer_core::geodesy::ecef_to_lla(
        &olayer_core::geodesy::Ecef::new(ecef[0], ecef[1], ecef[2]),
        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
    );
    geodetic.height.is_finite().then_some(geodetic.height)
}

fn wrap_longitude_radians(lon: f64) -> f64 {
    (lon + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

impl TerrainCacheKey {
    fn new(
        controller: &NativeController,
        exaggeration: f32,
        lod: Option<TerrainLodOptions>,
    ) -> Self {
        let lat = controller.camera.center.lat;
        let lon = controller.camera.center.lon;
        let probe_lat = (lat + 0.05).min(std::f64::consts::FRAC_PI_2 - 0.001);
        let probe_lon = (lon + 0.05 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
            - std::f64::consts::PI;
        let mut projection_reference = [0_u64; 6];
        for (slot, coordinate) in [
            LatLon::new(lat, lon, 0.0),
            LatLon::new(probe_lat, lon, 0.0),
            LatLon::new(lat, probe_lon, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            let offset = slot * 2;
            if let Ok((x, y)) = controller.projection.project(&coordinate) {
                projection_reference[offset] = x.to_bits();
                projection_reference[offset + 1] = y.to_bits();
            } else {
                projection_reference[offset] = u64::MAX;
                projection_reference[offset + 1] = u64::MAX;
            }
        }
        Self {
            terrain_revision: controller.terrain.revision(),
            view_mode: controller.view_mode.clone(),
            center_lat: controller.camera.center.lat.to_bits(),
            center_lon: controller.camera.center.lon.to_bits(),
            zoom: controller.camera.zoom.to_bits(),
            rotation: controller.camera.rotation.to_bits(),
            pitch: controller.camera.pitch.to_bits(),
            roll: controller.camera.roll.to_bits(),
            aspect_ratio: controller.camera.aspect_ratio.to_bits(),
            viewport_base_meters: controller.camera.viewport_base_meters.to_bits(),
            exaggeration: exaggeration.to_bits(),
            lod: lod.map(|options| {
                (
                    options.target_error_meters.to_bits(),
                    options.max_subdivisions,
                    options.max_source_samples,
                )
            }),
            projection_reference,
        }
    }
}

pub struct TerrainStyle {
    pub mode: u32,
    pub min_elevation: f32,
    pub max_elevation: f32,
    pub aircraft_altitude: f32,
    pub light_azimuth_deg: f32,
    pub light_altitude_deg: f32,
    pub contour_interval: f32,
}

/// Arguments required to upload a decoded raster tile to the GPU.
pub struct RasterTileUpload<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub key: &'a str,
    pub pixels: &'a [u8],
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub controller: &'a NativeController,
}

/// Encapsulates compiled WGPU pipelines, bind groups, shaders, and uniforms
/// for grids and base maps.
///
/// The GPU pipeline is the hardware-accelerated rendering engine for the native
/// desktop SDK.  It manages the `LineList` grid shader, uniform buffers for the
/// View-Projection matrix and grid color, and dynamic vertex buffers for
/// geodetic grid lines.
pub struct WgpuGpuPipeline {
    two_d: bool,
    pipeline_2d: wgpu::RenderPipeline,
    terrain_2d_pipeline: wgpu::RenderPipeline,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub grid_vertex_buffer: Option<wgpu::Buffer>,
    pub grid_vertices_len: usize,

    // Raster Rendering Resources
    pub raster_pipeline: wgpu::RenderPipeline,
    pub(super) raster_2d_pipeline: wgpu::RenderPipeline,
    pub raster_bind_group_layout: wgpu::BindGroupLayout,
    pub raster_sampler: wgpu::Sampler,
    pub loaded_gpu_tiles: std::collections::HashMap<String, WgpuRasterTile>,
    raster_tile_order: std::collections::VecDeque<String>,
    pub terrain_pipeline: wgpu::RenderPipeline,
    pub terrain_vertex_buffer: Option<wgpu::Buffer>,
    pub terrain_vertices_len: usize,
    pub terrain_style_buffer: wgpu::Buffer,
    terrain_cache_key: Option<TerrainCacheKey>,
    terrain_lod_options: Option<TerrainLodOptions>,
    terrain_lod_report: Option<TerrainLodReport>,
    terrain_cpu_vertices: Vec<TerrainVertex>,
    terrain_footprint: Option<TerrainMeshFootprint>,
}

impl WgpuGpuPipeline {
    pub fn new(device: &wgpu::Device, config_format: wgpu::TextureFormat) -> Self {
        Self::with_depth(device, config_format, None)
    }

    /// All passes using this pipeline must provide the configured depth attachment.
    pub fn with_depth(
        device: &wgpu::Device,
        config_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let depth_state = || {
            depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        // Simple WGSL shader for drawing lines (grid)
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl("struct VertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) color: vec4<f32>,\n};\n\n@group(0) @binding(0)\nvar<uniform> view_proj: mat4x4<f32>;\n@group(0) @binding(1)\nvar<uniform> grid_color: vec4<f32>;\n\n@vertex\nfn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {\n    var out: VertexOutput;\n    out.position = view_proj * vec4<f32>(pos, 1.0);\n    out.position.z = (out.position.z + out.position.w) * 0.5;\n    out.color = grid_color;\n    return out;\n}\n\n@fragment\nfn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {\n    return in.color;\n}\n".into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("View Proj Uniform Buffer"),
            size: 256 + 16, // mat4x4 (64) + padding + vec4 (16) to align second uniform offset to 256
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let terrain_style_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Style Uniform Buffer"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: Some(std::num::NonZeroU64::new(64).unwrap()),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 256,
                        size: Some(std::num::NonZeroU64::new(16).unwrap()),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &terrain_style_buffer,
                        offset: 0,
                        size: Some(std::num::NonZeroU64::new(32).unwrap()),
                    }),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mut grid_descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("Grid Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: depth_state(),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        };
        let pipeline = device.create_render_pipeline(&grid_descriptor);
        grid_descriptor.depth_stencil = depth_state().map(|mut state| {
            state.depth_write_enabled = false;
            state.depth_compare = wgpu::CompareFunction::Always;
            state
        });
        let pipeline_2d = device.create_render_pipeline(&grid_descriptor);

        // ---------------------------------------------------------------------
        // RASTER TILE RENDERING SETUP
        // ---------------------------------------------------------------------

        let raster_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Raster Tile Shader"),
            source: wgpu::ShaderSource::Wgsl(
                "
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = view_proj * vec4<f32>(in.position, 1.0);
    out.position.z = (out.position.z + out.position.w) * 0.5;
    out.tex_coords = in.tex_coords;
    return out;
}

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    if (color.a == 0.0) { discard; }
    return color;
}
            "
                .into(),
            ),
        });

        let raster_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let raster_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Raster Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let raster_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Raster Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout, &raster_bind_group_layout],
                push_constant_ranges: &[],
            });

        let mut raster_descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("Raster Render Pipeline"),
            layout: Some(&raster_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &raster_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20, // 3 floats position (12) + 2 floats uv (8)
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &raster_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: depth_state().map(|mut state| {
                state.bias = wgpu::DepthBiasState {
                    constant: -1,
                    slope_scale: 0.0,
                    clamp: 0.0,
                };
                state
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        };
        let raster_pipeline = device.create_render_pipeline(&raster_descriptor);
        raster_descriptor.depth_stencil = depth_state().map(|mut state| {
            state.depth_write_enabled = false;
            state.depth_compare = wgpu::CompareFunction::Always;
            state
        });
        let raster_2d_pipeline = device.create_render_pipeline(&raster_descriptor);

        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl("\nstruct TerrainStyle {
    mode: f32,
    min_elevation: f32,
    max_elevation: f32,
    aircraft_altitude: f32,
    light_dir: vec3<f32>,
    contour_interval: f32,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) elevation: f32,
    @location(3) slope: f32,
    @location(4) terrain_known: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) elevation: f32,
    @location(2) slope: f32,
    @location(3) terrain_known: f32,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(2)
var<uniform> terrain_style: TerrainStyle;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = view_proj * vec4<f32>(in.position, 1.0);
    out.position.z = (out.position.z + out.position.w) * 0.5;
    out.normal = in.normal;
    out.elevation = in.elevation;
    out.slope = in.slope;
    out.terrain_known = in.terrain_known;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.terrain_known < 0.5) { discard; }
    let amount = clamp((in.elevation - terrain_style.min_elevation) / max(1.0, terrain_style.max_elevation - terrain_style.min_elevation), 0.0, 1.0);
    let hypsometric = mix(vec3<f32>(0.08, 0.28, 0.12), vec3<f32>(0.72, 0.52, 0.22), amount);
    let slope_color = mix(vec3<f32>(0.08, 0.55, 0.18), vec3<f32>(0.9, 0.08, 0.04), clamp(in.slope / 55.0, 0.0, 1.0));
    let norm = normalize(in.normal);
    let hillshade = max(0.0, dot(norm, normalize(terrain_style.light_dir)));
    let shade_color = vec3<f32>(0.12, 0.2, 0.12) + vec3<f32>(0.76, 0.72, 0.5) * hillshade;

    // TAWS / CFIT alert mode (mode == 4.0):
    let delta = in.elevation - terrain_style.aircraft_altitude;
    var taws_color = mix(vec3<f32>(0.08, 0.35, 0.14), vec3<f32>(0.15, 0.45, 0.2), clamp((delta + 1500.0) / 900.0, 0.0, 1.0));
    if (delta >= -600.0) {
        taws_color = vec3<f32>(0.95, 0.82, 0.15);
    }
    if (delta >= -150.0) {
        taws_color = vec3<f32>(0.92, 0.12, 0.12);
    }
    taws_color = taws_color * (0.45 + 0.55 * hillshade);

    var color = hypsometric;
    if (terrain_style.mode == 1.0) { color = shade_color; }
    if (terrain_style.mode == 2.0) { color = slope_color; }
    if (terrain_style.mode == 3.0) { color = mix(hypsometric, shade_color, 0.55); }
    if (terrain_style.mode == 4.0) { color = taws_color; }

    // Procedural contour lines via screen derivative (fwidth) in WGSL
    if (terrain_style.contour_interval > 1.0) {
        let val = in.elevation / terrain_style.contour_interval;
        let c = abs(fract(val - 0.5) - 0.5) / max(0.0001, fwidth(val));
        let contour_alpha = 1.0 - clamp(c - 0.5, 0.0, 1.0);
        let contour_color = vec3<f32>(0.98, 0.85, 0.28);
        color = mix(color, contour_color, contour_alpha * 0.85);
    }

    return vec4<f32>(color, 1.0);
}
".into()),
        });

        let terrain_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Terrain Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let mut terrain_descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Mesh Pipeline"),
            layout: Some(&terrain_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TerrainVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 24,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 28,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 32,
                            shader_location: 4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: depth_state(),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        };
        let terrain_pipeline = device.create_render_pipeline(&terrain_descriptor);
        terrain_descriptor.depth_stencil = depth_state().map(|mut state| {
            state.depth_write_enabled = false;
            state.depth_compare = wgpu::CompareFunction::Always;
            state
        });
        let terrain_2d_pipeline = device.create_render_pipeline(&terrain_descriptor);

        Self {
            two_d: false,
            pipeline_2d,
            terrain_2d_pipeline,
            pipeline,
            bind_group,
            uniform_buffer,
            grid_vertex_buffer: None,
            grid_vertices_len: 0,
            raster_pipeline,
            raster_2d_pipeline,
            raster_bind_group_layout,
            raster_sampler,
            loaded_gpu_tiles: std::collections::HashMap::new(),
            raster_tile_order: std::collections::VecDeque::new(),
            terrain_pipeline,
            terrain_vertex_buffer: None,
            terrain_vertices_len: 0,
            terrain_style_buffer,
            terrain_cache_key: None,
            terrain_lod_options: None,
            terrain_lod_report: None,
            terrain_cpu_vertices: Vec::new(),
            terrain_footprint: None,
        }
    }

    /// Generates grid line vertices on the CPU without touching GPU buffers.
    ///
    /// This is extracted as a pure function so it can be benchmarked independently
    /// of the wgpu device/queue lifecycle.
    pub fn generate_grid_vertices(controller: &NativeController) -> Vec<f32> {
        let mut coords: Vec<f32> = Vec::new();
        if controller.view_mode == "3D" {
            let step = 10;
            let density = 60;
            for lon in (-180..=180).step_by(step) {
                let lon_rad = (lon as f64).to_radians();
                for i in 0..density {
                    let lat0 = -90.0 + (180.0 / density as f64) * i as f64;
                    let lat1 = -90.0 + (180.0 / density as f64) * (i + 1) as f64;
                    let p0 = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat0.to_radians(), lon_rad, 0.0),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    let p1 = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat1.to_radians(), lon_rad, 0.0),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    coords.push(p0.x as f32);
                    coords.push(p0.y as f32);
                    coords.push(p0.z as f32);
                    coords.push(p1.x as f32);
                    coords.push(p1.y as f32);
                    coords.push(p1.z as f32);
                }
            }
            for lat in (-80..=80).step_by(step) {
                let lat_rad = (lat as f64).to_radians();
                for i in 0..density {
                    let lon0 = -180.0 + (360.0 / density as f64) * i as f64;
                    let lon1 = -180.0 + (360.0 / density as f64) * (i + 1) as f64;
                    let p0 = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat_rad, lon0.to_radians(), 0.0),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    let p1 = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat_rad, lon1.to_radians(), 0.0),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    coords.push(p0.x as f32);
                    coords.push(p0.y as f32);
                    coords.push(p0.z as f32);
                    coords.push(p1.x as f32);
                    coords.push(p1.y as f32);
                    coords.push(p1.z as f32);
                }
            }
        } else {
            let step = 5;
            let density = 20;
            for lon in (-180..=180).step_by(step) {
                let lon_rad = (lon as f64).to_radians();
                for i in 0..density {
                    let lat0 = -80.0 + (160.0 / density as f64) * i as f64;
                    let lat1 = -80.0 + (160.0 / density as f64) * (i + 1) as f64;
                    if let (Ok(p0), Ok(p1)) = (
                        controller.projection.project(&LatLon::new(
                            lat0.to_radians(),
                            lon_rad,
                            0.0,
                        )),
                        controller.projection.project(&LatLon::new(
                            lat1.to_radians(),
                            lon_rad,
                            0.0,
                        )),
                    ) {
                        coords.push(p0.0 as f32);
                        coords.push(p0.1 as f32);
                        coords.push(0.0);
                        coords.push(p1.0 as f32);
                        coords.push(p1.1 as f32);
                        coords.push(0.0);
                    }
                }
            }
            for lat in (-80..=80).step_by(step) {
                let lat_rad = (lat as f64).to_radians();
                for i in 0..density {
                    let lon0 = -180.0 + (360.0 / density as f64) * i as f64;
                    let lon1 = -180.0 + (360.0 / density as f64) * (i + 1) as f64;
                    if let (Ok(p0), Ok(p1)) = (
                        controller.projection.project(&LatLon::new(
                            lat_rad,
                            lon0.to_radians(),
                            0.0,
                        )),
                        controller.projection.project(&LatLon::new(
                            lat_rad,
                            lon1.to_radians(),
                            0.0,
                        )),
                    ) {
                        coords.push(p0.0 as f32);
                        coords.push(p0.1 as f32);
                        coords.push(0.0);
                        coords.push(p1.0 as f32);
                        coords.push(p1.1 as f32);
                        coords.push(0.0);
                    }
                }
            }
        }
        coords
    }

    /// Rebuilds the grid vertex buffers based on the controller view mode and active projection.
    #[inline]
    pub fn rebuild_grid_buffers(
        &mut self,
        controller: &NativeController,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.two_d = controller.view_mode == "2D";
        let coords = Self::generate_grid_vertices(controller);

        if !coords.is_empty() {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Grid Vertex Buffer"),
                size: (coords.len() * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, bytemuck::cast_slice(&coords));
            self.grid_vertex_buffer = Some(b);
            self.grid_vertices_len = coords.len();
        } else {
            self.grid_vertex_buffer = None;
            self.grid_vertices_len = 0;
        }
    }

    /// Renders the grid using the compiled pipeline.
    #[inline]
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let Some(ref buffer) = self.grid_vertex_buffer {
            render_pass.set_pipeline(if self.two_d {
                &self.pipeline_2d
            } else {
                &self.pipeline
            });
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..(self.grid_vertices_len / 3) as u32, 0..1);
        }
    }

    pub fn generate_terrain_vertices(
        controller: &NativeController,
        exaggeration: f32,
    ) -> Vec<TerrainVertex> {
        let grid = 32usize;
        let span_meters = if controller.view_mode == "3D" {
            30_000_000.0 / controller.camera.zoom
        } else {
            controller.camera.viewport_base_meters / controller.camera.zoom
        };
        let lat_span = (span_meters / 111_000.0 / 2.0)
            .to_radians()
            .min(60.0f64.to_radians());
        let lon_span = lat_span / controller.camera.center.lat.cos().abs().max(0.15);
        let mut grid_vertices = vec![None; (grid + 1) * (grid + 1)];
        let mut elevations = vec![vec![None; grid + 1]; grid + 1];

        for (row, elevation_row) in elevations.iter_mut().enumerate() {
            let v = row as f64 / grid as f64;
            let lat = controller.camera.center.lat + (0.5 - v) * lat_span;
            for (column, elevation_cell) in elevation_row.iter_mut().enumerate() {
                let u = column as f64 / grid as f64;
                let lon = controller.camera.center.lon + (u - 0.5) * lon_span;
                let elevation = controller
                    .terrain
                    .get_elevation_status(lat, wrap_longitude_radians(lon))
                    .ok()
                    .and_then(|sample| sample.elevation_meters)
                    .filter(|elevation| elevation.is_finite());
                *elevation_cell = elevation;
                let height = if controller.view_mode == "2D" {
                    0.0
                } else {
                    elevation.unwrap_or(0.0) * exaggeration as f64
                };
                let position = if controller.view_mode == "3D" {
                    let ecef = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat, lon, height),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    Some([ecef.x as f32, ecef.y as f32, ecef.z as f32])
                } else {
                    let projected = controller
                        .projection
                        .project(&LatLon::new(lat, lon, 0.0))
                        .ok();
                    projected.map(|(x, y)| [x as f32, y as f32, height as f32])
                };
                if let Some(position) = position {
                    grid_vertices[row * (grid + 1) + column] = Some(TerrainVertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                        elevation: elevation.unwrap_or(0.0) as f32,
                        slope: 0.0,
                        terrain_known: if elevation.is_some() { 1.0 } else { 0.0 },
                    });
                }
            }
        }

        let spacing = (lat_span * 6_378_137.0 / grid as f64).max(1.0);
        for (row, elevation_row) in elevations.iter().enumerate() {
            for (column, center_elevation) in elevation_row.iter().enumerate() {
                let Some(center) = *center_elevation else {
                    continue;
                };
                let east_slope = terrain_axis_slope(
                    column.checked_sub(1).and_then(|left| elevation_row[left]),
                    column
                        .checked_add(1)
                        .and_then(|right| elevation_row.get(right).copied().flatten()),
                    center,
                    spacing,
                );
                let north_elevation = row
                    .checked_sub(1)
                    .and_then(|north| elevations.get(north))
                    .and_then(|north_row| north_row.get(column).copied().flatten());
                let south_elevation = row
                    .checked_add(1)
                    .and_then(|south| elevations.get(south))
                    .and_then(|south_row| south_row.get(column).copied().flatten());
                let south_slope =
                    terrain_axis_slope(north_elevation, south_elevation, center, spacing);
                let local_normal = [-east_slope, south_slope, 1.0];
                let normal_len = local_normal
                    .iter()
                    .map(|component| component * component)
                    .sum::<f64>()
                    .sqrt();
                let lat =
                    controller.camera.center.lat + (0.5 - row as f64 / grid as f64) * lat_span;
                let lon =
                    controller.camera.center.lon + (column as f64 / grid as f64 - 0.5) * lon_span;
                let normal = if controller.view_mode == "3D" {
                    local_normal_to_ecef(
                        lat,
                        lon,
                        [
                            local_normal[0] / normal_len,
                            local_normal[1] / normal_len,
                            local_normal[2] / normal_len,
                        ],
                    )
                } else {
                    [
                        (local_normal[0] / normal_len) as f32,
                        (local_normal[1] / normal_len) as f32,
                        (local_normal[2] / normal_len) as f32,
                    ]
                };
                if let Some(vertex) = grid_vertices[row * (grid + 1) + column].as_mut() {
                    vertex.slope = east_slope.hypot(south_slope).atan().to_degrees() as f32;
                    vertex.normal = normal;
                }
            }
        }

        let mut vertices = Vec::with_capacity(grid * grid * 6);
        let row_length = grid + 1;
        for (top_row, bottom_row) in grid_vertices
            .chunks_exact(row_length)
            .zip(grid_vertices[row_length..].chunks_exact(row_length))
        {
            for (top_pair, bottom_pair) in top_row.windows(2).zip(bottom_row.windows(2)) {
                let Some(top_left_vertex) = top_pair[0] else {
                    continue;
                };
                let Some(top_right_vertex) = top_pair[1] else {
                    continue;
                };
                let Some(bottom_left_vertex) = bottom_pair[0] else {
                    continue;
                };
                let Some(bottom_right_vertex) = bottom_pair[1] else {
                    continue;
                };
                let terrain_known = if top_left_vertex.terrain_known == 1.0
                    && top_right_vertex.terrain_known == 1.0
                    && bottom_left_vertex.terrain_known == 1.0
                    && bottom_right_vertex.terrain_known == 1.0
                {
                    1.0
                } else {
                    0.0
                };
                let mut cell_vertices = [
                    top_left_vertex,
                    top_right_vertex,
                    bottom_left_vertex,
                    top_right_vertex,
                    bottom_right_vertex,
                    bottom_left_vertex,
                ];
                for vertex in &mut cell_vertices {
                    vertex.terrain_known = terrain_known;
                }
                vertices.extend_from_slice(&cell_vertices);
            }
        }
        vertices
    }

    /// Generates terrain with opt-in, bounded whole-grid LOD. Error is evaluated only at
    /// sampled lattice points; this is a diagnostic estimate, not a global error bound.
    pub fn generate_terrain_vertices_with_lod(
        controller: &NativeController,
        exaggeration: f32,
        options: TerrainLodOptions,
    ) -> Result<(Vec<TerrainVertex>, TerrainLodReport), TerrainLodError> {
        if !options.target_error_meters.is_finite()
            || options.target_error_meters < 0.0
            || options.max_subdivisions == 0
            || options.max_subdivisions > 64
            || !options.max_subdivisions.is_power_of_two()
            || options.max_source_samples == 0
            || options.max_source_samples < 4
            || options.max_source_samples > 4_225
            || !exaggeration.is_finite()
            || exaggeration < 0.0
        {
            return Err(TerrainLodError::InvalidOptions);
        }
        let source_limit = options.max_source_samples;
        let max_level = options.max_subdivisions;
        let mut level = 1u32;
        let mut candidates = Vec::with_capacity(7);
        while level <= max_level {
            let required = level
                .checked_add(1)
                .and_then(|side| side.checked_mul(side))
                .ok_or(TerrainLodError::InvalidOptions)?;
            if required > source_limit {
                break;
            }
            candidates.push(level);
            level = level.saturating_mul(2);
        }
        if candidates.is_empty() {
            return Err(TerrainLodError::InvalidOptions);
        }
        let finest = candidates.last().copied().unwrap_or(1);
        // Sample one lattice, capped at 65×65 = 4,225 points.
        let grid = finest;
        let span_meters = if controller.view_mode == "3D" {
            30_000_000.0 / controller.camera.zoom
        } else {
            controller.camera.viewport_base_meters / controller.camera.zoom
        };
        let lat_span = (span_meters / 111_000.0 / 2.0)
            .to_radians()
            .min(60.0f64.to_radians());
        let lon_span = lat_span / controller.camera.center.lat.cos().abs().max(0.15);
        let side = usize::try_from(grid + 1).map_err(|_| TerrainLodError::InvalidOptions)?;
        let mut heights = Vec::with_capacity(side * side);
        let mut positions = Vec::with_capacity(side * side);
        let mut has_unknown = false;
        for row in 0..=grid {
            let v = f64::from(row) / f64::from(grid);
            let lat = controller.camera.center.lat + (0.5 - v) * lat_span;
            for column in 0..=grid {
                let u = f64::from(column) / f64::from(grid);
                let lon = controller.camera.center.lon + (u - 0.5) * lon_span;
                let sample = match controller
                    .terrain
                    .get_elevation_status(lat, wrap_longitude_radians(lon))
                {
                    Ok(sample) => Some(sample),
                    Err(TerrainError::TileNotLoaded(_, _)) => None,
                    Err(_) => return Err(TerrainLodError::TerrainQueryFailed),
                };
                let height = sample
                    .and_then(|sample| sample.elevation_meters)
                    .filter(|value| value.is_finite() && value.abs() <= f64::from(f32::MAX));
                has_unknown |= height.is_none();
                heights.push(height);
                let z = if controller.view_mode == "2D" {
                    0.0
                } else {
                    height.unwrap_or(0.0) * f64::from(exaggeration)
                };
                let position = if controller.view_mode == "3D" {
                    let ecef = olayer_core::geodesy::lla_to_ecef(
                        &LatLon::new(lat, lon, z),
                        &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                    );
                    [ecef.x, ecef.y, ecef.z]
                } else {
                    let (x, y) = controller
                        .projection
                        .project(&LatLon::new(lat, lon, 0.0))
                        .map_err(|_| TerrainLodError::ProjectionFailed)?;
                    [x, y, z]
                };
                if position
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > f64::from(f32::MAX))
                {
                    return Err(TerrainLodError::InvalidOptions);
                }
                positions.push(position);
            }
        }
        let mut chosen = finest;
        let mut observed = None;
        let mut satisfied = false;
        for candidate in candidates.iter().copied() {
            let stride = grid / candidate;
            let mut maximum: f64 = 0.0;
            let mut comparable = true;
            let mut has_probe = false;
            for row in 0..=grid {
                for column in 0..=grid {
                    if row % stride == 0 && column % stride == 0 {
                        continue;
                    }
                    let row = row as usize;
                    let column = column as usize;
                    let stride = stride as usize;
                    let r0 = (row / stride * stride).min(grid as usize - stride);
                    let c0 = (column / stride * stride).min(grid as usize - stride);
                    let r1 = r0 + stride;
                    let c1 = c0 + stride;
                    let indices = [
                        r0 * side + c0,
                        r0 * side + c1,
                        r1 * side + c0,
                        r1 * side + c1,
                    ];
                    if indices.iter().any(|index| heights[*index].is_none())
                        || heights[row * side + column].is_none()
                    {
                        comparable = false;
                        continue;
                    }
                    has_probe = true;
                    let tx = (column - c0) as f64 / stride as f64;
                    let ty = (row - r0) as f64 / stride as f64;
                    let [tl, tr, bl, br] = indices.map(|index| positions[index]);
                    let interp: [f64; 3] = if tx + ty <= 1.0 {
                        std::array::from_fn(|axis| {
                            tl[axis] * (1.0 - tx - ty) + tr[axis] * tx + bl[axis] * ty
                        })
                    } else {
                        std::array::from_fn(|axis| {
                            br[axis] * (tx + ty - 1.0)
                                + tr[axis] * (1.0 - ty)
                                + bl[axis] * (1.0 - tx)
                        })
                    };
                    let actual = positions[row * side + column];
                    let error = (0..3)
                        .map(|axis| (actual[axis] - interp[axis]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    maximum = maximum.max(error);
                }
            }
            if has_probe {
                observed = Some(maximum);
            }
            chosen = candidate;
            if has_probe && comparable && !has_unknown && maximum <= options.target_error_meters {
                satisfied = true;
                break;
            }
        }
        let stride = grid / chosen;
        let chosen_side =
            usize::try_from(chosen + 1).map_err(|_| TerrainLodError::InvalidOptions)?;
        let mut mesh = Vec::with_capacity(chosen as usize * chosen as usize * 6);
        let spacing = (lat_span * 6_378_137.0 / f64::from(chosen)).max(1.0);
        let vertex_at = |row: usize, column: usize| -> TerrainVertex {
            let fine_row = row * stride as usize;
            let fine_column = column * stride as usize;
            let index = fine_row * side + fine_column;
            let elevation = heights[index];
            let center = elevation.unwrap_or(0.0);
            let east = terrain_axis_slope(
                column
                    .checked_sub(1)
                    .and_then(|left| heights[fine_row * side + (left * stride as usize)]),
                column
                    .checked_add(1)
                    .filter(|right| *right <= chosen as usize)
                    .and_then(|right| heights[fine_row * side + (right * stride as usize)]),
                center,
                spacing,
            );
            let north = row
                .checked_sub(1)
                .and_then(|north_row| heights[(north_row * stride as usize) * side + fine_column]);
            let south = row
                .checked_add(1)
                .filter(|south_row| *south_row <= chosen as usize)
                .and_then(|south_row| heights[(south_row * stride as usize) * side + fine_column]);
            let south_slope = terrain_axis_slope(north, south, center, spacing);
            let local_normal = [-east, south_slope, 1.0];
            let normal_length = local_normal
                .iter()
                .map(|component| component * component)
                .sum::<f64>()
                .sqrt();
            let lat =
                controller.camera.center.lat + (0.5 - row as f64 / f64::from(chosen)) * lat_span;
            let lon =
                controller.camera.center.lon + (column as f64 / f64::from(chosen) - 0.5) * lon_span;
            let normal = if controller.view_mode == "3D" {
                local_normal_to_ecef(
                    lat,
                    lon,
                    [
                        local_normal[0] / normal_length,
                        local_normal[1] / normal_length,
                        local_normal[2] / normal_length,
                    ],
                )
            } else {
                [
                    (local_normal[0] / normal_length) as f32,
                    (local_normal[1] / normal_length) as f32,
                    (local_normal[2] / normal_length) as f32,
                ]
            };
            let position = positions[index].map(|value| value as f32);
            TerrainVertex {
                position,
                normal,
                elevation: center as f32,
                slope: east.hypot(south_slope).atan().to_degrees() as f32,
                terrain_known: if elevation.is_some() { 1.0 } else { 0.0 },
            }
        };
        let mut vertices = Vec::with_capacity(chosen_side * chosen_side);
        for row in 0..chosen_side {
            for column in 0..chosen_side {
                vertices.push(vertex_at(row, column));
            }
        }
        for out_row in 0..chosen as usize {
            for out_column in 0..chosen as usize {
                let r = out_row * stride as usize;
                let c = out_column * stride as usize;
                let indices = [
                    out_row * chosen_side + out_column,
                    out_row * chosen_side + out_column + 1,
                    (out_row + 1) * chosen_side + out_column,
                    (out_row + 1) * chosen_side + out_column + 1,
                ];
                let mut known = indices.iter().all(|index| heights[*index].is_some());
                // An unknown source sample anywhere inside the coarse cell must not be bridged.
                for fine_row in r..=r + stride as usize {
                    for fine_column in c..=c + stride as usize {
                        if heights[fine_row * side + fine_column].is_none() {
                            known = false;
                        }
                    }
                }
                let mut points = indices.map(|index| vertices[index]);
                for point in &mut points {
                    point.terrain_known = if known { 1.0 } else { 0.0 };
                }
                let [tl, tr, bl, br] = points;
                mesh.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            }
        }
        let samples = u32::try_from(heights.len()).map_err(|_| TerrainLodError::InvalidOptions)?;
        let triangle_count =
            u32::try_from(mesh.len() / 3).map_err(|_| TerrainLodError::InvalidOptions)?;
        Ok((
            mesh,
            TerrainLodReport {
                chosen_subdivisions: chosen,
                triangle_count,
                samples,
                max_observed_error: observed,
                threshold_satisfied: satisfied,
                has_unknown,
                budget_limited: finest < options.max_subdivisions
                    || (!satisfied && chosen == finest),
            },
        ))
    }

    /// Selects the bounded sampled LOD path for subsequent terrain rebuilds.
    /// Passing `None` restores the historical fixed 32-subdivision mesh.
    pub fn set_terrain_lod(
        &mut self,
        options: Option<TerrainLodOptions>,
    ) -> Result<(), TerrainLodError> {
        if let Some(options) = options {
            if !options.target_error_meters.is_finite()
                || options.target_error_meters < 0.0
                || options.max_subdivisions == 0
                || options.max_subdivisions > 64
                || !options.max_subdivisions.is_power_of_two()
                || options.max_source_samples == 0
                || options.max_source_samples < 4
                || options.max_source_samples > 4_225
            {
                return Err(TerrainLodError::InvalidOptions);
            }
        }
        self.terrain_lod_options = options;
        self.terrain_lod_report = None;
        self.terrain_cache_key = None;
        Ok(())
    }

    /// Returns the last quality report from a successful adaptive terrain rebuild.
    pub fn terrain_lod_report(&self) -> Option<TerrainLodReport> {
        self.terrain_lod_report
    }

    /// Samples the retained CPU copy of the currently rendered terrain triangles.
    /// Returns `None` for 2D mode, outside the local patch, or any cell containing unknown terrain.
    /// No GPU readback or source-grid interpolation is performed.
    pub fn terrain_mesh_height_at(&self, lat_rad: f64, lon_rad: f64) -> Option<f64> {
        let footprint = self.terrain_footprint?;
        sample_terrain_mesh_height(&self.terrain_cpu_vertices, footprint, lat_rad, lon_rad)
    }

    pub fn rebuild_terrain_buffers(
        &mut self,
        controller: &NativeController,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        exaggeration: f32,
    ) {
        self.two_d = controller.view_mode == "2D";
        let key = TerrainCacheKey::new(controller, exaggeration, self.terrain_lod_options);
        if self.terrain_cache_key.as_ref() == Some(&key) {
            return;
        }
        let (vertices, lod_report) = match self.terrain_lod_options {
            Some(options) => {
                match Self::generate_terrain_vertices_with_lod(controller, exaggeration, options) {
                    Ok((vertices, report)) => (vertices, Some(report)),
                    Err(_) => return,
                }
            }
            None => (
                Self::generate_terrain_vertices(controller, exaggeration),
                None,
            ),
        };
        if vertices.is_empty() {
            self.terrain_vertex_buffer = None;
            self.terrain_vertices_len = 0;
            self.terrain_cpu_vertices.clear();
            self.terrain_footprint = None;
            self.terrain_cache_key = Some(key);
            self.terrain_lod_report = lod_report;
            return;
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Vertex Buffer"),
            size: (vertices.len() * std::mem::size_of::<TerrainVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));
        self.terrain_vertex_buffer = Some(buffer);
        self.terrain_vertices_len = vertices.len();
        let subdivisions = lod_report.map_or(32, |report| report.chosen_subdivisions);
        let span_meters = if controller.view_mode == "3D" {
            30_000_000.0 / controller.camera.zoom
        } else {
            controller.camera.viewport_base_meters / controller.camera.zoom
        };
        let lat_span = (span_meters / 111_000.0 / 2.0)
            .to_radians()
            .min(60.0_f64.to_radians());
        let lon_span = lat_span / controller.camera.center.lat.cos().abs().max(0.15);
        self.terrain_cpu_vertices = vertices.clone();
        self.terrain_footprint = Some(TerrainMeshFootprint {
            center_lat: controller.camera.center.lat,
            center_lon: controller.camera.center.lon,
            lat_span,
            lon_span,
            subdivisions,
            view_mode: if controller.view_mode == "3D" {
                "3D"
            } else if controller.view_mode == "2.5D" {
                "2.5D"
            } else {
                "2D"
            },
        });
        self.terrain_cache_key = Some(key);
        self.terrain_lod_report = lod_report;
    }

    pub fn invalidate_terrain_cache(&mut self) {
        self.terrain_cache_key = None;
    }

    pub fn set_terrain_style(&self, queue: &wgpu::Queue, style: &TerrainStyle) {
        let az = (style.light_azimuth_deg as f64).to_radians();
        let alt = (style.light_altitude_deg as f64).to_radians();
        let lx = (az.sin() * alt.cos()) as f32;
        let ly = (az.cos() * alt.cos()) as f32;
        let lz = alt.sin() as f32;
        queue.write_buffer(
            &self.terrain_style_buffer,
            0,
            bytemuck::cast_slice(&[
                style.mode as f32,
                style.min_elevation,
                style.max_elevation,
                style.aircraft_altitude,
                lx,
                ly,
                lz,
                style.contour_interval,
            ]),
        );
    }

    pub fn render_terrain<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let Some(ref buffer) = self.terrain_vertex_buffer {
            render_pass.set_pipeline(if self.two_d {
                &self.terrain_2d_pipeline
            } else {
                &self.terrain_pipeline
            });
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..self.terrain_vertices_len as u32, 0..1);
        }
    }

    /// Uploads a decoded raster tile to GPU memory and creates its projected quads.
    pub fn upload_raster_tile(
        &mut self,
        upload: RasterTileUpload<'_>,
    ) -> Result<(), RasterTileUploadError> {
        self.two_d = upload.controller.view_mode == "2D";
        validate_raster_tile_upload(upload.pixels, upload.x, upload.y, upload.z)?;
        if self.loaded_gpu_tiles.contains_key(upload.key) {
            return Ok(());
        }
        let vertices = get_tile_vertices(upload.x, upload.y, upload.z, upload.controller)?;

        // 1. Create Texture
        let texture_size = wgpu::Extent3d {
            width: RASTER_TILE_SIZE,
            height: RASTER_TILE_SIZE,
            depth_or_array_layers: 1,
        };
        let texture = upload.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Tile Texture {}", upload.key)),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // 2. Upload Pixels
        upload.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            upload.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * RASTER_TILE_SIZE),
                rows_per_image: Some(RASTER_TILE_SIZE),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 3. Create Bind Group
        let bind_group = upload.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Tile Bind Group {}", upload.key)),
            layout: &self.raster_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.raster_sampler),
                },
            ],
        });

        // 4. Create Buffers
        let vertex_buffer = upload
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Tile Vertex Buffer {}", upload.key)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let index_buffer = upload
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Tile Index Buffer {}", upload.key)),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        if self.loaded_gpu_tiles.len() >= MAX_GPU_RASTER_TILES {
            if let Some(evicted_key) = self.raster_tile_order.pop_front() {
                self.loaded_gpu_tiles.remove(&evicted_key);
            }
        }
        let key = upload.key.to_string();
        self.raster_tile_order.push_back(key.clone());
        self.loaded_gpu_tiles.insert(
            key,
            WgpuRasterTile {
                key: upload.key.to_string(),
                x: upload.x,
                y: upload.y,
                z: upload.z,
                texture,
                bind_group,
                vertex_buffer,
                index_buffer,
            },
        );
        Ok(())
    }

    /// Rebuilds the quad vertex buffers for all uploaded tiles (e.g. when projection changes).
    pub fn rebuild_raster_tile_buffers(
        &mut self,
        device: &wgpu::Device,
        controller: &NativeController,
    ) {
        self.loaded_gpu_tiles.retain(|_, tile| {
            let Ok(vertices) = get_tile_vertices(tile.x, tile.y, tile.z, controller) else {
                return false;
            };
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Tile Vertex Buffer {}", tile.key)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            tile.vertex_buffer = vertex_buffer;
            true
        });
        self.raster_tile_order
            .retain(|key| self.loaded_gpu_tiles.contains_key(key));
    }

    /// Clears all raster textures from the GPU memory.
    pub fn clear_raster_tiles(&mut self) {
        self.loaded_gpu_tiles.clear();
        self.raster_tile_order.clear();
    }

    pub fn has_raster_tile(&self, key: &str) -> bool {
        self.loaded_gpu_tiles.contains_key(key)
    }

    /// Renders all uploaded raster tiles that are currently visible.
    pub fn render_raster_tiles<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        visible_keys: &std::collections::HashSet<String>,
    ) {
        if self.loaded_gpu_tiles.is_empty() {
            return;
        }
        render_pass.set_pipeline(if self.two_d {
            &self.raster_2d_pipeline
        } else {
            &self.raster_pipeline
        });
        render_pass.set_bind_group(0, &self.bind_group, &[]); // Uniforms

        for tile in self.loaded_gpu_tiles.values() {
            if visible_keys.contains(&tile.key) {
                render_pass.set_bind_group(1, &tile.bind_group, &[]); // Texture
                render_pass.set_vertex_buffer(0, tile.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(tile.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..1);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Helper mathematical functions for OSM / WMTS tile coordinates
// -----------------------------------------------------------------------------

fn validate_tile_coordinates(x: u32, y: u32, z: u32) -> Result<u32, RasterTileUploadError> {
    let Some(tile_count) = 1u32.checked_shl(z) else {
        return Err(RasterTileUploadError::InvalidTileCoordinates { x, y, z });
    };
    if x >= tile_count || y >= tile_count {
        return Err(RasterTileUploadError::InvalidTileCoordinates { x, y, z });
    }
    Ok(tile_count)
}

fn validate_raster_tile_upload(
    pixels: &[u8],
    x: u32,
    y: u32,
    z: u32,
) -> Result<(), RasterTileUploadError> {
    if pixels.len() != RASTER_TILE_BYTES {
        return Err(RasterTileUploadError::InvalidRgbaLength {
            expected: RASTER_TILE_BYTES,
            actual: pixels.len(),
        });
    }
    validate_tile_coordinates(x, y, z)?;
    Ok(())
}

fn tile_bounds_rad(x: u32, y: u32, z: u32) -> Result<(f64, f64, f64, f64), RasterTileUploadError> {
    let tile_count = validate_tile_coordinates(x, y, z)?;
    let Some(east_x) = x.checked_add(1) else {
        return Err(RasterTileUploadError::InvalidTileCoordinates { x, y, z });
    };
    let Some(south_y) = y.checked_add(1) else {
        return Err(RasterTileUploadError::InvalidTileCoordinates { x, y, z });
    };
    let n = f64::from(tile_count);
    let lon_west = (f64::from(x) / n) * 360.0 - 180.0;
    let lon_east = (f64::from(east_x) / n) * 360.0 - 180.0;

    let lat_north_rad = (std::f64::consts::PI * (1.0 - 2.0 * f64::from(y) / n))
        .sinh()
        .atan();
    let lat_south_rad = (std::f64::consts::PI * (1.0 - 2.0 * f64::from(south_y) / n))
        .sinh()
        .atan();

    Ok((
        lat_south_rad,
        lon_west.to_radians(),
        lat_north_rad,
        lon_east.to_radians(),
    ))
}

fn get_tile_vertices(
    x: u32,
    y: u32,
    z: u32,
    controller: &NativeController,
) -> Result<[RasterVertex; 4], RasterTileUploadError> {
    let (lat_south, lon_west, lat_north, lon_east) = tile_bounds_rad(x, y, z)?;

    let get_pos = |lat: f64, lon: f64| -> Result<[f32; 3], RasterTileUploadError> {
        if controller.view_mode == "3D" {
            let ecef = olayer_core::geodesy::lla_to_ecef(
                &LatLon::new(lat, lon, 0.0),
                &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
            );
            Ok([ecef.x as f32, ecef.y as f32, ecef.z as f32])
        } else {
            let proj = controller
                .projection
                .project(&LatLon::new(lat, lon, 0.0))
                .map_err(|_| RasterTileUploadError::ProjectionFailed)?;
            Ok([proj.0 as f32, proj.1 as f32, 0.0])
        }
    };

    let p_tl = get_pos(lat_north, lon_west)?;
    let p_bl = get_pos(lat_south, lon_west)?;
    let p_br = get_pos(lat_south, lon_east)?;
    let p_tr = get_pos(lat_north, lon_east)?;

    Ok([
        RasterVertex {
            position: p_tl,
            tex_coords: [0.0, 0.0],
        },
        RasterVertex {
            position: p_bl,
            tex_coords: [0.0, 1.0],
        },
        RasterVertex {
            position: p_br,
            tex_coords: [1.0, 1.0],
        },
        RasterVertex {
            position: p_tr,
            tex_coords: [1.0, 0.0],
        },
    ])
}

fn terrain_axis_slope(
    negative_neighbor: Option<f64>,
    positive_neighbor: Option<f64>,
    center: f64,
    spacing: f64,
) -> f64 {
    match (negative_neighbor, positive_neighbor) {
        (Some(negative), Some(positive)) => (positive - negative) / (2.0 * spacing),
        (None, Some(positive)) => (positive - center) / spacing,
        (Some(negative), None) => (center - negative) / spacing,
        (None, None) => 0.0,
    }
}

fn local_normal_to_ecef(lat: f64, lon: f64, normal: [f64; 3]) -> [f32; 3] {
    let east = [-lon.sin(), lon.cos(), 0.0];
    let north = [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()];
    let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
    let ecef = std::array::from_fn::<_, 3, _>(|index| {
        east[index] * normal[0] + north[index] * normal[1] + up[index] * normal[2]
    });
    let length = ecef
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt();
    [
        (ecef[0] / length) as f32,
        (ecef[1] / length) as f32,
        (ecef[2] / length) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_controller::NativeController;

    #[test]
    fn test_generate_grid_vertices_2d_not_empty() {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        assert!(!vertices.is_empty(), "2D grid should produce vertices");
        // Each vertex is 3 floats (x, y, z); each line segment is 2 vertices = 6 floats
        assert_eq!(
            vertices.len() % 6,
            0,
            "Vertex count must be a multiple of 6 (2 endpoints × 3 coords)"
        );
    }

    #[test]
    fn test_generate_terrain_vertices_has_two_triangles_per_cell() {
        let controller = NativeController::new((-23.62f64).to_radians(), (-46.65f64).to_radians())
            .expect("valid test controller center");
        let vertices = WgpuGpuPipeline::generate_terrain_vertices(&controller, 1.0);
        assert_eq!(vertices.len(), 32 * 32 * 6);
        assert!(vertices.iter().all(|vertex| {
            vertex.position.iter().all(|value| value.is_finite())
                && vertex.normal.iter().all(|value| value.is_finite())
                && vertex.elevation.is_finite()
                && vertex.slope.is_finite()
        }));
    }

    #[test]
    fn test_generate_grid_vertices_3d_not_empty() {
        let mut controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        assert!(!vertices.is_empty(), "3D grid should produce vertices");
        assert_eq!(
            vertices.len() % 6,
            0,
            "Vertex count must be a multiple of 6 (2 endpoints × 3 coords)"
        );
    }

    #[test]
    fn test_generate_grid_vertices_3d_uses_ecef_scale() {
        let mut controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // ECEF coordinates for Earth surface should be on the order of millions of meters.
        // Check that at least one vertex has a magnitude > 6_000_000 (roughly Earth radius).
        let max_abs = vertices.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!(
            max_abs > 6_000_000.0,
            "3D grid vertices should be in ECEF scale, max abs = {max_abs}"
        );
    }

    #[test]
    fn test_generate_grid_vertices_2d_z_is_zero() {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // 2D grid vertices always have z = 0.0 (flat plane)
        let (chunks, _) = vertices.as_chunks::<3>();
        for chunk in chunks {
            assert_eq!(chunk[2], 0.0, "2D grid vertices must have z = 0.0");
        }
    }

    #[test]
    fn test_generate_grid_vertices_3d_z_is_nonzero() {
        let mut controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // 3D grid uses ECEF so at least some z components should be non-zero
        let (chunks, _) = vertices.as_chunks::<3>();
        let has_nonzero_z = chunks.iter().any(|c| c[2] != 0.0);
        assert!(
            has_nonzero_z,
            "3D grid should have non-zero z components (ECEF)"
        );
    }

    #[test]
    fn terrain_mesh_keeps_missing_elevation_explicitly_unknown() {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let vertices = WgpuGpuPipeline::generate_terrain_vertices(&controller, 1.0);

        assert!(vertices.iter().all(|vertex| vertex.terrain_known == 0.0));
    }

    fn lod_options(target: f64, subdivisions: u32, samples: u32) -> TerrainLodOptions {
        TerrainLodOptions {
            target_error_meters: target,
            max_subdivisions: subdivisions,
            max_source_samples: samples,
        }
    }

    fn controller_with_flat_terrain() -> NativeController {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let grid = olayer_core::terrain::ElevationGrid::new(
            olayer_core::terrain::ElevationGridCrs::Geographic,
            [
                -std::f64::consts::PI,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
                std::f64::consts::FRAC_PI_2,
            ],
            2,
            2,
            vec![0.0; 4].into_boxed_slice(),
            None,
        )
        .expect("valid test elevation grid");
        controller
            .terrain
            .load_elevation_grid("lod-test".to_string(), grid);
        controller
    }

    fn controller_with_grid(samples: Vec<f64>, size: usize, zoom: f64) -> NativeController {
        let mut controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        controller.view_mode = "2.5D".to_string();
        controller.camera.zoom = zoom;
        controller.camera.viewport_base_meters = 1_000_000.0;
        let grid = olayer_core::terrain::ElevationGrid::new(
            olayer_core::terrain::ElevationGridCrs::Geographic,
            [-0.1, -0.1, 0.1, 0.1],
            size,
            size,
            samples.into_boxed_slice(),
            None,
        )
        .expect("valid test elevation grid");
        controller
            .terrain
            .load_elevation_grid("lod-test-grid".to_string(), grid);
        controller
    }

    fn controller_with_nodata() -> NativeController {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let grid = olayer_core::terrain::ElevationGrid::new(
            olayer_core::terrain::ElevationGridCrs::Geographic,
            [
                -std::f64::consts::PI,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
                std::f64::consts::FRAC_PI_2,
            ],
            2,
            2,
            vec![-9999.0; 4].into_boxed_slice(),
            Some(-9999.0),
        )
        .expect("valid test nodata grid");
        controller
            .terrain
            .load_elevation_grid("lod-nodata".to_string(), grid);
        controller
    }

    #[test]
    fn lod_uses_selected_grid_and_reports_unknown_terrain() {
        let controller = controller_with_flat_terrain();
        let (mesh, report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(1_000_000_000.0, 16, 289),
        )
        .expect("bounded LOD generation");
        assert_eq!(
            mesh.len(),
            report.chosen_subdivisions as usize * report.chosen_subdivisions as usize * 6
        );
        assert_eq!(report.samples, 289);
        assert_eq!(report.chosen_subdivisions, 1);
        assert!(!report.has_unknown);
        assert!(report.threshold_satisfied);
        assert!(mesh
            .iter()
            .all(|vertex| vertex.position.iter().all(|value| value.is_finite())));
        assert!(mesh.iter().all(|vertex| vertex.terrain_known == 1.0));
    }

    #[test]
    fn lod_keeps_an_affine_ramp_coarse_and_refines_a_sampled_ridge() {
        let ramp = controller_with_grid(vec![0.0, 100.0, 100.0, 200.0], 2, 100.0);
        let (_, ramp_report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &ramp,
            1.0,
            lod_options(0.1, 8, 81),
        )
        .expect("bounded ramp LOD");
        assert_eq!(ramp_report.chosen_subdivisions, 1);
        assert!(ramp_report.threshold_satisfied);

        let ridge =
            controller_with_grid(vec![0.0, 0.0, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0, 0.0], 3, 1.0);
        let (_, ridge_report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &ridge,
            1.0,
            lod_options(0.1, 8, 81),
        )
        .expect("bounded ridge LOD");
        assert!(ridge_report.chosen_subdivisions > 1);
        assert!(
            ridge_report.max_observed_error.unwrap_or_default() > 0.1,
            "report: {ridge_report:?}"
        );
        assert!(!ridge_report.threshold_satisfied);
    }

    #[test]
    fn lod_wraps_elevation_queries_at_the_antimeridian() {
        let mut controller = NativeController::new(0.0, std::f64::consts::PI - 0.01)
            .expect("valid date-line camera");
        controller.view_mode = "3D".to_string();
        let grid = olayer_core::terrain::ElevationGrid::new(
            olayer_core::terrain::ElevationGridCrs::Geographic,
            [-std::f64::consts::PI, -1.0, std::f64::consts::PI, 1.0],
            2,
            2,
            vec![100.0; 4].into_boxed_slice(),
            None,
        )
        .unwrap();
        controller
            .terrain
            .load_elevation_grid("world-grid".into(), grid);
        let (_, report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(1_000_000.0, 16, 289),
        )
        .unwrap();
        assert!(!report.has_unknown);
        assert!(report.triangle_count > 0);
    }

    #[test]
    fn lod_3d_mesh_places_camera_center_at_requested_ellipsoidal_height() {
        let mut controller = controller_with_grid(vec![500.0; 9], 3, 20.0);
        controller.view_mode = "3D".to_string();
        let (mesh, report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(0.0, 64, 4_225),
        )
        .expect("bounded 3D terrain LOD");
        assert_eq!(report.chosen_subdivisions, 64);
        let center = [6_378_637.0_f64, 0.0, 0.0];
        let error = mesh
            .iter()
            .map(|vertex| {
                let [x, y, z] = vertex.position.map(f64::from);
                ((x - center[0]).powi(2) + (y - center[1]).powi(2) + (z - center[2]).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(error < 1.0, "center vertex positional error was {error} m");
    }

    #[test]
    fn full_resolution_lod_positions_match_legacy_3d_mesh() {
        let mut controller = controller_with_grid(vec![500.0; 4], 2, 20.0);
        controller.view_mode = "3D".to_string();
        let fixed = WgpuGpuPipeline::generate_terrain_vertices(&controller, 1.0);
        let (adaptive, _) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(0.0, 32, 1_089),
        )
        .unwrap();
        assert_eq!(fixed.len(), adaptive.len());
        for (fixed, adaptive) in fixed.iter().zip(adaptive) {
            for (fixed_axis, adaptive_axis) in fixed.position.into_iter().zip(adaptive.position) {
                assert!((f64::from(fixed_axis) - f64::from(adaptive_axis)).abs() < 1.0e-6);
            }
            assert_eq!(fixed.terrain_known, adaptive.terrain_known);
        }
    }

    #[test]
    fn mesh_height_provider_samples_the_rendered_triangle_and_preserves_unknown_holes() {
        let mut vertices = vec![
            TerrainVertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                elevation: 0.0,
                slope: 0.0,
                terrain_known: 1.0,
            };
            6
        ];
        vertices[0].position = [-1.0, 1.0, 0.0];
        vertices[1].position = [1.0, 1.0, 1.0];
        vertices[2].position = [-1.0, -1.0, 1.0];
        vertices[3].position = [1.0, 1.0, 1.0];
        vertices[4].position = [1.0, -1.0, 2.0];
        vertices[5].position = [-1.0, -1.0, 1.0];
        let footprint = TerrainMeshFootprint {
            center_lat: 0.0,
            center_lon: 0.0,
            lat_span: 2.0,
            lon_span: 2.0,
            subdivisions: 1,
            view_mode: "2.5D",
        };
        assert_eq!(
            sample_terrain_mesh_height(&vertices, footprint, 0.0, 0.0),
            Some(1.0)
        );
        assert_eq!(
            sample_terrain_mesh_height(&vertices, footprint, 2.0, 0.0),
            None
        );
        vertices[1].terrain_known = 0.0;
        assert_eq!(
            sample_terrain_mesh_height(&vertices, footprint, 0.0, 0.0),
            None
        );
    }

    #[test]
    fn mesh_height_provider_converts_interpolated_ecef_triangle() {
        let ellipsoid = olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84();
        let lat_lons = [
            (-0.1, -0.1),
            (-0.1, 0.1),
            (0.1, -0.1),
            (-0.1, 0.1),
            (0.1, 0.1),
            (0.1, -0.1),
        ];
        let vertices = lat_lons.map(|(lat, lon)| {
            let position =
                olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat, lon, 100.0), &ellipsoid);
            let to_gpu_float = |value: f64| {
                assert!(value.is_finite() && value.abs() <= f64::from(f32::MAX));
                // The test intentionally models the renderer's finite f32 vertex representation.
                value as f32
            };
            TerrainVertex {
                position: [
                    to_gpu_float(position.x),
                    to_gpu_float(position.y),
                    to_gpu_float(position.z),
                ],
                normal: [0.0, 0.0, 1.0],
                elevation: 100.0,
                slope: 0.0,
                terrain_known: 1.0,
            }
        });
        let footprint = TerrainMeshFootprint {
            center_lat: 0.0,
            center_lon: 0.0,
            lat_span: 0.2,
            lon_span: 0.2,
            subdivisions: 1,
            view_mode: "3D",
        };
        let height = sample_terrain_mesh_height(&vertices, footprint, 0.0, 0.0).unwrap();
        assert!(height.is_finite());
        assert!(height < 100.0);
    }

    #[test]
    fn lod_caps_source_samples_and_rejects_impossible_budget() {
        let controller = controller_with_flat_terrain();
        let (_, report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(0.0, 64, 81),
        )
        .expect("bounded LOD generation");
        assert_eq!(report.samples, 81);
        assert!(report.budget_limited);
        assert!(matches!(
            WgpuGpuPipeline::generate_terrain_vertices_with_lod(
                &controller,
                1.0,
                lod_options(0.0, 64, 1)
            ),
            Err(TerrainLodError::InvalidOptions)
        ));
        assert!(matches!(
            WgpuGpuPipeline::generate_terrain_vertices_with_lod(
                &controller,
                1.0,
                lod_options(0.0, 48, 2_401)
            ),
            Err(TerrainLodError::InvalidOptions)
        ));
    }

    #[test]
    fn lod_nodata_prevents_threshold_claim_and_mesh_bridging() {
        let controller = controller_with_nodata();
        let (mesh, report) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
            &controller,
            1.0,
            lod_options(0.0, 8, 81),
        )
        .expect("bounded LOD generation");
        assert!(report.has_unknown);
        assert!(!report.threshold_satisfied);
        assert!(mesh.iter().all(|vertex| vertex.terrain_known == 0.0));
    }

    #[test]
    fn flat_surface_normal_is_rotated_into_ecef() {
        let lat = 0.7;
        let lon = -1.2;
        let normal = local_normal_to_ecef(lat, lon, [0.0, 0.0, 1.0]);
        let expected = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];

        for (actual, expected) in normal.into_iter().zip(expected) {
            assert!((f64::from(actual) - expected).abs() < 1.0e-6);
        }

        let east = local_normal_to_ecef(lat, lon, [1.0, 0.0, 0.0]);
        let north = local_normal_to_ecef(lat, lon, [0.0, 1.0, 0.0]);
        assert!((f64::from(east[0]) + lon.sin()).abs() < 1.0e-6);
        assert!((f64::from(east[1]) - lon.cos()).abs() < 1.0e-6);
        assert!(east[2].abs() < 1.0e-6);
        assert!((f64::from(north[0]) + lat.sin() * lon.cos()).abs() < 1.0e-6);
        assert!((f64::from(north[1]) + lat.sin() * lon.sin()).abs() < 1.0e-6);
        assert!((f64::from(north[2]) - lat.cos()).abs() < 1.0e-6);
    }

    #[test]
    fn terrain_cache_key_uses_exact_camera_state() {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");
        let original = TerrainCacheKey::new(&controller, 1.0, None);
        let mut moved = NativeController::new(0.0, 0.0).expect("valid test controller center");
        moved.camera.center.lon = 0.000_000_001;
        let mut zoomed = NativeController::new(0.0, 0.0).expect("valid test controller center");
        zoomed.camera.zoom += 0.000_000_001;
        let mut resized = NativeController::new(0.0, 0.0).expect("valid test controller center");
        resized.camera.aspect_ratio = 1.000_000_001;

        assert_ne!(original, TerrainCacheKey::new(&moved, 1.0, None));
        assert_ne!(original, TerrainCacheKey::new(&zoomed, 1.0, None));
        assert_ne!(original, TerrainCacheKey::new(&resized, 1.0, None));
        let mut rolled = NativeController::new(0.0, 0.0).expect("valid test controller center");
        rolled.camera.roll = 0.25;
        assert_ne!(original, TerrainCacheKey::new(&rolled, 1.0, None));
        assert_ne!(
            original,
            TerrainCacheKey::new(
                &controller,
                1.0,
                Some(TerrainLodOptions {
                    target_error_meters: 10.0,
                    max_subdivisions: 32,
                    max_source_samples: 1_089,
                })
            )
        );
    }

    #[test]
    fn raster_upload_validation_rejects_bad_rgba_and_tile_coordinates() {
        let rgba = vec![0; 256 * 256 * 4];
        assert!(validate_raster_tile_upload(&rgba, 0, 0, 0).is_ok());
        assert!(validate_raster_tile_upload(&rgba[..rgba.len() - 1], 0, 0, 0).is_err());
        assert!(validate_raster_tile_upload(&rgba, 1, 0, 0).is_err());
        assert!(validate_raster_tile_upload(&rgba, 0, 0, 32).is_err());
    }

    #[test]
    fn tile_bounds_reject_out_of_range_and_overflowing_zoom() {
        assert!(tile_bounds_rad(1, 0, 0).is_err());
        assert!(tile_bounds_rad(0, 0, 32).is_err());
        assert!(tile_bounds_rad(1, 1, 1).is_ok());
    }

    #[test]
    fn missing_neighbors_use_one_sided_gradient_without_zero_elevation() {
        assert_eq!(terrain_axis_slope(None, Some(120.0), 100.0, 10.0), 2.0);
        assert_eq!(terrain_axis_slope(Some(90.0), None, 100.0, 10.0), 1.0);
    }

    #[test]
    fn raster_projection_failure_is_reported_instead_of_origin_fallback() {
        let controller = NativeController::new(0.0, 0.0).expect("valid test controller center");

        assert!(matches!(
            get_tile_vertices(31, 16, 5, &controller),
            Err(RasterTileUploadError::ProjectionFailed)
        ));
    }
}

pub mod wfs;

#[cfg(test)]
mod depth_tests;
