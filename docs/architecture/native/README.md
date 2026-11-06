# Native SDK architecture

The native SDK owns controller/provider integration, GPU resources and C-FFI around the pure engines. Hosts own the window, device, render attachments, event loop and submission. The desktop demo is one host, not the SDK's mandatory execution environment.

| Component | Design |
|---|---|
| Controller and mode/camera state | [Native controller](native_controller/arch.md) |
| Layer ordering/visibility | [Layer manager](native_layer_manager/arch.md) |
| HTTP/filesystem adapters and source caches | [Map data stack](native_map_data_stack/arch.md) |
| Grid, terrain, raster and WFS rendering | [WGPU pipeline](wgpu_gpu_pipeline/arch.md) |
| CPU projection and target drawing | [CPU/vertex pipeline](wgpu_cpu_vertex_pipeline/arch.md) |
| C interoperability | [C-FFI bridge](c_ffi_bridge/arch.md) |
| Tactical tools | [Tools](tools/arch.md) |

Use the [native API reference](../../reference/native.md) for signatures, [map guides](../../guides/maps/README.md) for WMS/WFS/WCS, and [shared depth](../../guides/maps/depth.md) for compatible native render passes.

## Native Interaction and Flow Diagram

The diagram below illustrates the high-level cooperation of the native components during a typical radar tactical rendering frame:

```mermaid
sequenceDiagram
    autonumber
    participant Host as Host C++/Rust Application
    participant Ctrl as Native Controller
    participant Tools as Tactical Tools
    participant DS as Map Data Stack
    participant GPU as wgpu GPU Pipeline (Grid/Map)
    participant CPU as wgpu CPU/Vertex Pipeline (Targets)

    %% Ingestion
    Host->>DS: Inject new radar pings (target_update)
    DS->>Ctrl: Update data in InterpolationEngine
    Host->>Tools: Query tactical tools (RBL, PPL, Holding, ILS)
    
    %% Rendering Loop
    rect rgb(240, 248, 255)
        Note over Ctrl, Host: Redraw Loop (Frame Rendering)
        Ctrl->>Ctrl: get_target_fps() (Adjusts FPS based on state)
        Ctrl->>GPU: Update View-Projection Matrix (Uniform Buffer)
        
        %% Render Static Layers / Grid
        Ctrl->>GPU: Render grid lines (Line Draw Call)
        
        %% Render Dynamic Layers / Targets
        Ctrl->>Ctrl: interpolate_all(current_time)
        Ctrl-->>CPU: List of interpolated targets
        
        loop For each target
            Ctrl->>CPU: project_lla_to_screen(target_lla)
            CPU-->>CPU: Screen coordinates X, Y (2D/2.5D/3D)
            CPU->>CPU: Plot icons, heading vector, and data boxes (egui/canvas)
        end
        
        CPU->>Host: Presents the finished frame on the user screen
    end
```
