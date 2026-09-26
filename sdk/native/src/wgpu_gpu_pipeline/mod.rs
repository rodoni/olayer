use crate::native_controller::NativeController;
use olayer_core::geodesy::LatLon;
use wgpu::util::DeviceExt;

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
    view_mode: String,
    center_lat: u64,
    center_lon: u64,
    zoom: u64,
    rotation: u64,
    pitch: u64,
    aspect_ratio: u64,
    viewport_base_meters: u64,
    exaggeration: u32,
}

impl TerrainCacheKey {
    fn new(controller: &NativeController, exaggeration: f32) -> Self {
        Self {
            view_mode: controller.view_mode.clone(),
            center_lat: controller.camera.center.lat.to_bits(),
            center_lon: controller.camera.center.lon.to_bits(),
            zoom: controller.camera.zoom.to_bits(),
            rotation: controller.camera.rotation.to_bits(),
            pitch: controller.camera.pitch.to_bits(),
            aspect_ratio: controller.camera.aspect_ratio.to_bits(),
            viewport_base_meters: controller.camera.viewport_base_meters.to_bits(),
            exaggeration: exaggeration.to_bits(),
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
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub grid_vertex_buffer: Option<wgpu::Buffer>,
    pub grid_vertices_len: usize,

    // Raster Rendering Resources
    pub raster_pipeline: wgpu::RenderPipeline,
    pub raster_bind_group_layout: wgpu::BindGroupLayout,
    pub raster_sampler: wgpu::Sampler,
    pub loaded_gpu_tiles: std::collections::HashMap<String, WgpuRasterTile>,
    raster_tile_order: std::collections::VecDeque<String>,
    pub terrain_pipeline: wgpu::RenderPipeline,
    pub terrain_vertex_buffer: Option<wgpu::Buffer>,
    pub terrain_vertices_len: usize,
    pub terrain_style_buffer: wgpu::Buffer,
    terrain_cache_key: Option<TerrainCacheKey>,
}

impl WgpuGpuPipeline {
    pub fn new(device: &wgpu::Device, config_format: wgpu::TextureFormat) -> Self {
        // Simple WGSL shader for drawing lines (grid)
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl("struct VertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) color: vec4<f32>,\n};\n\n@group(0) @binding(0)\nvar<uniform> view_proj: mat4x4<f32>;\n@group(0) @binding(1)\nvar<uniform> grid_color: vec4<f32>;\n\n@vertex\nfn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {\n    var out: VertexOutput;\n    out.position = view_proj * vec4<f32>(pos, 1.0);\n    out.color = grid_color;\n    return out;\n}\n\n@fragment\nfn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {\n    return in.color;\n}\n".into()),
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

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

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
    out.tex_coords = in.tex_coords;
    return out;
}

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
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

        let raster_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

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
    out.normal = in.normal;
    out.elevation = in.elevation;
    out.slope = in.slope;
    out.terrain_known = in.terrain_known;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.terrain_known < 0.5) {
        let checker = (i32(in.position.x / 8.0) + i32(in.position.y / 8.0)) % 2;
        let unknown_color = select(vec3<f32>(0.35, 0.08, 0.48), vec3<f32>(0.72, 0.22, 0.82), checker == 0);
        return vec4<f32>(unknown_color, 0.94);
    }
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

    return vec4<f32>(color, 0.82);
}
".into()),
        });

        let terrain_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Terrain Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            grid_vertex_buffer: None,
            grid_vertices_len: 0,
            raster_pipeline,
            raster_bind_group_layout,
            raster_sampler,
            loaded_gpu_tiles: std::collections::HashMap::new(),
            raster_tile_order: std::collections::VecDeque::new(),
            terrain_pipeline,
            terrain_vertex_buffer: None,
            terrain_vertices_len: 0,
            terrain_style_buffer,
            terrain_cache_key: None,
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
            render_pass.set_pipeline(&self.pipeline);
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
        let span_meters = controller.camera.viewport_base_meters / controller.camera.zoom;
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
                    .get_elevation(lat.to_degrees(), lon.to_degrees())
                    .ok()
                    .filter(|elevation| elevation.is_finite());
                *elevation_cell = elevation;
                let height = elevation.unwrap_or(0.0) * exaggeration as f64;
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

        let spacing = (span_meters / grid as f64).max(1.0);
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

    pub fn rebuild_terrain_buffers(
        &mut self,
        controller: &NativeController,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        exaggeration: f32,
    ) {
        let key = TerrainCacheKey::new(controller, exaggeration);
        if self.terrain_cache_key.as_ref() == Some(&key) {
            return;
        }
        let vertices = Self::generate_terrain_vertices(controller, exaggeration);
        if vertices.is_empty() {
            self.terrain_vertex_buffer = None;
            self.terrain_vertices_len = 0;
            self.terrain_cache_key = Some(key);
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
        self.terrain_cache_key = Some(key);
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
            render_pass.set_pipeline(&self.terrain_pipeline);
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
        render_pass.set_pipeline(&self.raster_pipeline);
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
        let original = TerrainCacheKey::new(&controller, 1.0);
        let mut moved = NativeController::new(0.0, 0.0).expect("valid test controller center");
        moved.camera.center.lon = 0.000_000_001;
        let mut zoomed = NativeController::new(0.0, 0.0).expect("valid test controller center");
        zoomed.camera.zoom += 0.000_000_001;
        let mut resized = NativeController::new(0.0, 0.0).expect("valid test controller center");
        resized.camera.aspect_ratio = 1.000_000_001;

        assert_ne!(original, TerrainCacheKey::new(&moved, 1.0));
        assert_ne!(original, TerrainCacheKey::new(&zoomed, 1.0));
        assert_ne!(original, TerrainCacheKey::new(&resized, 1.0));
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
