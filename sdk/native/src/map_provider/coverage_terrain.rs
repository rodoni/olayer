//! Adapts decoded WCS scalar coverages to the shared terrain engine.
use super::NativeMapDataError;
use olayer_core::terrain::{ElevationGrid, ElevationGridCrs, TerrainEngine};
use olayer_map_core::Coverage;

/// Installs a north-up node grid. Samples are ellipsoidal heights in metres.
/// Callers must explicitly select an elevation coverage with this registration/datum.
pub fn install_wcs_terrain(
    engine: &TerrainEngine,
    id: &str,
    coverage: Coverage,
) -> Result<(), NativeMapDataError> {
    let coverage = coverage
        .validate()
        .map_err(|e| NativeMapDataError::OgcDecode(e.to_string()))?;
    let b = coverage.bounds;
    let mut bounds = [b.min_x, b.min_y, b.max_x, b.max_y];
    let crs = match b.crs.epsg_code() {
        4326 => {
            bounds = bounds.map(f64::to_radians);
            ElevationGridCrs::Geographic
        }
        3857 | 900913 => ElevationGridCrs::WebMercator,
        _ => {
            return Err(NativeMapDataError::OgcDecode(
                "unsupported WCS terrain CRS".into(),
            ))
        }
    };
    let grid = ElevationGrid::new(
        crs,
        bounds,
        coverage.width as usize,
        coverage.height as usize,
        coverage.samples,
        coverage.nodata,
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
}
