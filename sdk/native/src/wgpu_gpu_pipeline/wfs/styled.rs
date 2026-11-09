//! Cached, indexed geometry and submission-safe immutable styled draws.
use super::{
    build_path_mesh, fill, invalid, paths, Decoder, NativeController, WfsMeshOptions, WfsPaint,
    WfsRenderError, WfsStyleSheet, WfsVisibility, WgpuWfsPipeline, MAX_VERTICES,
};
use olayer_map_core::FeatureCollection;
use std::ops::Range;
use wgpu::util::DeviceExt;

const MAX_DRAWS: usize = 8192;

/// Collection-local ownership, independent of absent or duplicated GeoJSON IDs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct WfsFeatureIndex(usize);
impl WfsFeatureIndex {
    pub fn index(self) -> usize {
        self.0
    }
}

/// Owns validated geometry, original metadata and cached polygon triangulation.
/// Style changes only replace resolved paints; they do not fetch or triangulate.
#[derive(Debug, Clone)]
pub struct PreparedWfsFeatures {
    collection: FeatureCollection,
    decoded: Decoder,
    triangles: Vec<Vec<[f64; 3]>>,
    options: WfsMeshOptions,
    style: WfsStyleSheet,
    paints: Vec<WfsPaint>,
}

fn charge(total: &mut usize, amount: usize, limit: usize) -> Result<(), WfsRenderError> {
    *total = total
        .checked_add(amount)
        .filter(|v| *v <= limit)
        .ok_or_else(|| invalid("WFS aggregate budget exceeded"))?;
    Ok(())
}

impl PreparedWfsFeatures {
    /// Prepares all geometry, including hidden features. A sheet's paint controls
    /// filling; `options.fill_polygons` is only used by the legacy rendering API.
    ///
    /// # Errors
    /// Rejects invalid mesh options, geometry/topology, numeric properties, or
    /// aggregate work, storage and topology budgets. No partial object is returned.
    pub fn new(
        collection: FeatureCollection,
        options: WfsMeshOptions,
        style: WfsStyleSheet,
    ) -> Result<Self, WfsRenderError> {
        if !(1..=64).contains(&options.subdivisions)
            || !options.height_offset.is_finite()
            || !options.point_radius.is_finite()
            || options.point_radius <= 0.0
        {
            return Err(invalid("invalid WFS mesh options"));
        }
        let paints = style.evaluate(&collection)?;
        let decoded = paths(&collection)?;
        let mut work = 0;
        let subdivisions = usize::try_from(options.subdivisions)
            .map_err(|_| invalid("WFS subdivisions overflow"))?;
        for path in &decoded.paths {
            let amount = if path.point {
                4
            } else {
                path.positions
                    .len()
                    .saturating_sub(1)
                    .checked_mul(subdivisions)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or_else(|| invalid("WFS line work overflow"))?
            };
            charge(&mut work, amount, MAX_VERTICES)?;
        }
        let mut topology_work = 0;
        let mut prepared_vertices = 0;
        let mut triangles = Vec::with_capacity(collection.features.len());
        for (_, polygons) in &decoded.feature_ranges {
            let mut feature_triangles = Vec::new();
            for rings in &decoded.polygons[polygons.clone()] {
                let nodes = rings
                    .iter()
                    .try_fold(0usize, |sum, r| sum.checked_add(r.len()))
                    .ok_or_else(|| invalid("WFS polygon node overflow"))?;
                if nodes > olayer_core::volumetric::MAX_FILL_NODES {
                    return Err(invalid("WFS polygon node limit exceeded"));
                }
                charge(
                    &mut topology_work,
                    nodes
                        .checked_mul(nodes)
                        .ok_or_else(|| invalid("WFS topology overflow"))?,
                    8_000_000,
                )?;
                let lengths: Vec<_> = rings.iter().map(Vec::len).collect();
                let points: Vec<_> = rings.iter().flatten().copied().collect();
                let result = olayer_core::volumetric::triangulate_polygon_rings(&points, &lengths)
                    .map_err(|e| WfsRenderError(e.to_string()))?;
                charge(&mut prepared_vertices, result.len(), MAX_VERTICES)?;
                let amount = result
                    .len()
                    .checked_mul(subdivisions)
                    .and_then(|n| n.checked_mul(subdivisions))
                    .ok_or_else(|| invalid("WFS fill work overflow"))?;
                charge(&mut work, amount, MAX_VERTICES)?;
                feature_triangles.extend(result);
            }
            triangles.push(feature_triangles);
        }
        let result = Self {
            collection,
            decoded,
            triangles,
            options: WfsMeshOptions {
                fill_polygons: true,
                ..options
            },
            style,
            paints,
        };
        result.draw_count(&result.paints)?;
        Ok(result)
    }

    fn draw_count(&self, paints: &[WfsPaint]) -> Result<(), WfsRenderError> {
        let mut count = 0;
        for (index, paint) in paints.iter().enumerate() {
            if paint.visibility == WfsVisibility::Hidden {
                continue;
            }
            if paint.fill.is_some_and(|c| c.components()[3] > 0.0)
                && !self.triangles[index].is_empty()
            {
                charge(&mut count, 1, MAX_DRAWS)?;
            }
            if paint.stroke.is_some_and(|c| c.components()[3] > 0.0)
                && !self.decoded.feature_ranges[index].0.is_empty()
            {
                charge(&mut count, 1, MAX_DRAWS)?;
            }
        }
        Ok(())
    }

    /// Atomically installs new paints, preserving geometry and the prior style on failure.
    ///
    /// # Errors
    /// Rejects invalid referenced numeric values or too many drawable ranges.
    pub fn set_style(&mut self, style: WfsStyleSheet) -> Result<(), WfsRenderError> {
        let paints = style.evaluate(&self.collection)?;
        self.draw_count(&paints)?;
        self.style = style;
        self.paints = paints;
        Ok(())
    }
    pub fn collection(&self) -> &FeatureCollection {
        &self.collection
    }
    pub fn style(&self) -> &WfsStyleSheet {
        &self.style
    }
    pub fn paints(&self) -> &[WfsPaint] {
        &self.paints
    }

    fn mesh(
        &self,
        controller: &NativeController,
    ) -> Result<(Vec<[f32; 3]>, Vec<DrawRange>), WfsRenderError> {
        // Validate view/options even when all channels are hidden or disabled.
        super::WfsTransform::new(controller, self.options)?;
        let mut vertices = Vec::new();
        let mut ranges = Vec::new();
        // Preserve legacy ordering: all fills, then all strokes, each in feature order.
        for phase in [Phase::Fill, Phase::Stroke] {
            for (index, paint) in self.paints.iter().enumerate() {
                if paint.visibility == WfsVisibility::Hidden {
                    continue;
                }
                let Some(color) = (match phase {
                    Phase::Fill => paint.fill,
                    Phase::Stroke => paint.stroke,
                }) else {
                    continue;
                };
                if color.components()[3] == 0.0 {
                    continue;
                }
                let mesh = match phase {
                    Phase::Fill => {
                        fill::build_prepared_fill(&self.triangles[index], controller, self.options)?
                    }
                    Phase::Stroke => build_path_mesh(
                        &self.decoded.paths[self.decoded.feature_ranges[index].0.clone()],
                        controller,
                        self.options,
                    )?,
                };
                if mesh.is_empty() {
                    continue;
                }
                let start = u32::try_from(vertices.len())
                    .map_err(|_| invalid("WFS vertex offset overflow"))?;
                let mut total = vertices.len();
                charge(&mut total, mesh.len(), MAX_VERTICES)?;
                vertices.extend(mesh);
                let end = u32::try_from(vertices.len())
                    .map_err(|_| invalid("WFS vertex count overflow"))?;
                if ranges.len() == MAX_DRAWS {
                    return Err(invalid("WFS draw range limit exceeded"));
                }
                ranges.push(DrawRange {
                    feature: WfsFeatureIndex(index),
                    vertices: start..end,
                    color: color.components(),
                    phase,
                });
            }
        }
        Ok((vertices, ranges))
    }

    /// Projects cached geometry and creates immutable submission-owned resources.
    /// The returned draw is independent of subsequent style/view changes and of
    /// the legacy pipeline's mutable uniform. No queue writes are performed.
    ///
    /// # Errors
    /// Rejects nonfinite matrices, projection/terrain errors or aggregate output limits.
    pub fn prepare_draw(
        &self,
        device: &wgpu::Device,
        pipeline: &WgpuWfsPipeline,
        controller: &NativeController,
        matrix: &[f32; 16],
    ) -> Result<WgpuWfsDraw, WfsRenderError> {
        if !matrix.iter().all(|v| v.is_finite()) {
            return Err(invalid("invalid WFS matrix"));
        }
        let (vertices, ranges) = self.mesh(controller)?;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("WFS styled vertices"),
            contents: bytemuck::cast_slice(if vertices.is_empty() {
                &[[0.0; 3]]
            } else {
                &vertices
            }),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut draws = Vec::with_capacity(ranges.len());
        for range in ranges {
            let mut uniform = [0.0_f32; 24];
            uniform[..16].copy_from_slice(matrix);
            uniform[16..20].copy_from_slice(&range.color);
            uniform[20..24].copy_from_slice(&range.color);
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("WFS immutable draw uniform"),
                contents: bytemuck::cast_slice(&uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("WFS immutable draw binding"),
                layout: &pipeline.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            draws.push(DrawBinding {
                range,
                _uniform: uniform,
                bind_group,
            });
        }
        Ok(WgpuWfsDraw {
            buffer,
            draws,
            two_d: controller.view_mode == "2D",
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Fill,
    Stroke,
}
#[derive(Debug, Clone)]
struct DrawRange {
    feature: WfsFeatureIndex,
    vertices: Range<u32>,
    color: [f32; 4],
    phase: Phase,
}
#[derive(Debug)]
struct DrawBinding {
    range: DrawRange,
    _uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Exclusive GPU resources for an immutable styled snapshot. Retain through encoding.
#[derive(Debug)]
pub struct WgpuWfsDraw {
    buffer: wgpu::Buffer,
    draws: Vec<DrawBinding>,
    two_d: bool,
}
impl WgpuWfsDraw {
    /// Returns collection-local owners in draw order (fills before strokes).
    pub fn feature_indices(&self) -> impl Iterator<Item = WfsFeatureIndex> + '_ {
        self.draws.iter().map(|d| d.range.feature)
    }
    pub fn render<'a>(&'a self, pipeline: &'a WgpuWfsPipeline, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for draw in &self.draws {
            let selected = match (draw.range.phase, self.two_d) {
                (Phase::Fill, true) => &pipeline.fill_pipeline_2d,
                (Phase::Fill, false) => &pipeline.fill_pipeline,
                (Phase::Stroke, true) => &pipeline.pipeline_2d,
                (Phase::Stroke, false) => &pipeline.pipeline,
            };
            pass.set_pipeline(selected);
            pass.set_bind_group(0, &draw.bind_group, &[]);
            pass.draw(draw.range.vertices.clone(), 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{WfsMatchPriority, WfsPredicate, WfsScalar, WfsStyleRule};
    use super::*;

    fn collection() -> FeatureCollection {
        crate::decode_wfs_feature_collection(serde_json::json!({"type":"FeatureCollection","features":[
            {"type":"Feature","id":"same","properties":{"kind":"left","unsafe":9007199254740992_u64},"geometry":{"type":"Polygon","coordinates":[[[-1,-0.5],[-0.2,-0.5],[-0.2,0.5],[-1,0.5],[-1,-0.5]]] }},
            {"type":"Feature","id":"same","properties":{"kind":"right"},"geometry":{"type":"Polygon","coordinates":[[[0.2,-0.5],[1,-0.5],[1,0.5],[0.2,0.5],[0.2,-0.5]]] }}
        ]}).to_string().as_bytes(), olayer_map_core::CoordinateReferenceSystem::epsg(4326).unwrap()).unwrap()
    }
    fn sheet() -> WfsStyleSheet {
        WfsStyleSheet::new(
            WfsPaint {
                stroke: None,
                fill: Some([1.0, 0.0, 0.0, 1.0].try_into().unwrap()),
                ..Default::default()
            },
            vec![WfsStyleRule {
                predicate: WfsPredicate::Equals {
                    property: "kind".into(),
                    value: WfsScalar::String("right".into()),
                },
                paint: WfsPaint {
                    stroke: None,
                    fill: Some([0.0, 1.0, 0.0, 1.0].try_into().unwrap()),
                    ..Default::default()
                },
            }],
            WfsMatchPriority::FirstMatch,
        )
        .unwrap()
    }
    #[test]
    fn indexed_metadata_cached_triangles_and_atomic_style_update() {
        let mut prepared = PreparedWfsFeatures::new(
            collection(),
            WfsMeshOptions {
                subdivisions: 1,
                ..Default::default()
            },
            sheet(),
        )
        .unwrap();
        let pointer = prepared.triangles[0].as_ptr();
        let c = NativeController::new(0.0, 0.0).unwrap();
        let (_, ranges) = prepared.mesh(&c).unwrap();
        assert_eq!(
            ranges.iter().map(|r| r.feature.index()).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            prepared.collection().features[0].id,
            prepared.collection().features[1].id
        );
        let bad = WfsStyleSheet::new(
            Default::default(),
            vec![WfsStyleRule {
                predicate: WfsPredicate::Equals {
                    property: "unsafe".into(),
                    value: WfsScalar::Number(0.0),
                },
                ..Default::default()
            }],
            Default::default(),
        )
        .unwrap();
        assert!(prepared.set_style(bad).is_err());
        assert_eq!(prepared.style(), &sheet());
        prepared.set_style(WfsStyleSheet::default()).unwrap();
        assert_eq!(pointer, prepared.triangles[0].as_ptr());
        assert_eq!(prepared.paints(), &[WfsPaint::default(); 2]);
    }
    #[test]
    fn aggregate_line_and_fill_work_is_not_reset_per_feature() {
        let mut c = collection();
        c.features = vec![c.features[0].clone(); 1000];
        // Each feature fits separately, but the collection's subdivision work does not.
        assert!(PreparedWfsFeatures::new(c, WfsMeshOptions::default(), sheet()).is_err());
    }

    #[test]
    fn topology_holes_and_feature_association_survive_style_changes() {
        let collection = crate::decode_wfs_feature_collection(
            include_bytes!("../../../../../tests/fixtures/wfs-fill/features.json"),
            olayer_map_core::CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
        .unwrap();
        let mut prepared = PreparedWfsFeatures::new(
            collection.clone(),
            WfsMeshOptions {
                subdivisions: 1,
                ..Default::default()
            },
            sheet(),
        )
        .unwrap();
        let mut controller = super::super::tests::controller();
        for mode in ["2D", "2.5D", "3D"] {
            controller.view_mode = mode.into();
            let (actual, ranges) = prepared.mesh(&controller).unwrap();
            let expected = super::super::build_wfs_fill_mesh(
                &collection,
                &controller,
                WfsMeshOptions {
                    subdivisions: 1,
                    fill_polygons: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                actual, expected,
                "{mode}: styling must not change triangulation, holes or clipping"
            );
            assert!(ranges
                .iter()
                .all(|r| r.feature.index() < collection.features.len()));
        }
        let before = prepared.triangles.clone();
        prepared.set_style(WfsStyleSheet::default()).unwrap();
        assert_eq!(prepared.triangles, before);
        let (_, ranges) = prepared.mesh(&controller).unwrap();
        assert!(ranges.iter().all(|r| matches!(r.phase, Phase::Stroke)));
    }

    #[test]
    fn hidden_geometry_is_validated_and_failed_range_update_preserves_style() {
        let hidden = WfsStyleSheet::new(
            WfsPaint {
                visibility: WfsVisibility::Hidden,
                ..Default::default()
            },
            vec![],
            Default::default(),
        )
        .unwrap();
        let mut malformed = collection();
        malformed.features[0].geometry =
            serde_json::json!({"type":"Polygon","coordinates":[[[0,0],[1,1],[0,1],[1,0],[0,0]]]});
        assert!(PreparedWfsFeatures::new(
            malformed,
            WfsMeshOptions {
                subdivisions: 1,
                ..Default::default()
            },
            hidden.clone()
        )
        .is_err());
        let mut many = collection();
        many.features[0].geometry = serde_json::json!({"type":"Point","coordinates":[0,0]});
        many.features = vec![many.features[0].clone(); MAX_DRAWS + 1];
        let mut prepared = PreparedWfsFeatures::new(
            many,
            WfsMeshOptions {
                subdivisions: 1,
                ..Default::default()
            },
            hidden.clone(),
        )
        .unwrap();
        assert!(prepared.set_style(WfsStyleSheet::default()).is_err());
        assert_eq!(prepared.style(), &hidden);
        assert!(prepared
            .mesh(&NativeController::new(0.0, 0.0).unwrap())
            .unwrap()
            .0
            .is_empty());
    }

    #[test]
    #[ignore = "requires a headless WGPU adapter; run explicitly with --ignored"]
    fn immutable_styled_draw_has_two_colors_in_one_submission_all_modes() {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("headless WGPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .unwrap();
            let pipeline = WgpuWfsPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm, None);
            let mut prepared = PreparedWfsFeatures::new(
                collection(),
                WfsMeshOptions {
                    subdivisions: 4,
                    ..Default::default()
                },
                sheet(),
            )
            .unwrap();
            let mut controller = NativeController::new(0.0, 0.0).unwrap();
            controller.camera.viewport_base_meters = 600_000.0;
            for mode in ["2D", "2.5D", "3D"] {
                controller.view_mode = mode.into();
                controller.camera.zoom = if mode == "3D" { 20.0 } else { 1.0 };
                controller.camera.pitch = if mode == "2.5D" { 0.4 } else { 0.0 };
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
                prepared.set_style(sheet()).unwrap();
                let draw = prepared
                    .prepare_draw(&device, &pipeline, &controller, &matrix)
                    .unwrap();
                assert_eq!(
                    draw.feature_indices()
                        .map(WfsFeatureIndex::index)
                        .collect::<Vec<_>>(),
                    vec![0, 1]
                );
                // Neither subsequent style updates nor legacy uniform writes can
                // change the matrix/paint captured by this draw's bindings.
                prepared
                    .set_style(
                        WfsStyleSheet::new(
                            WfsPaint {
                                visibility: WfsVisibility::Hidden,
                                ..Default::default()
                            },
                            vec![],
                            Default::default(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                pipeline
                    .set_view(&queue, &matrix, [1.0, 0.0, 1.0, 1.0])
                    .unwrap();
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("styled WFS pixels"),
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
                let view = target.create_view(&Default::default());
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("styled WFS readback"),
                    size: 64 * 256,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
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
                    draw.render(&pipeline, &mut pass);
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
                        sender.send(result).unwrap();
                    });
                device.poll(wgpu::Maintain::Wait);
                receiver.recv().unwrap().unwrap();
                let pixels = readback.slice(..).get_mapped_range();
                let red = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] > 200 && p[1] < 30 && p[2] < 30)
                    .count();
                let green = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] < 30 && p[1] > 200 && p[2] < 30)
                    .count();
                assert!(
                    red > 4 && green > 4,
                    "{mode}: red={red}, green={green}; per-draw colors must coexist"
                );
            }
        });
    }
}
