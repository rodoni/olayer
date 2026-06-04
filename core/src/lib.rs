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
//! | `geodesy` | WGS84 coordinate conversions, geodetic solvers (Vincenty / Haversine) |
//! | `projections` | Cartographic projections (LCC, Stereographic, Web Mercator) |
//! | `terrain` | DTED elevation parsing, bilinear interpolation, vertical profiles |
//! | `sld` | OGC Styled Layer Descriptor (SLD) XML parser |
//! | `symbol_registry` | Pluggable symbology resolver (NATO / ICAO / declarative JSON) |
//! | `interpolator` | Dead-reckoning target interpolation for sensor fusion |

pub mod geodesy;
pub mod projections;
pub mod sld;
pub mod symbol_registry;
pub mod terrain;
pub mod interpolator;
