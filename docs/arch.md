# Software Architecture: Olayer
## Hybrid GIS Framework for Air Traffic Control (ATC)

This document describes the initial architecture of the **Olayer** project, mapped from the requirements defined in the [Technical Specification (spec.md)](file:///c:/Users/rafae/projects/rust/olayer/docs/spec.md). The design uses the **C4 Model** (Context, Containers, Components, and Processes/Code) to illustrate the division of responsibilities, data flows, and mission-critical structural decisions.

---

## 1. Level 1: System Context Diagram

The context diagram describes how the Olayer framework positions itself in relation to the actors (developers and operators) and external systems of the ATC solution.

```mermaid
graph TB
    %% C4 node styles
    classDef person fill:#08427B,stroke:#073b6e,color:#ffffff,stroke-width:2px;
    classDef system fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;
    classDef external fill:#999999,stroke:#888888,color:#ffffff,stroke-width:2px;

    %% Actors
    user["👩‍✈️ Air Traffic Controller<br>[End User]"]:::person
    dev["💻 Host Developer<br>[Software Engineer]"]:::person
    
    %% Main System (Olayer Boundary)
    subgraph Olayer_Boundary ["Olayer Framework"]
        olayer["🌍 Olayer GIS ATC Framework<br>[Software System]<br>GIS framework for high-performance spatial processing and projection."]:::system
    end
    
    %% External Systems
    host_app["📱 ATC Host Application<br>[Software System]<br>Client or ATC console application (Web/Desktop) that consumes Olayer."]:::system
    geoserver["🗺️ GeoServer / GeoWebCache<br>[External System]<br>Map server providing MVT, WMTS, and SLD styling."]:::external
    sensor_feed["📡 ATC Sensor Feed<br>[External System]<br>Raw data provider (ADS-B, ASTERIX, radar feeds)."]:::external
    terrain_source["🏔️ Terrain Server / Repository<br>[External System]<br>Provides elevation data (DTED files) via HTTP or locally."]:::external
    
    %% Relationships
    dev -->|Integrates and configures in code| host_app
    user -->|Views targets and interacts with the map| host_app
    host_app -->|Delegates GIS calculations and rendering| olayer
    host_app -->|Consumes and decodes raw data| sensor_feed
    olayer -->|Consumes map layers and styles| geoserver
    olayer -->|Consumes DTED elevation data| terrain_source

    linkStyle 0,1,2,3,4,5 stroke:#555,stroke-width:2px;
```

### Actors and Systems

| Element | Type | Description |
| :--- | :--- | :--- |
| **Air Traffic Controller** | User | Final operator who uses the radar screen to monitor routes, deviations, and safety alerts. |
| **Host Developer** | User | Developer who integrates the Olayer SDK into the client application (Web or Desktop). |
| **Olayer GIS ATC Framework** | System | The project scope: framework responsible for geodetic calculations, projections, target/terrain rendering, and GIS checks. |
| **ATC Host Application** | External System | The host software (e.g., TMA approach control terminal or en-route center). Manages sockets, business rules, and general interfaces. |
| **GeoServer / GeoWebCache** | External System | Map server that centralizes geographic files (sector boundaries, airways) and distributes them in optimized chunks (Tiles). |
| **ATC Sensor Feed** | External System | Network infrastructure that injects radar or ADS-B feeds into the host application. Olayer is agnostic to this network. |
| **Terrain Server / Repository** | External System | File server or local storage that provides terrain elevation data (DTED) upon request. |

---

## 2. Level 2: Container Diagram

Olayer is designed as a hybrid framework. It divides itself into a shared Rust core and specific bindings for web (WebAssembly) and desktop (Native) environments.

```mermaid
graph TB
    classDef container fill:#438DD5,stroke:#3b7cbd,color:#ffffff,stroke-width:2px;
    classDef external fill:#999999,stroke:#888888,color:#ffffff,stroke-width:2px;
    classDef host fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;

    subgraph Web_Browser ["Browser Environment (Web Client)"]
        host_web["📱 Host App Web<br>[TypeScript/React/Vue]"]:::host
        ts_sdk["📦 Olayer TS SDK<br>[TypeScript Container]<br>Manages the WebGL pipeline, inputs, and rendering loop."]:::container
        wasm_bind["🔗 WASM Bindings<br>[Rust/JS Bridge]<br>wasm-bindgen exports and memory buffer management."]:::container
        wasm_core["⚙️ Olayer Core (Rust WASM)<br>[WASM Module]<br>Logic engine compiled to WebAssembly. Geodesy, projections, and DTED indexing."]:::container
    end
    
    subgraph Desktop_OS ["Native Desktop Environment"]
        host_rust["🖥️ Native Host App<br>[Rust / C++]"]:::host
        native_sdk["📦 Olayer Native SDK<br>[Rust Container]<br>Native wrapper exposing local APIs and wgpu/Vulkan pipeline."]:::container
        rust_core["⚙️ Olayer Core (Native)<br>[Rust Library]<br>Native compilation of the core for the target architecture (x86_64/ARM)."]:::container
        local_disk["💽 Local DTED Storage<br>[File System]<br>DTED terrain files on local disk."]:::external
    end
    
    subgraph Map_Server_Stack ["Map Data Stack"]
        geoserver["🗺️ GeoServer + GWC<br>[GeoServer Container]<br>Provides Vector Tiles (MVT), WMTS, and SLD styles."]:::external
        postgis[("🗄️ PostgreSQL + PostGIS<br>[Database]<br>Stores spatial geographic features.")]:::external
        terrain_repo["🏔️ Static DTED Repository<br>[Data Store]<br>Stores binary terrain elevation files (DTED) via HTTP."]:::external
    end
    
    %% Web Flows
    host_web -->|Instantiates and initializes| ts_sdk
    ts_sdk -->|Calls via JS| wasm_bind
    wasm_bind -->|Executes core routines| wasm_core
    ts_sdk -->|Consumes MVT/WMTS and SLD via HTTP| geoserver
    ts_sdk -->|Downloads DTED files via HTTP| terrain_repo
    
    %% Native Flows
    host_rust -->|Imports and initializes| native_sdk
    native_sdk -->|Direct static function call| rust_core
    native_sdk -->|Consumes MVT/WMTS and SLD via HTTP| geoserver
    native_sdk -->|Reads DTED files from disk| local_disk

    %% Data Infrastructure
    geoserver -->|Spatial query via SQL| postgis

    linkStyle 0,1,2,3,4,5,6,7,8,9 stroke:#555,stroke-width:2px;
```

### Framework Containers

1. **Olayer Core (Rust - compilable to WASM and Native):**
   * **Responsibility:** All mission-critical mathematical engine. Has no direct I/O access to files or network in the WASM version (passive), processing only memory structures provided by the host layer.
   * **Technology:** Pure Rust (`f64`).
2. **WASM Bindings (wasm-bindgen):**
   * **Responsibility:** Memory transition bridge between the JS virtual machine and the WASM linear memory. Minimizes copies using direct buffer references (`ArrayBuffer` for DTED/MVT).
   * **Technology:** `wasm-bindgen`, `js-sys`, `web-sys`.
3. **Olayer TS SDK (TypeScript):**
   * **Responsibility:** Client SDK/Framework consumed by web applications. Manages the visual `<canvas>` element lifecycle, orchestrates WebGL/WebGPU shaders, and handles anti-overlapping label calculations (anti-cluttering) on the CPU.
   * **Technology:** TypeScript, WebGL 2.0 / WebGPU, Canvas 2D API.
4. **Olayer Native SDK (Rust):**
   * **Responsibility:** Wrapper for native desktop applications. Facilitates Core usage with local rendering engines.
   * **Technology:** Rust, optionally C/C++ bindings (`cbindgen`).

---

## 3. Level 3: Component Diagram (Internals of Core and SDK)

This diagram focuses on the internal modular organization of the **Olayer Core** and **Olayer TS SDK**, illustrating how components cooperate to perform cartographic projections and real-time rendering.

```mermaid
graph TB
    classDef component fill:#85B3D1,stroke:#668fa7,color:#000000,stroke-dasharray: 5 5,stroke-width:2px;
    classDef coreComponent fill:#E1F5FE,stroke:#0288D1,color:#01579B,stroke-width:2px;
    classDef wasmBridge fill:#FFF9C4,stroke:#FBC02D,color:#5D4037,stroke-width:2px;
    classDef nativeComponent fill:#C8E6C9,stroke:#388E3C,color:#1B5E20,stroke-width:2px;

    subgraph TS_SDK_Comp ["TypeScript SDK (Web)"]
        ts_controller["🎮 TS Controller<br>Loop (15/60 FPS) & Events"]:::component
        ts_layer_manager["🥞 TS Layer Manager<br>Composition and layer control"]:::component
        ts_map_data_stack["📥 TS Map Data Stack<br>Sources & Cache Manager (MVT/WMTS/DTED)"]:::component
        ts_gpu_pipe["🎨 WebGL/WebGPU Pipe<br>Static base map drawing"]:::component
        ts_cpu_pipe["🎯 WebGL/Canvas 2D Pipe<br>Symbols (Atlas) & Anti-clutter"]:::component
    end
    
    subgraph WASM_Bridge_Comp ["Web Interop"]
        wasm_bridge["🔗 Bridge WASM (wasm-bindgen)<br>TS/JS -> Rust memory bridge"]:::wasmBridge
    end

    subgraph Native_SDK_Comp ["Native SDK (Desktop)"]
        native_controller["🎮 Native Controller<br>Native loop & Window (winit)"]:::nativeComponent
        native_layer_manager["🥞 Native Layer Manager<br>Native layer composition and control"]:::nativeComponent
        native_map_data_stack["📥 Native Map Data Stack<br>Native Sources & Cache Manager"]:::nativeComponent
        native_gpu_pipe["🎨 wgpu Pipe (Matrix)<br>Terrain/background rendering (Vulkan/Metal/DX)"]:::nativeComponent
        native_cpu_pipe["🎯 wgpu Pipe (Vertex)<br>Symbols (Atlas) & Native anti-clutter"]:::nativeComponent
    end

    subgraph FFI_Bridge_Comp ["Native Interop"]
        ffi_bridge["🔗 C-FFI Bridge (cbindgen)<br>C-compatible exports for C++ Host"]:::wasmBridge
    end

    subgraph Rust_Core_Comp ["Agnostic Core Modules (Rust)"]
        geodesy["📐 Geodesy Module<br>ECEF/WGS84 geodetic conversions"]:::coreComponent
        camera["📷 Camera Module<br>CameraState management and View-Proj matrices for 2D/2.5D/3D"]:::coreComponent
        projections["🗺️ Projections Module<br>LCC, Stereographic, Web Mercator"]:::coreComponent
        terrain["⛰️ Terrain Engine (DTED)<br>Spatial index & O(1) Altitude"]:::coreComponent
        sld_parser["📄 SLD Parser<br>XML parser and symbol styles"]:::coreComponent
        symbol_registry["🎖️ Symbol Registry<br>Agnostic symbology registry and resolution"]:::coreComponent
        interpolator["⏱️ Target Interpolator<br>Dead Reckoning of dynamic targets"]:::coreComponent
    end

    %% TS SDK Relationships
    ts_controller --> ts_layer_manager
    ts_layer_manager --> ts_gpu_pipe
    ts_layer_manager --> ts_cpu_pipe
    ts_map_data_stack --> ts_controller
    ts_map_data_stack --> wasm_bridge
    ts_gpu_pipe --> wasm_bridge
    ts_cpu_pipe --> wasm_bridge

    %% Native SDK Relationships
    native_controller --> native_layer_manager
    native_layer_manager --> native_gpu_pipe
    native_layer_manager --> native_cpu_pipe
    native_map_data_stack --> native_controller
    native_map_data_stack --> ffi_bridge
    native_gpu_pipe --> ffi_bridge
    native_cpu_pipe --> ffi_bridge
    
    %% Internal WASM Bridge to Core
    wasm_bridge --> camera
    wasm_bridge --> terrain
    wasm_bridge --> sld_parser
    wasm_bridge --> symbol_registry
    wasm_bridge --> interpolator

    %% Internal FFI Bridge to Core
    ffi_bridge --> camera
    ffi_bridge --> terrain
    ffi_bridge --> sld_parser
    ffi_bridge --> symbol_registry
    ffi_bridge --> interpolator
    
    %% Direct Rust-to-Rust (Native SDK to Core)
    native_gpu_pipe --> camera
    native_gpu_pipe --> terrain
    native_cpu_pipe --> symbol_registry
    native_cpu_pipe --> interpolator

    %% Internal Rust Core Dependencies
    camera --> geodesy
    camera --> projections
    projections --> geodesy
    terrain --> geodesy
    interpolator --> geodesy
    symbol_registry --> sld_parser

    linkStyle 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33 stroke:#333,stroke-dasharray: 2 2;
```

### Component Details

#### 1. Rust Core Modules
* **[Geodesy Module](file:///c:/Users/rafae/projects/rust/olayer/core/src/geodesy):** Provides the mathematical functions based on the WGS84 reference ellipsoid. Performs bidirectional transformations between geographic coordinates $(\phi, \lambda, h)$ and Cartesian ECEF $(X, Y, Z)$.
* **[Camera Module](file:///c:/Users/rafae/projects/rust/olayer/core/src/camera):** Manages the three-dimensional geographic navigation state and camera attitude (center, zoom, bearing/yaw, pitch, roll) and calculates the View-Projection matrices for 2D, 2.5D, and 3D in a unified and performant manner.
* **[Projections Module](file:///c:/Users/rafae/projects/rust/olayer/core/src/projections):** Contains the mathematical formulas to project three-dimensional or geodetic points onto 2D planes. Implements the equations for Stereographic, LCC, and Mercator projections.
* **[Terrain Engine (DTED)](file:///c:/Users/rafae/projects/rust/olayer/core/src/terrain):** Manages DTED files in memory. Builds a simplified 2D spatial index (Grid) where each cell points to the loaded elevation bytes. Allows altitude queries at arbitrary coordinates to run in constant time $O(1)$.
* **[SLD Parser](file:///c:/Users/rafae/projects/rust/olayer/core/src/sld):** Syntactic parser (Parser) of XML that converts the OGC SLD (Styled Layer Descriptor) standard into structured style metadata.
* **[Symbol Registry](file:///c:/Users/rafae/projects/rust/olayer/core/src/symbol_registry):** Unified and agnostic symbology registry that resolves symbol codes (such as VOR or fighter jets) using simplified vector primitives generated from consolidated JSON library files. These JSON symbol files are pre-compiled from SVG files using the CLI tool `tools/symbol-compiler`. Rasterized symbols (PNG/JPG) are injected directly into the client SDK in the Texture Atlas, keeping the core lightweight and free of raster decoders.
* **[Target Interpolator](file:///c:/Users/rafae/projects/rust/olayer/core/src/interpolator):** Maintains the state table of dynamic targets in 3D geodetic space. For each target, records the last known state vector. Computes interpolated positions via 3D Dead Reckoning based on system time (WGS84 LatLon and heading), completely decoupled from screen projection.

#### 2. TypeScript SDK Components (Web Client)
* **TS Controller:** Controls the screen animation loop in the browser using `requestAnimationFrame` and manages dynamic FPS modulation (15 FPS idle / 60 FPS active).
* **TS Layer Manager:** Coordinates the layer stack (Layer Stack) on the Web, managing the optimized paint cycle with isolation of static and dynamic layers.
* **TS Map Data Stack:** Manages the web map data infrastructure. Implements the `MapDataSource` abstractions and manages sub-providers such as `VectorTileSource` (for MVT/GeoServer), `RasterTileSource` (WMTS/OpenStreetMap), and `TerrainTileSource` (dynamic terrain paging). Controls request queues, browser concurrency, and local LRU cache.
* **WebGL/WebGPU GPU Pipeline:** Binds static vertex buffers and renders on the GPU from $4 \times 4$ matrices sent by the WASM bridge.
* **WebGL/Canvas 2D CPU Pipeline:** Renders dynamic targets by resolving sprites in the GPU *Texture Atlas* and calculating label anti-overlapping.

#### 3. Native SDK Components (Desktop Client)
* **Native Controller:** Controls the native frame loop and manages local desktop window creation (using the `winit` crate or the host application's message loop).
* **Native Layer Manager:** Manages the native layer stack for visibility, blending, and repainting at the native level.
* **Native Map Data Stack:** Desktop equivalent of data infrastructure. Manages high-performance network connections (via `reqwest`), tactical format decoding, and efficient local disk I/O for DTED files.
* **wgpu GPU Pipeline:** Compiles pipelines and renders on the GPU (Vulkan, Metal, or DirectX 12) through the Rust `wgpu` library to draw 3D terrain and vector background maps.
* **wgpu CPU/Vertex Pipeline:** Renders dynamic targets on the desktop using instanced calls and *billboards* from a local texture atlas.

#### 4. Interoperability Layers (Bridges)
* **WASM Bridge (wasm-bindgen):** Memory transition and FFI bridge that exports Core Rust functions to the TypeScript/JavaScript format in the browser, using direct memory references.
* **C-FFI Bridge (cbindgen):** C-API export bridge (`libolayer_native.h`) generated by `cbindgen`, exposing interfaces compatible with direct binding for hosts in C, C++, or other compiled languages.

---

## 4. Level 4: Code and Process Flows (Sequence Diagrams)

### 4.1 Ping Ingestion and Dynamic Rendering Loop (FPS Throttling)

This diagram details how the system handles slow sensor data reception (usually 1 Hz) and renders it smoothly on the screen (15 to 60 FPS) using *Dead Reckoning*.

```mermaid
sequenceDiagram
    autonumber
    participant Host as Host App (TS / C++ / Rust)
    participant SDK as Olayer SDK (TS / Native)
    participant Core as Olayer Core (WASM / Native)
    participant GPU as GPU (WebGL / WebGPU / wgpu)

    Note over Host, Core: 1. Sensor Data Ingestion (Async ~1 Hz)
    Host->>SDK: updateTarget(id, latitude, longitude, altitude, heading, speed, timestamp)
    SDK->>Core: update_target(TargetState) (Via WASM or Native Link)
    Core->>Core: Saves to state registry (Interpolator)

    Note over Host, GPU: 2. Rendering Loop (Dynamic: 15 FPS idle / 60 FPS active)
    Host->>SDK: renderFrame(currentSystemTime, cameraState)
    
    rect rgb(230, 245, 255)
        Note over SDK, Core: Matrix Channel (GPU-oriented - Background and Terrain)
        SDK->>Core: get_view_projection_matrix(cameraState)
        Core-->>SDK: Matrix4x4 (LCC / Stereographic / ECEF)
        SDK->>GPU: Update Uniform / Render Pipeline ('u_viewProjMatrix')
        SDK->>GPU: DrawInstanced / DrawElements (Background Maps & Elevations)
    end

    rect rgb(255, 245, 230)
        Note over SDK, Core: Projected Vertex Channel (CPU-oriented - Targets and Symbols)
        SDK->>Core: interpolate_all(currentSystemTime)
        Core->>Core: Calculates Dead Reckoning in 3D geodetic (WGS84 ellipsoid)
        Core-->>SDK: List of interpolated targets [id, LatLon, heading_rad]
        SDK->>Core: project(LatLon) (For each target)
        Core-->>SDK: Screen Coordinates (X, Y)
        SDK->>SDK: Resolve symbols in Texture Atlas (ICAO/NATO) and execute Anti-cluttering
        SDK->>GPU: Render symbols (Billboards and Instanced sprites) and texts
    end
```

### 4.2 DTED Terrain Loading and Vertical Alert Processing (MSAW)

This diagram illustrates the loading of DTED files into memory and the calculation of vertical alerts and elevation profile, detailing the difference in data consumption between Web and Desktop.

```mermaid
sequenceDiagram
    autonumber
    participant Host as Host App (TS / C++ / Rust)
    participant SDK as Olayer SDK (TS / Native)
    participant Source as Terrain Source (HTTP / Disk)
    participant Core as Olayer Core (WASM / Native)

    Note over SDK, Source: Phase 1: Terrain Loading (On Demand)
    alt For Web Environment (TS Client)
        SDK->>Source: HTTP GET (DTED Tile)
        Source-->>SDK: ArrayBuffer (Binary data)
    else For Native Environment (Desktop Client)
        SDK->>Source: Local I/O Read (DTED file path)
        Source-->>SDK: Binary elevation buffer
    end
    SDK->>Core: load_dted_buffer(tileCoords, offset, length)
    Core->>Core: Parses DTED binary and inserts matrix into Grid Index
    Core-->>SDK: Success (Tile registered in Spatial Cache)

    Note over Host, Core: Phase 2: Real-time Alerts (MSAW)
    Host->>SDK: checkAltimetry(aircraftId)
    SDK->>Core: get_terrain_elevation(lat, lon)
    Core->>Core: O(1) access in active Grid Index cache
    Core-->>SDK: ground_altitude (meters WGS84)
    SDK->>SDK: Compare: (aircraft_alt - ground_altitude) < Safety Margin?
    SDK-->>Host: Returns MSAW Alert (True/False)

    Note over Host, Core: Phase 3: Vertical Profile Generation (2.5D View)
    Host->>SDK: getFlightVerticalProfile(routePoints, samplingStep)
    SDK->>Core: compute_vertical_profile(routePoints, samplingStep)
    loop For each sampled point on the route
        Core->>Core: Queries ground altitude in DTED indexer
    end
    Core-->>SDK: Array of profile coordinates [cumulative_distance, ground_altitude]
    SDK-->>Host: Returns data for 2.5D flight profile plotting
```

---

## 5. Critical Architectural Decisions (ADRs)

### ADR-001: Hybrid Rendering Pipeline (Matrices vs Vertices)
* **Context:** Drawing complex maps with geographic vectors generates millions of vertices. On the other hand, radar targets (airplanes) require fixedly rotated symbols and legible labels without 3D distortion (*Billboard* effect).
* **Decision:** The hybrid model was adopted.
  * The map background (MVT) and dense terrain are projected and rendered on the GPU using $4\times4$ matrix transformations computed in the Rust Core.
  * The airplane symbols and dynamic textual labels are projected from geodetic to 2D screen coordinates $(X,Y)$ in the Rust Core. The drawing itself occurs in a "flattened" and pixel-perfect manner on the screen, allowing efficient text anti-overlapping algorithms (anti-cluttering) on the CPU.
* **Consequence:** Excellent overall graphics performance combined with absolute readability and safety on ATC screens.

### ADR-002: Passive Resource Ingestion in Rust Core (WASM)
* **Context:** DTED terrain files and SLD styles reside on disk or external geographic servers. Code running in standard WebAssembly in browsers has severe security restrictions for native I/O (file system) and direct HTTP requests from the Rust Core could unnecessarily inflate the final binary.
* **Decision:** The Core in Rust is completely passive. It has no network drivers or disk readers. The TypeScript SDK downloads resources (MVT buffers, SLD XML files, and DTED ArrayBuffers) via native browser APIs (`fetch`) and injects the binary memory pointers into the methods exposed by WebAssembly.
* **Consequence:** Lightweight WASM binary, complete decoupling of data transport logic, and enhanced execution security.

### ADR-003: Motion Interpolation on the Client Side (Dead Reckoning)
* **Context:** Radar or ADS-B feeds arrive at the host application with intervals of 1 to 4 seconds. Updating aircraft on the screen directly at these pings will cause jerky animations and visual discomfort for controllers.
* **Decision:** Implement the kinematic estimation logic in the Core. The Host only reports the real positions with their historical timestamps. The Core performs the linear prediction calculation of the aircraft's current position based on the frame processing time and the reported speed/heading.
* **Consequence:** Continuous and smooth movement at 60 FPS, even under unstable networks or packet reception delays.

### ADR-004: WebAssembly Memory Lifecycle Management and Deallocation
* **Context:** WebAssembly (WASM) shares linear memory with JavaScript. Objects created in Rust (such as structs instantiated via `wasm-bindgen` wrapper) reside in the WASM heap and are not managed by the JavaScript Garbage Collector (GC). If the TypeScript SDK instantiates objects in Rust and loses references in JS without explicitly freeing them, the WASM memory will grow indefinitely, generating *out-of-memory* in long-duration executions (essential in ATC systems).
* **Decision:** The TypeScript SDK will implement strict lifecycle control of Rust/WASM objects.
  - Every structure created in Rust with a short lifecycle (e.g., discarded targets, quick query flight profiles) must have its `.free()` method explicitly invoked by the TS SDK.
  - For dense and variable-sized buffers (such as loaded DTED terrain grids), the SDK will manage a fixed-size cache with LRU (Least Recently Used) replacement policy. When a terrain tile is discarded from the cache, the SDK notifies the Rust Core to free the corresponding memory.
  - The Rust Core will use pre-allocated static vectors for highly dynamic data (such as the list of interpolated targets in the current frame), avoiding repeated memory allocations and deallocations at each rendering frame.
* **Consequence:** Long-term memory usage stability, predictable browser RAM consumption, and prevention of crashes due to memory exhaustion in continuous operational sessions.

### ADR-005: Display Layer Segregation and Graphics Optimization (Texture Atlases & Framebuffer Cache)
* **Context:** Drawing complete maps containing millions of static GIS polygons and relief textures together with dynamic targets in real-time at 60 FPS causes high overhead on the GPU and CPU due to frequent context changes and excessive draw calls. Complex military symbols (NATO APP-6) composed of multiple sub-vectors aggravate this problem if rendered individually each frame.
* **Decision:** The framework will adopt a layer-based segregated rendering strategy:
  - **Cycle Separation:** Static background map layers (MVT and elevation) will be rendered and composited into offscreen Framebuffers (Offscreen Render Targets) only when the camera undergoes physical changes. If the screen is static, the GPU only performs a quick redraw of this cached texture (*blitting*).
  - **Dynamic Texture Atlas:** Complex symbols decoded by the `Symbol Registry` will be rasterized once on the CPU and injected into a common Texture Atlas on the GPU.
  - **Instancing:** To draw thousands of aircraft and targets, the SDK will send a single buffer of dynamic data and perform one instanced draw call (`drawElementsInstanced`) based on the texture offsets of the Atlas, reducing thousands of draw calls to just one.
* **Consequence:** High frame rate (stable 60 FPS), free CPU time on the main thread for tactical processing, and very low battery/resource consumption on static monitoring panels.

### ADR-006: Importing and Resolving Custom Symbols (SVG and PNG)
* **Context:** In addition to standard procedural professional symbologies (ICAO/NATO), the host application needs to inject and render custom icons provided in vector (SVG) or rasterized (PNG) formats. The framework requires a workflow that unifies these external sources and maintains rendering consistency and performance in 2D and 3D visualizations.
* **Decision:** The responsibility for importing formats was separated by asset type, optimizing performance and keeping the Rust Core/WASM lightweight:
  - **Vector Symbols (SVG):** To avoid the computational cost and heavy dependencies of XML/SVG parsing at runtime in WASM, SVG files are processed at build time using the CLI tool **`tools/symbol-compiler`**. This tool maps SVG vector elements to pure Olayer primitives in a consolidated JSON file that is fed into the Core's `DeclarativeProvider` at runtime.
  - **Rasterized Symbols (PNG/JPG):** Rasterized images do not go through the Core. PNG/JPG loading is delegated to the TypeScript SDK via the `TextureAtlasManager::registerImageSymbol` method, which uses the browser's native image loading APIs and draws pixels directly into the Texture Atlas's offscreen canvas for submission to the GPU.
  - **Unification in 2D/3D Streams:** Once loaded into the Texture Atlas with their respective UV coordinates, imported symbols use the same instanced rendering pipeline. In the 2D flow, they are drawn as common flat sprites. In the 3D flow, they are rendered using *Billboard Shaders* that align the flat coordinates to the camera, preventing 3D perspective distortions and ensuring readability.
* **Consequence:** Full visual customization flexibility, zero overhead of heavy decoders or file interpreters in the WASM Core, and consistent high-performance rendering of thousands of simultaneous symbols.

---

## 6. Directory Structure Mapping with Components

The proposed physical repository structure is organized according to the architecture's division of responsibilities:

```text
olayer/
├── core/                         # [C4 Component: Olayer Core Engine]
│   ├── Cargo.toml
│   └── src/
│       ├── geodesy/              # Geodetic Formulas and ECEF Module (WGS84)
│       │   └── mod.rs
│       ├── projections/          # Stereographic, LCC, and Mercator Implementations
│       │   └── mod.rs
│       ├── terrain/              # DTED File Parsing and O(1) Altitude Index
│       │   └── mod.rs
│       ├── sld/                  # XML Parser for SLD Styling
│       │   └── mod.rs
│       └── interpolator/         # Dead Reckoning Logic for Target Tracking
│           └── mod.rs
│
sdk/
├── ts/                       # [C4 Component: Olayer TS SDK]
│   ├── package.json
│   ├── src/
│   │   ├── controller/       # Loop Management, FPS Throttler, and Events
│   │   ├── providers/        # WMTS, MVT, SLD network calls, and DTED injection
│   │   ├── renderer/         # WebGL Renderer (GPU) and Canvas (CPU)
│   │   └── index.ts          # Public TypeScript SDK API
│   ├── tsconfig.json
│   └── wasm/                 # [C4 Component: WASM Bindings Layer]
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs        # Exports with #[wasm_bindgen] for TS SDK
│
└── native/                   # [C4 Component: Olayer Native Environment]
    ├── c_ffi_bridge/         # [C4 Component: C-FFI Bridge]
    │   ├── Cargo.toml
    │   └── src/
    │       └── lib.rs        # C-compatible Exports / cbindgen header
    │
    └── desktop/              # [C4 Component: Olayer Native SDK & Demo]
        ├── Cargo.toml
        └── src/
            ├── lib.rs        # Native static interface / FPS throttler
            └── main.rs       # Native demo wgpu + winit + egui

```

---

## 7. Next Steps for Architecture Validation

To ratify the premises of this architecture document, the following experimental activities are planned:
1. **Mathematical Validation (Geodesy):** Creation of unit tests in the `geodesy` module comparing the geodetic distance between known airports calculated by the core with the official WGS84 reference model.
2. **WASM-TS Bound Benchmark:** Measurement of data transfer latency when loading 1MB DTED buffers between the TypeScript stack and the WASM linear memory to confirm the absence of bottlenecks at the edge.
3. **Dynamic Projection Test:** Rendering of a test sector with rapid runtime switching from Lambert Conformal Conic to Azimuthal Stereographic to ensure correct matrix and vertex updates.
