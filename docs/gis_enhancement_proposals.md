# Olayer GIS Enhancement Proposals: Architectural Scope & Core GIS Components

## 1. Architectural Scope & Domain Demarcation

### 1.1 The Boundary: GIS Framework vs. Complete ATC System

In air traffic management systems (e.g., Eurocontrol ARTAS, FAA ERAM/STARS, Thales TopSky, Indra AirSpace), software architecture is divided into distinct decoupled subsystems:

```
+-----------------------------------------------------------------------------------+
|                           AIR TRAFFIC CONTROL SYSTEM                              |
+-----------------------------------------------------------------------------------+
|  1. SDPS (Surveillance Data Processing) : Sensor fusion, Radar Tracking, ASTERIX |
|  2. FDPS (Flight Data Processing)       : Flight plans, BADA performance, FMS     |
|  3. Safety Nets Server                 : STCA, MSAW, APW alert state machines     |
|  4. CWP / HMI (Controller Working Position): User interface, audio, operational  |
|                                                                                   |
|  ===============================================================================  |
|  5. GIS & MAPPING DISPLAY ENGINE  <--- EXACT SCOPE OF OLAYER                     |
|     • WGS84 Geodetics, Projections, Local Frames (ENU/NED), WMM Magnetic Model    |
|     • 2D/2.5D/3D Camera Matrices, WebGL2/WebGPU/WGPU Render Pipelines             |
|     • Tactical Symbology (NATO/ICAO/SLD), Texture Atlas, Instanced Sprites        |
|     • Target Dead-Reckoning (Display Smoothing), Label Anti-Cluttering            |
|     • Aeronautical Vectors (Airspaces, Airways, AIXM), DTED/RGB Terrain           |
|     • Tactical Measurement Geometry (RBL, CRSR, Range Rings, Holding Patterns)    |
|     • Weather Radar Overlays (NEXRAD dBZ, Wind Fields, SIGMET Polygons)          |
+-----------------------------------------------------------------------------------+
```

### 1.2 Boundary Principles for Olayer

As defined in [Technical Specification (spec.md)](file:///c:/Users/rafae/projects/rust/olayer/docs/spec.md), Olayer is **strictly a GIS and display framework**. It must adhere to the following rules:

1. **What Belongs in Olayer (GIS Framework Domain):**
   - **Geodetic & Spatial Computations:** WGS84 conversions, ellipsoidal distance/azimuth, local tangent frames (ENU/NED), magnetic declination (WMM), geodesic polygon containment, along-track distance (ATD), cross-track error (XTK).
   - **Cartographic Projections & Multi-View Camera:** Stereographic, LCC, Mercator, Polar Stereographic, UTM, Orthographic; 2D/2.5D/3D matrix generation.
   - **Aviation Cartography & Tactical Visual Tools:** Dynamic Compass Rose, Range Rings, Azimuth (radar) grids, Range & Bearing Lines (RBL), Velocity Vector (PPL) leader lines, radar history trails (snail trails), holding pattern racetrack geometry, ILS approach cones.
   - **Aeronautical Data Ingestion & Styling:** AIXM 5.1, GeoJSON-Aviation, MVT vector tiles, OGC SLD/SE styles, MIL-STD-2525 / NATO APP-6 and ICAO symbology.
   - **Terrain & Elevation Providers:** DTED Levels 0/1/2, Cloud-Optimized GeoTIFF (COG), Mapbox RGB elevation tiles, $O(1)$ altitude queries, vertical profile slicing, terrain clearance geometry.
   - **Meteorological GIS Layers:** Gridded radar reflectivity (dBZ) textures, GRIB2 wind vector barbs, SIGMET/AIRMET polygon layers, dynamic elevation/weather isolines (contours).
   - **GPU Rendering & Anti-Cluttering:** Instanced symbol rendering, billboard shaders, 3D extruded airspace volumes, 3D flight ribbons, multi-octant force-directed label anti-overlapping.

2. **What Belongs in the ATC Host / SDPS / FDPS (OUT of Olayer Scope):**
   - **Surveillance Data Processing (SDPS):** ASTERIX network ingestion, multi-radar sensor fusion, raw plot correlation, radar tracking filters (Kalman/EKF), covariance gating. *(The host provides filtered target state vectors to Olayer).*
   - **Flight Data Processing (FDPS):** Full FMS route management, airline dispatch, BADA aircraft aerodynamic performance curves, AFTN/FIXM messaging. *(The host provides route coordinates to Olayer for geometric rendering).*
   - **Safety Net State Machines:** STCA / APW alert suppression logic, audio alarm dispatch, controller acknowledgment state machines. *(Olayer provides spatial/geometric distance queries; the host manages alert logic).*

---

## 2. Core GIS Enhancement Proposals

The following proposals are **strictly within the GIS domain**, expanding Olayer's spatial math, cartographic tools, aeronautical data support, weather overlays, and GPU rendering capabilities.

```
================================================================================
GIS COMPONENT PROPOSALS INDEX
--------------------------------------------------------------------------------
GIS-PROP-001 : Local Tangent Plane (ENU/NED) & World Magnetic Model (WMM) [IMPLEMENTED]
GIS-PROP-002 : Geodesic Spatial Analysis Engine (Containment, Buffers, XTK/ATD) [IMPLEMENTED]
GIS-PROP-003 : Tactical Aeronautical Measurement Tools & Dynamic Overlays [IMPLEMENTED]
GIS-PROP-004 : Aeronautical Data Ingestion (AIXM 5.1 & GeoJSON-Aviation) [IMPLEMENTED]
GIS-PROP-005 : Civil Cloud Terrain Ingestion (COG & Mapbox RGB Elevation) [IMPLEMENTED]
GIS-PROP-006 : Meteorological GIS Overlays (Radar dBZ, Wind Barbs & Isolines) [IMPLEMENTED]
GIS-PROP-007 : 3D Volumetric Airspaces & Trajectory Ribbon GPU Shaders [IMPLEMENTED]
GIS-PROP-008 : Advanced Multi-Octant Force-Directed Label Anti-Cluttering [PROPOSED]
================================================================================
```

---

### 2.1 `GIS-PROP-001`: Local Tangent Plane (ENU/NED) & World Magnetic Model (WMM)

#### Domain Context & GIS Need
1. **Local Tangent Plane (ENU - East, North, Up / NED - North, East, Down):**
   Essential for airport ground operations (A-SMGCS), precision approach runway visualization, and radar antenna relative coordinates. Converts geodetic coordinates to local metric offsets relative to a specific airport reference point or radar tower.
2. **World Magnetic Model (WMM-2025):**
   Aviation maps, runway markings (e.g., Runway 09/27), VOR radials, and magnetic compass roses operate on **Magnetic North**, not True North. A built-in WMM spherical harmonic calculator allows Olayer to compute local magnetic declination ($\Delta \theta$) at any $(\text{lat}, \text{lon}, \text{alt}, \text{epoch})$.

#### Mathematical Formulation
1. **Geodetic LLA $\rightarrow$ Local Tangent Plane (ENU):**
   With reference station $\mathbf{P}_0(\phi_0, \lambda_0, h_0)$ and target $\mathbf{P}(\phi, \lambda, h)$, compute $\Delta \mathbf{X} = \text{ECEF}(\mathbf{P}) - \text{ECEF}(\mathbf{P}_0)$:
   $$\begin{bmatrix} E \\ N \\ U \end{bmatrix} = \begin{bmatrix} -\sin \lambda_0 & \cos \lambda_0 & 0 \\ -\sin \phi_0 \cos \lambda_0 & -\sin \phi_0 \sin \lambda_0 & \cos \phi_0 \\ \cos \phi_0 \cos \lambda_0 & \cos \phi_0 \sin \lambda_0 & \sin \phi_0 \end{bmatrix} \Delta \mathbf{X}$$

2. **Radar Look Angles with 4/3 Atmospheric Refraction:**
   $$R_{\text{slant}} = \sqrt{E^2 + N^2 + U^2}, \quad \text{Azimuth} = \text{atan2}(E, N), \quad \text{Elevation} = \arcsin\left(\frac{U}{R_{\text{slant}}}\right)$$

3. **Magnetic Declination (WMM):**
   $$\text{Bearing}_{\text{mag}} = (\text{Bearing}_{\text{true}} - \text{Declination}) \pmod{2\pi}$$

#### Proposed Rust Core Interface (`core::geodesy::local_frame` & `core::geodesy::magnetic`)

```rust
pub mod local_frame {
    use crate::geodesy::coords::{LatLon, Point3D};

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct EnuPoint {
        pub east_m: f64,
        pub north_m: f64,
        pub up_m: f64,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct LocalTangentFrame {
        pub origin: LatLon,
        origin_ecef: Point3D,
    }

    impl LocalTangentFrame {
        pub fn new(origin: LatLon) -> Self;
        pub fn lla_to_enu(&self, lla: &LatLon) -> EnuPoint;
        pub fn enu_to_lla(&self, enu: &EnuPoint) -> LatLon;
        pub fn radar_look_angles(&self, target: &LatLon) -> (f64, f64, f64); // (Slant Range m, Azimuth rad, Elevation rad)
    }
}

pub mod magnetic {
    use crate::geodesy::coords::LatLon;

    pub struct MagneticModel;

    impl MagneticModel {
        /// Computes magnetic declination in radians for given WGS84 coordinate and epoch
        pub fn get_declination(point: &LatLon, epoch_decimal_year: f64) -> f64;
        pub fn true_to_magnetic(true_rad: f64, point: &LatLon, epoch: f64) -> f64;
        pub fn magnetic_to_true(mag_rad: f64, point: &LatLon, epoch: f64) -> f64;
    }
}
```

---

### 2.2 `GIS-PROP-002`: Geodesic Spatial Analysis Engine (Containment, Buffers, XTK/ATD)

#### Domain Context & GIS Need
Standard 2D flat GIS spatial algorithms fail on a globe (distortions near poles, antimeridian crossing, curvature). Olayer requires high-performance **WGS84 ellipsoidal/spherical geometric algorithms**:
1. **Geodesic Point-in-Polygon (Airspace Containment):** Testing whether a position is inside an airspace sector.
2. **Cross-Track Error (XTK) & Along-Track Distance (ATD):** Metric deviation of an aircraft from a planned geodesic airway route.
3. **Geodesic Buffers & Corridors:** Generating constant-width geodetic corridors around flight routes or restricted zones.
4. **Geodesic Line-Line Intersection:** Finding exact crossing points of two airways or flight paths on the ellipsoid.

```
                  Geodesic Route Leg (A -> B)
         A o===================================>o B
                  \          | XTK (Cross-Track Error)
                   \         v
                    \.......o Aircraft Position (P)
                    |------>| ATD (Along-Track Distance)
```

#### Proposed Rust Core Interface (`core::spatial::geometry`)

```rust
pub mod geometry {
    use crate::geodesy::coords::LatLon;

    #[derive(Debug, Clone, PartialEq)]
    pub struct GeodesicPolygon {
        pub vertices: Vec<LatLon>,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct RouteDeviation {
        pub cross_track_error_meters: f64, // Positive = Right of track, Negative = Left
        pub along_track_distance_meters: f64,
        pub nearest_point_on_route: LatLon,
    }

    impl GeodesicPolygon {
        pub fn new(vertices: Vec<LatLon>) -> Self;
        /// WGS84 spherical winding number containment check
        pub fn contains_point(&self, point: &LatLon) -> bool;
        /// Minimum geodesic distance from point to polygon boundary
        pub fn distance_to_boundary(&self, point: &LatLon) -> f64;
        /// Generates a geodesic buffer polygon with specified radius in meters
        pub fn generate_buffer(&self, radius_meters: f64, num_segments: usize) -> GeodesicPolygon;
    }

    /// Computes XTK and ATD of a position relative to a geodesic route segment
    pub fn compute_route_deviation(segment_start: &LatLon, segment_end: &LatLon, pos: &LatLon) -> RouteDeviation;

    /// Finds intersection point of two geodesic segments, if one exists
    pub fn geodesic_intersection(p1: &LatLon, p2: &LatLon, p3: &LatLon, p4: &LatLon) -> Option<LatLon>;
}
```

---

### 2.3 `GIS-PROP-003`: Tactical Aeronautical Measurement Tools & Dynamic Overlays *(STATUS: IMPLEMENTED)*

#### Domain Context & GIS Need
Aviation controllers interact with specialized cartographic overlay tools on the radar screen:
1. **Range and Bearing Lines (RBL / CRSR):** Measuring dynamic distance (NM), true/magnetic bearing, and relative intercept time between two points or aircraft.
2. **Projected Position Leaders (PPL):** Velocity vector line indicating projected position at 1-min, 2-min, 5-min intervals with time tick marks.
3. **Radar Snail Trails (History Dots):** Decaying multi-scan radar hit positions showing turn rate and acceleration trends.
4. **Holding Pattern & ILS Approach Cone Geometry Generators:** Generating standard racetrack holding geometry (inbound leg, standard rate-one turns) and ILS localizer/glideslope capture funnels from cartographic parameters.
5. **Compass Rose & Range Rings Overlay:** Azimuth grids anchored to radar centers, airports, or mouse cursor.

```
       Aircraft A (Track)
           [====>]--------------+--+--+------> Projected Position Leader (PPL)
           :  :  :              1m 2m 3m 5m
           Snail Trails (Decaying History Dots)
           \
            \ Range & Bearing Line (RBL)
             \
              v
           Aircraft B
```

#### Implemented SDK Component (`sdk/ts/src/tools/` & `sdk/native/src/tools/`)

* **Native Rust Component:** [`sdk/native/src/tools/mod.rs`](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/src/tools/mod.rs)
* **C-FFI Bindings & Headers:** [`sdk/native/src/c_ffi_bridge/mod.rs`](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/src/c_ffi_bridge/mod.rs), [`sdk/native/libolayer_native.h`](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/libolayer_native.h)
* **WASM Bridge:** [`sdk/ts/wasm/src/lib.rs`](file:///c:/Users/rafae/projects/rust/olayer/sdk/ts/wasm/src/lib.rs)
* **TypeScript SDK Component:** [`sdk/ts/src/tools/manager.ts`](file:///c:/Users/rafae/projects/rust/olayer/sdk/ts/src/tools/manager.ts), [`sdk/ts/src/tools/index.ts`](file:///c:/Users/rafae/projects/rust/olayer/sdk/ts/src/tools/index.ts)
* **TypeScript & Rust Unit Tests:** [`sdk/ts/src/tools/tools.test.ts`](file:///c:/Users/rafae/projects/rust/olayer/sdk/ts/src/tools/tools.test.ts), [`sdk/native/src/tools/mod.rs`](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/src/tools/mod.rs)
* **Architecture Documentation:** [`docs/sdk/ts/tools/arch.md`](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/tools/arch.md), [`docs/sdk/native/tools/arch.md`](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/native/tools/arch.md)


---

### 2.4 `GIS-PROP-004`: Aeronautical Data Ingestion (AIXM 5.1 & GeoJSON-Aviation)

#### Domain Context & GIS Need
Standard GIS engines consume general formats (Shapefile, GeoJSON). Aeronautical systems operate on the **AIXM (Aeronautical Information Exchange Model)** standard (ICAO / FAA / Eurocontrol) and ARINC 424 navigation databases.

Olayer needs a lightweight parser/loader for aeronautical features:
- Airspace Sectors, CTRs, TMAs, FIR boundaries (with vertical limits).
- Navaids (VOR, NDB, DME, TACAN, Fixes/Intersections) with frequencies and identifiers.
- Airways (En-route high/low ATS routes) with magnetic bearings and minimum flight altitudes (MEA).
- Airport runways with true/magnetic alignments, threshold coordinates, and elevation.

#### Proposed Rust Core Interface (`core::aeronautical`)

```rust
pub mod aeronautical {
    use crate::geodesy::coords::LatLon;

    #[derive(Debug, Clone, PartialEq)]
    pub struct AeronauticalAirspace {
        pub uid: String,
        pub name: String,
        pub airspace_type: String, // "CTR", "TMA", "FIR", "RESTRICTED", etc.
        pub lower_limit_m: f64,
        pub upper_limit_m: f64,
        pub boundary: Vec<LatLon>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AeronauticalNavaid {
        pub ident: String,
        pub name: String,
        pub navaid_type: String, // "VOR", "DME", "NDB", "FIX", "TACAN"
        pub coords: LatLon,
        pub frequency_mhz: Option<f64>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AeronauticalAirway {
        pub ident: String, // e.g. "UM616", "J50"
        pub waypoints: Vec<LatLon>,
        pub minimum_enroute_alt_m: f64,
    }

    pub fn parse_aixm_airspaces(xml_content: &str) -> Result<Vec<AeronauticalAirspace>, String>;
    pub fn parse_geojson_aviation(geojson_str: &str) -> Result<Vec<AeronauticalAirspace>, String>;
}
```

---

### 2.5 `GIS-PROP-005`: Civil Cloud Terrain Ingestion (COG & Mapbox RGB Elevation)

#### Domain Context & GIS Need
Currently, `core::terrain` parses military DTED (Digital Terrain Elevation Data) binary files. In modern civil aviation and commercial cloud deployments, elevation data is delivered as:
1. **Mapbox RGB / Terrarium Terrain Tiles (PNG):** RGB encoded elevation:
   $$h = -10000 + (R \cdot 256^2 + G \cdot 256 + B) \cdot 0.1$$
2. **Cloud-Optimized GeoTIFF (COG):** 32-bit float elevation matrices with HTTP Range Request tile reading.

Adding support for RGB terrain decoding and COG streaming allows Olayer to consume worldwide high-resolution elevation data from standard web tile servers (AWS Terrain, Mapbox, OpenTopography) alongside DTED.

#### Proposed Core & TS Provider Interface

```typescript
export class RgbTerrainSource extends MapDataSource {
  constructor(urlTemplate: string, encoding: "MapboxRGB" | "Terrarium");
  public async getElevationAt(lat: number, lon: number): Promise<number | null>;
  public async getVerticalProfile(route: Array<{ lat: number; lon: number }>, stepMeters: number): Promise<number[]>;
}
```

---

### 2.6 `GIS-PROP-006`: Meteorological GIS Overlays (Radar dBZ, Wind Barbs & Isolines) *(STATUS: IMPLEMENTED)*

#### Domain Context & GIS Need
Weather is a primary factor in aviation safety. A tactical GIS must render:
1. **Precipitation Radar Reflectivity (dBZ):** NEXRAD / RIDGE II / WMS-T weather radar raster grids (20 to 70 dBZ) with smooth GPU color-mapping (Light Green $\rightarrow$ Yellow $\rightarrow$ Red $\rightarrow$ Magenta).
2. **Winds Aloft (GRIB2 Gridded Vectors):** Gridded wind speed and direction fields rendered as standard aviation **Wind Barbs** (50kt pennant, 10kt barb, 5kt half-barb, calm circle) or animated flow streamlines.
3. **Hazard Polygons:** SIGMET, AIRMET, and Convective Storm cell polygons with pulsating alert borders, altitude bounds, and point containment queries.
4. **Dynamic Terrain / Pressure Isolines (Contours):** Generating smooth iso-altitude or isobar contour curves directly using Marching Squares.

```
       Wind Barb Symbology (Aviation Standard):
           O \ 50 kt (Pennant)
             | \ 10 kt (Full Barb)
             | \ 5 kt (Half Barb)
             |
             + (Origin Point)
```

#### Implemented Core & SDK Meteorological Architecture

* **Rust Core Engine:** `core::weather` (`radar_palette`, `wind_barb`, `isoline`, `sigmet`)
* **WASM Bridge:** `WasmSigmetDataset`, `colorize_dbz_grid`, `generate_wind_barb_geometry`, `generate_isolines`, `dbz_to_rgba`
* **C-FFI Bridge:** `olayer_weather_dbz_to_rgba`, `olayer_weather_generate_wind_barb`, `olayer_weather_generate_isolines`, `olayer_sigmet_dataset_*`
* **TypeScript SDK Layers:** `WeatherRadarLayer`, `WindBarbsLayer`, `SigmetLayer` (`sdk/ts/src/layers/`)

```typescript
export class WeatherRadarLayer extends Layer {
  constructor(id: string, options?: WeatherRadarLayerOptions);
  public setDbzGrid(grid: Float64Array | number[], width: number, height: number, boundsDeg: [number, number, number, number]): void;
  public setPalette(palette: "nexrad" | "icao" | "high_contrast"): void;
  public generateContourIsolines(isovalues: number[]): number[];
}

export class WindBarbsLayer extends Layer {
  constructor(id: string, options?: WindBarbsLayerOptions);
  public setStations(stations: WindStation[]): void;
  public addStation(station: WindStation): void;
  public getStationGeometry(id: string): number[] | undefined;
}

export class SigmetLayer extends Layer {
  constructor(id: string, options?: SigmetLayerOptions);
  public loadGeoJson(jsonContent: string): void;
  public findHazardsAt(latDeg: number, lonDeg: number, altM?: number): any[];
}
```

---

### 2.7 `GIS-PROP-007`: 3D Volumetric Airspaces & Trajectory Ribbon GPU Shaders

#### Domain Context & GIS Need
In 2.5D profile and 3D globe views:
- **3D Airspace Polyhedrons:** Flat 2D boundary lines do not convey vertical limits. Rendering 3D **extruded airspace volumes** with semi-transparent tinted sidewalls and Fresnel edge glow allows controllers to visually inspect whether aircraft climb/descent trajectories intersect controlled sectors.
- **3D Flight Path Ribbons:** Rendering historical or planned trajectories as continuous 3D ribbons colored by altitude, vertical speed, or flight level.

```
   Flight Level FL240 +-------------------------------+ (Ceiling)
                      |                               |
                      |        AIRSPACE SECTOR        |  <-- Semi-transparent
                      |       (3D Polyhedron)         |      tinted sidewalls
   Flight Level FL050 +-------------------------------+ (Floor)
                     /                                 /
   WGS84 Surface    +---------------------------------+ (Footprint)
```

#### Proposed GPU Vertex Extrusion Shader (WebGL2 / WGPU)

```glsl
#version 300 es
// Extrudes a 2D geodesic polygon into a 3D volumetric prism
in vec3 a_lla_floor;   // (lat_rad, lon_rad, floor_height_m)
in vec3 a_lla_ceiling; // (lat_rad, lon_rad, ceiling_height_m)
in float a_extrusion;  // 0.0 = floor vertex, 1.0 = ceiling vertex

uniform mat4 u_viewProjMatrix;
out float v_heightRatio;

void main() {
    v_heightRatio = a_extrusion;
    vec3 lla = mix(a_lla_floor, a_lla_ceiling, a_extrusion);
    vec3 ecef = lla_to_ecef(lla);
    gl_Position = u_viewProjMatrix * vec4(ecef, 1.0);
}
```

---

### 2.8 `GIS-PROP-008`: Advanced Multi-Octant Force-Directed Label Anti-Cluttering

#### Domain Context & GIS Need
In high-density terminal areas (TMA) and airport surfaces, radar data blocks (callsign, altitude, speed, wake category) overlap if placed at fixed offsets. Label anti-cluttering is a core GIS problem.

Olayer needs an **8-Octant Multi-Leader Resolver with Force-Directed Relaxation**:
- Computes leader arm placement across 8 standard positions (N, NE, E, SE, S, SW, W, NW).
- Applies a cost function scoring overlap area, proximity to aircraft heading vector, and leader line crossings.
- Solves label layout in $< 1\text{ ms}$ for 500+ simultaneous targets on the CPU rendering thread.

```
       8-Octant Dynamic Leader Arm Resolver:
                 (NW)    (N)    (NE)
                     \    |    /
              (W) --- [Target] --- (E)
                     /    |    \
                 (SW)    (S)    (SE)
```

---

## 3. Prioritization, Delivery Status & Roadmap

| Proposal ID | Module | Title | Status | Priority | Target Phase |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **GIS-PROP-001** | `core::geodesy` | Local Tangent Plane (ENU/NED) & World Magnetic Model (WMM) | ✅ **Completed** (Core, WASM, C-FFI) | **P0** | Phase 8 |
| **GIS-PROP-002** | `core::spatial` | Geodesic Spatial Analysis (Containment, Buffers, XTK/ATD) | ✅ **Completed** (Core, WASM, C-FFI) | **P0** | Phase 8 |
| **GIS-PROP-003** | `sdk::tools` | Tactical Aeronautical Measurement Tools (RBL, CRSR, PPL, Holding) | ✅ **Completed** (WASM & TS SDK) | **P1** | Phase 9 |
| **GIS-PROP-008** | `sdk::renderer` | 8-Octant Force-Directed Label Anti-Cluttering Engine | 🟡 **Partially Foundational** (4-quadrant greedy in `cpu.ts`) | **P1** | Phase 9 |
| **GIS-PROP-004** | `core::aeronautical` | Aeronautical Data Ingestion (AIXM 5.1 & GeoJSON-Aviation) | ✅ **Completed** (Core, WASM, C-FFI, TS SDK) | **P1** | Phase 9 |
| **GIS-PROP-005** | `core::terrain` | Civil Cloud Terrain Ingestion (COG & Mapbox RGB Elevation) | ✅ **Completed** (Core, WASM, C-FFI, TS SDK) | **P2** | Phase 10 |
| **GIS-PROP-006** | `sdk::layers` | Meteorological Overlays (Radar dBZ, Wind Barbs & Isolines) | ✅ **Completed** (Core, WASM, C-FFI, TS SDK) | **P2** | Phase 11 |
| **GIS-PROP-007** | `sdk::renderer` | 3D Volumetric Airspace & Trajectory Ribbon GPU Shaders | ⏳ **Pending** | **P2** | Phase 10 |

---

## 4. Architectural Summary

By maintaining a strict separation between the **GIS Display Engine (Olayer)** and the **ATC Host Systems (SDPS / FDPS / Safety Nets)**:
1. **Olayer remains clean, modular, and high-performance:** It focuses on mathematical precision, cartographic projections, tactical symbology, spatial queries, and real-time GPU/CPU rendering.
2. **Host applications retain full control:** The Host implements business rules, surveillance network protocols (ASTERIX), multi-sensor radar tracking filters, flight plan databases, and safety net alert dispatching, calling Olayer exclusively for spatial processing and visual rendering.
