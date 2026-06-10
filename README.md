# Olayer: Hybrid GIS Framework for Air Traffic Control (ATC)

Olayer is a mission-critical hybrid GIS framework designed for Air Traffic Control (ATC) scenarios. It provides robust 2D, 2.5D (flight profile perspective), and 3D (digital globe) map projections, high-performance tactical symbology rendering, and real-time target tracking and projection.

The architecture is multi-language, combining a high-performance, geodetically precise core written in **Rust** with high-level web rendering SDKs written in **TypeScript** (via WebAssembly) and native desktop environments.

---

## 🛠️ Repository Architecture

The project is structured as a monorepo containing the following components:

```text
├── core/                  # Pure Rust Core (Agnostic & Mathematical Engine)
│   ├── src/
│   │   ├── geodesy/       # Geodetic formulas, WGS84 ellipsoid, and ECEF coordinates
│   │   ├── camera/        # CameraState and View-Projection matrix generators for 2D/2.5D/3D
│   │   ├── terrain/       # DTED file parsing and O(1) elevation query indexing
│   │   ├── sld/           # Styled Layer Descriptor (SLD) XML rules parser
│   │   └── projections/   # Cartographic projections (Stereographic, LCC, Mercator)
│   └── benches/           # Performance benchmarks (geodesy, projections)
│
├── bindings/
│   ├── wasm/              # WebAssembly bridge (wasm-bindgen) exposing Core to TypeScript
│   └── c_ffi/             # Native C/FFI bindings for native desktop integrations
│
└── sdk/
    └── ts/                # TypeScript Client SDK for Browsers (WebGL2 + Canvas 2D)
        ├── src/           # SDK source files (LayerManager, Controller, etc.)
        └── demo/          # Interactive web demo application
```

---

## ✨ Features

- **High-Precision Geodesy Engine:** All kinematic math, camera locations, and physical positions are calculated in double-precision 64-bit float (`f64`) on the WGS84 ellipsoid.
- **Cartographic Projections:**
  - **Stereographic Azimuthal:** Preserves local angles, ideal for Terminal Maneuvering Area (TMA) radar displays.
  - **Lambert Conformal Conic (LCC):** Minimizes distortion along flight routes, optimal for En-Route displays.
  - **Mercator / Web Mercator:** Universal mapping projection.
- **Dynamic Camera & Multi-view Modes:**
  - **2D View:** Traditional flat orthographic map with rotation (bearing).
  - **2.5D View:** Tilted map perspective (supporting pitch/tilt from `-180°` to `180°` and roll).
  - **3D View:** Full virtual digital globe projection using Earth's ellipsoidal curvature.
- **Hybrid Rendering Pipeline:**
  - **GPU-Oriented Layer:** Render dense maps, vector tiles (MVT), and raster backgrounds efficiently using WebGL.
  - **CPU-Oriented Layer:** Interpolates aircraft targets (Dead Reckoning) on the WGS84 ellipsoid and projects positions to pixel space for pixel-perfect data blocks and symbols.
- **Symbology & Style Engine:**
  - Support for civil navigation aids (VOR, DME, TACAN, NDB) and runways.
  - NATO APP-6 / MIL-STD-2525 military symbol generator using procedurally assembled SIDC.
  - Dynamic Texture Atlas compilation supporting SVG and PNG custom icon imports.
- **Dynamic FPS Throttling:** Auto-scales frame rates (e.g., drops to 15 FPS when idle and ramps up to 60 FPS on interaction) to optimize energy and GPU resource usage.

---

## 🚀 Getting Started

### Prerequisites

- **Rust & Cargo:** For compiling the Core mathematical engine.
- **wasm-pack:** For building the WebAssembly module.
- **Node.js & npm:** For compiling and running the TypeScript SDK and Demo.

### 1. Build the WebAssembly Bindings

Compile the Rust core into WebAssembly npm-ready package:

```bash
# Navigate to the WASM bindings directory
cd bindings/wasm

# Build the WebAssembly package
wasm-pack build --target web
```

This outputs a compiled package under `bindings/wasm/pkg` which the TypeScript SDK references.

### 2. Set Up the TypeScript SDK & Demo

Install dependencies and run the local development server:

```bash
# Navigate to the TypeScript SDK directory
cd ../../sdk/ts

# Install project dependencies
npm install

# Start the Vite development server
npm run dev
```

By default, the Vite server will run at:
👉 **`http://localhost:3000/demo/index.html`**

---

## 📜 License

This project is licensed under the **BSD 2-Clause License**.
