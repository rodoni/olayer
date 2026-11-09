//! Adapts decoded WCS scalar coverages to the shared terrain engine.
use super::NativeMapDataError;
use olayer_core::terrain::{ElevationGrid, ElevationGridCrs, TerrainEngine};
use olayer_map_core::Coverage;
use std::convert::TryFrom;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageGridRegistration {
    Nodes,
    CellCenters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationUnit {
    Metres,
    InternationalFeet,
    UsSurveyFeet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalReference {
    Ellipsoidal,
    Orthometric,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageCoordinateSystem {
    GeographicDegrees,
    WebMercatorMetres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageTerrainInterpretation {
    pub registration: CoverageGridRegistration,
    pub unit: ElevationUnit,
    pub vertical_reference: VerticalReference,
}

impl Default for CoverageTerrainInterpretation {
    fn default() -> Self {
        Self {
            registration: CoverageGridRegistration::Nodes,
            unit: ElevationUnit::Metres,
            vertical_reference: VerticalReference::Ellipsoidal,
        }
    }
}

/// Supplies orthometric-to-ellipsoidal geoid undulation N in metres.
/// The conversion convention is `h_ellipsoidal = H_orthometric + N`.
/// `x`/`y` use the coordinate units indicated by `crs`: geographic degrees or
/// Web Mercator metres. Implementations are synchronous, bounded and own their data policy.
pub trait GeoidUndulationProvider {
    fn undulation_meters(
        &self,
        x: f64,
        y: f64,
        crs: CoverageCoordinateSystem,
    ) -> Result<Option<f64>, NativeMapDataError>;
}

/// Installs a north-up node grid. Samples are ellipsoidal heights in metres.
/// Callers must explicitly select an elevation coverage with this registration/datum.
pub fn install_wcs_terrain(
    engine: &TerrainEngine,
    id: &str,
    coverage: Coverage,
) -> Result<(), NativeMapDataError> {
    install_wcs_terrain_with_interpretation(
        engine,
        id,
        coverage,
        CoverageTerrainInterpretation::default(),
        None,
    )
}

/// Converts explicit coverage registration, units and datum before atomically replacing terrain.
pub fn install_wcs_terrain_with_interpretation(
    engine: &TerrainEngine,
    id: &str,
    coverage: Coverage,
    interpretation: CoverageTerrainInterpretation,
    geoid: Option<&dyn GeoidUndulationProvider>,
) -> Result<(), NativeMapDataError> {
    let coverage = coverage
        .validate()
        .map_err(|e| NativeMapDataError::OgcDecode(e.to_string()))?;
    let b = coverage.bounds;
    let mut bounds = [b.min_x, b.min_y, b.max_x, b.max_y];
    let (grid_crs, correction_crs) = match b.crs.epsg_code() {
        4326 => (
            ElevationGridCrs::Geographic,
            CoverageCoordinateSystem::GeographicDegrees,
        ),
        3857 | 900913 => (
            ElevationGridCrs::WebMercator,
            CoverageCoordinateSystem::WebMercatorMetres,
        ),
        _ => {
            return Err(NativeMapDataError::OgcDecode(
                "unsupported WCS terrain CRS".into(),
            ))
        }
    };
    if interpretation.vertical_reference == VerticalReference::Orthometric && geoid.is_none() {
        return Err(NativeMapDataError::InvalidTerrainData(
            "orthometric WCS coverage requires an explicit geoid correction provider".into(),
        ));
    }
    let width = usize::try_from(coverage.width).map_err(|_| {
        NativeMapDataError::InvalidTerrainData("coverage width exceeds addressable memory".into())
    })?;
    let height = usize::try_from(coverage.height).map_err(|_| {
        NativeMapDataError::InvalidTerrainData("coverage height exceeds addressable memory".into())
    })?;
    if interpretation.registration == CoverageGridRegistration::CellCenters {
        if width > 1 {
            let step = (bounds[2] - bounds[0]) / f64::from(coverage.width);
            bounds[0] += step / 2.0;
            bounds[2] -= step / 2.0;
        }
        if height > 1 {
            let step = (bounds[3] - bounds[1]) / f64::from(coverage.height);
            bounds[1] += step / 2.0;
            bounds[3] -= step / 2.0;
        }
    }
    let original_bounds = [b.min_x, b.min_y, b.max_x, b.max_y];
    let unit_scale = match interpretation.unit {
        ElevationUnit::Metres => 1.0,
        ElevationUnit::InternationalFeet => 0.3048,
        ElevationUnit::UsSurveyFeet => 1_200.0 / 3_937.0,
    };
    let nodata = coverage.nodata;
    let mut samples = coverage.samples.into_vec();
    for (index, sample) in samples.iter_mut().enumerate() {
        if !sample.is_finite() {
            return Err(NativeMapDataError::InvalidTerrainData(
                "coverage contains a non-finite sample".into(),
            ));
        }
        if nodata == Some(*sample) {
            continue;
        }
        let valid_type = match coverage.sample_type {
            olayer_map_core::CoverageSampleType::UInt8 => {
                sample.fract() == 0.0 && (0.0..=255.0).contains(sample)
            }
            olayer_map_core::CoverageSampleType::UInt16 => {
                sample.fract() == 0.0 && (0.0..=65_535.0).contains(sample)
            }
            olayer_map_core::CoverageSampleType::Int16 => {
                sample.fract() == 0.0 && (-32_768.0..=32_767.0).contains(sample)
            }
            olayer_map_core::CoverageSampleType::Float32 => sample.abs() <= f64::from(f32::MAX),
            olayer_map_core::CoverageSampleType::Float64 => true,
        };
        if !valid_type {
            return Err(NativeMapDataError::InvalidTerrainData(
                "coverage sample disagrees with its declared type".into(),
            ));
        }
        *sample *= unit_scale;
        if interpretation.vertical_reference == VerticalReference::Orthometric {
            let row = index / width;
            let column = index % width;
            let row_f64 = f64::from(u32::try_from(row).map_err(|_| {
                NativeMapDataError::InvalidTerrainData(
                    "coverage row exceeds supported dimensions".into(),
                )
            })?);
            let column_f64 = f64::from(u32::try_from(column).map_err(|_| {
                NativeMapDataError::InvalidTerrainData(
                    "coverage column exceeds supported dimensions".into(),
                )
            })?);
            let cell_center = interpretation.registration == CoverageGridRegistration::CellCenters;
            let x = if cell_center {
                original_bounds[0]
                    + (column_f64 + 0.5) * (original_bounds[2] - original_bounds[0])
                        / f64::from(coverage.width)
            } else if width == 1 {
                (original_bounds[0] + original_bounds[2]) / 2.0
            } else {
                original_bounds[0]
                    + column_f64 * (original_bounds[2] - original_bounds[0])
                        / f64::from(u32::try_from(width - 1).map_err(|_| {
                            NativeMapDataError::InvalidTerrainData(
                                "coverage width exceeds supported dimensions".into(),
                            )
                        })?)
            };
            let y = if cell_center {
                original_bounds[3]
                    - (row_f64 + 0.5) * (original_bounds[3] - original_bounds[1])
                        / f64::from(coverage.height)
            } else if height == 1 {
                (original_bounds[1] + original_bounds[3]) / 2.0
            } else {
                original_bounds[3]
                    - row_f64 * (original_bounds[3] - original_bounds[1])
                        / f64::from(u32::try_from(height - 1).map_err(|_| {
                            NativeMapDataError::InvalidTerrainData(
                                "coverage height exceeds supported dimensions".into(),
                            )
                        })?)
            };
            let undulation = geoid
                .ok_or_else(|| {
                    NativeMapDataError::InvalidTerrainData(
                        "orthometric WCS coverage requires a geoid correction provider".into(),
                    )
                })?
                .undulation_meters(x, y, correction_crs)?
                .ok_or_else(|| {
                    NativeMapDataError::InvalidTerrainData(
                        "geoid correction is missing at a valid coverage sample".into(),
                    )
                })?;
            if !undulation.is_finite() {
                return Err(NativeMapDataError::InvalidTerrainData(
                    "geoid correction is non-finite".into(),
                ));
            }
            *sample += undulation;
        }
        if !sample.is_finite() {
            return Err(NativeMapDataError::InvalidTerrainData(
                "coverage conversion overflowed".into(),
            ));
        }
    }
    if grid_crs == ElevationGridCrs::Geographic {
        bounds = bounds.map(f64::to_radians);
    }
    let grid = ElevationGrid::new(
        grid_crs,
        bounds,
        width,
        height,
        samples.into_boxed_slice(),
        nodata,
    )
    .map_err(|e| NativeMapDataError::InvalidTerrainData(e.to_string()))?;
    engine.load_elevation_grid(id.to_owned(), grid);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_wms_mesh, NativeController, NativeOgcClient, OgcTransport, WcsCoverageSource,
        WcsCoverageSourceConfig, WmsMeshOptions,
    };
    use olayer_map_core::{BoundingBox, CoordinateReferenceSystem, CoverageSampleType, UrlParts};
    use std::{
        num::NonZeroUsize,
        sync::{Arc, Mutex},
    };

    struct Fixture(Arc<Mutex<Vec<String>>>);
    impl OgcTransport for Fixture {
        fn get(&self, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
            self.0.lock().unwrap().push(url.into());
            Ok(include_bytes!("../../../../tests/fixtures/wcs/elevation.json").to_vec())
        }
    }

    #[test]
    fn one_wcs_fetch_drives_terrain_geometry_across_modes() {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let bounds = BoundingBox::new(-2.0, -1.0, 2.0, 1.0, crs).unwrap();
        let source = WcsCoverageSource::new(
            NativeOgcClient::new(Fixture(urls.clone()), NonZeroUsize::new(1024).unwrap()),
            WcsCoverageSourceConfig {
                endpoint: UrlParts::new("https://fixture.test", "wcs"),
                coverage_id: "terrain".into(),
                crs,
                bounds,
                width: 3,
                height: 3,
                sample_type: CoverageSampleType::Float64,
            },
        );
        let mut controller = NativeController::new(0.0, 0.0).unwrap();
        source
            .load_json_terrain(&controller.terrain, "wcs")
            .unwrap();
        for mode in ["2D", "2.5D", "3D"] {
            controller.view_mode = mode.into();
            let mesh = build_wms_mesh(
                bounds,
                &controller,
                WmsMeshOptions {
                    subdivisions: 2,
                    drape_terrain: true,
                },
            )
            .unwrap();
            assert!(!mesh.is_empty());
            if mode != "3D" {
                assert_eq!(mesh[0].position[2], if mode == "2D" { 0.0 } else { 500.0 });
            } else {
                let p = mesh[0].position;
                let expected = olayer_core::geodesy::lla_to_ecef(
                    &olayer_core::geodesy::LatLon {
                        lat: 1_f64.to_radians(),
                        lon: (-2_f64).to_radians(),
                        height: 500.0,
                    },
                    &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84(),
                );
                assert!((f64::from(p[0]) - expected.x).abs() < 1.0);
            }
        }
        assert_eq!(urls.lock().unwrap().len(), 1);
        assert!(urls.lock().unwrap()[0].contains("FORMAT=application%2Fjson"));
        assert!(controller.terrain.unload_elevation_grid("wcs"));
        assert!(controller.terrain.get_elevation_status(0.0, 0.0).is_err());
    }

    #[test]
    fn nodata_survives_decoder_and_invalid_replacement_is_atomic() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let bounds = BoundingBox::new(-1.0, -1.0, 1.0, 1.0, crs).unwrap();
        let coverage = crate::decode_wcs_json_coverage(
            br#"{"width":2,"height":2,"samples":[100,200,300,-9999],"nodata":-9999}"#,
            bounds,
            CoverageSampleType::Float64,
            None,
        )
        .unwrap();
        let engine = TerrainEngine::new();
        install_wcs_terrain(&engine, "wcs", coverage.clone()).unwrap();
        assert_eq!(
            engine
                .get_elevation_status(0.0, 0.0)
                .unwrap()
                .elevation_meters,
            None
        );
        assert_eq!(
            engine
                .get_elevation_status(1_f64.to_radians(), (-1_f64).to_radians())
                .unwrap()
                .elevation_meters,
            Some(100.0)
        );
        let mut bad = coverage;
        bad.samples[0] = f64::NAN;
        assert!(install_wcs_terrain(&engine, "wcs", bad).is_err());
        assert_eq!(
            engine
                .get_elevation_status(1_f64.to_radians(), (-1_f64).to_radians())
                .unwrap()
                .elevation_meters,
            Some(100.0)
        );
        engine.clear_all();
        assert!(engine.get_elevation_status(0.0, 0.0).is_err());
    }

    fn numeric_coverage(
        samples: Vec<f64>,
        width: u32,
        height: u32,
        nodata: Option<f64>,
    ) -> Coverage {
        Coverage {
            bounds: BoundingBox::new(
                -2.0,
                -1.0,
                2.0,
                1.0,
                CoordinateReferenceSystem::epsg(4326).unwrap(),
            )
            .unwrap(),
            width,
            height,
            sample_type: CoverageSampleType::Float64,
            nodata,
            samples: samples.into_boxed_slice(),
        }
    }

    struct ConstantGeoid(Option<f64>);
    impl GeoidUndulationProvider for ConstantGeoid {
        fn undulation_meters(
            &self,
            _x: f64,
            _y: f64,
            crs: CoverageCoordinateSystem,
        ) -> Result<Option<f64>, NativeMapDataError> {
            assert_eq!(crs, CoverageCoordinateSystem::GeographicDegrees);
            Ok(self.0)
        }
    }

    #[test]
    fn cell_center_registration_moves_node_extent_half_a_pixel() {
        let engine = TerrainEngine::new();
        let coverage = numeric_coverage((0..9).map(f64::from).collect(), 3, 3, None);
        install_wcs_terrain_with_interpretation(
            &engine,
            "cell-center",
            coverage,
            CoverageTerrainInterpretation {
                registration: CoverageGridRegistration::CellCenters,
                ..CoverageTerrainInterpretation::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(
            engine
                .get_elevation_status(0.0, (-1.0_f64).to_radians())
                .unwrap()
                .elevation_meters,
            Some(3.25)
        );
        assert!(engine
            .get_elevation_status(0.0, (-1.9_f64).to_radians())
            .is_err());
    }

    #[test]
    fn unit_and_vertical_conversions_are_explicit_and_nodata_stays_unknown() {
        let engine = TerrainEngine::new();
        let coverage = numeric_coverage(
            vec![
                100.0, 100.0, 100.0, 100.0, -9999.0, 100.0, 100.0, 100.0, 100.0,
            ],
            3,
            3,
            Some(-9999.0),
        );
        install_wcs_terrain_with_interpretation(
            &engine,
            "converted",
            coverage,
            CoverageTerrainInterpretation {
                unit: ElevationUnit::InternationalFeet,
                vertical_reference: VerticalReference::Orthometric,
                ..CoverageTerrainInterpretation::default()
            },
            Some(&ConstantGeoid(Some(30.0))),
        )
        .unwrap();
        assert_eq!(
            engine
                .get_elevation_status(0.0, 0.0)
                .unwrap()
                .elevation_meters,
            None
        );
        assert!(
            (engine
                .get_elevation_status(0.0, (-2.0_f64).to_radians())
                .unwrap()
                .elevation_meters
                .unwrap()
                - 60.48)
                .abs()
                < 1.0e-10
        );
    }

    #[test]
    fn missing_geoid_data_rejects_coverage_without_replacing_installed_grid() {
        let engine = TerrainEngine::new();
        install_wcs_terrain(&engine, "atomic", numeric_coverage(vec![25.0], 1, 1, None)).unwrap();
        let result = install_wcs_terrain_with_interpretation(
            &engine,
            "atomic",
            numeric_coverage(vec![100.0], 1, 1, None),
            CoverageTerrainInterpretation {
                vertical_reference: VerticalReference::Orthometric,
                ..CoverageTerrainInterpretation::default()
            },
            Some(&ConstantGeoid(None)),
        );
        assert!(result.is_err());
        assert_eq!(
            engine
                .get_elevation_status(0.0, 0.0)
                .unwrap()
                .elevation_meters,
            Some(25.0)
        );
    }
}
