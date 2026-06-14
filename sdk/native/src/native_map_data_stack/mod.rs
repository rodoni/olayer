use std::collections::HashMap;
use olayer_core::terrain::TerrainEngine;

// =============================================================================
// 1. DATA SOURCE TRAIT
// =============================================================================

/// Common interface for all native map data providers.
///
/// Used by `NativeMapDataStack` to register multiple sources (terrain, raster,
/// vector) behind a unified registry.
pub trait MapDataSource {
    /// Unique identifier for this data source.
    fn id(&self) -> &str;
    /// Clears the local provider cache.
    fn clear_cache(&mut self);
    /// Returns the number of cached items.
    fn cache_size(&self) -> usize;
}

// =============================================================================
// 2. MAP DATA STACK
// =============================================================================

/// Handles local disk I/O and buffering for geospatial data.
///
/// The native Map Data Stack is the infrastructure data layer of the native SDK.
/// It provides:
///
/// 1. **Generic registry** — `register_source` / `get_source` / `clear_cache`
///    for multiple data sources via the `MapDataSource` trait.
/// 2. **Direct DTED helpers** — `load_dted_file` / `load_dted_buffer` for
///    ad-hoc terrain loading into an existing `TerrainEngine`.
///
/// Unlike the Web SDK (which consumes tiles via HTTP), the native stack reads
/// directly from the local filesystem.
#[derive(Default)]
pub struct NativeMapDataStack {
    sources: HashMap<String, Box<dyn MapDataSource>>,
}

impl NativeMapDataStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a data source in the stack.
    ///
    /// Returns `Err` if a source with the same ID already exists.
    pub fn register_source(&mut self, source: Box<dyn MapDataSource>) -> Result<(), String> {
        let id = source.id().to_string();
        if self.sources.contains_key(&id) {
            return Err(format!("Data source '{}' already registered.", id));
        }
        self.sources.insert(id, source);
        Ok(())
    }

    /// Retrieves a registered data source by its identifier.
    pub fn get_source(&self, id: &str) -> Option<&dyn MapDataSource> {
        self.sources.get(id).map(|b| b.as_ref())
    }

    /// Clears the caches of all registered data sources.
    pub fn clear_cache(&mut self) {
        for source in self.sources.values_mut() {
            source.clear_cache();
        }
    }

    /// Returns the aggregate cache size across all registered sources.
    pub fn get_cache_size(&self) -> usize {
        self.sources.values().map(|s| s.cache_size()).sum()
    }

    // =========================================================================
    // Direct terrain helpers (backward compatible with existing main.rs usage)
    // =========================================================================

    /// Loads a DTED tile from a file path into the given terrain engine.
    pub fn load_dted_file(&self, path: &str, terrain: &mut TerrainEngine) -> Result<(), String> {
        let data =
            std::fs::read(path).map_err(|e| format!("Failed to read DTED file: {}", e))?;
        terrain
            .load_tile(&data)
            .map_err(|e| format!("Failed to parse DTED data: {:?}", e))?;
        Ok(())
    }

    /// Loads a DTED tile from a raw buffer into the given terrain engine.
    pub fn load_dted_buffer(&self, buffer: &[u8], terrain: &mut TerrainEngine) -> Result<(), String> {
        terrain
            .load_tile(buffer)
            .map_err(|e| format!("Failed to parse DTED data: {:?}", e))?;
        Ok(())
    }
}

// =============================================================================
// 3. TERRAIN DATA SOURCE (concrete implementation)
// =============================================================================

/// A concrete `MapDataSource` that wraps a `TerrainEngine` and tracks loaded
/// tiles so it can implement `clear_cache` and `cache_size`.
///
/// This is the native equivalent of `TerrainTileSource` in the Web SDK.
pub struct TerrainDataSource {
    id: String,
    engine: TerrainEngine,
    loaded_tiles: Vec<(i32, i32)>,
}

impl TerrainDataSource {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            engine: TerrainEngine::new(),
            loaded_tiles: Vec::new(),
        }
    }

    /// Loads a DTED tile from a file path into the internal engine.
    pub fn load_file(&mut self, path: &str) -> Result<(), String> {
        let data =
            std::fs::read(path).map_err(|e| format!("Failed to read DTED file: {}", e))?;
        let key = self
            .engine
            .load_tile(&data)
            .map_err(|e| format!("Failed to parse DTED data: {:?}", e))?;
        self.loaded_tiles.push((key.lat_deg, key.lon_deg));
        Ok(())
    }

    /// Loads a DTED tile from a raw buffer into the internal engine.
    pub fn load_buffer(&mut self, buffer: &[u8]) -> Result<(), String> {
        let key = self
            .engine
            .load_tile(buffer)
            .map_err(|e| format!("Failed to parse DTED data: {:?}", e))?;
        self.loaded_tiles.push((key.lat_deg, key.lon_deg));
        Ok(())
    }

    /// Unloads a specific tile by its coordinate degrees.
    pub fn unload_tile(&mut self, lat_deg: i32, lon_deg: i32) -> bool {
        let tile_key = olayer_core::terrain::engine::TileKey { lat_deg, lon_deg };
        let removed = self.engine.unload_tile(&tile_key);
        if removed {
            self.loaded_tiles
                .retain(|(lat, lon)| *lat != lat_deg || *lon != lon_deg);
        }
        removed
    }

    /// Queries elevation at the given coordinate degrees.
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, String> {
        self.engine
            .get_elevation(lat_deg, lon_deg)
            .map_err(|e| format!("{:?}", e))
    }
}

impl MapDataSource for TerrainDataSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn clear_cache(&mut self) {
        for (lat, lon) in &self.loaded_tiles {
            let key = olayer_core::terrain::engine::TileKey {
                lat_deg: *lat,
                lon_deg: *lon,
            };
            self.engine.unload_tile(&key);
        }
        self.loaded_tiles.clear();
    }

    fn cache_size(&self) -> usize {
        self.loaded_tiles.len()
    }
}

// =============================================================================
// 4. TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        id: String,
        cache: usize,
        cleared: bool,
    }

    impl MapDataSource for MockSource {
        fn id(&self) -> &str {
            &self.id
        }
        fn clear_cache(&mut self) {
            self.cache = 0;
            self.cleared = true;
        }
        fn cache_size(&self) -> usize {
            self.cache
        }
    }

    // -------------------------------------------------------------------------
    // MapDataStack tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_register_and_get_source() {
        let mut stack = NativeMapDataStack::new();
        stack
            .register_source(Box::new(MockSource {
                id: "terrain".to_string(),
                cache: 5,
                cleared: false,
            }))
            .unwrap();

        assert_eq!(stack.get_cache_size(), 5);
        assert_eq!(stack.get_source("terrain").unwrap().id(), "terrain");
        assert!(stack.get_source("missing").is_none());
    }

    #[test]
    fn test_duplicate_source_rejected() {
        let mut stack = NativeMapDataStack::new();
        stack
            .register_source(Box::new(MockSource {
                id: "terrain".to_string(),
                cache: 0,
                cleared: false,
            }))
            .unwrap();
        let result = stack.register_source(Box::new(MockSource {
            id: "terrain".to_string(),
            cache: 0,
            cleared: false,
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_cache() {
        let mut stack = NativeMapDataStack::new();
        stack
            .register_source(Box::new(MockSource {
                id: "s1".to_string(),
                cache: 4,
                cleared: false,
            }))
            .unwrap();
        stack
            .register_source(Box::new(MockSource {
                id: "s2".to_string(),
                cache: 6,
                cleared: false,
            }))
            .unwrap();

        stack.clear_cache();
        assert_eq!(stack.get_cache_size(), 0);
    }

    // -------------------------------------------------------------------------
    // TerrainDataSource tests
    // -------------------------------------------------------------------------

    fn create_mock_dted() -> Vec<u8> {
        let mut data = vec![b' '; 3428];
        data[0..4].copy_from_slice(b"UHL1");
        let lon_bytes = format!("{: <8}", "0480000W");
        data[4..12].copy_from_slice(lon_bytes.as_bytes());
        let lat_bytes = format!("{: <8}", "230000S");
        data[12..20].copy_from_slice(lat_bytes.as_bytes());
        data[20..24].copy_from_slice(b"0300");
        data[24..28].copy_from_slice(b"0300");
        let cols = format!("{:0>4}", 4);
        let rows = format!("{:0>4}", 4);
        data[47..51].copy_from_slice(cols.as_bytes());
        data[51..55].copy_from_slice(rows.as_bytes());

        let col_size = 11 + 4 * 2;
        for c in 0..4 {
            let mut col = vec![0u8; col_size];
            col[0] = 0xAA;
            col[1..4].copy_from_slice(&[0, 0, c as u8]);
            col[4..7].copy_from_slice(&[0, 0, 0]);
            for r in 0..4 {
                let height = (c * 10 + r) as i16;
                let be = height.to_be_bytes();
                let idx = 7 + r * 2;
                col[idx] = be[0];
                col[idx + 1] = be[1];
            }
            data.extend_from_slice(&col);
        }
        data
    }

    #[test]
    fn test_terrain_data_source_load_and_clear() {
        let mut terrain = TerrainDataSource::new("dted");
        assert_eq!(terrain.cache_size(), 0);

        let mock = create_mock_dted();
        terrain.load_buffer(&mock).unwrap();
        assert_eq!(terrain.cache_size(), 1);

        terrain.clear_cache();
        assert_eq!(terrain.cache_size(), 0);
    }

    #[test]
    fn test_terrain_data_source_elevation() {
        let mut terrain = TerrainDataSource::new("dted");
        let mock = create_mock_dted();
        terrain.load_buffer(&mock).unwrap();

        // Southwest corner (origin) → elevation 0
        let elev = terrain.get_elevation(-23.0, -48.0).unwrap();
        assert!((elev - 0.0).abs() < 1e-6);

        // Exact grid cell (col=1, row=1) → elevation = 1*10+1 = 11
        let elev2 = terrain
            .get_elevation(-23.0 + 1.0 / 3.0, -48.0 + 1.0 / 3.0)
            .unwrap();
        assert!((elev2 - 11.0).abs() < 1e-3);
    }

    #[test]
    fn test_terrain_data_source_unload_tile() {
        let mut terrain = TerrainDataSource::new("dted");
        let mock = create_mock_dted();
        terrain.load_buffer(&mock).unwrap();
        assert_eq!(terrain.cache_size(), 1);

        // Unload the tile
        assert!(terrain.unload_tile(-23, -48));
        assert_eq!(terrain.cache_size(), 0);

        // Query after unload should fail
        assert!(terrain.get_elevation(-23.0, -48.0).is_err());
    }
}
