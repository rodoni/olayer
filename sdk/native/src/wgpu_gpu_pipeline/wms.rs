//! Georeferenced WMS images using the existing native raster pipeline.
use super::{RasterVertex, WgpuGpuPipeline};
use crate::native_controller::NativeController;
use olayer_core::geodesy::{ellipsoid::Ellipsoid, lla_to_ecef, LatLon};
use olayer_map_core::{BoundingBox, MapImage};
use std::fmt;
use wgpu::util::DeviceExt;

const A: f64 = 6_378_137.0;
const B: f64 = 6_356_752.314_245_179;

#[derive(Debug, Clone, Copy)]
pub struct WmsMeshOptions {
    /// Grid cells per axis, 1..=64. Defaults to 16.
    pub subdivisions: u32,
    /// Sample controller terrain in 2.5D/3D; unknown samples leave holes.
    pub drape_terrain: bool,
}
impl Default for WmsMeshOptions {
    fn default() -> Self {
        Self {
            subdivisions: 16,
            drape_terrain: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmsRenderError(pub String);
impl fmt::Display for WmsRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for WmsRenderError {}

fn invalid(message: &str) -> WmsRenderError {
    WmsRenderError(message.to_owned())
}

fn validate_bounds(bounds: BoundingBox) -> Result<(), WmsRenderError> {
    let (x_limit, y_limit) = match bounds.crs.epsg_code() {
        4326 => (180.0, 90.0),
        3857 | 900913 => (std::f64::consts::PI * A, std::f64::consts::PI * A),
        _ => return Err(invalid("unsupported WMS image CRS")),
    };
    if ![bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .all(|v| v.is_finite())
        || bounds.min_x >= bounds.max_x
        || bounds.min_y >= bounds.max_y
        || bounds.min_x < -x_limit
        || bounds.max_x > x_limit
        || bounds.min_y < -y_limit
        || bounds.max_y > y_limit
    {
        return Err(invalid(
            "WMS image bounds are outside the supported CRS domain",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ClipVertex {
    attributes: [f64; 5],
    visibility: f64,
}

fn clipped_triangle(triangle: [ClipVertex; 3], output: &mut Vec<RasterVertex>) {
    let mut polygon = Vec::with_capacity(4);
    for i in 0..3 {
        let a = triangle[i];
        let b = triangle[(i + 1) % 3];
        if a.visibility >= 0.0 {
            polygon.push(a);
        }
        if (a.visibility >= 0.0) != (b.visibility >= 0.0) {
            let t = a.visibility / (a.visibility - b.visibility);
            polygon.push(ClipVertex {
                attributes: std::array::from_fn(|axis| {
                    a.attributes[axis] + t * (b.attributes[axis] - a.attributes[axis])
                }),
                visibility: 0.0,
            });
        }
    }
    for i in 1..polygon.len().saturating_sub(1) {
        for vertex in [polygon[0], polygon[i], polygon[i + 1]] {
            output.push(RasterVertex {
                position: std::array::from_fn(|axis| vertex.attributes[axis] as f32),
                tex_coords: [vertex.attributes[3] as f32, vertex.attributes[4] as f32],
            });
        }
    }
}

/// Builds north-up textured triangles in projected metres or ECEF metres.
/// Bounds remain x/y (longitude/latitude for EPSG:4326), independently of WMS wire axis order.
/// Rebuild after projection, camera, terrain, or display-mode changes.
pub fn build_wms_mesh(
    bounds: BoundingBox,
    controller: &NativeController,
    options: WmsMeshOptions,
) -> Result<Vec<RasterVertex>, WmsRenderError> {
    build_mesh(bounds, controller, options, |lat, lon| {
        controller
            .terrain
            .get_elevation_status(lat, lon)
            .ok()?
            .elevation_meters
    })
}

fn build_mesh(
    bounds: BoundingBox,
    controller: &NativeController,
    options: WmsMeshOptions,
    elevation: impl Fn(f64, f64) -> Option<f64>,
) -> Result<Vec<RasterVertex>, WmsRenderError> {
    validate_bounds(bounds)?;
    if !(1..=64).contains(&options.subdivisions) {
        return Err(invalid("WMS subdivisions must be 1..=64"));
    }
    if !matches!(controller.view_mode.as_str(), "2D" | "2.5D" | "3D") {
        return Err(invalid("unsupported WMS display dimension"));
    }
    let camera = &controller.camera;
    if ![
        camera.center.lat,
        camera.center.lon,
        camera.zoom,
        camera.rotation,
        camera.pitch,
        camera.roll,
    ]
    .iter()
    .all(|v| v.is_finite())
        || camera.zoom <= 0.0
    {
        return Err(invalid("WMS camera must be finite with positive zoom"));
    }
    let globe = controller.view_mode == "3D";
    let ellipsoid = Ellipsoid::wgs84();
    let mut eye = [0.0, 0.0, A + 15_000_000.0 / camera.zoom];
    for (axis, angle) in [
        (1, -camera.roll),
        (0, -camera.pitch),
        (2, camera.rotation),
        (0, std::f64::consts::FRAC_PI_2 - camera.center.lat),
        (2, camera.center.lon + std::f64::consts::FRAC_PI_2),
    ] {
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        let (x, y) = (eye[a], eye[b]);
        eye[a] = angle.cos() * x - angle.sin() * y;
        eye[b] = angle.sin() * x + angle.cos() * y;
    }
    let n = options.subdivisions as usize;
    let mut vertices = Vec::with_capacity((n + 1) * (n + 1));
    for row in 0..=n {
        let v = row as f64 / n as f64;
        let y = bounds.max_y - v * (bounds.max_y - bounds.min_y);
        for column in 0..=n {
            let u = column as f64 / n as f64;
            let x = bounds.min_x + u * (bounds.max_x - bounds.min_x);
            let (lat, lon) = if bounds.crs.epsg_code() == 4326 {
                (y.to_radians(), x.to_radians())
            } else {
                ((y / A).sinh().atan(), x / A)
            };
            let height = if controller.view_mode == "2D" || !options.drape_terrain {
                Some(0.0)
            } else {
                elevation(lat, lon)
            };
            let Some(height) = height else {
                vertices.push(None);
                continue;
            };
            let position = if globe {
                let ecef = lla_to_ecef(&LatLon::new(lat, lon, height), &ellipsoid);
                [ecef.x, ecef.y, ecef.z]
            } else {
                let (x, y) = controller
                    .projection
                    .project(&LatLon::new(lat, lon, 0.0))
                    .map_err(|_| invalid("WMS mesh projection failed"))?;
                [x, y, height]
            };
            if !position
                .iter()
                .all(|v| v.is_finite() && (*v as f32).is_finite())
                || !height.is_finite()
            {
                return Err(invalid("WMS mesh projection or elevation is not finite"));
            }
            vertices.push(Some(ClipVertex {
                attributes: [position[0], position[1], position[2], u, v],
                visibility: if globe {
                    (position[0] * eye[0] + position[1] * eye[1]) / (A * A)
                        + position[2] * eye[2] / (B * B)
                        - 1.0
                } else {
                    1.0
                },
            }));
        }
    }
    let mut output = Vec::with_capacity(n * n * 6);
    for row in 0..n {
        for column in 0..n {
            let a = row * (n + 1) + column;
            let b = a + n + 1;
            for [a, b, c] in [[a, b, a + 1], [a + 1, b, b + 1]] {
                if let (Some(a), Some(b), Some(c)) = (vertices[a], vertices[b], vertices[c]) {
                    clipped_triangle([a, b, c], &mut output);
                }
            }
        }
    }
    Ok(output)
}

/// GPU-owned image. Drop to release resources; fetch it once and rebuild geometry as the view changes.
pub struct WgpuWmsImage {
    bounds: BoundingBox,
    options: WmsMeshOptions,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}
impl WgpuWmsImage {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &WgpuGpuPipeline,
        image: &MapImage,
        controller: &NativeController,
        options: WmsMeshOptions,
    ) -> Result<Self, WmsRenderError> {
        if image.crs != image.bounds.crs {
            return Err(invalid("WMS image CRS differs from its bounds"));
        }
        let width = image.width.get();
        let height = image.height.get();
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| invalid("WMS image dimensions overflow RGBA byte length"))?;
        if image.pixels.len() as u64 != expected {
            return Err(invalid("WMS image is not packed RGBA8"));
        }
        if width > device.limits().max_texture_dimension_2d
            || height > device.limits().max_texture_dimension_2d
        {
            return Err(invalid("WMS image exceeds GPU texture limits"));
        }
        let vertices = build_wms_mesh(image.bounds, controller, options)?;
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("WMS image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("WMS image bind group"),
            layout: &pipeline.raster_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pipeline.raster_sampler),
                },
            ],
        });
        let vertex_buffer = Self::buffer(device, &vertices);
        Ok(Self {
            bounds: image.bounds,
            options,
            bind_group,
            vertex_buffer,
            vertex_count: vertices.len() as u32,
        })
    }

    fn buffer(device: &wgpu::Device, vertices: &[RasterVertex]) -> wgpu::Buffer {
        // wgpu requires a nonzero buffer even when unknown terrain hides the entire image.
        let empty = [RasterVertex {
            position: [0.0; 3],
            tex_coords: [0.0; 2],
        }];
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("WMS mesh"),
            contents: bytemuck::cast_slice(if vertices.is_empty() {
                &empty
            } else {
                vertices
            }),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        controller: &NativeController,
    ) -> Result<(), WmsRenderError> {
        let vertices = build_wms_mesh(self.bounds, controller, self.options)?;
        self.vertex_buffer = Self::buffer(device, &vertices);
        self.vertex_count = vertices.len() as u32;
        Ok(())
    }

    /// Draw inside the same color render pass used by `render_raster_tiles`.
    pub fn render<'a>(&'a self, pipeline: &'a WgpuGpuPipeline, pass: &mut wgpu::RenderPass<'a>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&pipeline.raster_pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_provider::{
        ogc::{NativeOgcClient, OgcTransport},
        sources::WmsMapSource,
        NativeMapDataError,
    };
    use olayer_core::terrain::RgbElevationEncoding;
    use olayer_map_core::{CoordinateReferenceSystem, UrlParts};
    use std::{
        num::{NonZeroU32, NonZeroUsize},
        sync::{Arc, Mutex},
    };

    fn bounds() -> BoundingBox {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../tests/fixtures/wms/scene.json"))
                .unwrap();
        let values = fixture["bounds"].as_array().unwrap();
        BoundingBox::new(
            values[0].as_f64().unwrap(),
            values[1].as_f64().unwrap(),
            values[2].as_f64().unwrap(),
            values[3].as_f64().unwrap(),
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap()
    }

    fn controller() -> NativeController {
        let controller = NativeController::new(0.0, 0.0).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../tests/fixtures/wms/scene.json"))
                .unwrap();
        let rgba: Vec<u8> = fixture["rgbaTerrain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        controller
            .terrain
            .load_rgb_tile(
                0,
                0,
                0,
                2,
                2,
                &rgba.repeat(4),
                RgbElevationEncoding::MapboxRgb,
            )
            .unwrap();
        controller
    }

    struct FixtureTransport(Arc<Mutex<Vec<String>>>);
    impl OgcTransport for FixtureTransport {
        fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            self.0.lock().unwrap().push(url.to_owned());
            Ok(include_bytes!("../../../../tests/fixtures/wms/quadrants.png").to_vec())
        }
    }
    fn source(urls: Arc<Mutex<Vec<String>>>, width: u32) -> WmsMapSource<FixtureTransport> {
        WmsMapSource::new(
            NativeOgcClient::new(FixtureTransport(urls), NonZeroUsize::new(4096).unwrap()),
            UrlParts::new("https://fixture.test", "wms"),
            "quadrants",
            bounds().crs,
            bounds(),
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(2).unwrap(),
        )
    }

    #[test]
    fn wms_fixture_flows_from_transport_to_all_three_display_dimensions() {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let image = source(urls.clone(), 2).fetch().unwrap();
        assert_eq!(&image.pixels[0..4], &[255, 0, 0, 255]);
        let mut controller = controller();
        controller.camera.pitch = 0.0;
        let options = WmsMeshOptions {
            subdivisions: 2,
            drape_terrain: true,
        };
        let planar = build_wms_mesh(image.bounds, &controller, options).unwrap();
        assert_eq!(planar.len(), 24);
        assert_eq!(planar[0].tex_coords, [0.0, 0.0]);
        assert_eq!(planar[1].tex_coords, [0.0, 0.5]);
        assert!(planar[0].position[0] < 0.0 && planar[0].position[1] > 0.0);
        assert_eq!(planar[0].position[2], 0.0);
        controller.view_mode = "2.5D".into();
        let terrain = build_wms_mesh(image.bounds, &controller, options).unwrap();
        assert!((terrain[0].position[2] - 500.0).abs() < 0.1);
        controller.view_mode = "3D".into();
        let globe = build_wms_mesh(image.bounds, &controller, options).unwrap();
        let expected = lla_to_ecef(
            &LatLon::new(1.0f64.to_radians(), -2.0f64.to_radians(), 500.0),
            &Ellipsoid::wgs84(),
        );
        for (actual, expected) in globe[0]
            .position
            .iter()
            .zip([expected.x, expected.y, expected.z])
        {
            assert!((f64::from(*actual) - expected).abs() < 0.5);
        }
        let requested = urls.lock().unwrap();
        assert_eq!(requested.len(), 1);
        assert!(requested[0].contains("BBOX=-1%2C-2%2C1%2C2"));
    }

    #[test]
    fn unknown_ground_leaves_holes_and_never_becomes_sea_level() {
        let mut controller = NativeController::new(0.0, 0.0).unwrap();
        controller.view_mode = "2.5D".into();
        let options = WmsMeshOptions {
            drape_terrain: true,
            ..Default::default()
        };
        assert!(build_wms_mesh(bounds(), &controller, options)
            .unwrap()
            .is_empty());
        assert!(build_mesh(bounds(), &controller, options, |_, _| Some(f64::NAN)).is_err());
    }

    #[test]
    fn globe_mesh_clips_horizon_and_accounts_for_camera_attitude() {
        let mut controller = controller();
        controller.view_mode = "3D".into();
        controller.camera.pitch = 0.0;
        let far = BoundingBox::new(170.0, -1.0, 175.0, 1.0, bounds().crs).unwrap();
        assert!(build_wms_mesh(far, &controller, WmsMeshOptions::default())
            .unwrap()
            .is_empty());
        let edge = BoundingBox::new(65.0, -1.0, 85.0, 1.0, bounds().crs).unwrap();
        let visible = build_wms_mesh(edge, &controller, WmsMeshOptions::default()).unwrap();
        assert!(!visible.is_empty());
        let tangent_x = A * A / (A + 15_000_000.0);
        assert!(visible
            .iter()
            .all(|v| f64::from(v.position[0]) >= tangent_x - 1.0));
        controller.camera.pitch = std::f64::consts::PI;
        assert!(
            build_wms_mesh(bounds(), &controller, WmsMeshOptions::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_misplaced_or_mismatched_images() {
        let controller = controller();
        assert!(build_wms_mesh(
            bounds(),
            &controller,
            WmsMeshOptions {
                subdivisions: 65,
                ..Default::default()
            }
        )
        .is_err());
        let mut invalid = bounds();
        invalid.min_x = -181.0;
        assert!(build_wms_mesh(invalid, &controller, WmsMeshOptions::default()).is_err());
        invalid = bounds();
        invalid.crs = CoordinateReferenceSystem::epsg(4978).unwrap();
        assert!(build_wms_mesh(invalid, &controller, WmsMeshOptions::default()).is_err());
        assert!(source(Arc::new(Mutex::new(Vec::new())), 3).fetch().is_err());
    }

    #[test]
    #[ignore = "requires a headless WGPU adapter; run explicitly with --ignored"]
    fn wms_image_renders_into_offscreen_pixels_in_all_modes() {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .expect("headless WGPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .unwrap();
            let pipeline = WgpuGpuPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let urls = Arc::new(Mutex::new(Vec::new()));
            let image = source(urls.clone(), 2).fetch().unwrap();
            let mut controller = controller();
            controller.camera.pitch = 0.0;
            controller.camera.viewport_base_meters = 600_000.0;
            let mut gpu_image = WgpuWmsImage::new(
                &device,
                &queue,
                &pipeline,
                &image,
                &controller,
                WmsMeshOptions {
                    drape_terrain: true,
                    ..Default::default()
                },
            )
            .unwrap();
            for mode in ["2D", "2.5D", "3D"] {
                controller.view_mode = mode.into();
                controller.camera.zoom = if mode == "3D" { 20.0 } else { 1.0 };
                controller.camera.pitch = if mode == "2.5D" { 0.4 } else { 0.0 };
                gpu_image.rebuild(&device, &controller).unwrap();
                let matrix = match mode {
                    "3D" => controller.camera.get_3d_view_proj_matrix().unwrap(),
                    "2.5D" => controller
                        .camera
                        .get_25d_view_proj_matrix(controller.projection.as_ref())
                        .unwrap(),
                    _ => controller
                        .camera
                        .get_2d_view_proj_matrix(controller.projection.as_ref())
                        .unwrap(),
                };
                queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::cast_slice(&matrix));
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("WMS offscreen test"),
                    size: wgpu::Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 64 * 256,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                let view = target.create_view(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    gpu_image.render(&pipeline, &mut pass);
                }
                encoder.copy_texture_to_buffer(
                    target.as_image_copy(),
                    wgpu::ImageCopyBuffer {
                        buffer: &readback,
                        layout: wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(64),
                        },
                    },
                    wgpu::Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit(Some(encoder.finish()));
                let (sender, receiver) = std::sync::mpsc::channel();
                readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        sender.send(result).unwrap()
                    });
                device.poll(wgpu::Maintain::Wait);
                receiver.recv().unwrap().unwrap();
                let pixels = readback.slice(..).get_mapped_range();
                for (corner, [r, g, b]) in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]]
                    .into_iter()
                    .enumerate()
                {
                    assert!(
                        pixels.chunks(4).any(|p| (i32::from(p[0]) - r).abs() < 35
                            && (i32::from(p[1]) - g).abs() < 35
                            && (i32::from(p[2]) - b).abs() < 35),
                        "{mode}: missing corner color {corner}"
                    );
                }
            }
            assert_eq!(
                urls.lock().unwrap().len(),
                1,
                "switching dimensions must not refetch the image"
            );
        });
    }
}
