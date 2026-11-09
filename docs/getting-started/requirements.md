# Build Requirements and Dependencies

This document provides the complete list of system packages, toolchains, runtime libraries, and language dependencies required to compile **Olayer Core**, the **TypeScript SDK & Web Demo**, and the **Native Desktop SDK & Demo**.

---

## 1. Toolchains & Core Runtimes

| Toolchain | Minimum Version | Purpose |
| :--- | :--- | :--- |
| **Rust & Cargo** | Current stable (Edition 2021; no declared/tested MSRV) | Compiles Olayer Core, WASM bridge, Native SDK, and Desktop Demo. |
| **wasm-pack** | `0.12+` | Compiles `sdk/ts/wasm` into an npm-compatible WebAssembly module. |
| **Node.js** | 22.13+ in 22.x, or 24+ | Runs Vite dev server, Vitest, and builds the TypeScript SDK. |
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
   nvm install 24
   nvm use 24
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
- The Core GeoTIFF decoder is implemented in the terrain module; `flate2` supplies decompression. The native image decoder separately supports TIFF.
- `flate2`: DEFLATE compression support for GeoTIFF and compressed terrain tiles.
- `thiserror`: Ergonomic strongly-typed error definitions across geodetic, terrain, and projection subsystems.

### WebAssembly Bridge (`sdk/ts/wasm/Cargo.toml`)
- `wasm-bindgen (^0.2)`: Interop layer exporting Rust data structures and methods to JavaScript/TypeScript.
- `js-sys`, `serde-wasm-bindgen`: JavaScript interop and serialized boundary values.

### Native SDK & Desktop Demo (`sdk/native` & `sdk/native/demo`)
- `wgpu (0.19)`: Cross-platform WebGPU-based native rendering (DirectX 12, Vulkan, Metal).
- `winit (0.29)`: Window creation, event loop, and OS input handling.
- `egui (0.27)`, `egui-wgpu`, `egui-winit`: Immediate-mode graphical user interface for camera and layer controls.
- `resvg (0.38)`, `usvg (0.38)`: Native SVG rasterizer for tactical military symbology.
- `bytemuck (1.16)`: Safe zero-copy GPU memory casting.
- `pollster (0.3)`: Synchronous blocking execution for native async GPU device requests.
- `env_logger (0.11)`, `log (0.4)`: Configurable logging backend.

---

## 5. Build and verification entry points

Use [getting started](README.md) for the single maintained build sequence and [testing](../guides/testing.md) for unit, browser, GPU and benchmark commands. Build WASM before installing the local TypeScript dependency. Exclude the desktop demo from headless workspace operations.

The runtime floor above follows the current dependency engines: jsdom 29 requires Node 22.13+ in the 22.x line or 24+, while Puppeteer 25 requires at least 22.12. CI selects Node 24 to match these requirements. Rust is built on stable in CI; the old 1.75 minimum was not maintained and is not an MSRV guarantee.

Package manifests/lockfiles are authoritative for dependencies. The platform package recipes above are setup examples; this documentation cleanup does not certify every distribution/toolchain combination.
