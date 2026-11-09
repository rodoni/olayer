use super::*;
use crate::{
    decode_wcs_json_coverage, decode_wfs_feature_collection, install_wcs_terrain, WfsMeshOptions,
    WgpuWfsFeatures, WgpuWfsPipeline, WgpuWmsImage, WmsMeshOptions,
};
use olayer_map_core::{
    BoundingBox, CoordinateReferenceSystem, CoverageSampleType, ImageFormat, MapImage,
};
use std::num::NonZeroU32;

fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    raster: &WgpuGpuPipeline,
    features: &WgpuWfsFeatures,
    lines: &WgpuWfsPipeline,
    image: Option<&WgpuWmsImage>,
    order: (bool, bool),
) -> Vec<u8> {
    let (terrain, reverse) = order;
    let size = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shared depth test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if reverse {
            features.render(lines, &mut pass);
        }
        if terrain {
            raster.render_terrain(&mut pass);
        }
        if let Some(image) = image {
            image.render(raster, &mut pass);
        }
        if !reverse {
            features.render(lines, &mut pass);
        }
    }
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().to_vec();
    readback.unmap();
    bytes
}
fn red_near(pixels: &[u8], matrix: &[f32; 16], mode: &str, height: f64) -> bool {
    let p = if mode == "3D" {
        [6_378_137.0 + height, 0.0, 0.0]
    } else {
        [0.0, 0.0, if mode == "2D" { 0.0 } else { height }]
    };
    let clip: [f64; 4] = std::array::from_fn(|row| {
        f64::from(matrix[row]) * p[0]
            + f64::from(matrix[row + 4]) * p[1]
            + f64::from(matrix[row + 8]) * p[2]
            + f64::from(matrix[row + 12])
    });
    let x = ((clip[0] / clip[3] + 1.0) * 32.0).floor() as i32;
    let y = ((1.0 - clip[1] / clip[3]) * 32.0).floor() as i32;
    (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            let (x, y) = (x + dx, y + dy);
            if !(0..64).contains(&x) || !(0..64).contains(&y) {
                return false;
            }
            let i = (y * 256 + x * 4) as usize;
            pixels[i] > 200 && pixels[i + 1] < 30 && pixels[i + 2] < 30
        })
    })
}
fn install(controller: &NativeController, bounds: BoundingBox, height: f64) {
    let data = serde_json::json!({"width":2,"height":2,"samples":[height,height,height,height],"nodata":-9999});
    install_wcs_terrain(
        &controller.terrain,
        "depth",
        decode_wcs_json_coverage(
            data.to_string().as_bytes(),
            bounds,
            CoverageSampleType::Float64,
            None,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires a headless WGPU adapter; run explicitly with --ignored"]
fn terrain_and_imagery_occlude_wfs_in_shared_depth() {
    pollster::block_on(async {
        let scene: serde_json::Value =
            serde_json::from_str(include_str!("../../../../tests/fixtures/depth/scene.json"))
                .unwrap();
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("headless adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();
        let mut raster = WgpuGpuPipeline::with_depth(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(wgpu::TextureFormat::Depth32Float),
        );
        raster
            .set_terrain_lod(Some(TerrainLodOptions {
                target_error_meters: 0.0,
                max_subdivisions: 32,
                max_source_samples: 1_089,
            }))
            .unwrap();
        let lines = WgpuWfsPipeline::new(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            Some(wgpu::TextureFormat::Depth32Float),
        );
        let mut controller = NativeController::new(0.0, 0.0).unwrap();
        controller.camera.viewport_base_meters = scene["viewportBaseMeters"].as_f64().unwrap();
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let b = &scene["bounds"];
        let bounds = BoundingBox::new(
            b[0].as_f64().unwrap(),
            b[1].as_f64().unwrap(),
            b[2].as_f64().unwrap(),
            b[3].as_f64().unwrap(),
            crs,
        )
        .unwrap();
        let terrain_height = scene["terrainHeight"].as_f64().unwrap();
        for mode in ["2D", "2.5D", "3D"] {
            controller.view_mode = mode.into();
            controller.camera.pitch = if mode == "2.5D" { 0.4 } else { 0.0 };
            controller.camera.zoom = if mode == "3D" { 20.0 } else { 1.0 };
            install(&controller, bounds, terrain_height);
            if mode == "3D" {
                let fixed = WgpuGpuPipeline::generate_terrain_vertices(&controller, 1.0);
                let (adaptive, _) = WgpuGpuPipeline::generate_terrain_vertices_with_lod(
                    &controller,
                    1.0,
                    TerrainLodOptions {
                        target_error_meters: 0.0,
                        max_subdivisions: 32,
                        max_source_samples: 1_089,
                    },
                )
                .unwrap();
                assert_eq!(fixed.len(), adaptive.len());
                for (fixed, adaptive) in fixed.iter().zip(&adaptive) {
                    assert_eq!(fixed.position, adaptive.position);
                    assert_eq!(fixed.terrain_known, adaptive.terrain_known);
                }
            }
            raster.rebuild_terrain_buffers(&controller, &device, &queue, 1.0);
            let report = raster.terrain_lod_report().expect("adaptive mesh report");
            assert_eq!(report.chosen_subdivisions, 32);
            assert!(report.samples <= 1_089 && report.budget_limited);
            let mesh_height = raster.terrain_mesh_height_at(0.0, 0.0);
            if mode == "2D" {
                assert!(mesh_height.is_none());
            } else {
                assert!(mesh_height.is_some_and(|height| (height - terrain_height).abs() < 50.0));
            }
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
            queue.write_buffer(&raster.uniform_buffer, 0, bytemuck::cast_slice(&matrix));
            lines
                .set_view(&queue, &matrix, [1.0, 0.0, 0.0, 1.0])
                .unwrap();
            for height in [
                scene["belowHeight"].as_f64().unwrap(),
                scene["aboveHeight"].as_f64().unwrap(),
            ] {
                let data = serde_json::json!({"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[0,0,height]},"properties":{}}]});
                let features = WgpuWfsFeatures::new(
                    &device,
                    decode_wfs_feature_collection(data.to_string().as_bytes(), crs).unwrap(),
                    &controller,
                    WfsMeshOptions {
                        point_radius: scene["pointRadius"].as_f64().unwrap(),
                        ..Default::default()
                    },
                )
                .unwrap();
                for reverse in [false, true] {
                    let pixels = frame(
                        &device,
                        &queue,
                        &raster,
                        &features,
                        &lines,
                        None,
                        (mode != "2D", reverse),
                    );
                    assert_eq!(
                        red_near(&pixels, &matrix, mode, height),
                        mode == "2D" || height > scene["terrainHeight"].as_f64().unwrap(),
                        "{mode} terrain: height={height}, reverse={reverse}"
                    );
                    for alpha in [0, 255] {
                        let image = MapImage {
                            pixels: vec![0, 0, 255, alpha].into_boxed_slice(),
                            width: NonZeroU32::new(1).unwrap(),
                            height: NonZeroU32::new(1).unwrap(),
                            format: ImageFormat::Png,
                            crs,
                            bounds,
                        };
                        let image = WgpuWmsImage::new(
                            &device,
                            &queue,
                            &raster,
                            &image,
                            &controller,
                            WmsMeshOptions {
                                drape_terrain: true,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        let pixels = frame(
                            &device,
                            &queue,
                            &raster,
                            &features,
                            &lines,
                            Some(&image),
                            (false, reverse),
                        );
                        let expected = alpha == 0
                            || if mode == "2D" {
                                !reverse
                            } else {
                                height > scene["terrainHeight"].as_f64().unwrap()
                            };
                        assert_eq!(
                            red_near(&pixels, &matrix, mode, height),
                            expected,
                            "{mode} imagery: height={height}, alpha={alpha}, reverse={reverse}"
                        );
                    }
                }
                // An all-nodata replacement must invalidate the terrain mesh and never occlude.
                install(&controller, bounds, -9999.0);
                raster.rebuild_terrain_buffers(&controller, &device, &queue, 1.0);
                assert!(raster.terrain_mesh_height_at(0.0, 0.0).is_none());
                let nodata_report = raster.terrain_lod_report().expect("nodata LOD report");
                assert!(nodata_report.has_unknown);
                assert!(!nodata_report.threshold_satisfied);
                let pixels = frame(
                    &device,
                    &queue,
                    &raster,
                    &features,
                    &lines,
                    None,
                    (mode != "2D", true),
                );
                assert!(
                    red_near(&pixels, &matrix, mode, height),
                    "{mode}: nodata terrain hid a feature"
                );
                install(
                    &controller,
                    bounds,
                    scene["terrainHeight"].as_f64().unwrap(),
                );
                raster.rebuild_terrain_buffers(&controller, &device, &queue, 1.0);
            }
        }
    });
}
