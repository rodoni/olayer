use olayer_core::volumetric::{RibbonMesh, VolumetricMesh};
use wgpu::util::DeviceExt;

/// GPU vertex for volumetric airspace mesh.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVolumetricVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub height_ratio: f32,
    pub is_edge: f32,
}

/// GPU vertex for trajectory flight ribbon mesh.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuRibbonVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub scalar: f32,
}

/// Uploaded GPU mesh buffer handle.
pub struct VolumetricGpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

/// Hardware-accelerated 3D WGPU rendering pipeline for volumetric airspaces and flight trajectory ribbons.
pub struct WgpuVolumetricPipeline {
    pub airspace_pipeline: wgpu::RenderPipeline,
    pub ribbon_pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl WgpuVolumetricPipeline {
    pub fn new(device: &wgpu::Device, config_format: wgpu::TextureFormat) -> Self {
        // 1. Airspace Shader with Fresnel edge glow
        let airspace_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Airspace Shader"),
            source: wgpu::ShaderSource::Wgsl(r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    base_color: vec4<f32>,
    edge_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) height_ratio: f32,
    @location(3) is_edge: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) height_ratio: f32,
    @location(2) is_edge: f32,
};

@vertex
fn vs_airspace(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.height_ratio = in.height_ratio;
    out.is_edge = in.is_edge;
    return out;
}

@fragment
fn fs_airspace(in: VertexOutput) -> @location(0) vec4<f32> {
    // Normal-based fresnel glow and height gradient
    let glow = in.is_edge * 0.4;
    let color = mix(uniforms.base_color, uniforms.edge_color, in.height_ratio * 0.3 + glow);
    return color;
}
"#.into()),
        });

        // 2. Trajectory Ribbon Shader with scalar altitude gradient
        let ribbon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Trajectory Ribbon Shader"),
            source: wgpu::ShaderSource::Wgsl(r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    base_color: vec4<f32>,
    edge_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) scalar: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) scalar: f32,
};

@vertex
fn vs_ribbon(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.scalar = in.scalar;
    return out;
}

@fragment
fn fs_ribbon(in: VertexOutput) -> @location(0) vec4<f32> {
    // Continuous gradient interpolation between base color (low) and edge color (high)
    let ramp_color = mix(uniforms.base_color, uniforms.edge_color, in.scalar);
    // Subtle edge border along lateral boundary (u = 0, u = 1)
    let edge_dist = min(in.uv.x, 1.0 - in.uv.x);
    let edge_boost = smoothstep(0.0, 0.15, edge_dist);
    return vec4<f32>(ramp_color.rgb * (0.7 + 0.3 * edge_boost), ramp_color.a);
}
"#.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volumetric Uniform Buffer"),
            size: 128, // 64 (mat4) + 16 (vec4) + 16 (vec4) + 32 (padding)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Volumetric Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Volumetric Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Airspace render pipeline with alpha blending
        let airspace_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Airspace Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &airspace_shader,
                entry_point: "vs_airspace",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVolumetricVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 24, shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &airspace_shader,
                entry_point: "fs_airspace",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None, // Render both front and back faces for translucent prisms
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Ribbon render pipeline
        let ribbon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ribbon Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ribbon_shader,
                entry_point: "vs_ribbon",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuRibbonVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 32, shader_location: 3 },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &ribbon_shader,
                entry_point: "fs_ribbon",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            airspace_pipeline,
            ribbon_pipeline,
            uniform_buffer,
            bind_group,
        }
    }

    /// Uploads an airspace volumetric mesh to GPU buffers.
    pub fn upload_airspace_mesh(device: &wgpu::Device, mesh: &VolumetricMesh) -> VolumetricGpuMesh {
        let gpu_verts: Vec<GpuVolumetricVertex> = mesh
            .vertices
            .iter()
            .map(|v| GpuVolumetricVertex {
                position: [v.position_ecef[0] as f32, v.position_ecef[1] as f32, v.position_ecef[2] as f32],
                normal: v.normal,
                height_ratio: v.height_ratio,
                is_edge: v.is_edge,
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Airspace Vertex Buffer"),
            contents: bytemuck::cast_slice(&gpu_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Airspace Index Buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        VolumetricGpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        }
    }

    /// Uploads a 3D trajectory ribbon mesh to GPU buffers.
    pub fn upload_ribbon_mesh(device: &wgpu::Device, ribbon: &RibbonMesh) -> VolumetricGpuMesh {
        let gpu_verts: Vec<GpuRibbonVertex> = ribbon
            .vertices
            .iter()
            .map(|v| GpuRibbonVertex {
                position: [v.position_ecef[0] as f32, v.position_ecef[1] as f32, v.position_ecef[2] as f32],
                normal: v.normal,
                uv: v.uv,
                scalar: v.scalar,
            })
            .collect();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ribbon Vertex Buffer"),
            contents: bytemuck::cast_slice(&gpu_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ribbon Index Buffer"),
            contents: bytemuck::cast_slice(&ribbon.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        VolumetricGpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: ribbon.indices.len() as u32,
        }
    }

    /// Updates uniform buffer with view projection matrix and colors.
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_proj: &[f32; 16],
        base_color: [f32; 4],
        edge_color: [f32; 4],
    ) {
        let mut uniform_bytes = Vec::with_capacity(128);
        uniform_bytes.extend_from_slice(bytemuck::cast_slice(view_proj));
        uniform_bytes.extend_from_slice(bytemuck::cast_slice(&base_color));
        uniform_bytes.extend_from_slice(bytemuck::cast_slice(&edge_color));
        queue.write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
    }
}
