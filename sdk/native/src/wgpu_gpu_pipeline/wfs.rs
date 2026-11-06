//! WFS line rendering with explicit CRS, altitude and geometry validation.
use crate::NativeController;
use olayer_core::geodesy::{ellipsoid::Ellipsoid, lla_to_ecef, LatLon};
use olayer_map_core::FeatureCollection;
use serde_json::Value;
use std::fmt;
use wgpu::util::DeviceExt;

const A: f64 = 6_378_137.0;
const B: f64 = 6_356_752.314_245_179;
const MAX_VERTICES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WfsAltitudeMode {
    Absolute,
    ClampToGround,
    RelativeToGround,
}

#[derive(Debug, Clone, Copy)]
pub struct WfsMeshOptions {
    pub altitude_mode: WfsAltitudeMode,
    pub height_offset: f64,
    /// Linear subdivisions per source segment, 1..=64.
    pub subdivisions: u32,
    /// Half-length of point crosses in metres.
    pub point_radius: f64,
}
impl Default for WfsMeshOptions {
    fn default() -> Self {
        Self {
            altitude_mode: WfsAltitudeMode::Absolute,
            height_offset: 0.0,
            subdivisions: 16,
            point_radius: 1000.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WfsRenderError(pub String);
impl fmt::Display for WfsRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for WfsRenderError {}
fn invalid(message: &str) -> WfsRenderError {
    WfsRenderError(message.into())
}

struct Path {
    positions: Vec<[f64; 3]>,
    point: bool,
}
struct Decoder {
    crs: u32,
    count: usize,
    paths: Vec<Path>,
}
impl Decoder {
    fn array<'a>(&self, value: &'a Value) -> Result<&'a Vec<Value>, WfsRenderError> {
        value
            .as_array()
            .ok_or_else(|| invalid("invalid WFS coordinate nesting"))
    }
    fn position(&mut self, value: &Value) -> Result<[f64; 3], WfsRenderError> {
        let p = self.array(value)?;
        if !(2..=3).contains(&p.len()) {
            return Err(invalid("WFS position requires x/y and optional height"));
        }
        let x = p[0]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| invalid("invalid WFS x"))?;
        let y = p[1]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| invalid("invalid WFS y"))?;
        let z = if p.len() == 3 {
            p[2].as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid("invalid WFS height"))?
        } else {
            0.0
        };
        self.count += 1;
        if self.count > MAX_VERTICES {
            return Err(invalid("WFS coordinate limit exceeded"));
        }
        let (limit_x, limit_y) = if self.crs == 4326 {
            (180.0, 90.0)
        } else {
            (std::f64::consts::PI * A, std::f64::consts::PI * A)
        };
        if x.abs() > limit_x || y.abs() > limit_y {
            return Err(invalid("WFS coordinate outside CRS domain"));
        }
        Ok(if self.crs == 4326 {
            [y.to_radians(), x.to_radians(), z]
        } else {
            [(y / A).sinh().atan(), x / A, z]
        })
    }
    fn line(&mut self, value: &Value, ring: bool) -> Result<(), WfsRenderError> {
        let raw = self.array(value)?;
        if raw.is_empty() {
            return Ok(());
        }
        let positions = raw
            .iter()
            .map(|v| self.position(v))
            .collect::<Result<Vec<_>, _>>()?;
        if positions.len() < if ring { 4 } else { 2 } {
            return Err(invalid("WFS line or ring is too short"));
        }
        if ring && positions.first() != positions.last() {
            return Err(invalid("WFS ring must be closed"));
        }
        if positions
            .windows(2)
            .any(|p| (p[0][1] - p[1][1]).abs() > std::f64::consts::PI)
        {
            return Err(invalid("split WFS paths at the antimeridian"));
        }
        self.paths.push(Path {
            positions,
            point: false,
        });
        Ok(())
    }
    fn geometry(&mut self, value: &Value, depth: usize) -> Result<(), WfsRenderError> {
        if value.is_null() {
            return Ok(());
        }
        if depth > 16 || !value.is_object() {
            return Err(invalid("invalid WFS geometry or nesting limit"));
        }
        let coords = &value["coordinates"];
        match value["type"].as_str() {
            Some("Point") => {
                let p = self.position(coords)?;
                self.paths.push(Path {
                    positions: vec![p],
                    point: true,
                });
            }
            Some("MultiPoint") => {
                for p in self.array(coords)? {
                    let p = self.position(p)?;
                    self.paths.push(Path {
                        positions: vec![p],
                        point: true,
                    });
                }
            }
            Some("LineString") => self.line(coords, false)?,
            Some("MultiLineString") => {
                for p in self.array(coords)? {
                    self.line(p, false)?;
                }
            }
            Some("Polygon") => {
                for p in self.array(coords)? {
                    self.line(p, true)?;
                }
            }
            Some("MultiPolygon") => {
                for polygon in self.array(coords)? {
                    for p in self.array(polygon)? {
                        self.line(p, true)?;
                    }
                }
            }
            Some("GeometryCollection") => {
                for child in self.array(&value["geometries"])? {
                    self.geometry(child, depth + 1)?;
                }
            }
            _ => return Err(invalid("unsupported WFS geometry type")),
        }
        Ok(())
    }
}

fn paths(collection: &FeatureCollection) -> Result<Vec<Path>, WfsRenderError> {
    let crs = collection.crs.epsg_code();
    if !matches!(crs, 4326 | 3857 | 900913) {
        return Err(invalid("unsupported WFS CRS"));
    }
    let mut decoder = Decoder {
        crs,
        count: 0,
        paths: Vec::new(),
    };
    for f in &collection.features {
        decoder.geometry(&f.geometry, 0)?;
    }
    Ok(decoder.paths)
}

/// Packed line list. Polygon rings remain independent, and nodata breaks segments.
pub fn build_wfs_mesh(
    collection: &FeatureCollection,
    controller: &NativeController,
    options: WfsMeshOptions,
) -> Result<Vec<[f32; 3]>, WfsRenderError> {
    if !(1..=64).contains(&options.subdivisions)
        || !options.height_offset.is_finite()
        || !options.point_radius.is_finite()
        || options.point_radius <= 0.0
    {
        return Err(invalid("invalid WFS mesh options"));
    }
    if !matches!(controller.view_mode.as_str(), "2D" | "2.5D" | "3D") {
        return Err(invalid("invalid WFS view mode"));
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
        return Err(invalid("invalid WFS camera"));
    }
    let globe = controller.view_mode == "3D";
    let eye = super::wms::globe_eye(controller);
    let visibility = |p: [f64; 3]| {
        if globe {
            (p[0] * eye[0] + p[1] * eye[1]) / (A * A) + p[2] * eye[2] / (B * B) - 1.0
        } else {
            1.0
        }
    };
    let project = |[lat, lon, z]: [f64; 3]| -> Result<Option<[f64; 3]>, WfsRenderError> {
        let height = if controller.view_mode == "2D" {
            0.0
        } else {
            let ground = if options.altitude_mode == WfsAltitudeMode::Absolute {
                Some(0.0)
            } else {
                controller
                    .terrain
                    .get_elevation_status(lat, lon)
                    .ok()
                    .and_then(|s| s.elevation_meters)
            };
            let Some(ground) = ground else {
                return Ok(None);
            };
            ground
                + if options.altitude_mode == WfsAltitudeMode::ClampToGround {
                    0.0
                } else {
                    z
                }
                + options.height_offset
        };
        let p = if globe {
            let p = lla_to_ecef(&LatLon::new(lat, lon, height), &Ellipsoid::wgs84());
            [p.x, p.y, p.z]
        } else {
            let (x, y) = controller
                .projection
                .project(&LatLon::new(lat, lon, 0.0))
                .map_err(|_| invalid("WFS projection failed"))?;
            [x, y, height]
        };
        if !p.iter().all(|v| v.is_finite()) {
            return Err(invalid("WFS projection or elevation is not finite"));
        }
        Ok(Some(p))
    };
    let mut output = Vec::new();
    let mut work = 0;
    let mut emit = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| -> Result<(), WfsRenderError> {
        work += 2;
        if work > MAX_VERTICES {
            return Err(invalid("WFS mesh vertex limit exceeded"));
        }
        let (Some(mut a), Some(mut b)) = (a, b) else {
            return Ok(());
        };
        let va = visibility(a);
        let vb = visibility(b);
        if va < 0.0 && vb < 0.0 {
            return Ok(());
        }
        if (va < 0.0) != (vb < 0.0) {
            let t = va / (va - vb);
            let p = std::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
            if va < 0.0 {
                a = p;
            } else {
                b = p;
            }
        }
        if output.len() + 2 > MAX_VERTICES {
            return Err(invalid("WFS mesh vertex limit exceeded"));
        }
        for p in [a, b] {
            let p = p.map(|v| v as f32);
            if !p.iter().all(|v| v.is_finite()) {
                return Err(invalid("WFS mesh exceeds GPU precision"));
            }
            output.push(p);
        }
        Ok(())
    };
    for path in paths(collection)? {
        if path.point {
            let [lat, lon, _] = path.positions[0];
            let Some(center) = project(path.positions[0])? else {
                continue;
            };
            let axes = if globe {
                [
                    [-lon.sin(), lon.cos(), 0.0],
                    [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()],
                ]
            } else {
                [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            };
            for axis in axes {
                emit(
                    Some(std::array::from_fn(|i| {
                        center[i] - options.point_radius * axis[i]
                    })),
                    Some(std::array::from_fn(|i| {
                        center[i] + options.point_radius * axis[i]
                    })),
                )?;
            }
        } else {
            for pair in path.positions.windows(2) {
                let [a, b] = [pair[0], pair[1]];
                let mut previous = project(a)?;
                for j in 1..=options.subdivisions {
                    let next = project(std::array::from_fn(|i| {
                        a[i] + (b[i] - a[i]) * f64::from(j) / f64::from(options.subdivisions)
                    }))?;
                    emit(previous, next)?;
                    previous = next;
                }
            }
        }
    }
    Ok(output)
}

/// Shared line pipeline. The optional depth format must match the host render pass.
pub struct WgpuWfsPipeline {
    pipeline: wgpu::RenderPipeline,
    pipeline_2d: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}
impl WgpuWfsPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WFS lines"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct Uniforms { matrix: mat4x4<f32>, color: vec4<f32> };
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@vertex fn vs(@location(0) position:vec3<f32>) -> @builtin(position) vec4<f32> {
    let clip = uniforms.matrix * vec4<f32>(position,1.0);
    return vec4<f32>(clip.xy, (clip.z + clip.w) * 0.5, clip.w);
}
@fragment fn fs() -> @location(0) vec4<f32> { return uniforms.color; }
"#
                .into(),
            ),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("WFS uniforms"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let mut descriptor = wgpu::RenderPipelineDescriptor {
            label: Some("WFS line pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
        };
        let pipeline = device.create_render_pipeline(&descriptor);
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false;
            depth.depth_compare = wgpu::CompareFunction::Always;
        }
        let pipeline_2d = device.create_render_pipeline(&descriptor);
        Self {
            pipeline,
            pipeline_2d,
            uniform,
            bind_group,
        }
    }
    /// Update before submitting a pass. Color is normalized RGBA.
    pub fn set_view(
        &self,
        queue: &wgpu::Queue,
        matrix: &[f32; 16],
        color: [f32; 4],
    ) -> Result<(), WfsRenderError> {
        if !matrix.iter().all(|v| v.is_finite())
            || !color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(invalid("invalid WFS matrix or color"));
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(matrix));
        queue.write_buffer(&self.uniform, 64, bytemuck::cast_slice(&color));
        Ok(())
    }
}

/// Owns a decoded feature collection and GPU lines; rebuild after view or terrain changes.
pub struct WgpuWfsFeatures {
    two_d: bool,
    collection: FeatureCollection,
    options: WfsMeshOptions,
    buffer: wgpu::Buffer,
    count: u32,
}
impl WgpuWfsFeatures {
    pub fn new(
        device: &wgpu::Device,
        collection: FeatureCollection,
        controller: &NativeController,
        options: WfsMeshOptions,
    ) -> Result<Self, WfsRenderError> {
        let vertices = build_wfs_mesh(&collection, controller, options)?;
        let count = vertices.len() as u32;
        Ok(Self {
            two_d: controller.view_mode == "2D",
            collection,
            options,
            buffer: Self::buffer(device, &vertices),
            count,
        })
    }
    fn buffer(device: &wgpu::Device, vertices: &[[f32; 3]]) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("WFS vertices"),
            contents: bytemuck::cast_slice(if vertices.is_empty() {
                &[[0.0; 3]]
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
    ) -> Result<(), WfsRenderError> {
        let vertices = build_wfs_mesh(&self.collection, controller, self.options)?;
        self.buffer = Self::buffer(device, &vertices);
        self.count = vertices.len() as u32;
        self.two_d = controller.view_mode == "2D";
        Ok(())
    }
    pub fn collection(&self) -> &FeatureCollection {
        &self.collection
    }
    pub fn render<'a>(&'a self, pipeline: &'a WgpuWfsPipeline, pass: &mut wgpu::RenderPass<'a>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(if self.two_d {
            &pipeline.pipeline_2d
        } else {
            &pipeline.pipeline
        });
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_provider::NativeMapDataError;
    use crate::{
        decode_wcs_json_coverage, decode_wfs_feature_collection, install_wcs_terrain,
        NativeOgcClient, OgcTransport, WfsFeatureSource,
    };
    use olayer_map_core::{BoundingBox, CoordinateReferenceSystem, CoverageSampleType, UrlParts};
    use std::{
        num::NonZeroUsize,
        sync::{Arc, Mutex},
    };

    struct FixtureTransport(Arc<Mutex<Vec<String>>>);
    impl OgcTransport for FixtureTransport {
        fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            self.0.lock().unwrap().push(url.into());
            Ok(include_bytes!("../../../../tests/fixtures/wfs/features.json").to_vec())
        }
    }
    fn source(urls: Arc<Mutex<Vec<String>>>) -> WfsFeatureSource<FixtureTransport> {
        WfsFeatureSource::new(
            NativeOgcClient::new(FixtureTransport(urls), NonZeroUsize::new(8192).unwrap()),
            UrlParts::new("https://fixture.test", "wfs"),
            "features",
            CoordinateReferenceSystem::epsg(4326).unwrap(),
        )
    }
    fn controller() -> NativeController {
        let c = NativeController::new(0.0, 0.0).unwrap();
        install_wcs_terrain(
            &c.terrain,
            "wcs",
            decode_wcs_json_coverage(
                include_bytes!("../../../../tests/fixtures/wcs/elevation.json"),
                BoundingBox::new(
                    -2.0,
                    -1.0,
                    2.0,
                    1.0,
                    CoordinateReferenceSystem::epsg(4326).unwrap(),
                )
                .unwrap(),
                CoverageSampleType::Float64,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        c
    }
    fn collection(value: Value) -> FeatureCollection {
        decode_wfs_feature_collection(serde_json::to_string(&serde_json::json!({"type":"FeatureCollection","features":[{"type":"Feature","geometry":value,"properties":{}}]})).unwrap().as_bytes(),CoordinateReferenceSystem::epsg(4326).unwrap()).unwrap()
    }

    #[test]
    fn mercator_and_multi_geometry_match_geographic_paths() {
        for name in [
            "EPSG:4326",
            "urn:ogc:def:crs:EPSG::4326",
            "http://www.opengis.net/def/crs/EPSG/0/4326",
        ] {
            let json = serde_json::json!({"type":"FeatureCollection","crs":{"type":"name","properties":{"name":name}},"features":[]});
            assert!(decode_wfs_feature_collection(
                json.to_string().as_bytes(),
                CoordinateReferenceSystem::epsg(4326).unwrap()
            )
            .is_ok());
        }
        let c = controller();
        let geographic =
            collection(serde_json::json!({"type":"LineString","coordinates":[[0,0],[1,1]]}));
        let mut mercator = collection(
            serde_json::json!({"type":"LineString","coordinates":[[0,0],[1_f64.to_radians()*A,1_f64.to_radians().tan().asinh()*A]]}),
        );
        mercator.crs = CoordinateReferenceSystem::epsg(3857).unwrap();
        assert_eq!(
            build_wfs_mesh(&geographic, &c, Default::default()).unwrap(),
            build_wfs_mesh(&mercator, &c, Default::default()).unwrap()
        );
        for geometry in [
            serde_json::json!({"type":"MultiPoint","coordinates":[[0,0],[1,1]]}),
            serde_json::json!({"type":"MultiLineString","coordinates":[[[0,0],[1,1]]]}),
            serde_json::json!({"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]]]}),
            serde_json::json!({"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[0,0]},null]}),
        ] {
            assert!(
                !build_wfs_mesh(&collection(geometry), &c, Default::default())
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            build_wfs_mesh(&collection(Value::Null), &c, Default::default())
                .unwrap()
                .is_empty()
        );
        assert!(decode_wfs_feature_collection(br#"{"type":"FeatureCollection","crs":{"type":"name","properties":{"name":"EPSG:3857"}},"features":[]}"#,CoordinateReferenceSystem::epsg(4326).unwrap()).is_err());
    }

    #[test]
    fn feature_fixture_renders_geometry_across_modes_from_one_fetch() {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let data = source(urls.clone()).fetch().unwrap();
        let mut c = controller();
        for mode in ["2D", "2.5D", "3D"] {
            c.view_mode = mode.into();
            let mesh = build_wfs_mesh(
                &data,
                &c,
                WfsMeshOptions {
                    subdivisions: 1,
                    altitude_mode: WfsAltitudeMode::RelativeToGround,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(mesh.len(), 22);
            if mode == "2D" {
                assert!(mesh.iter().all(|p| p[2] == 0.0));
            }
            if mode == "2.5D" {
                assert_eq!(mesh[0][2], 620.0);
            }
            if mode == "3D" {
                assert!((mesh[0][0] - 6_378_757.0).abs() < 1.0);
            }
        }
        assert_eq!(data.features[0].properties["metadata"]["kind"], "test");
        assert_eq!(urls.lock().unwrap().len(), 1);
    }
    #[test]
    fn missing_terrain_breaks_segments_and_globe_clips_far_side() {
        let mut c = controller();
        c.terrain.unload_elevation_grid("wcs");
        c.view_mode = "2.5D".into();
        let line =
            collection(serde_json::json!({"type":"LineString","coordinates":[[-1,0],[1,0]]}));
        assert!(build_wfs_mesh(
            &line,
            &c,
            WfsMeshOptions {
                altitude_mode: WfsAltitudeMode::ClampToGround,
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
        c.view_mode = "3D".into();
        let far =
            collection(serde_json::json!({"type":"LineString","coordinates":[[170,0],[175,0]]}));
        assert!(build_wfs_mesh(&far, &c, Default::default())
            .unwrap()
            .is_empty());
        let edge =
            collection(serde_json::json!({"type":"LineString","coordinates":[[65,0],[85,0]]}));
        let mesh = build_wfs_mesh(&edge, &c, Default::default()).unwrap();
        assert!(!mesh.is_empty());
        assert!(mesh
            .iter()
            .all(|p| f64::from(p[0]) >= A * A / (A + 15_000_000.0) - 1.0));
    }
    #[test]
    fn rejects_bad_geometry_and_preserves_height_modes() {
        let mut c = controller();
        c.view_mode = "2.5D".into();
        for value in [
            serde_json::json!({"type":"LineString","coordinates":[[179,0],[-179,0]]}),
            serde_json::json!({"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[2,1]]]}),
            serde_json::json!({"type":"Point","coordinates":[181,0]}),
        ] {
            assert!(build_wfs_mesh(&collection(value), &c, Default::default()).is_err());
        }
        let line = collection(
            serde_json::json!({"type":"LineString","coordinates":[[0,0,100],[1,0,200]]}),
        );
        assert_eq!(
            build_wfs_mesh(&line, &c, Default::default()).unwrap()[0][2],
            100.0
        );
        assert_eq!(
            build_wfs_mesh(
                &line,
                &c,
                WfsMeshOptions {
                    altitude_mode: WfsAltitudeMode::ClampToGround,
                    height_offset: 10.0,
                    ..Default::default()
                }
            )
            .unwrap()[0][2],
            510.0
        );
    }
    #[test]
    #[ignore = "requires a headless WGPU adapter; run explicitly with --ignored"]
    fn wfs_features_render_into_offscreen_pixels_in_all_modes() {
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
            let pipeline = WgpuWfsPipeline::new(
                &device,
                wgpu::TextureFormat::Rgba8Unorm,
                Some(wgpu::TextureFormat::Depth32Float),
            );
            let urls = Arc::new(Mutex::new(Vec::new()));
            let collection = source(urls.clone()).fetch().unwrap();
            let mut controller = controller();
            controller.camera.pitch = 0.0;
            controller.camera.viewport_base_meters = 600_000.0;
            let mut gpu_features = WgpuWfsFeatures::new(
                &device,
                collection,
                &controller,
                WfsMeshOptions {
                    altitude_mode: WfsAltitudeMode::RelativeToGround,
                    point_radius: 12000.0,
                    ..Default::default()
                },
            )
            .unwrap();
            for mode in ["2D", "2.5D", "3D"] {
                controller.view_mode = mode.into();
                controller.camera.zoom = if mode == "3D" { 20.0 } else { 1.0 };
                controller.camera.pitch = if mode == "2.5D" { 0.4 } else { 0.0 };
                gpu_features.rebuild(&device, &controller).unwrap();
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
                pipeline
                    .set_view(&queue, &matrix, [0.0, 1.0, 0.0, 1.0])
                    .unwrap();
                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("WFS offscreen test"),
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
                let depth = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("WFS depth"),
                    size: wgpu::Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let depth_view = depth.create_view(&Default::default());
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
                    gpu_features.render(&pipeline, &mut pass);
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
                for (lon, lat, z) in [
                    (0_f64, 0_f64, 120.0),
                    (-1.0, -0.5, 100.0),
                    (-1.5, 0.8, 0.0),
                    (-0.4, -0.3, 0.0),
                ] {
                    let height = if mode == "2D" { 0.0 } else { 500.0 + z };
                    let p = if mode == "3D" {
                        let p = lla_to_ecef(
                            &LatLon::new(lat.to_radians(), lon.to_radians(), height),
                            &Ellipsoid::wgs84(),
                        );
                        [p.x, p.y, p.z]
                    } else {
                        let (x, y) = controller
                            .projection
                            .project(&LatLon::new(lat.to_radians(), lon.to_radians(), 0.0))
                            .unwrap();
                        [x, y, height]
                    };
                    let clip: [f64; 4] = std::array::from_fn(|row| {
                        f64::from(matrix[row]) * p[0]
                            + f64::from(matrix[4 + row]) * p[1]
                            + f64::from(matrix[8 + row]) * p[2]
                            + f64::from(matrix[12 + row])
                    });
                    let px = ((clip[0] / clip[3] + 1.0) * 32.0).floor() as i32;
                    let py = ((1.0 - clip[1] / clip[3]) * 32.0).floor() as i32;
                    let found = (-2..=2).any(|dy| {
                        (-2..=2).any(|dx| {
                            let (x, y) = (px + dx, py + dy);
                            if !(0..64).contains(&x) || !(0..64).contains(&y) {
                                return false;
                            }
                            let index = (y * 256 + x * 4) as usize;
                            pixels[index] < 30 && pixels[index + 1] > 200 && pixels[index + 2] < 30
                        })
                    });
                    assert!(found, "{mode}: missing WFS feature at {lon},{lat}");
                }
            }
            assert_eq!(
                urls.lock().unwrap().len(),
                1,
                "switching dimensions must not refetch WFS features"
            );
        });
    }
}
