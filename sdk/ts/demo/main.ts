import init, {
  WasmProjection,
  OlayerController,
  Layer,
  WebGLRenderer,
  CPURenderer,
  TileLayer,
  RasterTileSource,
  VectorTileSource,
  VectorTileLayer,
  WasmStyleRegistry,
} from "../src";

// Pre-define coordinates for São Paulo (TMA SP) in radians
const SP_LAT_RAD = -23.62 * (Math.PI / 180);
const SP_LON_RAD = -46.65 * (Math.PI / 180);

// Global State
let currentProjType = "Stereographic";
let activeProjection: WasmProjection;
let controller: OlayerController;
let gridLayer: GridLayer;
let radarLayer: RadarLayer;
let tileLayer: TileLayer | null = null;
let vectorLayer: VectorTileLayer | null = null;
let selectedTargetId: string | null = null;

// Helper to create a mock DTED Level 0 binary tile in memory
function createMockDted0(latStr: string, lonStr: string, numCols: number, numRows: number): Uint8Array {
  const colSize = 11 + numRows * 2;
  const totalSize = 3428 + numCols * colSize;
  const data = new Uint8Array(totalSize);

  // Fill header with spaces
  data.fill(32); // ASCII space

  // Set UHL1 signature
  data.set([85, 72, 76, 49], 0); // "UHL1"

  // Set longitude
  const lonPadded = lonStr.padEnd(8, " ");
  for (let i = 0; i < 8; i++) {
    data[4 + i] = lonPadded.charCodeAt(i);
  }

  // Set latitude
  const latPadded = latStr.padEnd(8, " ");
  for (let i = 0; i < 8; i++) {
    data[12 + i] = latPadded.charCodeAt(i);
  }

  // Set spacing
  data.set([48, 51, 48, 48], 20); // "0300"
  data.set([48, 51, 48, 48], 24); // "0300"

  // Set num_cols
  const colsStr = numCols.toString().padStart(4, "0");
  for (let i = 0; i < 4; i++) {
    data[47 + i] = colsStr.charCodeAt(i);
  }

  // Set num_rows
  const rowsStr = numRows.toString().padStart(4, "0");
  for (let i = 0; i < 4; i++) {
    data[51 + i] = rowsStr.charCodeAt(i);
  }

  // Write column data
  let offset = 3428;
  for (let c = 0; c < numCols; c++) {
    data[offset] = 0xAA; // Block sentinel
    const valOffset = offset + 7;
    for (let r = 0; r < numRows; r++) {
      const latFraction = r / numRows;
      const lonFraction = c / numCols;
      // Generate elevation ranging from 300m to 1200m
      const elevation = Math.round(500 + 400 * Math.sin(latFraction * Math.PI * 4) * Math.cos(lonFraction * Math.PI * 4));
      
      const idx = valOffset + r * 2;
      data[idx] = (elevation >> 8) & 0xFF;     // MSB
      data[idx + 1] = elevation & 0xFF;        // LSB
    }
    offset += colSize;
  }

  return data;
}

// Draw the 2.5D vertical flight profile chart
function drawVerticalProfile(target: any, profileData: number[]) {
  const canvas = document.getElementById("profileCanvas") as HTMLCanvasElement;
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  const width = canvas.width;
  const height = canvas.height;

  // Clear
  ctx.clearRect(0, 0, width, height);

  // Parse profileData: [distance, elevation, lat, lon, height, ...]
  const numPoints = profileData.length / 5;
  if (numPoints < 2) return;

  const points: { distance: number; elevation: number; height: number }[] = [];
  let maxElev = 1000; // default min height scale in meters
  let maxDist = 1;

  for (let i = 0; i < numPoints; i++) {
    const dist = profileData[i * 5];
    const elev = profileData[i * 5 + 1];
    const ptHeight = profileData[i * 5 + 4];
    points.push({ distance: dist, elevation: elev, height: ptHeight });
    if (elev > maxElev) maxElev = elev;
    if (ptHeight > maxElev) maxElev = ptHeight;
    if (dist > maxDist) maxDist = dist;
  }

  // Add a bit of padding to Y scale
  maxElev *= 1.2;

  // Coordinate mapping helpers
  const getX = (dist: number) => (dist / maxDist) * (width - 40) + 20;
  const getY = (h: number) => height - 15 - (h / maxElev) * (height - 35);

  // Draw grid lines
  ctx.strokeStyle = "rgba(255, 255, 255, 0.05)";
  ctx.lineWidth = 1;
  for (let hStep = 0; hStep <= maxElev; hStep += 2000) {
    const y = getY(hStep);
    ctx.beginPath();
    ctx.moveTo(20, y);
    ctx.lineTo(width - 20, y);
    ctx.stroke();
    // Label
    ctx.fillStyle = "rgba(255, 255, 255, 0.3)";
    ctx.font = "8px 'Outfit', sans-serif";
    ctx.fillText(`${Math.round(hStep * 3.28084)} ft`, 2, y + 3);
  }

  // Draw terrain filled path
  ctx.beginPath();
  ctx.moveTo(getX(points[0].distance), getY(0));
  for (const p of points) {
    ctx.lineTo(getX(p.distance), getY(p.elevation));
  }
  ctx.lineTo(getX(points[points.length - 1].distance), getY(0));
  ctx.closePath();

  const terrainGrad = ctx.createLinearGradient(0, getY(maxElev), 0, height);
  terrainGrad.addColorStop(0, "rgba(141, 110, 99, 0.45)"); // Brownish terrain top
  terrainGrad.addColorStop(1, "rgba(62, 39, 35, 0.1)");
  ctx.fillStyle = terrainGrad;
  ctx.fill();

  ctx.strokeStyle = "rgba(141, 110, 99, 0.8)"; // Terrain stroke
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(getX(points[0].distance), getY(points[0].elevation));
  for (const p of points) {
    ctx.lineTo(getX(p.distance), getY(p.elevation));
  }
  ctx.stroke();

  // Draw aircraft path (flight altitude profile)
  ctx.strokeStyle = "rgba(0, 176, 255, 0.5)";
  ctx.lineWidth = 2;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  ctx.moveTo(getX(points[0].distance), getY(target.position.height));
  ctx.lineTo(getX(points[points.length - 1].distance), getY(target.position.height));
  ctx.stroke();
  ctx.setLineDash([]);

  // Find aircraft X position (which corresponds to 30km along the 80km path)
  const aircraftDist = 30000;
  const aircraftX = getX(aircraftDist);
  const aircraftY = getY(target.position.height);

  // Look up ground elevation under the aircraft
  let groundElev = 0;
  let minDistDiff = Infinity;
  for (const p of points) {
    const diff = Math.abs(p.distance - aircraftDist);
    if (diff < minDistDiff) {
      minDistDiff = diff;
      groundElev = p.elevation;
    }
  }

  const clearance = target.position.height - groundElev;
  const cfitHazard = clearance < 300; // < 1000 ft clearance

  // Draw clearance indicator line
  ctx.strokeStyle = cfitHazard ? "#ff1744" : "#00e676";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(aircraftX, aircraftY);
  ctx.lineTo(aircraftX, getY(groundElev));
  ctx.stroke();

  // Draw aircraft symbol (dot with halo)
  ctx.fillStyle = "#00b0ff";
  ctx.beginPath();
  ctx.arc(aircraftX, aircraftY, 5, 0, 2 * Math.PI);
  ctx.fill();
  ctx.strokeStyle = "#ffffff";
  ctx.lineWidth = 1;
  ctx.stroke();

  // Draw clearance text overlay
  ctx.fillStyle = cfitHazard ? "#ff1744" : "#00e676";
  ctx.font = "bold 9px 'Outfit', sans-serif";
  ctx.fillText(
    `CLEARANCE: ${Math.round(clearance * 3.28084)} FT ${cfitHazard ? "⚠️ CFIT HAZARD!" : "OK"}`,
    aircraftX + 10,
    (aircraftY + getY(groundElev)) / 2 + 3
  );

  // Draw labels
  ctx.fillStyle = "#ffffff";
  ctx.font = "bold 9px 'Outfit', sans-serif";
  ctx.fillText(`ALT: ${Math.round(target.position.height * 3.28084)} FT`, aircraftX - 30, aircraftY - 10);
}

// 1. Grid Layer (Static WebGL background)
class GridLayer extends Layer {
  private renderer: WebGLRenderer;
  private projection: WasmProjection;
  private viewMode = "2D";

  constructor(gl: WebGL2RenderingContext, projection: WasmProjection, viewMode = "2D") {
    super("static_grid");
    this.renderer = new WebGLRenderer(gl);
    this.projection = projection;
    this.viewMode = viewMode;
    this.renderer.rebuildGrid(this.projection, this.viewMode);
  }

  public updateProjection(newProjection: WasmProjection, viewMode: string): void {
    this.projection = newProjection;
    this.viewMode = viewMode;
    this.renderer.rebuildGrid(this.projection, this.viewMode);
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    this.renderer.renderGrid(viewProjMatrix);
  }

  public renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void {
    // Grid has no dynamic screen overlays
  }
}

// 2. Radar Traffic Layer (Dynamic Canvas 2D overlay)
class RadarLayer extends Layer {
  private controller: OlayerController;
  public cpuRenderer: CPURenderer;

  constructor(controller: OlayerController) {
    super("dynamic_radar_traffic");
    this.controller = controller;
    this.cpuRenderer = new CPURenderer(controller.ctx2d);
  }

  public renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void {
    // Traffic has no static elements
  }

  public renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void {
    this.cpuRenderer.beginFrame();

    const camera = this.controller.getCameraState();
    const viewMode = this.controller.getViewMode();
    const viewProjMatrix = this.controller.currentViewProjMatrix;
    
    // Get camera center projected planar coordinates
    let centerXY: number[] = [0, 0];
    if (viewMode !== "3D") {
      try {
        centerXY = this.controller.projection.project(
          camera.center_lat,
          camera.center_lon,
          camera.center_height
        );
      } catch {
        camera.free();
        return;
      }
    }

    // Retrieve active targets list interpolated at currentTime
    let targets: any[] = [];
    try {
      const jsVal = this.controller.interpolator.interpolate_all(currentTime);
      if (jsVal) {
        targets = jsVal as any[];
      }
    } catch (err) {
      console.error("Failed to interpolate targets:", err);
    }

    // Update target count in UI
    const targetCountEl = document.getElementById("targetCountVal");
    if (targetCountEl) {
      targetCountEl.innerText = targets.length.toString();
    }

    // Render 2.5D profile chart if a target is selected
    if (selectedTargetId) {
      const selectedTarget = targets.find(t => t.id === selectedTargetId);
      if (selectedTarget) {
        // Generate route coords from -30km to +50km
        const routeCoords: number[] = [];
        const R = 6378137.0;
        const stepMeters = 2000;
        const startDist = -30000;
        const endDist = 50000;
        const heading = selectedTarget.heading_rad ?? 0.0;
        
        for (let dist = startDist; dist <= endDist; dist += stepMeters) {
          // Calculate coordinate at offset 'dist' along the heading
          const latOffset = (dist * Math.cos(heading)) / R;
          const lonOffset = (dist * Math.sin(heading)) / (R * Math.cos(selectedTarget.position.lat));

          const sampleLat = selectedTarget.position.lat + latOffset;
          const sampleLon = selectedTarget.position.lon + lonOffset;

          // Convert to degrees for get_vertical_profile
          routeCoords.push(sampleLat * 180 / Math.PI, sampleLon * 180 / Math.PI, selectedTarget.position.height);
        }

        try {
          const profileData = this.controller.terrainEngine.get_vertical_profile(
            new Float64Array(routeCoords),
            stepMeters
          );
          if (profileData) {
            drawVerticalProfile(selectedTarget, Array.from(profileData));
          }
        } catch (err) {
          console.error("Failed to calculate vertical profile:", err);
        }
      } else {
        selectedTargetId = null;
        const panel = document.getElementById("profile-panel");
        if (panel) panel.style.display = "none";
      }
    }

    // Render each aircraft
    for (const t of targets) {
      const screenPos = this.cpuRenderer.projectToScreen(
        this.controller.projection,
        t.position.lat,
        t.position.lon,
        t.position.height,
        centerXY[0],
        centerXY[1],
        camera.zoom,
        camera.rotation,
        camera.viewport_base_meters,
        this.controller.glCanvas.width,
        this.controller.glCanvas.height,
        viewMode,
        viewProjMatrix,
        camera.center_lat,
        camera.center_lon
      );

      if (screenPos) {
        let symbolId = "civil:plane";
        if (t.id.startsWith("TAM") || t.id.startsWith("GLO")) {
          symbolId = "civil:plane";
        } else if (t.id.startsWith("AZU")) {
          symbolId = "mil:fighter";
        } else {
          symbolId = "mil:cargo";
        }

        let symbolUv = undefined;
        try {
          symbolUv = this.controller.atlasManager.registerWasmSymbol(
            symbolId,
            this.controller.symbolRegistry,
            this.controller.styleRegistry
          );
        } catch (err) {
          console.error("Failed to register WASM symbol in atlas:", err);
        }

        const atlasCanvas = (this.controller.atlasManager as any).atlasCanvas;

        this.cpuRenderer.drawTarget(
          {
            id: t.id,
            position: t.position,
            heading_rad: t.heading_rad,
          },
          screenPos,
          this.controller.projection,
          centerXY[0],
          centerXY[1],
          camera.zoom,
          camera.rotation,
          camera.viewport_base_meters,
          this.controller.glCanvas.width,
          this.controller.glCanvas.height,
          t.speed_mps ?? 180.0,
          atlasCanvas,
          symbolUv,
          viewMode,
          viewProjMatrix,
          camera.center_lat,
          camera.center_lon
        );
      }
    }

    camera.free();
  }
}

// Simulated Target Feed Generator (Radar updates every 1.5s)
interface SimulatedTarget {
  id: string;
  lat: number; // radians
  lon: number; // radians
  alt: number; // meters
  speed: number; // m/s
  heading: number; // radians
}

const activeSimulatedTargets: SimulatedTarget[] = [];

function generateRandomTarget(): void {
  const callsigns = ["TAM", "GLO", "AZU", "ARG", "TAP", "AAL", "KLM", "DLH"];
  const randomCall = callsigns[Math.floor(Math.random() * callsigns.length)] + Math.floor(100 + Math.random() * 900);
  
  // Random offset around São Paulo center (approx +-50km)
  const offsetRadiusRad = (15 + Math.random() * 80) * 1000 / 6378137.0;
  const angle = Math.random() * 2 * Math.PI;

  const lat = SP_LAT_RAD + offsetRadiusRad * Math.cos(angle);
  const lon = SP_LON_RAD + offsetRadiusRad * Math.sin(angle);
  const alt = 1000 + Math.random() * 6000; // 3k to 23k feet
  const speed = 180 + Math.random() * 70; // 350-500 KT
  const heading = Math.random() * 2 * Math.PI;

  const newTarget: SimulatedTarget = {
    id: randomCall,
    lat,
    lon,
    alt,
    speed,
    heading,
  };

  activeSimulatedTargets.push(newTarget);
  
  // Feed target ping directly to interpolator
  controller.interpolator.update_target(
    newTarget.id,
    newTarget.lat,
    newTarget.lon,
    newTarget.alt,
    newTarget.speed,
    newTarget.heading,
    0.0, // level flight
    Date.now() / 1000
  );
  
  controller.triggerActive();
}

// Rebuilds and updates map layers based on GUI selections
function updateMapLayers(): void {
  if (!controller) return;

  // 1. Remove and destroy existing custom layers
  if (tileLayer) {
    controller.layerManager.removeLayer(tileLayer.id);
    tileLayer.destroy(controller.gl);
    tileLayer = null;
  }
  if (vectorLayer) {
    controller.layerManager.removeLayer(vectorLayer.id);
    vectorLayer.destroy(controller.gl);
    vectorLayer = null;
  }

  // Also remove standard layers temporarily so we can re-add in order
  if (gridLayer) {
    controller.layerManager.removeLayer(gridLayer.id);
  }
  if (radarLayer) {
    controller.layerManager.removeLayer(radarLayer.id);
  }

  // 2. Read values from GUI
  const mapSourceSelect = document.getElementById("mapSourceSelect") as HTMLSelectElement;
  const mapSource = mapSourceSelect ? mapSourceSelect.value : "osm";

  const hostInput = document.getElementById("geoserverHostInput") as HTMLInputElement;
  let host = hostInput ? hostInput.value.trim() : "http://localhost:8080/geoserver";
  host = host.replace(/\/+$/, ""); // Remove trailing slashes

  const baseLayerInput = document.getElementById("geoserverLayerInput") as HTMLInputElement;
  const baseLayerName = baseLayerInput ? baseLayerInput.value.trim() : "topp:states";

  const vectorOverlayCheckbox = document.getElementById("showVectorOverlayCheckbox") as HTMLInputElement;
  const showVector = vectorOverlayCheckbox ? vectorOverlayCheckbox.checked : false;

  const vectorLayerInput = document.getElementById("vectorLayerInput") as HTMLInputElement;
  const vectorLayerName = vectorLayerInput ? vectorLayerInput.value.trim() : "topp:states";

  // Toggle DOM element visibility based on selections
  const geoserverConfigGroup = document.getElementById("geoserverConfigGroup");
  if (geoserverConfigGroup) {
    geoserverConfigGroup.style.display = (mapSource === "geoserver_wms" || mapSource === "geoserver_tms") ? "block" : "none";
  }

  const vectorConfigGroup = document.getElementById("vectorConfigGroup");
  if (vectorConfigGroup) {
    vectorConfigGroup.style.display = showVector ? "block" : "none";
  }

  // 3. Rebuild Base Tile Layer if configured
  if (mapSource === "osm") {
    const osmSource = new RasterTileSource(
      controller.gl,
      "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
      1000
    );
    controller.dataManager.registerSource(osmSource);
    tileLayer = new TileLayer("osm_base_map", osmSource);
    tileLayer.opacity = 0.35;
  } else if (mapSource === "geoserver_wms") {
    const wmsSource = new RasterTileSource(
      controller.gl,
      (x, y, z) => {
        // Compute EPSG:3857 Bbox for XYZ tile coordinate
        const size = 20037508.342789244 * 2;
        const numTiles = 1 << z;
        const tileSize = size / numTiles;
        const minX = -20037508.342789244 + x * tileSize;
        const maxX = minX + tileSize;
        const maxY = 20037508.342789244 - y * tileSize;
        const minY = maxY - tileSize;
        return `${host}/wms?service=WMS&version=1.1.1&request=GetMap&layers=${baseLayerName}&styles=&bbox=${minX},${minY},${maxX},${maxY}&width=256&height=256&srs=EPSG:3857&format=image/png&transparent=true`;
      },
      1000
    );
    controller.dataManager.registerSource(wmsSource);
    tileLayer = new TileLayer("geoserver_base_map", wmsSource);
    tileLayer.opacity = 0.35;
  } else if (mapSource === "geoserver_tms") {
    const tmsSource = new RasterTileSource(
      controller.gl,
      (x, y, z) => {
        const y_tms = (1 << z) - 1 - y;
        return `${host}/gwc/service/tms/1.0.0/${baseLayerName}@EPSG:900913@png/${z}/${x}/${y_tms}.png`;
      },
      1000
    );
    controller.dataManager.registerSource(tmsSource);
    tileLayer = new TileLayer("geoserver_base_map", tmsSource);
    tileLayer.opacity = 0.35;
  }

  // 4. Rebuild Vector Layer if configured
  if (showVector) {
    const vecSource = new VectorTileSource(
      (x, y, z) => {
        const y_tms = (1 << z) - 1 - y;
        return `${host}/gwc/service/tms/1.0.0/${vectorLayerName}@EPSG:900913@geojson/${z}/${x}/${y_tms}.geojson`;
      },
      1000
    );
    vectorLayer = new VectorTileLayer("geoserver_vector_layer", vecSource);
    vectorLayer.opacity = 0.7;
  }

  // 5. Add layers back to Manager in correct rendering stack order
  if (tileLayer) {
    controller.layerManager.addLayer(tileLayer);
  }
  if (vectorLayer) {
    controller.layerManager.addLayer(vectorLayer);
  }
  if (gridLayer) {
    controller.layerManager.addLayer(gridLayer);
  }
  if (radarLayer) {
    controller.layerManager.addLayer(radarLayer);
  }

  // Trigger repaint
  controller.triggerActive();
}

// Startup execution after WASM loading
async function start() {
  // 1. Initialize WebAssembly
  console.log("Initializing WebAssembly...");
  await init();
  console.log("WebAssembly initialized successfully!");

  // 2. Set active projection
  activeProjection = WasmProjection.new_stereographic(SP_LAT_RAD, SP_LON_RAD);

  // 3. Initialize Controller
  const glCanvas = document.getElementById("glCanvas") as HTMLCanvasElement;
  const canvas2D = document.getElementById("canvas2D") as HTMLCanvasElement;

  controller = new OlayerController({
    glCanvas,
    canvas2D,
    projection: activeProjection,
    initialCenterLatRad: SP_LAT_RAD,
    initialCenterLonRad: SP_LON_RAD,
    initialZoom: 1.0,
    viewportBaseMeters: 250000.0, // 250 km base TMA size
  });

  // Register symbols library
  const symbolsJson = JSON.stringify({
    library_name: "OlayerAviationSymbols",
    symbols: {
      "civil:plane": {
        bbox: [-12.0, -12.0, 12.0, 12.0],
        anchor: [0.0, 0.0],
        primitives: [
          {
            type: "Circle",
            cx: 0.0,
            cy: 0.0,
            r: 5.0,
            fill: { r: 0, g: 230, b: 118, a: 255 },
            stroke: { color: { r: 0, g: 100, b: 50, a: 255 }, width: 1.0 }
          },
          {
            type: "Path",
            commands: "M 0,-10 L 0,10 M -8,0 L 8,0 M -4,6 L 4,6",
            stroke: { color: { r: 0, g: 230, b: 118, a: 255 }, width: 1.5 }
          }
        ]
      },
      "mil:fighter": {
        bbox: [-12.0, -12.0, 12.0, 12.0],
        anchor: [0.0, 0.0],
        primitives: [
          {
            type: "Path",
            commands: "M 0,-12 L -6,2 L -10,6 L -2,4 L 0,10 L 2,4 L 10,6 L 6,2 Z",
            fill: { r: 0, g: 176, b: 255, a: 180 },
            stroke: { color: { r: 0, g: 176, b: 255, a: 255 }, width: 1.5 }
          }
        ]
      },
      "mil:cargo": {
        bbox: [-15.0, -15.0, 15.0, 15.0],
        anchor: [0.0, 0.0],
        primitives: [
          {
            type: "Path",
            commands: "M 0,-12 L -4,-8 L -14,-2 L -4,-2 L 0,8 L 4,-2 L 14,-2 L 4,-8 Z",
            fill: { r: 255, g: 145, b: 0, a: 180 },
            stroke: { color: { r: 255, g: 145, b: 0, a: 255 }, width: 1.5 }
          }
        ]
      }
    }
  });
  controller.symbolRegistry.register_declarative_provider(symbolsJson);

  const sldXml = `<?xml version="1.0" encoding="UTF-8"?>
  <StyledLayerDescriptor version="1.0.0">
      <NamedLayer>
          <Name>civil:plane</Name>
          <UserStyle>
              <FeatureTypeStyle>
                  <Rule>
                      <PointSymbolizer>
                          <Graphic>
                              <Mark>
                                  <Fill>
                                      <CssParameter name="fill">#00E676</CssParameter>
                                  </Fill>
                              </Mark>
                          </Graphic>
                      </PointSymbolizer>
                  </Rule>
              </FeatureTypeStyle>
          </UserStyle>
      </NamedLayer>
  </StyledLayerDescriptor>`;
  controller.styleRegistry = WasmStyleRegistry.parse(sldXml);

  // Load mock DTED Level 0 tiles for São Paulo TMA area to support 2.5D flight profile
  console.log("Loading mock DTED tiles...");
  for (let lat = -25; lat <= -22; lat++) {
    for (let lon = -48; lon <= -45; lon++) {
      const latStr = Math.abs(lat).toString().padStart(2, "0") + "0000" + (lat < 0 ? "S" : "N");
      const lonStr = Math.abs(lon).toString().padStart(3, "0") + "0000" + (lon < 0 ? "W" : "E");
      const tile = createMockDted0(latStr, lonStr, 100, 100);
      try {
        controller.terrainEngine.load_tile(tile);
      } catch (e) {
        console.error(`Failed to load tile for lat=${lat}, lon=${lon}:`, e);
      }
    }
  }
  console.log("Mock DTED tiles loaded successfully!");

  // Set controller on window for layers to access
  (window as any).olayerController = controller;

  // Create static grid and radar layers (independent of base map URL)
  gridLayer = new GridLayer(controller.gl, activeProjection, "2D");
  radarLayer = new RadarLayer(controller);

  // Initialize map layers based on GUI selections
  updateMapLayers();

  // Start render loop
  controller.startLoop();

  // Populate with 5 initial planes
  for (let i = 0; i < 5; i++) {
    generateRandomTarget();
  }

  // Hook simulated radar update interval (1 Hz update rate)
  setInterval(() => {
    const R = 6378137.0;
    const timeStep = 1.0; // 1 second update interval

    for (const t of activeSimulatedTargets) {
      // Calculate new position based on speed and heading
      const latOffset = (t.speed * timeStep * Math.cos(t.heading)) / R;
      const lonOffset = (t.speed * timeStep * Math.sin(t.heading)) / (R * Math.cos(t.lat));

      t.lat += latOffset;
      t.lon += lonOffset;

      // Optional slight flight level variations
      if (Math.random() > 0.8) {
        t.alt += (Math.random() > 0.5 ? 100 : -100);
        t.alt = Math.max(200, t.alt); // Keep above ground
      }

      // Feed updated target ping
      controller.interpolator.update_target(
        t.id,
        t.lat,
        t.lon,
        t.alt,
        t.speed,
        t.heading,
        0.0,
        Date.now() / 1000
      );
    }
  }, 1000);

  // Hook UI Stats Loop (FPS)
  setInterval(() => {
    const fpsEl = document.getElementById("fpsVal");
    if (fpsEl) {
      fpsEl.innerText = `${controller.getFPS()} FPS`;
    }
  }, 500);

  // Hook Event Listeners
  document.getElementById("mapSourceSelect")?.addEventListener("change", updateMapLayers);
  document.getElementById("geoserverHostInput")?.addEventListener("input", updateMapLayers);
  document.getElementById("geoserverLayerInput")?.addEventListener("input", updateMapLayers);
  document.getElementById("showVectorOverlayCheckbox")?.addEventListener("change", updateMapLayers);
  document.getElementById("vectorLayerInput")?.addEventListener("input", updateMapLayers);

  document.getElementById("addAircraftBtn")?.addEventListener("click", () => {
    generateRandomTarget();
  });

  document.getElementById("clearTargetsBtn")?.addEventListener("click", () => {
    for (const t of activeSimulatedTargets) {
      controller.interpolator.remove_target(t.id);
    }
    activeSimulatedTargets.length = 0;
    selectedTargetId = null;
    const panel = document.getElementById("profile-panel");
    if (panel) panel.style.display = "none";
    controller.triggerActive();
  });

  // Aircraft click handler for vertical profile select
  canvas2D.addEventListener("click", (e) => {
    const rect = canvas2D.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    let nearestTarget: any = null;
    let minDist = 15; // click threshold in pixels

    const camera = controller.getCameraState();
    const viewMode = controller.getViewMode();
    const viewProjMatrix = controller.currentViewProjMatrix;
    
    let centerXY: number[] = [0, 0];
    if (viewMode !== "3D") {
      try {
        centerXY = controller.projection.project(
          camera.center_lat,
          camera.center_lon,
          camera.center_height
        );
      } catch {}
    }

    let targets: any[] = [];
    try {
      const jsVal = controller.interpolator.interpolate_all(Date.now() / 1000);
      if (jsVal) {
        targets = jsVal as any[];
      }
    } catch {}

    for (const t of targets) {
      const screenPos = radarLayer.cpuRenderer.projectToScreen(
        controller.projection,
        t.position.lat,
        t.position.lon,
        t.position.height,
        centerXY[0],
        centerXY[1],
        camera.zoom,
        camera.rotation,
        camera.viewport_base_meters,
        canvas2D.width,
        canvas2D.height,
        viewMode,
        viewProjMatrix,
        camera.center_lat,
        camera.center_lon
      );

      if (screenPos) {
        const dx = screenPos.x - mouseX;
        const dy = screenPos.y - mouseY;
        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < minDist) {
          minDist = dist;
          nearestTarget = t;
        }
      }
    }

    camera.free();

    if (nearestTarget) {
      selectedTargetId = nearestTarget.id;
      const panel = document.getElementById("profile-panel");
      if (panel) panel.style.display = "block";
      const callsignEl = document.getElementById("profileCallsign");
      if (callsignEl) callsignEl.innerText = selectedTargetId;
    } else {
      selectedTargetId = null;
      const panel = document.getElementById("profile-panel");
      if (panel) panel.style.display = "none";
    }
  });

  const projSelect = document.getElementById("projectionSelect") as HTMLSelectElement;
  projSelect?.addEventListener("change", (e) => {
    const selected = (e.target as HTMLSelectElement).value;
    if (selected === currentProjType) return;

    const oldProj = activeProjection;
    let viewMode: "2D" | "2.5D" | "3D" = "2D";
    
    if (selected === "Stereographic") {
      activeProjection = WasmProjection.new_stereographic(SP_LAT_RAD, SP_LON_RAD);
      viewMode = "2D";
    } else if (selected === "LCC") {
      activeProjection = WasmProjection.new_lcc(
        -20 * (Math.PI / 180),
        -25 * (Math.PI / 180),
        SP_LAT_RAD,
        SP_LON_RAD
      );
      viewMode = "2D";
    } else if (selected === "Mercator") {
      activeProjection = WasmProjection.new_web_mercator();
      viewMode = "2D";
    } else if (selected === "2.5D") {
      activeProjection = WasmProjection.new_web_mercator(); // Web Mercator base for 2.5D
      viewMode = "2.5D";
    } else if (selected === "3D") {
      viewMode = "3D";
      activeProjection = WasmProjection.new_web_mercator(); // dummy projection, unused in 3D
    }

    controller.setViewMode(viewMode);
    (controller as any).projection = activeProjection;
    gridLayer.updateProjection(activeProjection, viewMode);
    
    // Update camera controls panel visibility
    updateCameraControlsVisibility(viewMode);

    currentProjType = selected;
    controller.triggerActive();

    if (oldProj && oldProj !== activeProjection) {
      setTimeout(() => oldProj.free(), 100);
    }
  });

  // Set up initial camera controls visibility
  updateCameraControlsVisibility("2D");

  // Zoom range input listener
  document.getElementById("zoomRange")?.addEventListener("input", (e) => {
    const val = parseFloat((e.target as HTMLInputElement).value);
    controller.setZoom(val);
  });

  document.getElementById("zoomInBtn")?.addEventListener("click", () => {
    const currentZoom = controller.getZoom();
    controller.setZoom(currentZoom * 1.3);
  });

  document.getElementById("zoomOutBtn")?.addEventListener("click", () => {
    const currentZoom = controller.getZoom();
    controller.setZoom(currentZoom / 1.3);
  });

  // Bearing range input listener
  document.getElementById("bearingRange")?.addEventListener("input", (e) => {
    const val = parseFloat((e.target as HTMLInputElement).value);
    controller.setRotation(val * Math.PI / 180);
  });

  // Pitch range input listener
  document.getElementById("pitchRange")?.addEventListener("input", (e) => {
    const val = parseFloat((e.target as HTMLInputElement).value);
    (controller as any).setPitch(val * Math.PI / 180);
  });

  // Roll range input listener
  document.getElementById("rollRange")?.addEventListener("input", (e) => {
    const val = parseFloat((e.target as HTMLInputElement).value);
    (controller as any).setRoll(val * Math.PI / 180);
  });

  // Reset Camera button listener
  document.getElementById("resetCameraBtn")?.addEventListener("click", () => {
    controller.setZoom(1.0);
    controller.setRotation(0.0);
    if ((controller as any).setPitch) {
      const defaultPitch = controller.getViewMode() === "2.5D" ? 35 * Math.PI / 180 : 0.0;
      (controller as any).setPitch(defaultPitch);
    }
    if ((controller as any).setRoll) {
      (controller as any).setRoll(0.0);
    }
    controller.setCenter(SP_LAT_RAD, SP_LON_RAD);
    controller.triggerActive();
  });

  // Run Benchmark button listener
  document.getElementById("runBenchmarkBtn")?.addEventListener("click", () => {
    window.location.href = "./benchmark.html";
  });

  // Periodically synchronize sliders with camera state
  setInterval(syncCameraSliders, 50);
}

function updateCameraControlsVisibility(viewMode: string) {
  const ctrlZoomGroup = document.getElementById("ctrlZoomGroup");
  const ctrlBearingGroup = document.getElementById("ctrlBearingGroup");
  const ctrlPitchGroup = document.getElementById("ctrlPitchGroup");
  const ctrlRollGroup = document.getElementById("ctrlRollGroup");

  if (viewMode === "2D") {
    if (ctrlZoomGroup) ctrlZoomGroup.style.display = "block";
    if (ctrlBearingGroup) ctrlBearingGroup.style.display = "block";
    if (ctrlPitchGroup) ctrlPitchGroup.style.display = "none";
    if (ctrlRollGroup) ctrlRollGroup.style.display = "none";
  } else if (viewMode === "2.5D") {
    if (ctrlZoomGroup) ctrlZoomGroup.style.display = "block";
    if (ctrlBearingGroup) ctrlBearingGroup.style.display = "block";
    if (ctrlPitchGroup) ctrlPitchGroup.style.display = "block";
    if (ctrlRollGroup) ctrlRollGroup.style.display = "none";
  } else if (viewMode === "3D") {
    if (ctrlZoomGroup) ctrlZoomGroup.style.display = "block";
    if (ctrlBearingGroup) ctrlBearingGroup.style.display = "block";
    if (ctrlPitchGroup) ctrlPitchGroup.style.display = "block";
    if (ctrlRollGroup) ctrlRollGroup.style.display = "block";
  }
}

function syncCameraSliders() {
  if (!controller) return;

  const zoom = controller.getZoom();
  const rotation = controller.getRotation(); // in radians
  const pitch = (controller as any).getPitch ? (controller as any).getPitch() : 0;
  const roll = (controller as any).getRoll ? (controller as any).getRoll() : 0;

  // Zoom
  const zoomRange = document.getElementById("zoomRange") as HTMLInputElement;
  const zoomValText = document.getElementById("zoomValText");
  if (zoomRange && document.activeElement !== zoomRange) {
    zoomRange.value = zoom.toFixed(2);
  }
  if (zoomValText) {
    zoomValText.innerText = `${zoom.toFixed(1)}x`;
  }

  // Bearing
  const bearingDeg = Math.round((rotation * 180 / Math.PI) % 360);
  const bearingRange = document.getElementById("bearingRange") as HTMLInputElement;
  const bearingValText = document.getElementById("bearingValText");
  if (bearingRange && document.activeElement !== bearingRange) {
    bearingRange.value = bearingDeg.toString();
  }
  if (bearingValText) {
    bearingValText.innerText = `${bearingDeg}°`;
  }

  // Pitch
  const pitchDeg = Math.round(pitch * 180 / Math.PI);
  const pitchRange = document.getElementById("pitchRange") as HTMLInputElement;
  const pitchValText = document.getElementById("pitchValText");
  if (pitchRange && document.activeElement !== pitchRange) {
    pitchRange.value = pitchDeg.toString();
  }
  if (pitchValText) {
    pitchValText.innerText = `${pitchDeg}°`;
  }

  // Roll
  const rollDeg = Math.round(roll * 180 / Math.PI);
  const rollRange = document.getElementById("rollRange") as HTMLInputElement;
  const rollValText = document.getElementById("rollValText");
  if (rollRange && document.activeElement !== rollRange) {
    rollRange.value = rollDeg.toString();
  }
  if (rollValText) {
    rollValText.innerText = `${rollDeg}°`;
  }
}

// Launch application
start().catch(err => {
  console.error("Failed to start Olayer Demo:", err);
});
