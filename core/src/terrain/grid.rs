use super::TerrainError;

/// Coordinate system of an elevation grid; geographic bounds use radians.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationGridCrs {
    Geographic,
    WebMercator,
}

/// North-to-south row-major node grid. The outer nodes lie on the bounds.
/// Heights are metres above the ellipsoid; no vertical datum conversion is implied.
#[derive(Debug, Clone)]
pub struct ElevationGrid {
    crs: ElevationGridCrs,
    bounds: [f64; 4],
    width: usize,
    height: usize,
    samples: Box<[f64]>,
    nodata: Option<f64>,
}

impl ElevationGrid {
    /// Validates a decoded scalar grid before it can enter the terrain cache.
    pub fn new(
        crs: ElevationGridCrs,
        bounds: [f64; 4],
        width: usize,
        height: usize,
        samples: Box<[f64]>,
        nodata: Option<f64>,
    ) -> Result<Self, TerrainError> {
        let (x_limit, y_limit) = match crs {
            ElevationGridCrs::Geographic => (std::f64::consts::PI, std::f64::consts::FRAC_PI_2),
            ElevationGridCrs::WebMercator => (
                std::f64::consts::PI * 6_378_137.0,
                std::f64::consts::PI * 6_378_137.0,
            ),
        };
        if !bounds.iter().all(|v| v.is_finite())
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[0] < -x_limit
            || bounds[2] > x_limit
            || bounds[1] < -y_limit
            || bounds[3] > y_limit
            || width == 0
            || height == 0
            || width
                .checked_mul(height)
                .is_none_or(|n| n > 16_777_216 || n != samples.len())
            || samples.iter().any(|v| !v.is_finite())
            || nodata.is_some_and(|v| !v.is_finite())
        {
            return Err(TerrainError::InvalidInput(
                "invalid elevation grid bounds, dimensions or samples".into(),
            ));
        }
        Ok(Self {
            crs,
            bounds,
            width,
            height,
            samples,
            nodata,
        })
    }

    /// `None` means outside; `Some(None)` preserves an unknown contributing sample.
    pub fn sample(&self, lat: f64, lon: f64) -> Option<Option<f64>> {
        if !lat.is_finite()
            || !lon.is_finite()
            || lat.abs() > std::f64::consts::FRAC_PI_2
            || lon.abs() > std::f64::consts::PI
        {
            return None;
        }
        let (x, y) = match self.crs {
            ElevationGridCrs::Geographic => (lon, lat),
            ElevationGridCrs::WebMercator => (lon * 6_378_137.0, lat.tan().asinh() * 6_378_137.0),
        };
        let [west, south, east, north] = self.bounds;
        if x < west || x > east || y < south || y > north {
            return None;
        }
        let col = (x - west) / (east - west) * (self.width - 1) as f64;
        let row = (north - y) / (north - south) * (self.height - 1) as f64;
        let x0 = (col.floor() as usize).min(self.width - 1);
        let y0 = (row.floor() as usize).min(self.height - 1);
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = col - x0 as f64;
        let ty = row - y0 as f64;
        let mut height = 0.0;
        for (x, y, weight) in [
            (x0, y0, (1.0 - tx) * (1.0 - ty)),
            (x1, y0, tx * (1.0 - ty)),
            (x0, y1, (1.0 - tx) * ty),
            (x1, y1, tx * ty),
        ] {
            if weight == 0.0 {
                continue;
            }
            let value = self.samples[y * self.width + x];
            if self.nodata == Some(value) {
                return Some(None);
            }
            height += value * weight;
        }
        Some(Some(height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_bounds_cache_and_prefers_latest_grid_including_unknown_samples() {
        use super::super::TerrainEngine;
        let engine = TerrainEngine::with_capacity(1);
        let make = |height, nodata| {
            ElevationGrid::new(
                ElevationGridCrs::Geographic,
                [-0.1, -0.1, 0.1, 0.1],
                1,
                1,
                vec![height].into_boxed_slice(),
                nodata,
            )
            .unwrap()
        };
        engine.load_elevation_grid("a".into(), make(100.0, None));
        engine.load_elevation_grid("b".into(), make(-9999.0, Some(-9999.0)));
        assert!(!engine.unload_elevation_grid("a"));
        assert_eq!(
            engine
                .get_elevation_status(0.0, 0.0)
                .unwrap()
                .elevation_meters,
            None
        );
        engine.load_elevation_grid("b".into(), make(200.0, None));
        assert_eq!(
            engine
                .get_elevation_status(0.0, 0.0)
                .unwrap()
                .elevation_meters,
            Some(200.0)
        );
        engine.clear_all();
        assert!(engine.get_elevation_status(0.0, 0.0).is_err());
    }

    #[test]
    fn north_up_interpolation_preserves_only_contributing_nodata() {
        let grid = ElevationGrid::new(
            ElevationGridCrs::Geographic,
            [-0.1, -0.1, 0.1, 0.1],
            2,
            2,
            vec![100.0, 200.0, 300.0, -9999.0].into_boxed_slice(),
            Some(-9999.0),
        )
        .unwrap();
        assert_eq!(grid.sample(0.1, -0.1), Some(Some(100.0)));
        assert_eq!(grid.sample(0.1, 0.0), Some(Some(150.0)));
        assert_eq!(grid.sample(0.0, 0.0), Some(None));
        assert_eq!(grid.sample(0.2, 0.0), None);
    }
    #[test]
    fn mercator_interpolates_in_source_coordinates() {
        let grid = ElevationGrid::new(
            ElevationGridCrs::WebMercator,
            [-1000.0, -1000.0, 1000.0, 1000.0],
            2,
            2,
            vec![100.0, 200.0, 300.0, 400.0].into_boxed_slice(),
            None,
        )
        .unwrap();
        assert_eq!(grid.sample(0.0, 0.0), Some(Some(250.0)));
    }
    #[test]
    fn rejects_invalid_grids() {
        for (bounds, w, h, samples) in [
            ([0.0, 0.0, 0.0, 1.0], 1, 1, vec![0.0]),
            ([0.0, 0.0, 1.0, 1.0], usize::MAX, 2, vec![]),
            ([0.0, 0.0, 1.0, 1.0], 1, 1, vec![f64::NAN]),
        ] {
            assert!(ElevationGrid::new(
                ElevationGridCrs::Geographic,
                bounds,
                w,
                h,
                samples.into_boxed_slice(),
                None
            )
            .is_err());
        }
    }
}
