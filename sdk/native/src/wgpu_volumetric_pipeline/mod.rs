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
        Self::with_depth(device, config_format, None)
    }

    /// Creates translucent volumetric pipelines that test against a host depth attachment
    /// without writing depth. Callers using this variant must bind the configured attachment.
    pub fn with_depth(
        device: &wgpu::Device,
        config_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let depth_state = || {
            depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        };
        let clip_depth_remap = if depth_format.is_some() {
            "out.clip_position.z = (out.clip_position.z + out.clip_position.w) * 0.5;"
        } else {
            ""
        };
        // 1. Airspace Shader with Fresnel edge glow
        let airspace_source = r#"
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
    /*CLIP_DEPTH_REMAP*/
    out.normal = in.normal;
    out.height_ratio = in.height_ratio;
    out.is_edge = in.is_edge;
    return out;
}

@fragment
fn fs_airspace(in: VertexOutput) -> @location(0) vec4<f32> {
    let glow = in.is_edge * 0.4;
    return mix(uniforms.base_color, uniforms.edge_color, in.height_ratio * 0.3 + glow);
}
"#
        .replace("/*CLIP_DEPTH_REMAP*/", clip_depth_remap);
        let airspace_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Airspace Shader"),
            source: wgpu::ShaderSource::Wgsl(airspace_source.into()),
        });

        // 2. Trajectory Ribbon Shader with scalar altitude gradient
        let ribbon_source = r#"
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
    /*CLIP_DEPTH_REMAP*/
    out.uv = in.uv;
    out.scalar = in.scalar;
    return out;
}

@fragment
fn fs_ribbon(in: VertexOutput) -> @location(0) vec4<f32> {
    let ramp_color = mix(uniforms.base_color, uniforms.edge_color, in.scalar);
    let edge_dist = min(in.uv.x, 1.0 - in.uv.x);
    let edge_boost = smoothstep(0.0, 0.15, edge_dist);
    return vec4<f32>(ramp_color.rgb * (0.7 + 0.3 * edge_boost), ramp_color.a);
}
"#
        .replace("/*CLIP_DEPTH_REMAP*/", clip_depth_remap);
        let ribbon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Trajectory Ribbon Shader"),
            source: wgpu::ShaderSource::Wgsl(ribbon_source.into()),
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
            depth_stencil: depth_state(),
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
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 32,
                            shader_location: 3,
                        },
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
            depth_stencil: depth_state(),
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
                position: [
                    v.position_ecef[0] as f32,
                    v.position_ecef[1] as f32,
                    v.position_ecef[2] as f32,
                ],
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
                position: [
                    v.position_ecef[0] as f32,
                    v.position_ecef[1] as f32,
                    v.position_ecef[2] as f32,
                ],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a headless WGPU adapter; run explicitly with --ignored"]
    fn translucent_volumetric_mesh_tests_depth_without_writing_it() {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("headless adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .unwrap();
            let pipeline = WgpuVolumetricPipeline::with_depth(
                &device,
                wgpu::TextureFormat::Rgba8Unorm,
                Some(wgpu::TextureFormat::Depth32Float),
            );
            let mut uniforms = [0.0_f32; 32];
            for index in [0, 5, 10, 15] {
                uniforms[index] = 1.0;
            }
            uniforms[16..20].copy_from_slice(&[1.0, 0.0, 0.0, 0.5]);
            uniforms[20..24].copy_from_slice(&[1.0, 0.0, 0.0, 0.5]);
            queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::cast_slice(&uniforms));
            let vertex = |x, y, z| GpuVolumetricVertex {
                position: [x, y, z],
                normal: [0.0, 0.0, 1.0],
                height_ratio: 0.0,
                is_edge: 0.0,
            };
            let vertices = [
                vertex(-1.0, -1.0, -0.2),
                vertex(3.0, -1.0, -0.2),
                vertex(-1.0, 3.0, -0.2),
                vertex(-1.0, -1.0, 0.0),
                vertex(3.0, -1.0, 0.0),
                vertex(-1.0, 3.0, 0.0),
            ];
            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("volumetric depth fixture"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let size = wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            };
            let color = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("volumetric color"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let depth = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("pre-existing opaque depth"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let color_view = color.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("translucent volumes over opaque depth"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.6),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&pipeline.airspace_pipeline);
                pass.set_bind_group(0, &pipeline.bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("volumetric readback"),
                size: 256 * 4,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                color.as_image_copy(),
                wgpu::ImageCopyBuffer {
                    buffer: &readback,
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(4),
                    },
                },
                size,
            );
            queue.submit(Some(encoder.finish()));
            let (send, receive) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = send.send(result);
                });
            device.poll(wgpu::Maintain::Wait);
            receive.recv().unwrap().unwrap();
            let bytes = readback.slice(..).get_mapped_range();
            let center_red = bytes[256 + 4];
            assert!(
                center_red > 170,
                "both translucent layers should blend without depth writes; got {center_red}"
            );
        });
    }
}
