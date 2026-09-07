# Build Requirements and Dependencies

This document provides the complete list of system packages, toolchains, runtime libraries, and language dependencies required to compile **Olayer Core**, the **TypeScript SDK & Web Demo**, and the **Native Desktop SDK & Demo**.

---

## 1. Toolchains & Core Runtimes

| Toolchain | Minimum Version | Purpose |
| :--- | :--- | :--- |
| **Rust & Cargo** | `1.75+` (Edition 2021) | Compiles Olayer Core, WASM bridge, Native SDK, and Desktop Demo. |
| **wasm-pack** | `0.12+` | Compiles `sdk/ts/wasm` into an npm-compatible WebAssembly module. |
| **Node.js** | `v18.x`, `v20.x`, or `v22.x` (LTS recommended) | Runs Vite dev server, Vitest, and builds the TypeScript SDK. |
| **npm** | `v9+` | Package manager for `sdk/ts` and `tools/symbol-compiler`. |

### Installing Core Toolchains

1. **Rust:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup target add wasm32-unknown-unknown
   ```

2. **wasm-pack:**
   ```bash
   cargo install wasm-pack
   # Or install via pre-built binaries:
   # curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
   ```

3. **Node.js & npm:**
   Install Node.js via [nodejs.org](https://nodejs.org/) or using `nvm`:
   ```bash
   nvm install 20
   nvm use 20
   ```

---

## 2. Operating System & Native Graphics Dependencies

The Native Desktop Demo (`sdk/native/demo`) utilizes `wgpu`, `winit`, and `egui`, requiring native windowing, input, and GPU driver headers.

### Linux

#### Ubuntu / Debian / Pop!_OS / Linux Mint
```bash
sudo apt-get update && sudo apt-get install -y \
    build-essential \
    pkg-config \
    cmake \
    clang \
    libclang-dev \
    libx11-dev \
    libxcb1-dev \
    libxcursor-dev \
    libxrandr-dev \
    libxi-dev \
    libxinerama-dev \
    libxkbcommon-dev \
    libwayland-dev \
    libasound2-dev \
    libudev-dev \
    libvulkan-dev \
    vulkan-tools \
    mesa-vulkan-drivers
```

#### Fedora / RHEL / CentOS Stream
```bash
sudo dnf install -y \
    gcc \
    gcc-c++ \
    pkgconf-pkg-config \
    cmake \
    clang-devel \
    libX11-devel \
    libxcb-devel \
    libXcursor-devel \
    libXrandr-devel \
    libXi-devel \
    libXinerama-devel \
    libxkbcommon-devel \
    wayland-devel \
    alsa-lib-devel \
    systemd-devel \
    vulkan-headers \
    vulkan-loader-devel \
    mesa-vulkan-drivers
```

#### Arch Linux / Manjaro
```bash
sudo pacman -S --needed \
    base-devel \
    pkgconf \
    cmake \
    clang \
    libx11 \
    libxcb \
    libxcursor \
    libxrandr \
    libxi \
    libxinerama \
    libxkbcommon \
    wayland \
    alsa-lib \
    vulkan-headers \
    vulkan-icd-loader \
    vulkan-tools
```

### Windows (10 / 11)

- **C++ Compiler:** Visual Studio 2022 (Community or Build Tools) with the **"Desktop development with C++"** workload installed (includes MSVC C++ compiler, Windows 10/11 SDK, and CMake tools).
- **Graphics API:** DirectX 12 is supported out-of-the-box on Windows 10/11. Updated GPU vendor display drivers (NVIDIA, AMD, or Intel) are recommended.

### macOS (12+ Monterey, Ventura, Sonoma)

- **Command Line Tools:**
  ```bash
  xcode-select --install
  ```
- **Graphics API:** Metal backend is natively supported on Apple Silicon (M1/M2/M3/M4) and modern Intel Macs.

---

## 3. Node.js & Web Packages

### TypeScript SDK & Web Demo (`sdk/ts`)

Located in `sdk/ts/package.json`:

#### Runtime Dependencies
- `@mapbox/vector-tile (^3.0.0)`: Parsing Mapbox Vector Tiles (MVT/PBF).
- `pbf (^5.1.2)`: Protobuf decoding for binary vector tiles.
- `olayer-wasm (file:./wasm/pkg)`: Locally compiled WebAssembly bindings bridging Rust Core to TypeScript.

#### Development & Build Dependencies
- `typescript (^5.4.5)`: TypeScript compiler and type checking.
- `vite (^5.2.11)`: Fast development server with Hot Module Replacement (HMR) and bundling.
- `vite-plugin-wasm (^3.0.1)`: Direct WebAssembly loading in Vite.
- `vite-plugin-top-level-await (^1.4.1)`: Asynchronous WASM initialization support.
- `vitest (^4.1.8)`: Test runner for headless unit and integration tests.
- `canvas (^3.2.3)`: Node.js Cairo-backed canvas for headless tests.
- `jsdom (^29.1.1)`: DOM environment for browser-agnostic tests.
- `puppeteer (^25.1.0)`: Automated end-to-end browser testing.

### SVG Symbol Compiler CLI (`tools/symbol-compiler`)

Located in `tools/symbol-compiler/package.json`:

- `commander (^12.1.0)`: CLI option parsing and argument handling.
- `fast-xml-parser (^4.4.0)`: High-performance SVG XML parser converting vector assets into Olayer declarative JSON libraries.
- `typescript (^5.4.5)`: TypeScript toolchain for compiling the CLI utility.

---

## 4. Rust Workspace Crates

Managed via Cargo in `Cargo.toml`:

### Core Engine (`core/Cargo.toml`)
- `serde`, `serde_json`: High-performance serialization/deserialization for declarative symbols, AIXM GeoJSON, and terrain formats.
- `quick-xml`: Fast XML streaming parser for AIXM 5.1 and SLD descriptors.
- `roxmltree`: Read-only XML DOM tree parser for complex symbology rules.
- `tiff`: Pure-Rust TIFF / Cloud-Optimized GeoTIFF (COG) elevation decoder.
- `flate2`: DEFLATE compression support for GeoTIFF and compressed terrain tiles.
- `thiserror`: Ergonomic strongly-typed error definitions across geodetic, terrain, and projection subsystems.

### WebAssembly Bridge (`sdk/ts/wasm/Cargo.toml`)
- `wasm-bindgen (^0.2)`: Interop layer exporting Rust data structures and methods to JavaScript/TypeScript.
- `js-sys`, `web-sys`: Web API interfaces (performance timers, console logging).

### Native SDK & Desktop Demo (`sdk/native` & `sdk/native/demo`)
- `wgpu (0.19)`: Cross-platform WebGPU-based native rendering (DirectX 12, Vulkan, Metal).
- `winit (0.29)`: Window creation, event loop, and OS input handling.
- `egui (0.27)`, `egui-wgpu`, `egui-winit`: Immediate-mode graphical user interface for camera and layer controls.
- `resvg (0.38)`, `usvg (0.38)`: Native SVG rasterizer for tactical military symbology.
- `bytemuck (1.16)`: Safe zero-copy GPU memory casting.
- `pollster (0.3)`: Synchronous blocking execution for native async GPU device requests.
- `env_logger (0.11)`, `log (0.4)`: Configurable logging backend.

---

## 5. Build and Compilation Guide

### Order of Compilation

Because the TypeScript SDK depends on the locally compiled WebAssembly output, follow this sequential build order:

```text
1. sdk/ts/wasm           --> wasm-pack build --target web
2. sdk/ts                --> npm install
3. Workspace (Native)    --> cargo build --workspace
```

### Step-by-Step Instructions

#### 1. Compile WebAssembly & Run the TypeScript Web Demo
```bash
# Step 1: Build the WebAssembly module
cd sdk/ts/wasm
wasm-pack build --target web

# Step 2: Install TypeScript SDK dependencies and start the dev server
cd ..
npm install
npm run dev
```
Open your browser at: **`http://localhost:3000/demo/index.html`**

#### 2. Run Headless Tests
```bash
# Run Rust Core, WASM, and Native tests (headless)
cargo test --workspace --exclude olayer-desktop-demo

# Run TypeScript SDK unit tests
cd sdk/ts
npm run test:run
```

#### 3. Build and Run the Native Desktop Demo
```bash
# From workspace root (requires windowing system and GPU)
cargo run -p olayer-desktop-demo --release
```

#### 4. Compile the Symbol Compiler CLI
```bash
cd tools/symbol-compiler
npm install
npm run build
```

---

## 6. Troubleshooting & Common Pitfalls

- **Module Not Found `olayer-wasm`:** Ensure `wasm-pack build --target web` was executed inside `sdk/ts/wasm` *before* running `npm install` in `sdk/ts`.
- **Desktop Demo Crash on Headless Servers / CI:** The desktop demo requires a valid GPU context and active display server (X11 / Wayland on Linux, DX12 on Windows, Metal on macOS). In headless CI environments, exclude the demo using `--exclude olayer-desktop-demo`.
- **Vulkan Validation / Hook Conflicts on Windows:** The desktop demo automatically defaults to DirectX 12 (`wgpu::Backends::DX12`) on Windows to bypass third-party Vulkan overlay hooks (such as Discord, OBS, or RivaTuner).
- **Missing Audio / X11 Headers on Linux:** If `cargo build` fails on Linux while linking `winit` or `cpal`/`alsa`, install the `libx11-dev`, `libasound2-dev`, and `libudev-dev` packages.
