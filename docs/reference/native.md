# Native SDK API

## Re-exports from `lib.rs`

```rust
pub use native_controller::NativeController;
pub use native_layer_manager::{Layer, NativeLayerManager};
pub use map_provider::{MapDataSource, NativeMapDataStack, TerrainDataSource, GeoserverWmtsSource};
pub use wgpu_gpu_pipeline::{RasterTileUpload, WgpuGpuPipeline};
// RasterVertex/WgpuRasterTile are available through the wgpu_gpu_pipeline module.
pub use wgpu_cpu_vertex_pipeline::{WgpuCpuVertexPipeline, project_lla_to_screen, rasterize_svg};
```

## `NativeController`

```rust
pub struct NativeController {
    pub terrain: TerrainEngine,
    pub interpolator: InterpolationEngine,
    pub projection: Box<dyn Projection + Send + Sync>,
    pub camera: CameraState,
    pub view_mode: String,  // "2D" | "2.5D" | "3D"
}

impl NativeController {
    pub fn new(center_lat: f64, center_lon: f64) -> Result<Self, NativeControllerError>
    pub fn create_geoserver_source(&self, id: &str, base_url: &str, layer_name: &str) -> GeoserverWmtsSource
    pub fn trigger_active(&mut self)
    pub fn check_active(&mut self) -> bool
pub fn get_target_fps(&mut self) -> u32   // 60 (active) or 15 (idle)
pub fn resolve_altitude(&self, lat_rad: f64, lon_rad: f64, input_height: f64, mode: AltitudeMode, policy: AltitudeUnknownPolicy, mesh_height: Option<f64>) -> Result<f64, TerrainError>
}
```

Public fields: `terrain: TerrainEngine`, `interpolator: InterpolationEngine`, `projection: Box<dyn Projection + Send + Sync>`, `camera: CameraState`, `view_mode: String`.

## `Layer` trait & `NativeLayerManager`

```rust
pub trait Layer {
    fn id(&self) -> &str;
    fn is_visible(&self) -> bool;
    fn set_visible(&mut self, visible: bool);
    fn is_static(&self) -> bool;           // true = redraw only on camera change
}

pub struct NativeLayerManager {
    pub show_grid: bool,
    pub show_targets: bool,
    pub show_hud: bool,
    pub show_terrain: bool,
}

impl NativeLayerManager {
    pub fn new() -> Self                                 // + Default
    pub fn add_layer(&mut self, layer: Box<dyn Layer>) -> Result<(), String>
    pub fn remove_layer(&mut self, id: &str) -> bool
    pub fn reorder_layer(&mut self, id: &str, new_index: usize) -> Result<(), String>
    pub fn get_layers(&self) -> &[Box<dyn Layer>]
    pub fn visible_static_layers(&self) -> Vec<&dyn Layer>
    pub fn visible_dynamic_layers(&self) -> Vec<&dyn Layer>
    pub fn set_layer_visibility(&mut self, id: &str, visible: bool) -> Result<(), String>
    pub fn set_all_visibility(&mut self, visible: bool)
}
```

The manager stores layers in back-to-front render order. `show_*` toggles are hard-wired convenience flags used by the native demo and do not affect custom layers. "Static" layers are regenerated only on camera/projection changes; "dynamic" layers are redrawn every frame.

## `MapDataSource` trait & `NativeMapDataStack`

```rust
pub trait MapDataSource {
    fn id(&self) -> &str;
    fn clear_cache(&mut self);
    fn cache_size(&self) -> usize;
}

impl NativeMapDataStack {
    pub fn new() -> Self
    pub fn register_source(&mut self, source: Box<dyn MapDataSource>) -> Result<(), String>
    pub fn get_source(&self, id: &str) -> Option<&dyn MapDataSource>
    pub fn clear_cache(&mut self)
    pub fn get_cache_size(&self) -> usize
    pub fn load_dted_file(&self, path: &str, terrain: &mut TerrainEngine) -> Result<(), String>
    pub fn load_dted_buffer(&self, buffer: &[u8], terrain: &mut TerrainEngine) -> Result<(), String>
}

impl TerrainDataSource {
    pub fn new(id: &str) -> Self
    pub fn load_file(&mut self, path: &str) -> Result<(), String>
    pub fn load_buffer(&mut self, buffer: &[u8]) -> Result<(), String>
    pub fn unload_tile(&mut self, lat_deg: i32, lon_deg: i32) -> bool
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, String>
    pub fn clear_cache(&mut self)        // unloads all tracked tiles
    pub fn cache_size(&self) -> usize    // number of tracked tiles
}
// impl MapDataSource for TerrainDataSource

impl GeoserverWmtsSource {                                      // + Clone
    pub fn new(id: &str, base_url: &str, layer_name: &str) -> Self  // spawns background worker thread
    pub fn load_tile(&self, x: u32, y: u32, z: u32)                // async, non-blocking
    pub fn get_tile_pixels(&self, x: u32, y: u32, z: u32) -> Option<Vec<u8>>  // RGBA8
    pub fn get_cached_keys(&self) -> Vec<String>                   // e.g. ["z/x/y", ...]
    pub fn base_url(&self) -> &str
    pub fn layer_name(&self) -> &str
    pub fn clear_cache(&mut self)
    pub fn cache_size(&self) -> usize
}
// impl MapDataSource for GeoserverWmtsSource
```

## `WgpuGpuPipeline`

### Raster Tile Structs

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RasterVertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

pub struct WgpuRasterTile {
    pub key: String,
    pub x: u32, pub y: u32, pub z: u32,
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

pub struct RasterTileUpload<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub key: &'a str,
    pub pixels: &'a [u8],       // RGBA8
    pub x: u32, pub y: u32, pub z: u32,
    pub controller: &'a NativeController,
}
```

### `WgpuGpuPipeline` Struct

```rust
pub struct WgpuGpuPipeline {
    // Grid rendering
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub grid_vertex_buffer: Option<wgpu::Buffer>,
    pub grid_vertices_len: usize,
    // Raster rendering
    pub raster_pipeline: wgpu::RenderPipeline,
    pub raster_bind_group_layout: wgpu::BindGroupLayout,
    pub raster_sampler: wgpu::Sampler,
    pub loaded_gpu_tiles: HashMap<String, WgpuRasterTile>,
}
```

All fields are public so advanced users can bind their own uniforms; most callers only need the methods below.

### `impl WgpuGpuPipeline`

```rust
pub fn new(device: &wgpu::Device, config_format: wgpu::TextureFormat) -> Self
pub fn generate_grid_vertices(controller: &NativeController) -> Vec<f32>   // pure CPU, no GPU needed
pub fn rebuild_grid_buffers(&mut self, controller: &NativeController, device: &wgpu::Device, queue: &wgpu::Queue)
pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>)
pub fn upload_raster_tile(&mut self, upload: RasterTileUpload<'_>)         // hard-coded 256x256 RGBA8 texture
pub fn rebuild_raster_tile_buffers(&mut self, device: &wgpu::Device, controller: &NativeController)
pub fn clear_raster_tiles(&mut self)
pub fn render_raster_tiles<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, visible_keys: &HashSet<String>)
```

`render` draws the grid; `render_raster_tiles` draws visible quads. The raster pipeline expects an already-bound uniform bind group at group 0 and a per-tile texture/sampler bind group at group 1. Tile vertex positions are generated in either ECEF (3D) or projected planar metres (2D/2.5D) based on `controller.view_mode`.

## CPU target rendering

The [CPU/vertex implementation](../../sdk/native/src/wgpu_cpu_vertex_pipeline/mod.rs) exposes:

```rust,ignore
pub fn project_lla_to_screen(
    lat: f64, lon: f64, alt: f64, viewport: &ProjectionViewport<'_>,
) -> Option<egui::Pos2>

pub fn rasterize_svg(
    svg_data: &str, width: u32, height: u32,
) -> Result<Vec<u8>, SvgRasterizeError>
```

`ProjectionViewport` groups the view mode, camera, projection, view-projection matrix and screen size. `WgpuCpuVertexPipeline::draw_targets` takes a painter, target slice, selected target ID, viewport and simulated-speed map; it does not take the old separate controller/matrix/width/height argument list.

## OGC and shared-depth additions

The [native exports](../../sdk/native/src/lib.rs) include `NativeOgcClient`, transports, WMS/WFS/WCS sources, `install_wcs_terrain`, WMS/WFS mesh builders and GPU handles. Use the [map guides](../guides/maps/README.md) for their lifecycle and geometry contracts.

`WgpuGpuPipeline::with_depth(device, color_format, depth_format)` opts into a matching depth attachment; `new` retains the compatibility configuration without depth. WFS has line/fill pipelines and `set_fill_color`; see [depth](../guides/maps/depth.md) and [polygon fills](../guides/maps/wfs.md#polygon-fills-and-interior-holes).
