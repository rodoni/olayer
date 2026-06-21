#![allow(clippy::too_many_arguments)]

use std::sync::Arc;
use olayer_core::geodesy::LatLon;
use olayer_core::projections::{Projection, CameraState};
use usvg::TreeParsing;
use crate::native_controller::NativeController;

/// Struct responsible for CPU-side projections and plotting targets/billboards in egui.
///
/// The CPU/Vertex Pipeline calculates screen pixel coordinates $(X, Y)$ from
/// geodetic coordinates (latitude, longitude, altitude) of dynamic radar targets.
/// It draws them without 3D perspective distortion (**Billboard** effect), and
/// manages heading vectors, tactical data labels, and the 2.5D flight profile.
#[derive(Default)]
pub struct WgpuCpuVertexPipeline {}

impl WgpuCpuVertexPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Plots the interpolated target list on the given egui painter.
    #[inline]
    pub fn draw_targets(
        &self,
        painter: &egui::Painter,
        targets: &[olayer_core::interpolator::InterpolatedTarget],
        selected_target_id: &Option<Arc<str>>,
        controller: &NativeController,
        view_proj_matrix: &[f32; 16],
        width: u32,
        height: u32,
        simulated_speeds: &std::collections::HashMap<Arc<str>, f64>,
    ) {
        for t in targets {
            let speed_mps = simulated_speeds.get(t.id.as_ref()).copied().unwrap_or(0.0);
            if let Some(pos) = project_lla_to_screen(
                t.position.lat,
                t.position.lon,
                t.position.height,
                &controller.view_mode,
                &controller.camera,
                controller.projection.as_ref(),
                view_proj_matrix,
                width,
                height,
            ) {
                // Draw target dot
                let color = if selected_target_id.as_ref() == Some(&t.id) {
                    egui::Color32::from_rgb(0, 176, 255)
                } else {
                    egui::Color32::from_rgb(0, 230, 118)
                };
                painter.circle_filled(pos, 4.0, color);
                painter.rect_stroke(
                    egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0)),
                    0.0,
                    egui::Stroke::new(1.0, color),
                );

                // Draw heading vector (1-min projection vector)
                let r_earth = 6378137.0;
                let vector_time = 60.0;
                let lat_offset = (speed_mps * vector_time * t.heading_rad.cos()) / r_earth;
                let lon_offset = (speed_mps * vector_time * t.heading_rad.sin()) / (r_earth * t.position.lat.cos());
                if let Some(end_pos) = project_lla_to_screen(
                    t.position.lat + lat_offset,
                    t.position.lon + lon_offset,
                    t.position.height,
                    &controller.view_mode,
                    &controller.camera,
                    controller.projection.as_ref(),
                    view_proj_matrix,
                    width,
                    height,
                ) {
                    painter.line_segment([pos, end_pos], egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 176, 255)));
                }

                // Target data blocks (labels)
                let altitude_feet = (t.position.height * 3.28084) as i32;
                let fl = altitude_feet / 100;
                let speed_knots = (speed_mps * 1.94384) as i32;
                let label_text = format!("{}\nFL{:03} {}KT", t.id, fl, speed_knots);

                painter.text(
                    pos + egui::vec2(12.0, -20.0),
                    egui::Align2::LEFT_CENTER,
                    label_text,
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
        }
    }
}

/// Projects WGS84 LLA coordinates to planar/3D screen coordinates.
///
/// * **3D** — Converts to ECEF, applies horizon occlusion culling, then multiplies
///   by the 3D View-Projection matrix.
/// * **2.5D** — Projects the base using the active planar projection, adds altitude as
///   Z, then multiplies by the 2.5D perspective matrix.
/// * **2D** — Projects using the active map projection (Stereographic, LCC, Mercator),
///   rotates and scales by camera bearing/zoom.
#[inline]
pub fn project_lla_to_screen(
    lat: f64,
    lon: f64,
    alt: f64,
    view_mode: &str,
    camera: &CameraState,
    projection: &dyn Projection,
    view_proj_matrix: &[f32; 16],
    width: u32,
    height: u32,
) -> Option<egui::Pos2> {
    if view_mode == "3D" {
        let xyz = olayer_core::geodesy::lla_to_ecef(&LatLon::new(lat, lon, alt), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
        let x = xyz.x as f32;
        let y = xyz.y as f32;
        let z = xyz.z as f32;

        // Horizon occlusion culling
        let r_earth = 6378137.0f64;
        let base_dist = 15000000.0f64;
        let dist = r_earth + (base_dist / camera.zoom);
        let cam_xyz = olayer_core::geodesy::lla_to_ecef(&LatLon::new(camera.center.lat, camera.center.lon, dist - r_earth), &olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84());
        let dot = cam_xyz.x * xyz.x + cam_xyz.y * xyz.y + cam_xyz.z * xyz.z;
        if dot < r_earth * r_earth {
            return None;
        }

        let m = view_proj_matrix;
        let w_ndc = m[3] * x + m[7] * y + m[11] * z + m[15];
        if w_ndc <= 0.0 {
            return None;
        }
        let x_ndc = (m[0] * x + m[4] * y + m[8] * z + m[12]) / w_ndc;
        let y_ndc = (m[1] * x + m[5] * y + m[9] * z + m[13]) / w_ndc;
        Some(egui::pos2(
            (x_ndc + 1.0) * 0.5 * width as f32,
            (1.0 - y_ndc) * 0.5 * height as f32,
        ))
    } else if view_mode == "2.5D" {
        if let Ok(xy) = projection.project(&LatLon::new(lat, lon, 0.0)) {
            let x = xy.0 as f32;
            let y = xy.1 as f32;
            let z = alt as f32;
            let m = view_proj_matrix;
            let w_ndc = m[3] * x + m[7] * y + m[11] * z + m[15];
            if w_ndc <= 0.0 {
                return None;
            }
            let x_ndc = (m[0] * x + m[4] * y + m[8] * z + m[12]) / w_ndc;
            let y_ndc = (m[1] * x + m[5] * y + m[9] * z + m[13]) / w_ndc;
            Some(egui::pos2(
                (x_ndc + 1.0) * 0.5 * width as f32,
                (1.0 - y_ndc) * 0.5 * height as f32,
            ))
        } else {
            None
        }
    } else if let Ok(xy) = projection.project(&LatLon::new(lat, lon, alt)) {
        let cx_cy = projection.project(&camera.center).unwrap_or((0.0, 0.0));
        let tx = xy.0 - cx_cy.0;
        let ty = xy.1 - cx_cy.1;
        let rx = tx * (-camera.rotation).cos() - ty * (-camera.rotation).sin();
        let ry = tx * (-camera.rotation).sin() + ty * (-camera.rotation).cos();
        let w_meters = (camera.viewport_base_meters / camera.zoom) as f32;
        let aspect = camera.aspect_ratio as f32;
        let h_meters = w_meters / aspect;
        let ndc_x = rx as f32 / (w_meters / 2.0);
        let ndc_y = ry as f32 / (h_meters / 2.0);
        Some(egui::pos2(
            (ndc_x + 1.0) * 0.5 * width as f32,
            (1.0 - ndc_y) * 0.5 * height as f32,
        ))
    } else {
        None
    }
}

/// Rasterizes SVG symbol data using resvg into a raw RGBA buffer.
pub fn rasterize_svg(svg_data: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg_data, &opt)
        .map_err(|e| format!("Failed to parse SVG: {:?}", e))?;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or("Failed to allocate Pixmap")?;

    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    Ok(pixmap.take())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use olayer_core::projections::Stereographic;
    use olayer_core::geodesy::ellipsoid::Ellipsoid;

    #[test]
    fn test_project_lla_to_screen_2d_center() {
        let projection = Stereographic::new(0.0, 0.0, Ellipsoid::wgs84());
        let camera = CameraState::with_attitude(
            LatLon::new(0.0, 0.0, 0.0),
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            100000.0,
        );
        let vp = camera.get_2d_view_proj_matrix(&projection).unwrap();

        let pos = project_lla_to_screen(
            0.0, 0.0, 0.0,
            "2D",
            &camera,
            &projection,
            &vp,
            800, 600,
        );
        assert!(pos.is_some());
        let p = pos.unwrap();
        // Center of screen at 800x600 with no rotation should be roughly (400, 300)
        assert!((p.x - 400.0).abs() < 1.0);
        assert!((p.y - 300.0).abs() < 1.0);
    }

    #[test]
    fn test_project_lla_to_screen_2d_north_offset() {
        let projection = Stereographic::new(0.0, 0.0, Ellipsoid::wgs84());
        let camera = CameraState::with_attitude(
            LatLon::new(0.0, 0.0, 0.0),
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            100000.0,
        );
        let vp = camera.get_2d_view_proj_matrix(&projection).unwrap();

        // A point slightly north of center should be above center on screen
        let pos = project_lla_to_screen(
            0.01, 0.0, 0.0,
            "2D",
            &camera,
            &projection,
            &vp,
            800, 600,
        );
        assert!(pos.is_some());
        let p = pos.unwrap();
        // Screen Y goes down, so north is smaller Y
        assert!(p.y < 300.0);
    }

    #[test]
    fn test_project_lla_to_screen_2d_east_offset() {
        let projection = Stereographic::new(0.0, 0.0, Ellipsoid::wgs84());
        let camera = CameraState::with_attitude(
            LatLon::new(0.0, 0.0, 0.0),
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            100000.0,
        );
        let vp = camera.get_2d_view_proj_matrix(&projection).unwrap();

        // A point slightly east of center should be to the right on screen
        let pos = project_lla_to_screen(
            0.0, 0.01, 0.0,
            "2D",
            &camera,
            &projection,
            &vp,
            800, 600,
        );
        assert!(pos.is_some());
        let p = pos.unwrap();
        assert!(p.x > 400.0);
    }

    #[test]
    fn test_project_lla_to_screen_2d_rotation() {
        let projection = Stereographic::new(0.0, 0.0, Ellipsoid::wgs84());
        let camera = CameraState::with_attitude(
            LatLon::new(0.0, 0.0, 0.0),
            1.0,
            std::f64::consts::PI / 2.0, // 90-degree rotation
            0.0,
            0.0,
            1.0,
            100000.0,
        );
        let vp = camera.get_2d_view_proj_matrix(&projection).unwrap();

        // With 90° rotation, a point north of center should appear to the right
        let pos = project_lla_to_screen(
            0.01, 0.0, 0.0,
            "2D",
            &camera,
            &projection,
            &vp,
            800, 600,
        );
        assert!(pos.is_some());
        let p = pos.unwrap();
        assert!(p.x > 400.0);
    }

    #[test]
    fn test_project_lla_to_screen_singularity() {
        let projection = Stereographic::new(0.0, 0.0, Ellipsoid::wgs84());
        let camera = CameraState::with_attitude(
            LatLon::new(0.0, 0.0, 0.0),
            1.0,
            0.0,
            0.0,
            0.0,
            1.0,
            100000.0,
        );
        let vp = camera.get_2d_view_proj_matrix(&projection).unwrap();

        // The antipode (0, 180°) is a singularity for Stereographic centered at (0, 0)
        let pos = project_lla_to_screen(
            0.0, std::f64::consts::PI, 0.0,
            "2D",
            &camera,
            &projection,
            &vp,
            800, 600,
        );
        assert!(pos.is_none());
    }

    #[test]
    fn test_rasterize_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="red"/></svg>"#;
        let result = rasterize_svg(svg, 64, 64);
        assert!(result.is_ok());
        let pixels = result.unwrap();
        // 64 x 64 x 4 bytes (RGBA)
        assert_eq!(pixels.len(), 64 * 64 * 4);
    }
}
