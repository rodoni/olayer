//! Olayer Core — Geospatial computation engine for aviation and tactical display systems.
//!
//! This crate provides the mathematical and logical foundation for the Olayer
//! framework, written in Rust and designed for deployment via native binaries
//! (FFI) and WebAssembly (WASM).
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `geodesy` | WGS84 coordinate conversions, local frames (ENU/NED), WMM-2025 magnetism, geodesic spatial analysis (containment, XTK/ATD, buffers), geodetic solvers |
//! | `projections` | Cartographic projections (LCC, Stereographic, Web Mercator) |
//! | `terrain` | DTED elevation parsing, bilinear interpolation, vertical profiles |
//! | `sld` | OGC Styled Layer Descriptor (SLD) XML parser |
//! | `symbol_registry` | Pluggable symbology resolver (NATO / ICAO / declarative JSON) |
//! | `interpolator` | Dead-reckoning target interpolation for sensor fusion |
//! | `aeronautical` | Aeronautical data ingestion (AIXM 5.1 & GeoJSON-Aviation) |
//! | `weather` | Meteorological overlays (dBZ radar colorization, aviation wind barbs, Marching Squares isolines, SIGMETs) |
//! | `volumetric` | 3D volumetric airspace polyhedrons and 3D flight trajectory ribbon mesh generation |

pub mod geodesy;
pub mod camera;
pub mod projections;
pub mod sld;
pub mod symbol_registry;
pub mod terrain;
pub mod interpolator;
pub mod aeronautical;
pub mod weather;
pub mod volumetric;
