use olayer_core::geodesy::LatLon;
use crate::native_controller::NativeController;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RasterVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
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
            ],
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
            source: wgpu::ShaderSource::Wgsl("
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
            ".into()),
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

        let raster_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let raster_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
                    let p0 = olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat0.to_radians(), lon_rad, 0.0), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
                    let p1 = olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat1.to_radians(), lon_rad, 0.0), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
                    coords.push(p0.x as f32); coords.push(p0.y as f32); coords.push(p0.z as f32);
                    coords.push(p1.x as f32); coords.push(p1.y as f32); coords.push(p1.z as f32);
                }
            }
            for lat in (-80..=80).step_by(step) {
                let lat_rad = (lat as f64).to_radians();
                for i in 0..density {
                    let lon0 = -180.0 + (360.0 / density as f64) * i as f64;
                    let lon1 = -180.0 + (360.0 / density as f64) * (i + 1) as f64;
                    let p0 = olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat_rad, lon0.to_radians(), 0.0), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
                    let p1 = olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat_rad, lon1.to_radians(), 0.0), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
                    coords.push(p0.x as f32); coords.push(p0.y as f32); coords.push(p0.z as f32);
                    coords.push(p1.x as f32); coords.push(p1.y as f32); coords.push(p1.z as f32);
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
                    if let (Ok(p0), Ok(p1)) = (controller.projection.project(&LatLon::new(lat0.to_radians(), lon_rad, 0.0)), controller.projection.project(&LatLon::new(lat1.to_radians(), lon_rad, 0.0))) {
                        coords.push(p0.0 as f32); coords.push(p0.1 as f32); coords.push(0.0);
                        coords.push(p1.0 as f32); coords.push(p1.1 as f32); coords.push(0.0);
                    }
                }
            }
            for lat in (-80..=80).step_by(step) {
                let lat_rad = (lat as f64).to_radians();
                for i in 0..density {
                    let lon0 = -180.0 + (360.0 / density as f64) * i as f64;
                    let lon1 = -180.0 + (360.0 / density as f64) * (i + 1) as f64;
                    if let (Ok(p0), Ok(p1)) = (controller.projection.project(&LatLon::new(lat_rad, lon0.to_radians(), 0.0)), controller.projection.project(&LatLon::new(lat_rad, lon1.to_radians(), 0.0))) {
                        coords.push(p0.0 as f32); coords.push(p0.1 as f32); coords.push(0.0);
                        coords.push(p1.0 as f32); coords.push(p1.1 as f32); coords.push(0.0);
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

    /// Uploads a decoded raster tile to GPU memory and creates its projected quads.
    pub fn upload_raster_tile(&mut self, upload: RasterTileUpload<'_>) {
        if self.loaded_gpu_tiles.contains_key(upload.key) {
            return;
        }

        // 1. Create Texture
        let texture_size = wgpu::Extent3d {
            width: 256,
            height: 256,
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
                bytes_per_row: Some(4 * 256),
                rows_per_image: Some(256),
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
        let vertices = get_tile_vertices(upload.x, upload.y, upload.z, upload.controller);
        let vertex_buffer = upload.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Tile Vertex Buffer {}", upload.key)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let index_buffer = upload.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Tile Index Buffer {}", upload.key)),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        self.loaded_gpu_tiles.insert(
            upload.key.to_string(),
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
    }

    /// Rebuilds the quad vertex buffers for all uploaded tiles (e.g. when projection changes).
    pub fn rebuild_raster_tile_buffers(&mut self, device: &wgpu::Device, controller: &NativeController) {
        for tile in self.loaded_gpu_tiles.values_mut() {
            let vertices = get_tile_vertices(tile.x, tile.y, tile.z, controller);
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Tile Vertex Buffer {}", tile.key)),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            tile.vertex_buffer = vertex_buffer;
        }
    }

    /// Clears all raster textures from the GPU memory.
    pub fn clear_raster_tiles(&mut self) {
        self.loaded_gpu_tiles.clear();
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
                render_pass.set_index_buffer(tile.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..1);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Helper mathematical functions for OSM / WMTS tile coordinates
// -----------------------------------------------------------------------------

fn tile_bounds_rad(x: u32, y: u32, z: u32) -> (f64, f64, f64, f64) {
    let n = 2.0f64.powi(z as i32);
    let lon_west = (x as f64 / n) * 360.0 - 180.0;
    let lon_east = ((x + 1) as f64 / n) * 360.0 - 180.0;
    
    let lat_north_rad = (std::f64::consts::PI * (1.0 - 2.0 * (y as f64) / n)).sinh().atan();
    let lat_south_rad = (std::f64::consts::PI * (1.0 - 2.0 * ((y + 1) as f64) / n)).sinh().atan();
    
    (
        lat_south_rad,
        lon_west.to_radians(),
        lat_north_rad,
        lon_east.to_radians(),
    )
}

fn get_tile_vertices(x: u32, y: u32, z: u32, controller: &NativeController) -> [RasterVertex; 4] {
    let (lat_south, lon_west, lat_north, lon_east) = tile_bounds_rad(x, y, z);
    
    let get_pos = |lat: f64, lon: f64| {
        if controller.view_mode == "3D" {
            let ecef = olayer_core::geodesy::lla_to_ecef(
                &LatLon::new(lat, lon, 0.0),
                &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
            );
            [ecef.x as f32, ecef.y as f32, ecef.z as f32]
        } else {
            let proj = controller.projection.project(&LatLon::new(lat, lon, 0.0)).unwrap_or((0.0, 0.0));
            [proj.0 as f32, proj.1 as f32, 0.0]
        }
    };

    let p_tl = get_pos(lat_north, lon_west);
    let p_bl = get_pos(lat_south, lon_west);
    let p_br = get_pos(lat_south, lon_east);
    let p_tr = get_pos(lat_north, lon_east);

    [
        RasterVertex { position: p_tl, tex_coords: [0.0, 0.0] },
        RasterVertex { position: p_bl, tex_coords: [0.0, 1.0] },
        RasterVertex { position: p_br, tex_coords: [1.0, 1.0] },
        RasterVertex { position: p_tr, tex_coords: [1.0, 0.0] },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_controller::NativeController;

    #[test]
    fn test_generate_grid_vertices_2d_not_empty() {
        let controller = NativeController::new(0.0, 0.0);
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        assert!(!vertices.is_empty(), "2D grid should produce vertices");
        // Each vertex is 3 floats (x, y, z); each line segment is 2 vertices = 6 floats
        assert_eq!(vertices.len() % 6, 0, "Vertex count must be a multiple of 6 (2 endpoints × 3 coords)");
    }

    #[test]
    fn test_generate_grid_vertices_3d_not_empty() {
        let mut controller = NativeController::new(0.0, 0.0);
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        assert!(!vertices.is_empty(), "3D grid should produce vertices");
        assert_eq!(vertices.len() % 6, 0, "Vertex count must be a multiple of 6 (2 endpoints × 3 coords)");
    }

    #[test]
    fn test_generate_grid_vertices_3d_uses_ecef_scale() {
        let mut controller = NativeController::new(0.0, 0.0);
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // ECEF coordinates for Earth surface should be on the order of millions of meters.
        // Check that at least one vertex has a magnitude > 6_000_000 (roughly Earth radius).
        let max_abs = vertices.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        assert!(max_abs > 6_000_000.0, "3D grid vertices should be in ECEF scale, max abs = {max_abs}");
    }

    #[test]
    fn test_generate_grid_vertices_2d_z_is_zero() {
        let controller = NativeController::new(0.0, 0.0);
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // 2D grid vertices always have z = 0.0 (flat plane)
        for chunk in vertices.chunks_exact(3) {
            assert_eq!(chunk[2], 0.0, "2D grid vertices must have z = 0.0");
        }
    }

    #[test]
    fn test_generate_grid_vertices_3d_z_is_nonzero() {
        let mut controller = NativeController::new(0.0, 0.0);
        controller.view_mode = "3D".to_string();
        let vertices = WgpuGpuPipeline::generate_grid_vertices(&controller);
        // 3D grid uses ECEF so at least some z components should be non-zero
        let has_nonzero_z = vertices.chunks_exact(3).any(|c| c[2] != 0.0);
        assert!(has_nonzero_z, "3D grid should have non-zero z components (ECEF)");
    }
}
