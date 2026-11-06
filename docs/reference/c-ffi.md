# C API

All functions return `int32_t`. `>= 0` = success. Negative codes: `-1` null, `-2` parse/missing, `-3` invalid UTF-8, `-4` invalid state, `-99` panic caught.

## C-Compatible Structs

```c
struct C_LatLon { double lat; double lon; double height; };
struct C_InterpolatedTarget { char *id; double lat; double lon; double height; double heading_rad; int32_t quality; };
struct C_ProfilePoint { double distance_meters; double ground_elevation; double lat; double lon; double height; };
```

## Terrain Functions

```c
TerrainEngine* olayer_terrain_engine_create(void);

int olayer_terrain_engine_load_tile(
    TerrainEngine* engine, const uint8_t* data, size_t length,
    int32_t* out_lat_deg, int32_t* out_lon_deg);

int olayer_terrain_engine_unload_tile(TerrainEngine* engine, int32_t lat_deg, int32_t lon_deg);

int olayer_terrain_engine_load_rgb_tile(
    TerrainEngine* engine, uint32_t z, uint32_t x, uint32_t y,
    int encoding_code, const uint8_t* rgba_data, size_t rgba_len,
    size_t width, size_t height); // encoding_code: 0 = MapboxRgb, 1 = Terrarium

int olayer_terrain_engine_load_geotiff(
    TerrainEngine* engine, const uint8_t* data, size_t length,
    double* out_min_lat_deg, double* out_min_lon_deg,
    double* out_max_lat_deg, double* out_max_lon_deg);

int olayer_terrain_decode_mapbox_rgb(uint8_t r, uint8_t g, uint8_t b, double* out_elevation);
int olayer_terrain_decode_terrarium_rgb(uint8_t r, uint8_t g, uint8_t b, double* out_elevation);

int olayer_terrain_engine_get_elevation(
    TerrainEngine* engine, double lat_deg, double lon_deg, double* out_elevation);

int olayer_terrain_engine_get_elevation_rad(
    TerrainEngine* engine, double lat_rad, double lon_rad, double* out_elevation);

// Returns 0 for valid, 1 for unknown/null elevation, or a negative error.
int olayer_terrain_engine_get_elevation_status(
    TerrainEngine* engine, double lat_rad, double lon_rad, double* out_elevation);

// mode: 0 absolute, 1 clamp-to-ground, 2 relative-to-ground, 3 relative-to-mesh
// unknown_policy: 0 reject, 1 use-absolute, 2 use-zero
int olayer_terrain_engine_resolve_altitude(
    TerrainEngine* engine, double lat_rad, double lon_rad,
    double input_height, int mode, int unknown_policy,
    double mesh_height, double* out_height);

// Returns 0 safe, 1 warning, 2 unknown terrain, or a negative error.
int olayer_terrain_engine_calculate_clearance(
    TerrainEngine* engine, double lat_rad, double lon_rad,
    double aircraft_height_meters, double minimum_clearance_meters,
    bool reject_unknown, double* out_clearance);

int olayer_terrain_engine_get_vertical_profile(
    TerrainEngine* engine,
    const double* route_lat, const double* route_lon, const double* route_height,
    size_t route_len, double step_meters,
    struct C_ProfilePoint** out_profile, size_t* out_count);
// route_lat/route_lon are in degrees; route_height in metres

int olayer_terrain_engine_set_cache_capacity(TerrainEngine* engine, size_t capacity);
size_t olayer_terrain_engine_cache_size(TerrainEngine* engine);
void olayer_terrain_engine_clear_cache(TerrainEngine* engine);
void olayer_terrain_engine_free(TerrainEngine* engine);
void olayer_profile_points_free(struct C_ProfilePoint* points, size_t count);
```

## Interpolator Functions

```c
InterpolationEngine* olayer_interpolator_create(void);
InterpolationEngine* olayer_interpolator_create_with_threshold(double stale_threshold);

int olayer_interpolator_update(
    InterpolationEngine* engine, const char* id,
    double lat, double lon, double height,       // radians, metres
    double speed_mps, double track_heading_rad,
    double vertical_rate_mps, double time);

int olayer_interpolator_remove(InterpolationEngine* engine, const char* id);

int olayer_interpolator_interpolate_all(
    InterpolationEngine* engine, double current_time,
    struct C_InterpolatedTarget** out_targets, size_t* out_count);

void olayer_interpolated_targets_free(struct C_InterpolatedTarget* targets, size_t count);
void olayer_interpolator_free(InterpolationEngine* engine);
```

## Geodesy, Topocentric Frames & WMM Functions

```c
int olayer_geodesy_lla_to_ecef(double lat_rad, double lon_rad, double alt_m, double *out_x, double *out_y, double *out_z);
int olayer_geodesy_ecef_to_lla(double x, double y, double z, double *out_lat_rad, double *out_lon_rad, double *out_alt_m);

int olayer_local_frame_lla_to_enu(const struct C_LatLon *origin, const struct C_LatLon *target, double *out_east, double *out_north, double *out_up);
int olayer_local_frame_enu_to_lla(const struct C_LatLon *origin, double east, double north, double up, struct C_LatLon *out_target);
int olayer_local_frame_radar_look_angles(const struct C_LatLon *origin, const struct C_LatLon *target, double k_factor, double *out_slant_range_m, double *out_azimuth_rad, double *out_elevation_rad);

int olayer_wmm_evaluate(double lat_rad, double lon_rad, double alt_m, double year_decimal, double *out_declination_rad, double *out_inclination_rad, double *out_total_intensity_nt);
int olayer_wmm_true_to_magnetic(double true_bearing_rad, double declination_rad, double *out_mag_bearing_rad);
int olayer_wmm_magnetic_to_true(double mag_bearing_rad, double declination_rad, double *out_true_bearing_rad);
```

## Geodesic Spatial Analysis Functions

```c
int olayer_spatial_contains_point(const struct C_LatLon *polygon_vertices, uintptr_t num_vertices, const struct C_LatLon *point, bool *out_contains);
int olayer_spatial_cross_track_deviation(const struct C_LatLon *p1, const struct C_LatLon *p2, const struct C_LatLon *target, double *out_xtk_m, double *out_atd_m);
int olayer_spatial_generate_buffer(const struct C_LatLon *centerline, uintptr_t centerline_len, double buffer_radius_m, struct C_LatLon *out_polygon, uintptr_t max_polygon_pts, uintptr_t *out_polygon_count);
int olayer_spatial_geodesic_intersection(const struct C_LatLon *p1, const struct C_LatLon *p2, const struct C_LatLon *p3, const struct C_LatLon *p4, struct C_LatLon *out_intersection, bool *out_has_intersection);
```

## Tactical Aeronautical Measurement Functions

```c
int olayer_tactical_measure_rbl(const struct C_LatLon *p1, const struct C_LatLon *p2, double *out_distance_m, double *out_init_bearing_rad, double *out_final_bearing_rad);
int olayer_tactical_generate_ppl_leader(const struct C_LatLon *pos, double speed_mps, double heading_rad, double lookahead_sec, double tick_interval_sec, struct C_LatLon *out_leader_end, struct C_LatLon *out_ticks, uintptr_t max_ticks, uintptr_t *out_ticks_count);
int olayer_tactical_generate_holding_pattern(const struct C_LatLon *fix, double inbound_bearing_rad, double inbound_leg_sec, int32_t turn_direction, double speed_mps, double standard_rate_dps, struct C_LatLon *out_points, uintptr_t max_points, uintptr_t *out_points_count);
int olayer_tactical_generate_ils_cone(const struct C_LatLon *threshold, double runway_heading_rad, double glide_slope_rad, double localizer_half_angle_rad, double range_m, struct C_LatLon *out_centerline_end, struct C_LatLon *out_left_boundary, struct C_LatLon *out_right_boundary);
```

## Aeronautical Data Ingestion Functions

```c
int olayer_aeronautical_dataset_create_from_aixm(const char *xml_content, struct AeronauticalDataset **out_dataset);
int olayer_aeronautical_dataset_create_from_geojson(const char *geojson_content, struct AeronauticalDataset **out_dataset);
int olayer_aeronautical_dataset_total_count(const struct AeronauticalDataset *dataset, uintptr_t *out_count);
int olayer_aeronautical_dataset_free(struct AeronauticalDataset *dataset);
```

## Meteorological & SIGMET Functions

```c
int olayer_weather_colorize_dbz_grid(const float *dbz_grid, uintptr_t width, uintptr_t height, int32_t palette_type, uint8_t *out_rgba_pixels, uintptr_t max_bytes, uintptr_t *out_bytes_count);
int olayer_weather_generate_wind_barb(float origin_x, float origin_y, float speed_kts, float dir_rad, float staff_length_px, bool southern_hemisphere, float *out_line_segments, uintptr_t max_floats, uintptr_t *out_floats_count, bool *out_is_calm);
int olayer_sigmet_dataset_create_from_geojson(const char *geojson_content, struct SigmetDataset **out_dataset);
int olayer_sigmet_dataset_total_count(const struct SigmetDataset *dataset, uintptr_t *out_count);
int olayer_sigmet_dataset_free(struct SigmetDataset *dataset);
```

## Volumetric Meshes & Label Anti-Cluttering Functions

```c
int olayer_volumetric_generate_airspace_mesh(const struct C_LatLon *polygon, uintptr_t polygon_len, double floor_m, double ceiling_m, float *out_vertices, uintptr_t max_vertices_floats, uintptr_t *out_vertices_count, uint32_t *out_indices, uintptr_t max_indices, uintptr_t *out_indices_count);
int olayer_volumetric_generate_trajectory_ribbon(const struct C_LatLon *waypoints, uintptr_t waypoints_len, double ribbon_width_m, const double *scalars, uintptr_t scalars_len, float *out_vertices, uintptr_t max_vertices_floats, uintptr_t *out_vertices_count, uint32_t *out_indices, uintptr_t max_indices, uintptr_t *out_indices_count);

int olayer_declutter_solve_labels(const struct C_LabelTarget *targets, uintptr_t targets_len, float leader_length_px, float safety_margin_px, struct C_LabelPlacement *out_placements, uintptr_t max_placements, uintptr_t *out_placements_count);
```

## Memory Ownership Rules

| Allocation | Deletion | Rule |
|-----------|----------|------|
| `_create()` | `_free()` | Rust allocates, Rust frees |
| `const uint8_t* data` | Host | Host allocates, Host frees (Rust reads only) |
| `_get_*` output arrays | `_free()` on returned pointer + count | Rust allocates, caller frees via paired free function |

## Scalar-grid and triangulation additions

The generated header also declares `olayer_terrain_engine_load_elevation_grid`, `olayer_terrain_engine_unload_elevation_grid`, `olayer_terrain_engine_revision`, and `olayer_triangulate_polygon_rings`. See [WCS terrain](../guides/maps/wcs-terrain.md) and [polygon fills](../guides/maps/wfs.md#polygon-fills-and-interior-holes) for coordinate/count/size-query contracts. The [header](../../sdk/native/libolayer_native.h) is authoritative for pointer safety and function-specific statuses.
