use std::sync::Arc;
use egui::Ui;
use olayer_core::interpolator::InterpolatedTarget;
use olayer_core::geodesy::LatLon;
use olayer_native::NativeController;

/// Draws the 2.5D flight profile panel for the selected target.
pub fn draw_flight_profile(
    ui: &mut Ui,
    controller: &mut NativeController,
    target: &InterpolatedTarget,
    selected_id: &Arc<str>,
) {
    ui.heading(format!("✈️ 2.5D Flight Profile: {}", selected_id));

    // Fetch vertical profile from the terrain engine
    let mut route_coords = Vec::new();
    let r_earth = 6378137.0;
    let step = 2000.0;
    for dist in (-30000..=50000).step_by(2000) {
        let lat_offset = (dist as f64 * target.heading_rad.cos()) / r_earth;
        let lon_offset = (dist as f64 * target.heading_rad.sin()) / (r_earth * target.position.lat.cos());
        route_coords.push(LatLon::new(target.position.lat + lat_offset, target.position.lon + lon_offset, target.position.height));
    }

    if let Ok(profile) = controller.terrain.get_vertical_profile(&route_coords, step) {
        let response = ui.allocate_response(egui::vec2(ui.available_width(), 100.0), egui::Sense::hover());
        let rect = response.rect;
        let p = ui.painter();

        // Draw background
        p.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(80));

        let max_elev = profile.iter().map(|pt| pt.ground_elevation).fold(1000.0f64, |m, val| m.max(val)).max(target.position.height) * 1.2;
        let max_dist = 80000.0f64;

        let get_x = |d: f64| rect.min.x + 20.0 + (d / max_dist) as f32 * (rect.width() - 40.0);
        let get_y = |h: f64| rect.max.y - 15.0 - (h / max_elev) as f32 * (rect.height() - 25.0);

        // Draw ground profile
        let mut points = Vec::new();
        points.push(egui::pos2(get_x(0.0), get_y(0.0)));
        for pt in &profile {
            points.push(egui::pos2(get_x(pt.distance_meters), get_y(pt.ground_elevation)));
        }
        points.push(egui::pos2(get_x(max_dist), get_y(0.0)));
        p.add(egui::Shape::convex_polygon(points, egui::Color32::from_rgba_unmultiplied(141, 110, 99, 100), egui::Stroke::new(1.5, egui::Color32::from_rgb(141, 110, 99))));

        // Draw aircraft line
        let ac_y = get_y(target.position.height);
        p.line_segment(
            [egui::pos2(get_x(0.0), ac_y), egui::pos2(get_x(max_dist), ac_y)],
            egui::Stroke::new(2.0, egui::Color32::from_rgba_unmultiplied(0, 176, 255, 128)),
        );

        // Draw aircraft dot (represented at 30km distance)
        let ac_x = get_x(30000.0);
        p.circle_filled(egui::pos2(ac_x, ac_y), 5.0, egui::Color32::from_rgb(0, 176, 255));

        // Altimetry clearance check
        let ground_under_ac = controller.terrain.get_elevation(target.position.lat.to_degrees(), target.position.lon.to_degrees()).unwrap_or(0.0);
        let clearance = target.position.height - ground_under_ac;
        let hazard = clearance < 300.0;
        let clearance_color = if hazard { egui::Color32::from_rgb(255, 23, 68) } else { egui::Color32::from_rgb(0, 230, 118) };

        // Draw clearance line
        p.line_segment(
            [egui::pos2(ac_x, ac_y), egui::pos2(ac_x, get_y(ground_under_ac))],
            egui::Stroke::new(1.5, clearance_color),
        );

        p.text(
            egui::pos2(ac_x + 10.0, (ac_y + get_y(ground_under_ac)) / 2.0),
            egui::Align2::LEFT_CENTER,
            format!("CLEARANCE: {} FT{}", (clearance * 3.28084) as i32, if hazard { " ⚠️ CFIT HAZARD!" } else { " OK" }),
            egui::FontId::proportional(10.0),
            clearance_color,
        );
    }
}
