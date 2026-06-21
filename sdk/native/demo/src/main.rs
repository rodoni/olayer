use std::sync::Arc;
use winit::{
    event::{Event, WindowEvent, MouseButton, ElementState, MouseScrollDelta},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use olayer_core::geodesy::LatLon;
use olayer_core::projections::{Stereographic, LambertConformalConic, WebMercator};
use olayer_native::{
    NativeController, NativeLayerManager, NativeMapDataStack, MapDataSource, GeoserverWmtsSource, WgpuGpuPipeline, WgpuCpuVertexPipeline, project_lla_to_screen
};

struct SimulatedTarget {
    id: String,
    lat: f64,
    lon: f64,
    alt: f64,
    speed: f64,
    heading: f64,
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Olayer GIS ATC - Desktop Demo")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720))
            .build(&event_loop)
            .unwrap(),
    );

    // Setup wgpu - prefer DX12 on Windows to bypass Vulkan overlay hook issues
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..Default::default()
    });
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    })).unwrap();

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
        },
        None,
    )).unwrap();

    let size = window.inner_size();
    let mut config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
    if size.width > 0 && size.height > 0 {
        surface.configure(&device, &config);
    }

    // Setup egui
    let egui_ctx = egui::Context::default();
    let mut egui_state = egui_winit::State::new(
        egui_ctx.clone(),
        egui_ctx.viewport_id(),
        &window,
        None,
        None,
    );
    let mut egui_renderer = egui_wgpu::Renderer::new(&device, config.format, None, 1);

    // Initialize Native SDK Components
    let sp_lat = -23.62f64.to_radians();
    let sp_lon = -46.65f64.to_radians();
    let mut controller = NativeController::new(sp_lat, sp_lon);
    controller.camera.aspect_ratio = size.width as f64 / size.height as f64;
    controller.camera.viewport_base_meters = 1000000.0;

    let mut layer_manager = NativeLayerManager::new();
    let mut map_data_stack = NativeMapDataStack::new();
    let mut gpu_pipeline = WgpuGpuPipeline::new(&device, config.format);
    let cpu_pipeline = WgpuCpuVertexPipeline::new();

    // Load mock DTED Level 0 tiles for São Paulo TMA area using MapDataStack
    for lat in -25i32..=-22i32 {
        for lon in -48i32..=-45i32 {
            let lat_str = format!("{:02}0000{}", lat.abs(), if lat < 0 { "S" } else { "N" });
            let lon_str = format!("{:03}0000{}", lon.abs(), if lon < 0 { "W" } else { "E" });
            
            // Build mock DTED Level 0 binary tile
            let col_size = 11 + 100 * 2;
            let total_size = 3428 + 100 * col_size;
            let mut data = vec![32u8; total_size];
            data[0..4].copy_from_slice(b"UHL1");
            data[4..12].copy_from_slice(format!("{: <8}", lon_str).as_bytes());
            data[12..20].copy_from_slice(format!("{: <8}", lat_str).as_bytes());
            data[20..24].copy_from_slice(b"0300");
            data[24..28].copy_from_slice(b"0300");
            data[47..51].copy_from_slice(b"0100");
            data[51..55].copy_from_slice(b"0100");
            
            let mut offset = 3428;
            for c in 0..100 {
                data[offset] = 0xAA;
                let val_offset = offset + 7;
                for r in 0..100 {
                    let lat_fraction = r as f64 / 100.0;
                    let lon_fraction = c as f64 / 100.0;
                    let elevation = (500.0 + 400.0 * (lat_fraction * std::f64::consts::PI * 4.0).sin() * (lon_fraction * std::f64::consts::PI * 4.0).cos()) as i16;
                    let be = elevation.to_be_bytes();
                    let idx = val_offset + r * 2;
                    data[idx] = be[0];
                    data[idx + 1] = be[1];
                }
                offset += col_size;
            }
            
            let _ = map_data_stack.load_dted_buffer(&data, &mut controller.terrain);
        }
    }

    // App state
    let mut simulated_targets: Vec<SimulatedTarget> = Vec::new();
    let mut selected_target_id: Option<String> = None;
    let mut projection_name = "Stereographic".to_string();

    // GeoServer Integration State
    let mut geoserver_url = "http://localhost:8080/geoserver/gwc/service/wmts".to_string();
    let mut geoserver_layer = "olayer:world_map".to_string();
    let mut geoserver_source: Option<GeoserverWmtsSource> = None;
    let mut tile_zoom: u32 = 10;
    let mut auto_zoom = true;
    let mut auto_fetch_tiles = false;
    let mut egui_tile_textures: std::collections::HashMap<String, egui::TextureHandle> = std::collections::HashMap::new();

    // Helper to calculate OpenStreetMap / Web Mercator tile coordinates
    let latlon_to_tile = |lat_rad: f64, lon_rad: f64, zoom: u32| -> (u32, u32) {
        let lon_deg = lon_rad.to_degrees();
        let n = 2.0f64.powi(zoom as i32);
        
        let x = (((lon_deg + 180.0) / 360.0) * n).clamp(0.0, n - 1.0) as u32;
        
        let lat_rad_val = lat_rad;
        let sec = 1.0 / lat_rad_val.cos();
        let tan = lat_rad_val.tan();
        let y_val = (1.0 - ((tan + sec).abs().ln() / std::f64::consts::PI)) / 2.0;
        let y = (y_val * n).clamp(0.0, n - 1.0) as u32;
        
        (x, y)
    };
    
    // Mouse drag state
    let mut is_dragging = false;
    let mut last_mouse_pos = egui::Pos2::ZERO;
    let mut is_right_click_or_shift = false;

    // Initial grid rebuild
    gpu_pipeline.rebuild_grid_buffers(&controller, &device, &queue);

    // Dynamic radar update ticker
    let start_time = std::time::Instant::now();
    let mut last_radar_update = std::time::Instant::now();
    let mut frame_count = 0;
    let mut last_fps_calculation = std::time::Instant::now();
    let mut calculated_fps = 0.0;

    // Hook loop
    event_loop.run(move |event, window_target| {
        match event {
            Event::AboutToWait => {
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => {
                let r = egui_state.on_window_event(&window, &event);
                if r.consumed {
                    return;
                }

                match event {
                    WindowEvent::CloseRequested => window_target.exit(),
                    WindowEvent::Resized(new_size) => {
                        if new_size.width > 0 && new_size.height > 0 {
                            config.width = new_size.width;
                            config.height = new_size.height;
                            surface.configure(&device, &config);
                            controller.camera.aspect_ratio = new_size.width as f64 / new_size.height as f64;
                            controller.trigger_active();
                            window.request_redraw();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if config.width == 0 || config.height == 0 {
                            return;
                        }
                        // FPS meter
                        frame_count += 1;
                        let elapsed = last_fps_calculation.elapsed();
                        if elapsed.as_secs_f32() >= 0.5 {
                            calculated_fps = frame_count as f32 / elapsed.as_secs_f32();
                            frame_count = 0;
                            last_fps_calculation = std::time::Instant::now();
                        }

                        // Simulated radar updates at 1 Hz
                        if last_radar_update.elapsed().as_secs_f32() >= 1.0 {
                            let r_earth = 6378137.0;
                            let dt = 1.0;
                            for t in &mut simulated_targets {
                                let lat_offset = (t.speed * dt * t.heading.cos()) / r_earth;
                                let lon_offset = (t.speed * dt * t.heading.sin()) / (r_earth * t.lat.cos());
                                t.lat += lat_offset;
                                t.lon += lon_offset;
                                
                                let _ = controller.interpolator.update_target(olayer_core::interpolator::TargetState {
                                    id: t.id.clone(),
                                    last_position: LatLon::new(t.lat, t.lon, t.alt),
                                    speed_mps: t.speed,
                                    track_heading_rad: t.heading,
                                    vertical_rate_mps: 0.0,
                                    last_ping_time: start_time.elapsed().as_secs_f64(),
                                });
                            }
                            last_radar_update = std::time::Instant::now();
                            controller.trigger_active();
                        }

                        // Render Frame
                        let current_time = start_time.elapsed().as_secs_f64();
                        let interpolated_targets = controller.interpolator.interpolate_all(current_time).unwrap_or_default();

                        let aspect = controller.camera.aspect_ratio;
                        let w_meters = controller.camera.viewport_base_meters / controller.camera.zoom;
                        let h_meters = w_meters / aspect;

                        // Dynamically adjust tile zoom based on camera viewport width to keep the resolution appropriate
                        if auto_zoom {
                            let c_earth = 40_075_016.0;
                            // Target around 6 tiles across the screen width
                            let target_tiles = 6.0;
                            let z_ideal = ((target_tiles * c_earth) / w_meters).log2();
                            tile_zoom = (z_ideal.round() as i32).clamp(0, 18) as u32;
                        }

                        // Calculate visible tile keys in the viewport
                        let mut visible_keys = std::collections::HashSet::new();
                        let center_lat = controller.camera.center.lat;
                        let center_lon = controller.camera.center.lon;
                        let (tx, ty) = latlon_to_tile(center_lat, center_lon, tile_zoom);

                        if controller.view_mode != "3D" {
                            if let Ok(cx_cy) = controller.projection.project(&controller.camera.center) {
                                let cx = cx_cy.0;
                                let cy = cx_cy.1;

                                // Sample a grid of points on the screen/viewport to cover rotation and projection distortion
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

                                // Increase safety margin radius to 8 to cover wide screens / rotation without gaps
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

                            // Increase safety margin radius to 8 to cover wide screens / rotation without gaps
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

                        // Sync loaded tiles from GeoServer source to GPU textures
                        if let Some(ref source) = geoserver_source {
                            let keys = source.get_cached_keys();
                            for key in keys {
                                if visible_keys.contains(&key) {
                                    let parts: Vec<&str> = key.split('/').collect();
                                    if parts.len() == 3 {
                                        if let (Ok(z), Ok(x), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                                            if let Some(pixels) = source.get_tile_pixels(x, y, z) {
                                                gpu_pipeline.upload_raster_tile(&device, &queue, &key, &pixels, x, y, z, &controller);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Update View-Projection Matrix
                        let view_proj_matrix = if controller.view_mode == "3D" {
                            controller.camera.get_3d_view_proj_matrix().unwrap_or([0.0; 16])
                        } else if controller.view_mode == "2.5D" {
                            controller.camera.get_25d_view_proj_matrix(controller.projection.as_ref()).unwrap_or([0.0; 16])
                        } else {
                            controller.camera.get_2d_view_proj_matrix(controller.projection.as_ref()).unwrap_or([0.0; 16])
                        };

                        // Write uniform buffers
                        queue.write_buffer(&gpu_pipeline.uniform_buffer, 0, bytemuck::cast_slice(&view_proj_matrix));
                        // Grid color: Sleek Green (0.0, 0.8, 0.4, 0.8)
                        queue.write_buffer(&gpu_pipeline.uniform_buffer, 256, bytemuck::cast_slice(&[0.0f32, 0.8f32, 0.4f32, 0.8f32]));

                        // egui rendering context
                        let raw_input = egui_state.take_egui_input(&window);
                        egui_ctx.begin_frame(raw_input);

                        // HUD UI Panel
                        if layer_manager.show_hud {
                            egui::Window::new("🌍 Olayer Desktop SDK")
                                .default_width(340.0)
                                .resizable(false)
                                .show(&egui_ctx, |ui| {
                                    ui.label("Radar Hybrid Visualization Engine (Rust + WGPU)");
                                    ui.separator();

                                    ui.horizontal(|ui| {
                                        ui.label("Projection:");
                                        let old_proj = projection_name.clone();
                                        egui::ComboBox::from_id_source("proj_select")
                                            .selected_text(&projection_name)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut projection_name, "Stereographic".to_string(), "Stereographic Azimuthal (Radar)");
                                                ui.selectable_value(&mut projection_name, "LCC".to_string(), "Lambert Conformal Conic (En-Route)");
                                                ui.selectable_value(&mut projection_name, "Mercator".to_string(), "Web Mercator (Macro)");
                                                ui.selectable_value(&mut projection_name, "2.5D".to_string(), "2.5D Perspective Map");
                                                ui.selectable_value(&mut projection_name, "3D".to_string(), "3D Virtual Globe");
                                            });

                                        if projection_name != old_proj {
                                            if projection_name == "Stereographic" {
                                                controller.view_mode = "2D".to_string();
                                                controller.projection = Box::new(Stereographic::new(controller.camera.center.lat, controller.camera.center.lon, olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84()));
                                            } else if projection_name == "LCC" {
                                                controller.view_mode = "2D".to_string();
                                                controller.projection = Box::new(LambertConformalConic::new(-20.0f64.to_radians(), -25.0f64.to_radians(), controller.camera.center.lat, controller.camera.center.lon, olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84()));
                                            } else if projection_name == "Mercator" {
                                                controller.view_mode = "2D".to_string();
                                                controller.projection = Box::new(WebMercator::new(olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84()));
                                            } else if projection_name == "2.5D" {
                                                controller.view_mode = "2.5D".to_string();
                                                controller.projection = Box::new(WebMercator::new(olayer_core::geodesy::ellipsoid::Ellipsoid::wgs84()));
                                            } else if projection_name == "3D" {
                                                controller.view_mode = "3D".to_string();
                                            }
                                            gpu_pipeline.rebuild_grid_buffers(&controller, &device, &queue);
                                            gpu_pipeline.rebuild_raster_tile_buffers(&device, &controller);
                                            controller.trigger_active();
                                        }
                                    });

                                    ui.separator();
                                    ui.label("Layer Toggles:");
                                    ui.checkbox(&mut layer_manager.show_grid, "Show Geodetic Grid");
                                    ui.checkbox(&mut layer_manager.show_targets, "Show Radar Targets");
                                    ui.checkbox(&mut layer_manager.show_terrain, "Show Raster Map");

                                    ui.separator();
                                    ui.collapsing("🗺️ GeoServer WMTS Connection", |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("Base URL:");
                                            ui.text_edit_singleline(&mut geoserver_url);
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Layer:");
                                            ui.text_edit_singleline(&mut geoserver_layer);
                                        });

                                        if geoserver_source.is_none() {
                                            if ui.button("⚡ Connect / Register WMTS").clicked() {
                                                let source = controller.create_geoserver_source("geoserver", &geoserver_url, &geoserver_layer);
                                                let _ = map_data_stack.register_source(Box::new(source.clone()));
                                                geoserver_source = Some(source);
                                            }
                                        } else {
                                            ui.colored_label(egui::Color32::from_rgb(0, 230, 118), "✓ Registered in MapDataStack");
                                            
                                            ui.separator();
                                            
                                            // Tile settings
                                            ui.horizontal(|ui| {
                                                ui.checkbox(&mut auto_zoom, "Auto Zoom");
                                                if !auto_zoom {
                                                    ui.label("Zoom Level:");
                                                    ui.add(egui::Slider::new(&mut tile_zoom, 0..=18));
                                                } else {
                                                    ui.label(format!("(Auto Level: {})", tile_zoom));
                                                }
                                            });

                                            // Calculate current tile coordinates based on camera center
                                            let center_lat = controller.camera.center.lat;
                                            let center_lon = controller.camera.center.lon;
                                            let (tx, ty) = latlon_to_tile(center_lat, center_lon, tile_zoom);

                                            ui.label("Centered Tile (OSM/WMTS):");
                                            ui.label(format!("  • Matrix Set: EPSG:900913"));
                                            ui.label(format!("  • TileMatrix: EPSG:900913:{}", tile_zoom));
                                            ui.label(format!("  • TileCol (X): {}", tx));
                                            ui.label(format!("  • TileRow (Y): {}", ty));
                                            ui.checkbox(&mut auto_fetch_tiles, "Auto-Request visible tiles");

                                            let source = geoserver_source.as_ref().unwrap();
                                            
                                            if auto_fetch_tiles {
                                                for key in &visible_keys {
                                                    let parts: Vec<&str> = key.split('/').collect();
                                                    if parts.len() == 3 {
                                                        if let (Ok(z), Ok(x), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                                                            source.load_tile(x, y, z);
                                                        }
                                                    }
                                                }
                                            }

                                            ui.horizontal(|ui| {
                                                if ui.button("📥 Request Visible Tiles").clicked() {
                                                    for key in &visible_keys {
                                                        let parts: Vec<&str> = key.split('/').collect();
                                                        if parts.len() == 3 {
                                                            if let (Ok(z), Ok(x), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                                                                source.load_tile(x, y, z);
                                                            }
                                                        }
                                                    }
                                                }
                                                if ui.button("🗑 Clear Cache").clicked() {
                                                    let mut source_mut = source.clone();
                                                    source_mut.clear_cache();
                                                    egui_tile_textures.clear();
                                                    gpu_pipeline.clear_raster_tiles();
                                                }
                                            });

                                            // Check tile status
                                            let tile_key = format!("{}/{}/{}", tile_zoom, tx, ty);
                                            let is_loaded = source.get_tile_pixels(tx, ty, tile_zoom).is_some();

                                            if is_loaded {
                                                ui.colored_label(egui::Color32::from_rgb(0, 230, 118), "Status: Loaded in Memory");
                                                
                                                // Load into egui texture if not already done
                                                let texture = egui_tile_textures.entry(tile_key.clone()).or_insert_with(|| {
                                                    let pixels = source.get_tile_pixels(tx, ty, tile_zoom).unwrap();
                                                    egui_ctx.load_texture(
                                                        format!("tile_{}", tile_key),
                                                        egui::ColorImage::from_rgba_unmultiplied([256, 256], &pixels),
                                                        Default::default()
                                                    )
                                                });
                                                
                                                // Draw the tile preview
                                                ui.separator();
                                                ui.label("Preview (256x256):");
                                                ui.image(&*texture);
                                            } else {
                                                ui.colored_label(egui::Color32::from_rgb(255, 179, 0), "Status: Idle / Loading...");
                                            }

                                            ui.separator();
                                            ui.label(format!("Memory Cache size: {} tiles", source.cache_size()));
                                        }
                                    });

                                    ui.separator();
                                    if ui.button("🛰️ Inject Radar Target").clicked() {
                                        let callsigns = ["TAM", "GLO", "AZU", "TAP", "AAL"];
                                        let id = format!("{}{}", callsigns[rand::random::<usize>() % callsigns.len()], rand::random::<u32>() % 900 + 100);
                                        let offset = (15.0 + rand::random::<f64>() * 80.0) * 1000.0 / 6378137.0;
                                        let angle = rand::random::<f64>() * 2.0 * std::f64::consts::PI;
                                        let lat = sp_lat + offset * angle.cos();
                                        let lon = sp_lon + offset * angle.sin();
                                        let alt = 1000.0 + rand::random::<f64>() * 6000.0;
                                        let speed = 180.0 + rand::random::<f64>() * 70.0;
                                        let heading = rand::random::<f64>() * 2.0 * std::f64::consts::PI;

                                        let target = SimulatedTarget { id: id.clone(), lat, lon, alt, speed, heading };
                                        let _ = controller.interpolator.update_target(olayer_core::interpolator::TargetState {
                                            id: target.id.clone(),
                                            last_position: LatLon::new(target.lat, target.lon, target.alt),
                                            speed_mps: target.speed,
                                            track_heading_rad: target.heading,
                                            vertical_rate_mps: 0.0,
                                            last_ping_time: current_time,
                                        });
                                        simulated_targets.push(target);
                                        controller.trigger_active();
                                    }

                                    if ui.button("🗑️ Clear Targets").clicked() {
                                        for t in &simulated_targets {
                                            controller.interpolator.remove_target(&t.id);
                                        }
                                        simulated_targets.clear();
                                        selected_target_id = None;
                                        controller.trigger_active();
                                    }

                                    ui.separator();
                                    ui.label(format!("Frame Rate: {:.1} FPS", calculated_fps));
                                    ui.label(format!("Active Targets: {}", interpolated_targets.len()));
                                  });
                        }

                        // Draw aircraft targets on the egui painter
                        let painter = egui_ctx.layer_painter(egui::LayerId::background());

                        if layer_manager.show_targets {
                            let simulated_speeds: std::collections::HashMap<String, f64> = simulated_targets
                                .iter()
                                .map(|st| (st.id.clone(), st.speed))
                                .collect();

                            cpu_pipeline.draw_targets(
                                &painter,
                                &interpolated_targets,
                                &selected_target_id,
                                &controller,
                                &view_proj_matrix,
                                config.width,
                                config.height,
                                &simulated_speeds,
                            );
                        }

                        // Flight Profile Panel
                        if let Some(ref selected_id) = selected_target_id {
                            if let Some(target) = interpolated_targets.iter().find(|t| &t.id == selected_id) {
                                egui::TopBottomPanel::bottom("flight_profile")
                                    .resizable(false)
                                    .default_height(140.0)
                                    .show(&egui_ctx, |ui| {
                                        ui.heading(format!("✈️ 2.5D Flight Profile: {}", selected_id));
                                        
                                        // Fetch vertical profile from WASM/Rust Core engine
                                        let mut route_coords = Vec::new();
                                        let r_earth = 6378137.0;
                                        let step = 2000.0;
                                        for dist in (-30000..=50000).step_by(2000) {
                                            let lat_offset = (dist as f64 * target.heading_rad.cos()) / r_earth;
                                            let lon_offset = (dist as f64 * target.heading_rad.sin()) / (r_earth * target.position.lat.cos());
                                            route_coords.push(LatLon::new(target.position.lat + lat_offset, target.position.lon + lon_offset, target.position.height));
                                        }

                                        if let Ok(profile) = controller.terrain.get_vertical_profile(&route_coords, step) {
                                            // Draw custom elevation chart
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
                                    });
                            }
                        }

                        // egui end frame
                        let full_output = egui_ctx.end_frame();
                        let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

                        // Upload egui textures to GPU
                        for (id, image_delta) in &full_output.textures_delta.set {
                            egui_renderer.update_texture(&device, &queue, *id, image_delta);
                        }

                        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

                        // Update egui screen resources
                        let screen_descriptor = egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [config.width, config.height],
                            pixels_per_point: full_output.pixels_per_point,
                        };
                        
                        let cmd_buffers = egui_renderer.update_buffers(
                            &device,
                            &queue,
                            &mut encoder,
                            &paint_jobs,
                            &screen_descriptor,
                        );

                        let current_texture = surface.get_current_texture().unwrap();
                        let view = current_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

                        // WGPU render pass for background + grid + egui UI
                        {
                            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Render Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.08, g: 0.09, b: 0.12, a: 1.0 }),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });

                             // Draw Raster Map Tiles in WGPU
                             if layer_manager.show_terrain {
                                 gpu_pipeline.render_raster_tiles(&mut render_pass, &visible_keys);
                             }

                             // Draw Grid Lines in WGPU
                             if layer_manager.show_grid {
                                 gpu_pipeline.render(&mut render_pass);
                             }

                            // Draw egui UI overlay
                            egui_renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
                        }

                        // Submit
                        queue.submit(cmd_buffers.into_iter().chain(std::iter::once(encoder.finish())));
                        current_texture.present();

                        for id in &full_output.textures_delta.free {
                            egui_renderer.free_texture(id);
                        }

                        // Loop scheduling for active (60 FPS) vs idle (15 FPS)
                        let target_fps = controller.get_target_fps();
                        let frame_delay = std::time::Duration::from_millis(1000 / target_fps as u64);
                        std::thread::sleep(frame_delay);
                        window.request_redraw();
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let current_pos = egui::pos2(position.x as f32, position.y as f32);
                        if is_dragging {
                            let dx = current_pos.x - last_mouse_pos.x;
                            let dy = current_pos.y - last_mouse_pos.y;
                            controller.trigger_active();

                            if controller.view_mode == "3D" {
                                if is_right_click_or_shift {
                                    controller.camera.rotation = (controller.camera.rotation - dx as f64 * 0.005) % (2.0 * std::f64::consts::PI);
                                    controller.camera.pitch = (controller.camera.pitch + dy as f64 * 0.005).clamp(-std::f64::consts::FRAC_PI_2 + 0.01, std::f64::consts::FRAC_PI_2 - 0.01);
                                } else {
                                    controller.camera.center.lon = (controller.camera.center.lon - dx as f64 * 0.005) % (2.0 * std::f64::consts::PI);
                                    controller.camera.center.lat = (controller.camera.center.lat + dy as f64 * 0.005).clamp(-std::f64::consts::FRAC_PI_2 + 0.01, std::f64::consts::FRAC_PI_2 - 0.01);
                                    controller.projection.update_center(controller.camera.center.lat, controller.camera.center.lon);
                                }
                            } else if controller.view_mode == "2.5D" && is_right_click_or_shift {
                                controller.camera.rotation = (controller.camera.rotation - dx as f64 * 0.005) % (2.0 * std::f64::consts::PI);
                                controller.camera.pitch = (controller.camera.pitch + dy as f64 * 0.005).clamp(-std::f64::consts::FRAC_PI_2 + 0.01, std::f64::consts::FRAC_PI_2 - 0.01);
                            } else if controller.view_mode == "2D" && is_right_click_or_shift {
                                controller.camera.rotation = (controller.camera.rotation - dx as f64 * 0.005) % (2.0 * std::f64::consts::PI);
                            } else {
                                // Pan camera in planar meters
                                if let Ok(cx_cy) = controller.projection.project(&controller.camera.center) {
                                    let aspect = controller.camera.aspect_ratio as f32;
                                    let w_meters = (controller.camera.viewport_base_meters / controller.camera.zoom) as f32;
                                    let h_meters = w_meters / aspect;
                                    let meters_px_x = w_meters / config.width as f32;
                                    let meters_px_y = h_meters / config.height as f32;

                                    let cos_theta = (-controller.camera.rotation).cos() as f32;
                                    let sin_theta = (-controller.camera.rotation).sin() as f32;
                                    let rx = dx * cos_theta - dy * sin_theta;
                                    let ry = dx * sin_theta + dy * cos_theta;

                                    let new_cx = cx_cy.0 - rx as f64 * meters_px_x as f64;
                                    let new_cy = cx_cy.1 + ry as f64 * meters_px_y as f64;

                                    if let Ok(lla) = controller.projection.unproject(new_cx, new_cy) {
                                        controller.camera.center = lla;
                                        controller.projection.update_center(lla.lat, lla.lon);
                                        gpu_pipeline.rebuild_grid_buffers(&controller, &device, &queue);
                                        gpu_pipeline.rebuild_raster_tile_buffers(&device, &controller);
                                    }
                                }
                            }
                            last_mouse_pos = current_pos;
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        let is_pressed = state == ElementState::Pressed;
                        if button == MouseButton::Left || button == MouseButton::Right {
                            is_dragging = is_pressed;
                            is_right_click_or_shift = button == MouseButton::Right || egui_ctx.input(|i| i.modifiers.shift);
                            if is_pressed {
                                // Handle click selection on aircraft
                                let mouse_pos = egui_ctx.input(|i| i.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO));
                                last_mouse_pos = mouse_pos;
                                controller.trigger_active();

                                let view_proj_matrix = if controller.view_mode == "3D" {
                                    controller.camera.get_3d_view_proj_matrix().unwrap_or([0.0; 16])
                                } else if controller.view_mode == "2.5D" {
                                    controller.camera.get_25d_view_proj_matrix(controller.projection.as_ref()).unwrap_or([0.0; 16])
                                } else {
                                    controller.camera.get_2d_view_proj_matrix(controller.projection.as_ref()).unwrap_or([0.0; 16])
                                };
                                let size = window.inner_size();

                                let mut nearest_target: Option<String> = None;
                                let mut min_dist = 15.0f32;
                                for t in &simulated_targets {
                                    if let Some(pos) = project_lla_to_screen(
                                        t.lat,
                                        t.lon,
                                        t.alt,
                                        &controller.view_mode,
                                        &controller.camera,
                                        controller.projection.as_ref(),
                                        &view_proj_matrix,
                                        size.width,
                                        size.height,
                                    ) {
                                        let dist = pos.distance(mouse_pos);
                                        if dist < min_dist {
                                            min_dist = dist;
                                            nearest_target = Some(t.id.clone());
                                        }
                                    }
                                }
                                if nearest_target.is_some() {
                                    selected_target_id = nearest_target;
                                } else if mouse_pos.x > 360.0 { // click outside control panel clears selection
                                    selected_target_id = None;
                                }
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        controller.trigger_active();
                        let scroll = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 100.0,
                        };
                        let factor = if scroll > 0.0 { 1.1f64 } else { 0.9f64 };
                        controller.camera.zoom = (controller.camera.zoom * factor).clamp(0.02, 1000.0);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }).unwrap();
}
