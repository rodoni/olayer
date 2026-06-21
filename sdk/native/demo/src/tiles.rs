use std::collections::HashSet;
use olayer_native::NativeController;

/// Converts geodetic coordinates to OpenStreetMap / Web Mercator tile indices.
pub fn latlon_to_tile(lat_rad: f64, lon_rad: f64, zoom: u32) -> (u32, u32) {
    let lon_deg = lon_rad.to_degrees();
    let n = 2.0f64.powi(zoom as i32);

    let x = (((lon_deg + 180.0) / 360.0) * n).clamp(0.0, n - 1.0) as u32;

    let sec = 1.0 / lat_rad.cos();
    let tan = lat_rad.tan();
    let y_val = (1.0 - ((tan + sec).abs().ln() / std::f64::consts::PI)) / 2.0;
    let y = (y_val * n).clamp(0.0, n - 1.0) as u32;

    (x, y)
}

/// Computes the set of WMTS tile keys visible in the current viewport.
pub fn compute_visible_tile_keys(controller: &NativeController, tile_zoom: u32) -> HashSet<String> {
    let mut visible_keys = HashSet::new();
    let center_lat = controller.camera.center.lat;
    let center_lon = controller.camera.center.lon;
    let (tx, ty) = latlon_to_tile(center_lat, center_lon, tile_zoom);

    let w_meters = controller.camera.viewport_base_meters / controller.camera.zoom;
    let h_meters = w_meters / controller.camera.aspect_ratio;

    if controller.view_mode != "3D" {
        if let Ok(cx_cy) = controller.projection.project(&controller.camera.center) {
            let cx = cx_cy.0;
            let cy = cx_cy.1;

            let offsets = [
                (0.0, 0.0),
                (-0.5, -0.5),
                (-0.5, 0.5),
                (0.5, -0.5),
                (0.5, 0.5),
                (-0.5, 0.0),
                (0.5, 0.0),
                (0.0, -0.5),
                (0.0, 0.5),
            ];

            let mut min_x = u32::MAX;
            let mut max_x = 0;
            let mut min_y = u32::MAX;
            let mut max_y = 0;

            let cos_r = controller.camera.rotation.cos();
            let sin_r = controller.camera.rotation.sin();

            for &(ox, oy) in &offsets {
                let dx_cam = ox * w_meters;
                let dy_cam = oy * h_meters;

                let dx = dx_cam * cos_r - dy_cam * sin_r;
                let dy = dx_cam * sin_r + dy_cam * cos_r;

                let px = cx + dx;
                let py = cy + dy;

                if let Ok(lla) = controller.projection.unproject(px, py) {
                    let (tx_val, ty_val) = latlon_to_tile(lla.lat, lla.lon, tile_zoom);
                    min_x = min_x.min(tx_val);
                    max_x = max_x.max(tx_val);
                    min_y = min_y.min(ty_val);
                    max_y = max_y.max(ty_val);
                }
            }

            let max_radius = 8;
            let final_min_x = min_x.max(tx.saturating_sub(max_radius));
            let final_max_x = max_x.min(tx + max_radius);
            let final_min_y = min_y.max(ty.saturating_sub(max_radius));
            let final_max_y = max_y.min(ty + max_radius);

            for x_idx in final_min_x..=final_max_x {
                for y_idx in final_min_y..=final_max_y {
                    visible_keys.insert(format!("{}/{}/{}", tile_zoom, x_idx, y_idx));
                }
            }
        }
    } else {
        let span_lat = (h_meters / 111000.0).min(90.0);
        let lat_rad = controller.camera.center.lat;
        let span_lon = (w_meters / (111000.0 * lat_rad.cos().abs().max(0.01))).min(180.0);

        let lat_min = (lat_rad.to_degrees() - span_lat).to_radians();
        let lat_max = (lat_rad.to_degrees() + span_lat).to_radians();
        let lon_min = (controller.camera.center.lon.to_degrees() - span_lon).to_radians();
        let lon_max = (controller.camera.center.lon.to_degrees() + span_lon).to_radians();

        let t_min = latlon_to_tile(lat_min, lon_min, tile_zoom);
        let t_max = latlon_to_tile(lat_max, lon_max, tile_zoom);

        let min_x = t_min.0.min(t_max.0);
        let max_x = t_min.0.max(t_max.0);
        let min_y = t_min.1.min(t_max.1);
        let max_y = t_min.1.max(t_max.1);

        let max_radius = 8;
        let final_min_x = min_x.max(tx.saturating_sub(max_radius));
        let final_max_x = max_x.min(tx + max_radius);
        let final_min_y = min_y.max(ty.saturating_sub(max_radius));
        let final_max_y = max_y.min(ty + max_radius);

        for x_idx in final_min_x..=final_max_x {
            for y_idx in final_min_y..=final_max_y {
                visible_keys.insert(format!("{}/{}/{}", tile_zoom, x_idx, y_idx));
            }
        }
    }

    if visible_keys.is_empty() {
        visible_keys.insert(format!("{}/{}/{}", tile_zoom, tx, ty));
    }

    visible_keys
}

/// Returns the ideal WMTS zoom level for the current viewport width.
pub fn ideal_tile_zoom(controller: &NativeController) -> u32 {
    let w_meters = controller.camera.viewport_base_meters / controller.camera.zoom;
    let c_earth = 40_075_016.0;
    let target_tiles = 6.0;
    let z_ideal = ((target_tiles * c_earth) / w_meters).log2();
    (z_ideal.round() as i32).clamp(0, 18) as u32
}
