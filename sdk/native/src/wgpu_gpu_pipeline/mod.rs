use olayer_core::geodesy::LatLon;
use crate::native_controller::NativeController;

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

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            grid_vertex_buffer: None,
            grid_vertices_len: 0,
        }
    }

    /// Rebuilds the grid vertex buffers based on the controller view mode and active projection.
    pub fn rebuild_grid_buffers(
        &mut self,
        controller: &NativeController,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
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
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let Some(ref buffer) = self.grid_vertex_buffer {
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..(self.grid_vertices_len / 3) as u32, 0..1);
        }
    }
}
