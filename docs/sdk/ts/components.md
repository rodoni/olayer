# TypeScript SDK (Web)
## TypeScript SDK Components (C4 Model - Level 3)

This document presents the high-level organization of the **Olayer TS SDK** components (located in `sdk/ts`), serving as a navigation map for the architecture documentation of each specific submodule.

---

## 1. TS SDK Component Diagram

The diagram below details the internal structure of the TypeScript SDK and its interactions with the WebAssembly bridge, the Host application, and the browser's graphics APIs.

```mermaid
graph TB
    classDef host fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;
    classDef jsComponent fill:#FFF9C4,stroke:#FBC02D,color:#5D4037,stroke-width:2px;
    classDef wasmBridge fill:#FFE082,stroke:#FFB300,color:#5D4037,stroke-width:2px;
    classDef browser fill:#E1F5FE,stroke:#0288D1,color:#01579B,stroke-width:2px;

    %% Host App
    host["📱 Host App Web<br>[TypeScript/React/Vue]"]:::host

    subgraph TS_SDK_Boundary ["Olayer TS SDK (sdk/ts)"]
        %% Main Components
        controller["🎮 TS Controller<br>[Component]<br>Manages the animation loop, camera events, and FPS control."]:::jsComponent
        layer_manager["🥞 Layer Manager<br>[Component]<br>Manages the layer stack (Layer Stack) and segregates repainting."]:::jsComponent
        map_data_stack["📥 TS Map Data Stack<br>[Component]<br>Manages map data infrastructure, data sources, and caches."]:::jsComponent
        
        %% Graphics Pipeline
        gpu_pipeline["🎨 GPU Render Pipeline<br>[Component]<br>Rendering of terrain meshes and static maps (WebGL2)."]:::jsComponent
        cpu_pipeline["🎯 CPU/Target Pipeline<br>[Component]<br>Target projection, anti-cluttering, and label drawing."]:::jsComponent
        atlas_manager["🖼️ Texture Atlas Manager<br>[Component]<br>Compiles and compacts symbols (SVG, PNG, procedural) on the GPU."]:::jsComponent
    end

    %% Edge Elements
    wasm_bridge["🔗 Bridge WASM (wasm-bindgen)<br>[WASM Interop]"]:::wasmBridge
    canvas_2d["🖥️ Canvas 2D API<br>[Browser API]"]:::browser
    webgl_ctx["🎮 WebGL2 / WebGPU Context<br>[Browser API]"]:::browser

    %% Input and Output Flows
    host -->|1. Configures and interacts| controller
    host -->|2. Sends radar pings| controller
    controller -->|Registers targets| wasm_bridge
    map_data_stack -->|3. Injects map and relief binaries| wasm_bridge

    %% Internal SDK Flows
    controller -->|Cycle coordinator| layer_manager
    layer_manager -->|Paints background and terrain| gpu_pipeline
    layer_manager -->|Paints targets and labels| cpu_pipeline
    
    gpu_pipeline -->|Queries matrices and terrain| wasm_bridge
    cpu_pipeline -->|Queries interpolated positions| wasm_bridge
    cpu_pipeline -->|Requires UV coordinates| atlas_manager

    %% Physical rendering
    gpu_pipeline -->|Draws on buffer| webgl_ctx
    cpu_pipeline -->|Draws on buffer| canvas_2d
    atlas_manager -->|Generates and updates texture| webgl_ctx

    linkStyle 0,1,2,3,4,5,6,7,8,9,10,11,12 stroke:#555,stroke-width:1.5px;
```

---

## 2. Submodule Details and Architecture

Each main SDK component is documented in a detailed architecture file (`arch.md`) located in its respective technical specification directory:

### 🎮 2.1 TS Controller
Unified entry point of the SDK. Acts as the maestro of the lifecycle, main loop, and dynamic FPS throttling.
* Complete technical detail: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/controller/arch.md)

### 🥞 2.2 Layer Manager
Coordinator of the layer stack (Layer Stack), responsible for ordering and rendering optimization segregation between dynamic and static elements.
* Complete technical detail: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/layers/arch.md)

### 📥 2.3 Map Data Stack (Providers)
Module responsible for on-demand loading, paging, and intelligent caching (with LRU policy) of cartographic data (MVT, WMTS) and terrain (DTED).
* Complete technical detail: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/providers/arch.md)

### 🎨 2.4 Render Pipelines & Texture Atlas
Graphic drawing engines. Contains the GPU rendering pipeline (WebGL2), the CPU radar pipeline (with anti-overlap algorithm/anti-cluttering), and the Texture Atlas Manager.
* Complete technical detail: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/renderer/arch.md)
