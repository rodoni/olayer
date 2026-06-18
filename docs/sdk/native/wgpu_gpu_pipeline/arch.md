# Architecture: wgpu GPU Pipeline

This document details the architectural design and technical specification of the **wgpu GPU Pipeline** component of the Olayer Native SDK.

---

## 1. Overview

The **wgpu GPU Pipeline** is the hardware-accelerated graphics rendering engine of the Native SDK, implemented through the multiplatform `wgpu` library in Rust. It is designed to draw large-scale geographic elements with high frame rates and low bus latency (CPU/GPU), leveraging native APIs such as Vulkan, Metal, and DirectX 12.

```mermaid
graph LR
    Core[Rust Core Matrices] -->|Uniform Buffer| GPU[GPU Pipeline]
    Grid[Grid Vertices] -->|Vertex Buffer| GPU
    GPU -->|Render Pass| Surface[Screen Surface]
```

---

## 2. Configuration and Initialization

1. **WGPU Instance:** Created with the default descriptor.
2. **Surface:** Bound to the native `winit` window.
3. **Adapter & Device:** Requested with preference for high performance (`HighPerformance`).
4. **Surface Configuration:** Defines width/height and color format based on the physical window dimensions.
The WGPU initialization and surface creation process occurs natively and synchronously at the entry point [main.rs](../../../../sdk/native/demo/src/main.rs):

---

## 3. Drawing Pipeline and WGSL Shader

### 3.1 WGSL Shader
The base grid shader (latitude and longitude lines) is written in the WGSL shading language and compiled at runtime:
```rust
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1)
var<uniform> grid_color: vec4<f32>;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = view_proj * vec4<f32>(pos, 1.0);
    out.color = grid_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
```

### 3.2 Buffer Allocation and Rendering Pipeline
* **Uniform Buffer:** Allocates 80 bytes (64 bytes for the `view_proj` projection matrix and 16 bytes for the `grid_color` color vector).
* **Vertex Buffer:** Managed in `rebuild_grid_buffers`. Rebuilds grid line points and sends them to the GPU when the active projection is changed.
* **Rendering Pipeline:** Configured with `LineList` topology for fast line drawing, enabled color blending (`ALPHA_BLENDING`), and writing to all color channels.

---

## 4. Integration in the Frame Rendering Loop

During repainting (Redraw), the `CommandEncoder` and corresponding `RenderPass` draw the grid on the GPU applying the appropriate buffers:

```rust
render_pass.set_pipeline(&pipeline);
render_pass.set_bind_group(0, &bind_group, &[]);
render_pass.set_vertex_buffer(0, buffer.slice(..));
render_pass.draw(0..(grid_vertices.len() / 3) as u32, 0..1);
```

---

## 5. Unit Testing

The `WgpuGpuPipeline` module includes a suite of unit tests verifying correct generation of the geodetic grid vertices under different view modes:
* **2D Grid Verification:** Assures that vertices are populated and follow the topology rules (multiple of 6 coordinates per segment: 2 endpoints × 3 coords), and verifies that all Z components are exactly `0.0`.
* **3D Grid Verification:** Validates that 3D globe coordinates are generated, using ECEF (Earth-Centered, Earth-Fixed) scale magnitudes where at least some points exceed the Earth's radius ($\approx 6.0 \times 10^6\text{ m}$).
* **Coordinate Stability:** Ensures that 3D grid vertices contain non-zero Z values when the camera/controller is oriented in 3D mode.

