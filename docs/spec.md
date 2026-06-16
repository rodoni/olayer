
# Technical Specification: Olayer
# Hybrid GIS Framework for Air Traffic Control (ATC)

## 1. Overview and Scope

The objective of this project is the development of a mission-critical GIS framework for air traffic control (ATC) scenarios in 2D, with native support for 2.5D (Flight Profile) and 3D (Digital Globe) views.

The framework must be **strictly focused on the GIS domain** (geographic processing, mathematical transformations, rendering, and terrain indexing), delegating the ingestion of raw network data (such as Asterix protocols or ADS-B feeds) entirely to the host application (*Host*).

---

## 2. Architectural Assumptions and Technology Stack

To guarantee memory safety, portability, and near-native performance in both web and local environments, the project will adopt a multi-language approach:

* **Agnostic Core (Rust):** All geodetic calculation engines, projection algorithms, SLD style parsers, and DTED file indexing will be written in pure Rust.
* **Hybrid Distribution (WebAssembly + Native):** * **Browsers:** The Rust Core will be compiled to **WebAssembly (WASM)**, providing a binding layer to be consumed via **TypeScript**.
* **Local Systems:** The Core will be consumed directly as a native dependency in Rust.


* **Mathematical Abstraction:** The calculation engine will be 100% agnostic to screen projection. It will operate exclusively in geodetic coordinates based on the WGS84 ellipsoid ($\phi, \lambda, h$) and geocentric Cartesian ECEF coordinates ($X, Y, Z$) with 64-bit floating point precision (`f64`).

---

## 3. Layer Design (Architecture)

```
+---------------------------------------------------------------+
|                      Application Layer                      |
|       (Host App: TypeScript Web / Rust Local Application)      |
+---------------------------------------------------------------+
                                |
                                v
+---------------------------------------------------------------+
|                 Visual Abstraction Layer                    |
|       (Hybrid Pipeline: GPU Matrices & CPU Coordinates)      |
+---------------------------------------------------------------+
                                |
                                v
+---------------------------------------------------------------+
|                Agnostic Rust Core (WASM)                  |
|     (Geodetic Calculations, State Prediction, DTED Cache)     |
+---------------------------------------------------------------+
                                |
                                v
+---------------------------------------------------------------+
|                      Data Providers                      |
|         (MVT/WMS Buffers from GeoServer, DTED Buffers)          |
+---------------------------------------------------------------+

```

---

## 4. Hybrid Rendering Pipeline

To optimize the balance between large-scale graphics performance and plotting precision of targets, the framework will implement a **hybrid rendering strategy**:

### A. Matrix Channel (GPU-oriented)

* **Use:** Rendering of dense terrain (DTED) and vector or raster background maps originating from **GeoServer** (MVT - Mapbox Vector Tiles / WMS).
* **Mechanism:** The Core calculates and exports $4 \times 4$ transformation matrices based on the active projection and camera state. The *Host* application injects these matrices directly into the GPU Shaders (WebGL / WebGPU / Vulkan). *Zoom* and *pan* operations update the matrix without reprocessing vertices on the CPU.

### B. Projected Vertex Channel (CPU-oriented)

* **Use:** Rendering of radar plots, data labels (*data blocks*), heading vectors, and dynamic target symbols.
* **Mechanism:** The physical interpolation of targets (Dead Reckoning) occurs strictly in 3D geodetic coordinates on the WGS84 ellipsoid. For rendering, the client SDK (TypeScript or Native) queries the interpolated geodetic positions (`LatLon`) and converts them into screen coordinates $(X, Y)$ and depth using the active projection resolver (Projections Engine) of the Olayer Core.
* **Operational Advantage:** Keeps the kinematic logic completely agnostic of display, avoids perspective distortions on symbols in 3D views (automatic *Billboard* effect in rendering), and allows the execution of label anti-overlap (*anti-cluttering*) algorithms on the CPU in a stable manner.

### C. Layer-based Rendering Structure (Layer Stack)

To provide operational flexibility and optimize rendering workload, the visualization is structured in a stack of **Layers** with segregated cycles and repaint frequencies:
* **Dynamic Layers (Tactical Targets, Weather Radar, and Interactive Rulers):** Updated in real-time in each screen animation cycle (up to 60 FPS) overlapping the composited texture of the static layers, without cost of reprocessing the background.

---

## 5. GIS Functional Requirements

### 5.1 Projection, View, and Camera Control Support

The framework must support dynamic runtime switching between the following cartographic projections and display modes, with unified management through the **Camera Engine**:

* **Azimuthal Stereographic:** Focus on approach radars (TMA) and preservation of local angles.
* **Lambert Conformal Conic (LCC):** Focus on long-distance En-Route route maps.
* **Mercator / Web Mercator:** Standard macro compatibility.
* **2.5D View (Tilted Flat Perspective Map):** Three-dimensional perspective projection overlaid on a projected plane. Uses a standard tilt (pitch/tilt) of **35 degrees** (declined top/bird's-eye perspective, improving target and relief visualization compared to the old static angle of 55 degrees).
* **3D View (Virtual Globe):** Direct transformation of ellipsoidal coordinates to Cartesian ECEF.

#### Dynamic Camera Control (Zoom, Bearing, Pitch, Roll)
The **Camera Engine** (`core::camera`) of the Olayer Core provides unified control over camera attitude in radians, integrated with the following View-Projection matrices:
- **Zoom (linear scale):** Applied in 2D, 2.5D, and 3D modes.
- **Bearing / Rotation (yaw):** Controls the azimuthal orientation of the camera in 2D, 2.5D, and 3D modes.
- **Pitch / Tilt (vertical inclination):** Controls the inclination of the horizon in 2.5D (0° to 85°) and 3D (-90° to 90°) modes.
- **Roll (lateral roll):** Available in 2.5D and 3D modes for full flight attitude movement support.

### 5.2 Standardized Symbology (ICAO and NATO) and Symbol Registry

The framework manages professional symbol libraries for civil aviation and defense in a performant and modular way, dividing responsibilities between offline vector compilation (SVG) and dynamic rasterized image loading (PNG):

* **Compilation and Import of Vector Symbols (SVG):**
  - To keep the WASM Core lightweight and avoid heavy SVG interpreters at runtime, the import and handling of SVG files are done at build time via the CLI tool **`tools/symbol-compiler`**.
  - The compiler recursively parses paths, circles, texts, and styles (including CSS colors, opacities, and dash patterns) from SVG files, mapping them to a declarative library JSON in the Core's `DeclarativeLibraryDto` standard.
  - The Rust Core consumes this library in consolidated format through the `DeclarativeProvider` registered in the `SymbolRegistry`.
* **Loading of Rasterized Symbols (PNG/JPG) via SDK:**
  - Symbols containing PNG or JPG images are injected directly into the TypeScript SDK through the `TextureAtlasManager::registerImageSymbol` method.
  - The SDK uses browser native APIs to load and render pixels asynchronously, drawing them directly into the Texture Atlas Canvas for upload to the GPU. The raster decoding logic is entirely handled by the browser, without altering the WASM Core structure.
* **Civil (ICAO) and Military (NATO APP-6 / MIL-STD-2525) Symbology:**
  - Standard civil symbol packages (VOR, NDB, DME, TACAN, etc.) and military tactical packages (affiliation frames, fighter icons, cargo, etc.) are provided as modular base SVGs pre-compiled by the build tool or injectable via compiled JSON.
* **Performance Strategy (Texture Atlas & Instancing):**
  - To avoid draw call overhead, the SDK compiles on-demand generated symbols (WASM vector primitives or images loaded via PNG) into a single shared GPU texture (**Texture Atlas / Spritesheet**).
  - The plotting of thousands of radar targets uses a single instanced draw call (`drawElementsInstanced`) referencing the Atlas UV coordinates, eliminating CPU bottlenecks and extra transfers.
  - The final renderer applies *Billboard Shaders* to keep symbols flat and front-facing to the controller, even in 3D or inclined 2.5D globe views.
* **Compatibility with 2D/3D Streams:**
  - The Texture Atlas and symbol projection are natively compatible with all view modes (2D flat, 2.5D profile, and 3D virtual globe).
  - In the **2D/2.5D** flow, Atlas symbols are rendered directly as flat sprites in screen coordinates.
  - In the **3D** flow, the renderer projects the 3D origin of the aircraft and draws symbols using *Billboards* in 3D space, ensuring they remain readable and with consistent visual scale without angular perspective distortion.
* **SLD Styling:** The Core contains an XML parser for **SLD (Styled Layer Descriptor)** files that converts static styling rules into dynamic style metadata applied over the rules of symbols resolved in the Symbol Registry.

### 5.3 Passive Integration with DTED

* The GIS engine will not make I/O requests to read files from disk in web mode. It will accept passive injection of elevation chunks via memory buffers (`ArrayBuffer` or mapped structures).
* The Core will provide $O(1)$ complexity lookups to determine ground altitude and calculate the vertical *Clearance* safety margin of an aircraft (MSAW alerts).

---

## 6. Time Synchronization and Dynamic FPS Control

The system must rigidly decouple sensor data reception (typically at 1 Hz) from screen rendering, allowing strict frames per second (FPS) control.

### 6.1 Predictive Interpolation (Dead Reckoning)

Dynamic targets will be registered in the Rust Core through a **State Vector** (`TargetState`), containing the ellipsoidal position of the last *ping* (WGS84 `LatLon`), real heading in radians, horizontal speed in meters/second, vertical speed in meters/second, and the *timestamp* of capture.
When the *Host* application requests a frame render, it passes the current system *timestamp*. The Core computes the estimated target position (3D geodetic) linearly and smoothly (using the reference ellipsoid and `Geodesy Engine` functions) between sensor updates, without coupling to projections.

### 6.2 FPS Management

The *Host* application controls the *time-step throttling*, allowing dynamic update rate changes for hardware resource preservation:

* **Economic Mode (Ex: 15-20 FPS):** Activated when the screen and camera are static. Smoothness of aircraft movement is maintained via interpolation.
* **Responsive Mode (Ex: 60 FPS):** Activated on demand via user interface events (while the controller drags the map or changes zoom), automatically returning to economic mode after screen stabilization.
---

## 7. Map Server Infrastructure (Data Providers)

To feed the framework with cartographic and aviation structural data, the project will adopt the following server stack:

### 7.1 Application Server: GeoServer
* **Recommended Version:** 2.22.x or higher (with stable Vector Tiles extension support).
* **Consumed Protocols:** * **WMTS / MVT (Mapbox Vector Tiles):** Used for massive loading of vector background maps (borders, coastlines, urban areas) and high-density airways. The Rust Core will apply the active projection (e.g., LCC) over the MVT vertices.
  * **WFS (Web Feature Service):** Used for point metadata queries (e.g., fetching exact coordinates of a runway threshold or information of a radio-aid/VOR).
* **Styling:** GeoServer will centralize the structural `.sld` files that the framework will consume via API to synchronize visual identity.

### 7.2 Storage: PostgreSQL + PostGIS
* The geographic database will store complex spatial features with `GIST` indexing to optimize sector rendering requests.

### 7.3 Delivery Optimization: GeoWebCache (GWC)
* Every background map request from the framework must mandatorily hit the GeoWebCache layer in MVT or WMTS format (Raster, for satellite images). Use of pure/dynamic WMS for real-time operational screens is prohibited to avoid map server overload.

### 7.4 Map Data Stack (Map Data Stack)
To isolate the network and file management from WebGL rendering and radar calculations, the SDKs implement the data stack based on `MapDataSource`:
* **`VectorTileSource` (MVT / GeoServer):** Manages paging and geometric calculation of the camera's visible limits (Bounding Box) in real-time, performing parallel searches of vector blocks in GeoServer.
* **`RasterTileSource` (WMTS / OSM):** Controls map image download and asynchronous texture upload to the GPU.
* **`TerrainTileSource` (DTED / Terrain):** Automatic paging based on controller position. Replaces pure passive injection with a dynamic network resolver with download queues and LRU (Least Recently Used) memory eviction algorithm to ensure stable RAM/WASM consumption.
* **Decoupling by Concurrency:** Complex geographic format decoding (MVT/DTED) will be executed in support threads (Web Workers in browser, local threads in desktop) so that the main rendering thread never blocks radar traffic.

---

## 8. Proposed Code Repository Structure

```text
├── core/                  # Pure Rust Code (Agnostic and Mathematical)
│   ├── Cargo.toml
│   └── src/
│       ├── geodesy/       # Geodetic Formulas and ECEF Module (WGS84)
│       ├── camera/        # CameraState Management and View-Proj Matrices for 2D/2.5D/3D
│       ├── terrain/       # DTED File Parsing and O(1) Altitude Index
│       ├── sld/           # XML Parser for Styled Layer Descriptor Files
│       └── projections/   # Projection Algorithms (Stereographic, LCC, Mercator)
│
└── sdk/
    ├── ts/                # TypeScript SDK for Browsers
    │   └── wasm/          # wasm-bindgen export layer for TypeScript
    │
    └── native/            # Native Desktop SDK and C-FFI
        ├── c_ffi_bridge/  # C-FFI Export (cbindgen)
        └── desktop/       # Native desktop application (WGPU/winit/egui)
```

---



