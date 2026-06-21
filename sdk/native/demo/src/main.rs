use std::sync::Arc;
use winit::{
    event::{Event, WindowEvent, MouseButton, ElementState, MouseScrollDelta},
    event_loop::EventLoop,
    window::WindowBuilder,
};
use olayer_core::geodesy::LatLon;
use olayer_core::projections::{Stereographic, LambertConformalConic, WebMercator};
use olayer_native::{
    NativeController, NativeLayerManager, NativeMapDataStack, MapDataSource, GeoserverWmtsSource, WgpuGpuPipeline, WgpuCpuVertexPipeline, RasterTileUpload, project_lla_to_screen
};

mod sim;
mod tiles;
mod ui;

use sim::SimulatedTarget;

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
    let mut selected_target_id: Option<Arc<str>> = None;
    let mut projection_name = "Stereographic".to_string();

    // GeoServer Integration State
    let mut geoserver_url = "http://localhost:8080/geoserver/gwc/service/wmts".to_string();
    let mut geoserver_layer = "olayer:world_map".to_string();
    let mut geoserver_source: Option<GeoserverWmtsSource> = None;
    let mut tile_zoom: u32 = 10;
    let mut auto_zoom = true;
    let mut auto_fetch_tiles = false;
    let mut egui_tile_textures: std::collections::HashMap<String, egui::TextureHandle> = std::collections::HashMap::new();

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
                            let current_time = start_time.elapsed().as_secs_f64();
                            sim::update_simulated_targets(&mut simulated_targets, &mut controller, current_time, 1.0);
                            last_radar_update = std::time::Instant::now();
                        }

                        // Render Frame
                        let current_time = start_time.elapsed().as_secs_f64();
                        let interpolated_targets = controller.interpolator.interpolate_all(current_time).unwrap_or_default();

                        // Dynamically adjust tile zoom based on camera viewport width to keep the resolution appropriate
                        if auto_zoom {
                            tile_zoom = tiles::ideal_tile_zoom(&controller);
                        }

                        // Calculate visible tile keys in the viewport
                        let visible_keys = tiles::compute_visible_tile_keys(&controller, tile_zoom);

                        // Sync loaded tiles from GeoServer source to GPU textures
                        if let Some(ref source) = geoserver_source {
                            let keys = source.get_cached_keys();
                            for key in keys {
                                if visible_keys.contains(&key) {
                                    let parts: Vec<&str> = key.split('/').collect();
                                    if parts.len() == 3 {
                                        if let (Ok(z), Ok(x), Ok(y)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                                            if let Some(pixels) = source.get_tile_pixels(x, y, z) {
                                                gpu_pipeline.upload_raster_tile(RasterTileUpload {
                                                    device: &device,
                                                    queue: &queue,
                                                    key: &key,
                                                    pixels: &pixels,
                                                    x,
                                                    y,
                                                    z,
                                                    controller: &controller,
                                                });
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
                                            let (tx, ty) = tiles::latlon_to_tile(center_lat, center_lon, tile_zoom);

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

                                        let target = SimulatedTarget { id: Arc::from(id), lat, lon, alt, speed, heading };
                                        let _ = controller.interpolator.update_target(olayer_core::interpolator::TargetState {
                                            id: Arc::clone(&target.id),
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
                            let simulated_speeds: std::collections::HashMap<Arc<str>, f64> = simulated_targets
                                .iter()
                                .map(|st| (Arc::clone(&st.id), st.speed))
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
                            if let Some(target) = interpolated_targets.iter().find(|t| t.id.as_ref() == selected_id.as_ref()) {
                                egui::TopBottomPanel::bottom("flight_profile")
                                    .resizable(false)
                                    .default_height(140.0)
                                    .show(&egui_ctx, |ui| {
                                        ui::draw_flight_profile(ui, &mut controller, target, selected_id);
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

                                let mut nearest_target: Option<Arc<str>> = None;
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
