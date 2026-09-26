use olayer_core::terrain::TerrainEngine;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::io::{Cursor, Read};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

const MAX_HTTP_RESPONSE_BYTES: usize = 1024 * 1024;
const WMTS_TILE_SIZE: u32 = 256;
const WMTS_TILE_RGBA_BYTES: usize = 256 * 256 * 4;
const WMTS_DECODER_MAX_ALLOC_BYTES: u64 = 4 * 1024 * 1024;

/// Errors returned while loading local map data or decoding WMTS tiles.
#[derive(Debug)]
pub enum NativeMapDataError {
    /// An underlying file or HTTP response body could not be read.
    Io(std::io::Error),
    /// A map data source with the same identifier is already registered.
    DuplicateSource(String),
    /// DTED input could not be parsed by the terrain engine.
    InvalidTerrainData(String),
    /// Elevation lookup failed in the terrain engine.
    TerrainQuery(String),
    /// An HTTP response exceeded the configured byte limit.
    ResponseTooLarge { max_bytes: usize },
    /// The WMTS HTTP request failed.
    HttpRequestFailed,
    /// A decoded map tile did not have the dimensions expected by the GPU.
    InvalidTileDimensions { width: u32, height: u32 },
    /// An image could not be identified or decoded.
    ImageDecode(String),
}

impl fmt::Display for NativeMapDataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "map data I/O failed: {error}"),
            Self::DuplicateSource(id) => write!(formatter, "data source '{id}' already registered"),
            Self::InvalidTerrainData(message) => write!(formatter, "invalid DTED data: {message}"),
            Self::TerrainQuery(message) => {
                write!(formatter, "terrain elevation query failed: {message}")
            }
            Self::ResponseTooLarge { max_bytes } => {
                write!(formatter, "HTTP response exceeds {max_bytes} bytes")
            }
            Self::HttpRequestFailed => formatter.write_str("WMTS HTTP request failed"),
            Self::InvalidTileDimensions { width, height } => write!(
                formatter,
                "WMTS tile dimensions are {width}x{height}; expected 256x256"
            ),
            Self::ImageDecode(message) => write!(formatter, "WMTS image decode failed: {message}"),
        }
    }
}

impl std::error::Error for NativeMapDataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

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
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::DuplicateSource`] when the ID is already registered.
    pub fn register_source(
        &mut self,
        source: Box<dyn MapDataSource>,
    ) -> Result<(), NativeMapDataError> {
        let id = source.id().to_string();
        if self.sources.contains_key(&id) {
            return Err(NativeMapDataError::DuplicateSource(id));
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
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::Io`] if the file cannot be read, or
    /// [`NativeMapDataError::InvalidTerrainData`] if the bytes are not valid DTED data.
    pub fn load_dted_file(
        &self,
        path: &str,
        terrain: &mut TerrainEngine,
    ) -> Result<(), NativeMapDataError> {
        let data = std::fs::read(path).map_err(NativeMapDataError::Io)?;
        terrain
            .load_tile(&data)
            .map_err(|error| NativeMapDataError::InvalidTerrainData(format!("{error:?}")))?;
        Ok(())
    }

    /// Loads a DTED tile from a raw buffer into the given terrain engine.
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::InvalidTerrainData`] if the bytes are not valid DTED data.
    pub fn load_dted_buffer(
        &self,
        buffer: &[u8],
        terrain: &mut TerrainEngine,
    ) -> Result<(), NativeMapDataError> {
        terrain
            .load_tile(buffer)
            .map_err(|error| NativeMapDataError::InvalidTerrainData(format!("{error:?}")))?;
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
    loaded_tiles: HashSet<(i32, i32)>,
}

impl TerrainDataSource {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            engine: TerrainEngine::new(),
            loaded_tiles: HashSet::new(),
        }
    }

    /// Loads a DTED tile from a file path into the internal engine.
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::Io`] if the file cannot be read, or
    /// [`NativeMapDataError::InvalidTerrainData`] if the bytes are not valid DTED data.
    pub fn load_file(&mut self, path: &str) -> Result<(), NativeMapDataError> {
        let data = std::fs::read(path).map_err(NativeMapDataError::Io)?;
        let key = self
            .engine
            .load_tile(&data)
            .map_err(|error| NativeMapDataError::InvalidTerrainData(format!("{error:?}")))?;
        self.loaded_tiles.insert((key.lat_deg, key.lon_deg));
        Ok(())
    }

    /// Loads a DTED tile from a raw buffer into the internal engine.
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::InvalidTerrainData`] if the bytes are not valid DTED data.
    pub fn load_buffer(&mut self, buffer: &[u8]) -> Result<(), NativeMapDataError> {
        let key = self
            .engine
            .load_tile(buffer)
            .map_err(|error| NativeMapDataError::InvalidTerrainData(format!("{error:?}")))?;
        self.loaded_tiles.insert((key.lat_deg, key.lon_deg));
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
    ///
    /// # Errors
    /// Returns [`NativeMapDataError::TerrainQuery`] if no loaded terrain tile can satisfy the query.
    pub fn get_elevation(&self, lat_deg: f64, lon_deg: f64) -> Result<f64, NativeMapDataError> {
        self.engine
            .get_elevation(lat_deg, lon_deg)
            .map_err(|error| NativeMapDataError::TerrainQuery(format!("{error:?}")))
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

const DEFAULT_WMTS_CACHE_CAPACITY: usize = 128;

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
    order: Arc<Mutex<VecDeque<String>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    generation: Arc<Mutex<u64>>,
    worker: Arc<WmtsWorker>,
}

enum WorkerCommand {
    Load(String, u64),
    Shutdown,
}

struct WmtsWorker {
    tx: Sender<WorkerCommand>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn read_http_body(reader: impl Read) -> Result<Vec<u8>, NativeMapDataError> {
    let mut bytes = Vec::new();
    reader
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(NativeMapDataError::Io)?;
    if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
        return Err(NativeMapDataError::ResponseTooLarge {
            max_bytes: MAX_HTTP_RESPONSE_BYTES,
        });
    }
    Ok(bytes)
}

fn decode_wmts_tile(bytes: &[u8]) -> Result<Vec<u8>, NativeMapDataError> {
    if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
        return Err(NativeMapDataError::ResponseTooLarge {
            max_bytes: MAX_HTTP_RESPONSE_BYTES,
        });
    }

    let reader = image::io::Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| NativeMapDataError::ImageDecode(error.to_string()))?;
    let format = reader
        .format()
        .ok_or_else(|| NativeMapDataError::ImageDecode("unrecognized image format".to_string()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| NativeMapDataError::ImageDecode(error.to_string()))?;
    if width != WMTS_TILE_SIZE || height != WMTS_TILE_SIZE {
        return Err(NativeMapDataError::InvalidTileDimensions { width, height });
    }

    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(WMTS_TILE_SIZE);
    limits.max_image_height = Some(WMTS_TILE_SIZE);
    limits.max_alloc = Some(WMTS_DECODER_MAX_ALLOC_BYTES);

    let mut reader = image::io::Reader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| NativeMapDataError::ImageDecode(error.to_string()))?;
    let rgba = image.to_rgba8().into_raw();
    if rgba.len() != WMTS_TILE_RGBA_BYTES {
        return Err(NativeMapDataError::ImageDecode(format!(
            "decoded RGBA buffer has {} bytes; expected {WMTS_TILE_RGBA_BYTES}",
            rgba.len()
        )));
    }
    Ok(rgba)
}

fn fetch_wmts_tile(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, NativeMapDataError> {
    let response = agent
        .get(url)
        .call()
        .map_err(|_| NativeMapDataError::HttpRequestFailed)?;
    let bytes = read_http_body(response.into_reader())?;
    decode_wmts_tile(&bytes)
}

fn insert_cached_tile(
    cache: &Mutex<HashMap<String, Vec<u8>>>,
    order: &Mutex<VecDeque<String>>,
    key: &str,
    pixels: Vec<u8>,
) {
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    let Ok(mut keys) = order.lock() else {
        return;
    };

    cache.insert(key.to_string(), pixels);
    keys.retain(|cached_key| cached_key != key);
    keys.push_back(key.to_string());
    while keys.len() > DEFAULT_WMTS_CACHE_CAPACITY {
        if let Some(oldest) = keys.pop_front() {
            cache.remove(&oldest);
        }
    }
}

fn touch_cached_key(order: &Mutex<VecDeque<String>>, key: &str) {
    if let Ok(mut keys) = order.lock() {
        keys.retain(|cached_key| cached_key != key);
        keys.push_back(key.to_string());
    }
}

impl GeoserverWmtsSource {
    /// Creates a new `GeoserverWmtsSource` and spawns its background worker thread.
    pub fn new(id: &str, base_url: &str, layer_name: &str) -> Self {
        let (tx_req, rx_req) = channel::<WorkerCommand>();
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let order = Arc::new(Mutex::new(VecDeque::new()));
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let generation = Arc::new(Mutex::new(0_u64));

        let cache_clone = cache.clone();
        let pending_clone = pending.clone();
        let order_clone = order.clone();
        let generation_clone = generation.clone();
        let base_url = base_url.to_string();
        let layer_name = layer_name.to_string();

        let base_url_for_thread = base_url.clone();
        let layer_name_for_thread = layer_name.clone();

        // Spawn background worker thread
        let handle = std::thread::spawn(move || {
            log::debug!("WMTS background worker started");
            while let Ok(command) = rx_req.recv() {
                let (key, request_generation) = match command {
                    WorkerCommand::Load(key, request_generation) => {
                        if generation_clone
                            .lock()
                            .map(|value| *value != request_generation)
                            .unwrap_or(true)
                        {
                            if let Ok(mut p) = pending_clone.lock() {
                                p.remove(&key);
                            }
                            continue;
                        }
                        (key, request_generation)
                    }
                    WorkerCommand::Shutdown => break,
                };
                log::debug!("Received WMTS tile request: {key}");
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

                log::debug!("Fetching WMTS tile {key}");

                // Fetch tile bytes via ureq with timeout to prevent blocking forever
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(5))
                    .timeout_read(std::time::Duration::from_secs(5))
                    .build();

                match fetch_wmts_tile(&agent, &url) {
                    Ok(raw_pixels) => {
                        log::debug!("Decoded 256x256 RGBA WMTS tile {key}");
                        let current_generation =
                            generation_clone.lock().map(|value| *value).unwrap_or(0);
                        if current_generation == request_generation {
                            insert_cached_tile(&cache_clone, &order_clone, &key, raw_pixels);
                        }
                    }
                    Err(error) => {
                        log::error!("Failed to load WMTS tile {key}: {error}");
                    }
                }

                // Always remove from pending when done (success or failure)
                if let Ok(mut p) = pending_clone.lock() {
                    p.remove(&key);
                }
            }
        });

        let worker = Arc::new(WmtsWorker {
            tx: tx_req,
            handle: Mutex::new(Some(handle)),
        });

        Self {
            id: id.to_string(),
            base_url,
            layer_name,
            cache,
            order,
            pending,
            generation,
            worker,
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
        let request_generation = self.generation.lock().map(|value| *value).unwrap_or(0);
        let _ = self
            .worker
            .tx
            .send(WorkerCommand::Load(key, request_generation));
    }

    /// Retrieves the loaded raw RGBA8 pixel bytes for a tile.
    ///
    /// Returns `None` if the tile is still loading or failed to load.
    pub fn get_tile_pixels(&self, x: u32, y: u32, z: u32) -> Option<Vec<u8>> {
        let key = format!("{}/{}/{}", z, x, y);
        let cache = self.cache.lock().ok()?;
        let pixels = cache.get(&key).cloned();
        if pixels.is_some() {
            touch_cached_key(&self.order, &key);
        }
        pixels
    }

    /// Checks whether a tile is cached without cloning its pixel buffer.
    pub fn has_tile(&self, x: u32, y: u32, z: u32) -> bool {
        let key = format!("{}/{}/{}", z, x, y);
        let Ok(cache) = self.cache.lock() else {
            return false;
        };
        let present = cache.contains_key(&key);
        if present {
            touch_cached_key(&self.order, &key);
        }
        present
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
        if let Ok(mut generation) = self.generation.lock() {
            *generation = generation.wrapping_add(1);
        }
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
        if let Ok(mut keys) = self.order.lock() {
            keys.clear();
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

impl Drop for GeoserverWmtsSource {
    fn drop(&mut self) {
        if Arc::strong_count(&self.worker) != 1 {
            return;
        }
        let _ = self.worker.tx.send(WorkerCommand::Shutdown);
        if let Ok(mut handle) = self.worker.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
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
        assert!(matches!(
            result,
            Err(NativeMapDataError::DuplicateSource(id)) if id == "terrain"
        ));
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
        let mut source = GeoserverWmtsSource::new(
            "geo",
            "http://localhost:8080/geoserver/gwc/service/wmts",
            "test_layer",
        );
        assert_eq!(source.id(), "geo");
        assert_eq!(
            source.base_url(),
            "http://localhost:8080/geoserver/gwc/service/wmts"
        );
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

    #[test]
    fn http_response_body_is_bounded() {
        let within_limit = vec![0; MAX_HTTP_RESPONSE_BYTES];
        assert_eq!(
            read_http_body(within_limit.as_slice()).unwrap().len(),
            MAX_HTTP_RESPONSE_BYTES
        );

        let over_limit = vec![0; MAX_HTTP_RESPONSE_BYTES + 1];
        assert!(matches!(
            read_http_body(over_limit.as_slice()),
            Err(NativeMapDataError::ResponseTooLarge { .. })
        ));
    }

    #[test]
    fn wmts_decode_requires_a_256_by_256_tile() {
        let mut valid_png = Vec::new();
        image::DynamicImage::new_rgba8(WMTS_TILE_SIZE, WMTS_TILE_SIZE)
            .write_to(
                &mut std::io::Cursor::new(&mut valid_png),
                image::ImageOutputFormat::Png,
            )
            .unwrap();
        assert_eq!(
            decode_wmts_tile(&valid_png).unwrap().len(),
            WMTS_TILE_RGBA_BYTES
        );

        let mut invalid_png = Vec::new();
        image::DynamicImage::new_rgba8(WMTS_TILE_SIZE + 1, WMTS_TILE_SIZE)
            .write_to(
                &mut std::io::Cursor::new(&mut invalid_png),
                image::ImageOutputFormat::Png,
            )
            .unwrap();
        assert!(matches!(
            decode_wmts_tile(&invalid_png),
            Err(NativeMapDataError::InvalidTileDimensions {
                width: 257,
                height: 256
            })
        ));
    }

    #[test]
    fn duplicate_terrain_loads_are_tracked_once() {
        let mut terrain = TerrainDataSource::new("dted");
        let mock = create_mock_dted();
        terrain.load_buffer(&mock).unwrap();
        terrain.load_buffer(&mock).unwrap();
        assert_eq!(terrain.cache_size(), 1);
    }

    #[test]
    fn wmts_cache_insertions_and_existence_queries_preserve_unique_lru_keys() {
        let source = GeoserverWmtsSource::new(
            "geo-cache",
            "http://localhost:8080/geoserver/wmts",
            "test_layer",
        );
        insert_cached_tile(
            &source.cache,
            &source.order,
            "0/0/0",
            vec![255; WMTS_TILE_RGBA_BYTES],
        );
        insert_cached_tile(
            &source.cache,
            &source.order,
            "0/0/0",
            vec![0; WMTS_TILE_RGBA_BYTES],
        );

        assert!(source.has_tile(0, 0, 0));
        assert_eq!(source.get_cached_keys(), vec!["0/0/0".to_string()]);
        assert_eq!(
            source.get_tile_pixels(0, 0, 0),
            Some(vec![0; WMTS_TILE_RGBA_BYTES])
        );
        assert!(!source.has_tile(1, 0, 0));
        assert_eq!(source.order.lock().unwrap().len(), 1);

        for index in 1..DEFAULT_WMTS_CACHE_CAPACITY {
            let key = format!("0/{index}/0");
            insert_cached_tile(
                &source.cache,
                &source.order,
                &key,
                vec![u8::try_from(index).unwrap()],
            );
        }
        assert!(source.has_tile(0, 0, 0));
        insert_cached_tile(&source.cache, &source.order, "0/999/0", vec![1]);
        assert_eq!(source.cache_size(), DEFAULT_WMTS_CACHE_CAPACITY);
        assert!(source.has_tile(0, 0, 0));
        assert!(!source.has_tile(1, 0, 0));
        assert_eq!(
            source.order.lock().unwrap().len(),
            DEFAULT_WMTS_CACHE_CAPACITY
        );
        let lru_keys = source.order.lock().unwrap();
        assert_eq!(
            lru_keys.iter().collect::<HashSet<_>>().len(),
            lru_keys.len()
        );
    }
}
