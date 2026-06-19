use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender};
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
        let tile_key = olayer_core::terrain::TileKey { lat_deg, lon_deg };
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
            let key = olayer_core::terrain::TileKey {
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
// 3.1 GEOSERVER WMTS DATA SOURCE
// =============================================================================

/// A concrete `MapDataSource` that loads raster tiles from GeoServer WMTS endpoint.
///
/// It uses background thread workers to fetch PNG/JPEG tiles and decodes them
/// asynchronously into raw RGBA pixels to prevent UI rendering freezes.
#[derive(Clone)]
pub struct GeoserverWmtsSource {
    id: String,
    base_url: String,
    layer_name: String,
    cache: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    tx_request: Sender<String>,
}

impl GeoserverWmtsSource {
    /// Creates a new `GeoserverWmtsSource` and spawns its background worker thread.
    pub fn new(id: &str, base_url: &str, layer_name: &str) -> Self {
        let (tx_req, rx_req) = channel::<String>();
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashSet::new()));

        let cache_clone = cache.clone();
        let pending_clone = pending.clone();
        let base_url = base_url.to_string();
        let layer_name = layer_name.to_string();

        let base_url_for_thread = base_url.clone();
        let layer_name_for_thread = layer_name.clone();

        // Spawn background worker thread
        std::thread::spawn(move || {
            println!("[GeoserverWmtsSource] Background worker thread started.");
            while let Ok(key) = rx_req.recv() {
                println!("[GeoserverWmtsSource] Received request for tile key: {}", key);
                // Parse key "z/x/y"
                let parts: Vec<&str> = key.split('/').collect();
                if parts.len() != 3 {
                    continue;
                }
                let z: u32 = parts[0].parse().unwrap_or(0);
                let x: u32 = parts[1].parse().unwrap_or(0);
                let y: u32 = parts[2].parse().unwrap_or(0);

                // Build GeoServer WMTS request URL (EPSG:900913 is the standard Web Mercator MatrixSet)
                let url = if base_url_for_thread.contains('?') {
                    format!("{}&service=WMTS&request=GetTile&version=1.0.0&layer={}&style=&tilematrixset=EPSG:900913&TileMatrix=EPSG:900913:{}&TileRow={}&TileCol={}&format=image/png", base_url_for_thread, layer_name_for_thread, z, y, x)
                } else {
                    format!("{}?service=WMTS&request=GetTile&version=1.0.0&layer={}&style=&tilematrixset=EPSG:900913&TileMatrix=EPSG:900913:{}&TileRow={}&TileCol={}&format=image/png", base_url_for_thread, layer_name_for_thread, z, y, x)
                };

                println!("[GeoserverWmtsSource] Fetching tile from URL: {}", url);

                // Fetch tile bytes via ureq with timeout to prevent blocking forever
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(5))
                    .timeout_read(std::time::Duration::from_secs(5))
                    .build();

                match agent.get(&url).call() {
                    Ok(response) => {
                        println!("[GeoserverWmtsSource] HTTP response ok for url: {}", url);
                        let mut bytes = Vec::new();
                        let mut reader = response.into_reader();
                        if reader.read_to_end(&mut bytes).is_ok() {
                            println!("[GeoserverWmtsSource] Read {} bytes for tile {}", bytes.len(), key);
                            // Decode image using image crate to raw RGBA8
                            match image::load_from_memory(&bytes) {
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    let width = rgba.width();
                                    let height = rgba.height();
                                    let raw_pixels = rgba.into_raw();
                                    let mut transparent = 0;
                                    let mut opaque = 0;
                                    let mut unique_colors = std::collections::HashSet::new();
                                    for chunk in raw_pixels.chunks_exact(4) {
                                        if chunk[3] == 0 {
                                            transparent += 1;
                                        } else {
                                            opaque += 1;
                                            unique_colors.insert((chunk[0], chunk[1], chunk[2]));
                                        }
                                    }
                                    println!("[GeoserverWmtsSource] Decoded image: {}x{}. Transparent pixels: {}, Opaque: {}. Unique opaque colors: {}", 
                                             width, height, transparent, opaque, unique_colors.len());

                                    // Insert into cache
                                    if let Ok(mut c) = cache_clone.lock() {
                                        c.insert(key.clone(), raw_pixels);
                                    }
                                }
                                Err(e) => {
                                    println!("[GeoserverWmtsSource] Failed to decode image: {:?}", e);
                                    log::error!("Failed to decode image from GeoServer: {:?}", e);
                                }
                            }
                        } else {
                            println!("[GeoserverWmtsSource] Failed to read response body for tile {}", key);
                        }
                    }
                    Err(e) => {
                        println!("[GeoserverWmtsSource] HTTP request failed for URL: {}. Error: {:?}", url, e);
                        log::error!("Failed to fetch tile from GeoServer at {}: {:?}", url, e);
                    }
                }

                // Always remove from pending when done (success or failure)
                if let Ok(mut p) = pending_clone.lock() {
                    p.remove(&key);
                }
            }
        });

        Self {
            id: id.to_string(),
            base_url,
            layer_name,
            cache,
            pending,
            tx_request: tx_req,
        }
    }

    /// Triggers asynchronous loading of a tile if it is not already loaded or pending.
    pub fn load_tile(&self, x: u32, y: u32, z: u32) {
        let key = format!("{}/{}/{}", z, x, y);

        // Check if already in cache
        if let Ok(c) = self.cache.lock() {
            if c.contains_key(&key) {
                return;
            }
        }

        // Check if already pending
        if let Ok(mut p) = self.pending.lock() {
            if p.contains(&key) {
                return;
            }
            p.insert(key.clone());
        }

        // Send request to worker thread
        let _ = self.tx_request.send(key);
    }

    /// Retrieves the loaded raw RGBA8 pixel bytes for a tile.
    ///
    /// Returns `None` if the tile is still loading or failed to load.
    pub fn get_tile_pixels(&self, x: u32, y: u32, z: u32) -> Option<Vec<u8>> {
        let key = format!("{}/{}/{}", z, x, y);
        if let Ok(c) = self.cache.lock() {
            c.get(&key).cloned()
        } else {
            None
        }
    }

    /// Returns a list of all tile keys currently loaded in the memory cache.
    pub fn get_cached_keys(&self) -> Vec<String> {
        if let Ok(c) = self.cache.lock() {
            c.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Returns the base URL of the GeoServer WMTS endpoint.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the layer name configuration.
    pub fn layer_name(&self) -> &str {
        &self.layer_name
    }
}

impl MapDataSource for GeoserverWmtsSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn clear_cache(&mut self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
        if let Ok(mut p) = self.pending.lock() {
            p.clear();
        }
    }

    fn cache_size(&self) -> usize {
        if let Ok(c) = self.cache.lock() {
            c.len()
        } else {
            0
        }
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

    #[test]
    fn test_geoserver_wmts_source_cache_logic() {
        let mut source = GeoserverWmtsSource::new("geo", "http://localhost:8080/geoserver/gwc/service/wmts", "test_layer");
        assert_eq!(source.id(), "geo");
        assert_eq!(source.base_url(), "http://localhost:8080/geoserver/gwc/service/wmts");
        assert_eq!(source.layer_name(), "test_layer");
        assert_eq!(source.cache_size(), 0);
        assert!(source.get_tile_pixels(0, 0, 0).is_none());

        // Simulate successful download/decode by inserting manually into cache
        {
            let mut cache = source.cache.lock().unwrap();
            cache.insert("0/0/0".to_string(), vec![255; 16]);
        }

        assert_eq!(source.cache_size(), 1);
        let pixels = source.get_tile_pixels(0, 0, 0).unwrap();
        assert_eq!(pixels, vec![255; 16]);
        assert_eq!(source.get_cached_keys(), vec!["0/0/0".to_string()]);

        source.clear_cache();
        assert_eq!(source.cache_size(), 0);
        assert!(source.get_tile_pixels(0, 0, 0).is_none());
    }
}
