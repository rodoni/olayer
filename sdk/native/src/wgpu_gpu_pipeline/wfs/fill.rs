use super::{invalid, paths, WfsMeshOptions, WfsRenderError, WfsTransform, MAX_VERTICES};
use crate::NativeController;
use olayer_map_core::FeatureCollection;

/// Packed triangle list for validated polygon interiors, preserving holes and nodata.
pub fn build_wfs_fill_mesh(
    collection: &FeatureCollection,
    controller: &NativeController,
    options: WfsMeshOptions,
) -> Result<Vec<[f32; 3]>, WfsRenderError> {
    WfsTransform::new(controller, options)?;
    let triangles = prepare_wfs_fill(collection, options)?;
    build_prepared_fill(&triangles, controller, options)
}

pub(super) fn build_prepared_fill(
    triangles: &[[f64; 3]],
    controller: &NativeController,
    options: WfsMeshOptions,
) -> Result<Vec<[f32; 3]>, WfsRenderError> {
    let transform = WfsTransform::new(controller, options)?;
    if !options.fill_polygons {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    let mut work = 0;
    let mut emit = |triangle: [[f64; 3]; 3]| -> Result<(), WfsRenderError> {
        work += 3;
        if work > MAX_VERTICES {
            return Err(invalid("WFS fill work limit exceeded"));
        }
        let mut vertices = Vec::new();
        for p in triangle {
            let Some(p) = transform.project(p)? else {
                return Ok(());
            };
            vertices.push((p, transform.visibility(p)));
        }
        let mut clipped = Vec::new();
        for i in 0..3 {
            let (a, va) = vertices[i];
            let (b, vb) = vertices[(i + 1) % 3];
            if va >= 0.0 {
                clipped.push(a);
            }
            if (va >= 0.0) != (vb >= 0.0) {
                let t = va / (va - vb);
                clipped.push(std::array::from_fn(|k| a[k] + t * (b[k] - a[k])));
            }
        }
        for i in 1..clipped.len().saturating_sub(1) {
            if output.len() + 3 > MAX_VERTICES {
                return Err(invalid("WFS fill vertex limit exceeded"));
            }
            for p in [clipped[0], clipped[i], clipped[i + 1]] {
                let p = p.map(|v| v as f32);
                if !p.iter().all(|v| v.is_finite()) {
                    return Err(invalid("WFS fill exceeds GPU precision"));
                }
                output.push(p);
            }
        }
        Ok(())
    };
    let n = options.subdivisions;
    for triangle in triangles.as_chunks::<3>().0 {
        let [a, b, c] = *triangle;
        let point = |i: u32, j: u32| {
            std::array::from_fn(|k| {
                a[k] + (b[k] - a[k]) * f64::from(i) / f64::from(n)
                    + (c[k] - a[k]) * f64::from(j) / f64::from(n)
            })
        };
        for i in 0..n {
            for j in 0..n - i {
                emit([point(i, j), point(i + 1, j), point(i, j + 1)])?;
                if i + j + 1 < n {
                    emit([point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)])?;
                }
            }
        }
    }
    Ok(output)
}

pub(super) fn prepare_wfs_fill(
    collection: &FeatureCollection,
    options: WfsMeshOptions,
) -> Result<Vec<[f64; 3]>, WfsRenderError> {
    if !options.fill_polygons {
        return Ok(Vec::new());
    }
    let mut prepared = Vec::new();
    for rings in paths(collection)?.polygons {
        let lengths: Vec<_> = rings.iter().map(Vec::len).collect();
        let points: Vec<_> = rings.into_iter().flatten().collect();
        let triangles = olayer_core::volumetric::triangulate_polygon_rings(&points, &lengths)
            .map_err(|e| WfsRenderError(e.to_string()))?;
        if (prepared.len() + triangles.len())
            .checked_mul(options.subdivisions as usize * options.subdivisions as usize)
            .is_none_or(|work| work > MAX_VERTICES)
        {
            return Err(invalid("WFS fill work limit exceeded"));
        }
        prepared.extend(triangles);
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_wcs_json_coverage, decode_wfs_feature_collection, install_wcs_terrain};
    use olayer_map_core::{BoundingBox, CoordinateReferenceSystem, CoverageSampleType};
    #[test]
    fn concavities_holes_and_altitude_match_browser_fixture() {
        let crs = CoordinateReferenceSystem::epsg(4326).unwrap();
        let collection = decode_wfs_feature_collection(
            include_bytes!("../../../../../tests/fixtures/wfs-fill/features.json"),
            crs,
        )
        .unwrap();
        let mut c = NativeController::new(0.0, 0.0).unwrap();
        let options = WfsMeshOptions {
            fill_polygons: true,
            subdivisions: 1,
            ..Default::default()
        };
        let mesh = build_wfs_fill_mesh(&collection, &c, options).unwrap();
        assert!(!mesh.is_empty());
        assert!(mesh.iter().all(|p| p[2] == 0.0));
        assert!(build_wfs_fill_mesh(&collection, &c, Default::default())
            .unwrap()
            .is_empty());
        let bounds = BoundingBox::new(-2.0, -1.0, 2.0, 1.0, crs).unwrap();
        install_wcs_terrain(
            &c.terrain,
            "wcs",
            decode_wcs_json_coverage(
                include_bytes!("../../../../../tests/fixtures/wcs/elevation.json"),
                bounds,
                CoverageSampleType::Float64,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        c.view_mode = "2.5D".into();
        let options = WfsMeshOptions {
            altitude_mode: super::super::WfsAltitudeMode::RelativeToGround,
            ..options
        };
        assert!(build_wfs_fill_mesh(&collection, &c, options)
            .unwrap()
            .iter()
            .all(|p| (p[2] - 750.0).abs() < 0.01));
        c.terrain.unload_elevation_grid("wcs");
        assert!(build_wfs_fill_mesh(&collection, &c, options)
            .unwrap()
            .is_empty());
        c.view_mode = "3D".into();
        let far=decode_wfs_feature_collection(br#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[[[170,0],[175,0],[175,1],[170,0]]]}}]}"#,crs).unwrap();
        assert!(build_wfs_fill_mesh(
            &far,
            &c,
            WfsMeshOptions {
                fill_polygons: true,
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
    }
}
