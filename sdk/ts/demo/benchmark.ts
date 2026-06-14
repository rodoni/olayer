import init, {
  WasmProjection,
  OlayerController,
  CPURenderer,
} from "../src";

// Coordinate center for São Paulo TMA (same as main demo)
const SP_LAT_RAD = -23.62 * (Math.PI / 180);
const SP_LON_RAD = -46.65 * (Math.PI / 180);

interface SimulatedTarget {
  id: string;
  lat: number;
  lon: number;
  alt: number;
  speed: number;
  heading: number;
}

// Global state
let controller: OlayerController | null = null;
let activeProjection: WasmProjection | null = null;
let cpuRenderer: CPURenderer | null = null;
let simulatedTargets: SimulatedTarget[] = [];
let targetCount = 5000;
let isPaused = false;
let lastRadarUpdate = 0;
let startSystemTime = 0;

// Metrics tracking
let frameTimes: number[] = [];
let lastFrameTime = 0;
let calculatedFps = 60.0;
let lastFpsUpdate = 0;

// Logs console helper
const consoleLogEl = document.getElementById("consoleLog") as HTMLDivElement;
function log(msg: string) {
  if (consoleLogEl) {
    const timestamp = new Date().toLocaleTimeString();
    consoleLogEl.innerText = `[${timestamp}] ${msg}\n` + consoleLogEl.innerText;
  }
}

// Initialize application
async function start() {
  log("Loading WASM module...");
  await init();
  log("WASM loaded. Initializing controller...");

  // Setup canvas
  const canvas = document.getElementById("benchmarkCanvas") as HTMLCanvasElement;
  const container = document.getElementById("benchmark-container");

  // Create hidden glCanvas for controller logic
  const glCanvas = document.createElement("canvas");
  glCanvas.style.position = "absolute";
  glCanvas.style.top = "0";
  glCanvas.style.left = "0";
  glCanvas.style.width = "100%";
  glCanvas.style.height = "100%";
  glCanvas.style.visibility = "hidden";
  glCanvas.style.pointerEvents = "none";
  glCanvas.style.zIndex = "0";
  container?.appendChild(glCanvas);

  const ctx = canvas.getContext("2d");
  if (!ctx) {
    log("Error: Could not get 2D canvas context!");
    return;
  }

  // Create active projection
  activeProjection = WasmProjection.new_stereographic(SP_LAT_RAD, SP_LON_RAD);

  // Initialize Controller
  controller = new OlayerController({
    glCanvas,
    canvas2D: canvas,
    projection: activeProjection,
    initialCenterLatRad: SP_LAT_RAD,
    initialCenterLonRad: SP_LON_RAD,
    initialZoom: 1.0,
    viewportBaseMeters: 250000.0,
  });
  controller.setViewMode("2D");

  // Setup CPU Renderer
  cpuRenderer = new CPURenderer(ctx);

  log("GIS controller configured. Generating targets...");
  regenerateTargets();

  // Setup UI Listeners
  setupUI();

  // Start Loops
  startSystemTime = performance.now();
  lastFrameTime = startSystemTime;
  lastRadarUpdate = startSystemTime;
  requestAnimationFrame(renderLoop);

  log("Stress benchmark active. Render loop started at 60 FPS.");
}

function regenerateTargets() {
  if (!controller) return;

  log(`Clearing old targets...`);
  // Remove all current targets from the interpolator
  for (const t of simulatedTargets) {
    controller.interpolator.remove_target(t.id);
  }

  simulatedTargets = [];
  log(`Generating ${targetCount} new random aviation targets...`);

  const nowSec = (performance.now() - startSystemTime) / 1000.0;

  for (let i = 1; i <= targetCount; i++) {
    // Distribute targets randomly within SP TMA (approx 150km radius)
    // 1 degree is approx 111km
    const radius = 0.15 + Math.random() * 1.5;
    const angle = Math.random() * Math.PI * 2.0;

    const lat = SP_LAT_RAD + (radius * Math.cos(angle)) * (Math.PI / 180);
    const lon = SP_LON_RAD + (radius * Math.sin(angle)) * (Math.PI / 180);

    const speed = 120 + Math.random() * 160; // 120 to 280 m/s
    const heading = Math.random() * Math.PI * 2.0;
    const alt = 1000 + Math.random() * 11000; // 1km to 12km

    const id = `STR${i.toString().padStart(4, "0")}`;
    const target: SimulatedTarget = { id, lat, lon, alt, speed, heading };
    simulatedTargets.push(target);

    // Seed the interpolator
    controller.interpolator.update_target(
      target.id,
      target.lat,
      target.lon,
      target.alt,
      target.speed,
      target.heading,
      0.0,
      nowSec
    );
  }

  log(`Target generation complete. ${targetCount} targets loaded.`);
  
  const activeMetric = document.getElementById("activeMetric");
  if (activeMetric) {
    activeMetric.innerText = targetCount.toLocaleString();
  }
}

function setupUI() {
  // Target Slider
  const targetSlider = document.getElementById("targetSlider") as HTMLInputElement;
  const targetCountText = document.getElementById("targetCountText") as HTMLSpanElement;
  targetSlider.addEventListener("input", (e) => {
    targetCount = parseInt((e.target as HTMLInputElement).value);
    targetCountText.innerText = targetCount.toLocaleString();
  });
  targetSlider.addEventListener("change", () => {
    regenerateTargets();
  });

  // Projection Selector
  const projectionSelect = document.getElementById("projectionSelect") as HTMLSelectElement;
  projectionSelect.addEventListener("change", (e) => {
    const selected = (e.target as HTMLSelectElement).value;
    log(`Changing map projection system to ${selected}...`);
    
    const oldProj = activeProjection;
    if (selected === "Stereographic") {
      activeProjection = WasmProjection.new_stereographic(SP_LAT_RAD, SP_LON_RAD);
    } else if (selected === "LCC") {
      activeProjection = WasmProjection.new_lcc(
        -20 * (Math.PI / 180),
        -25 * (Math.PI / 180),
        SP_LAT_RAD,
        SP_LON_RAD
      );
    } else if (selected === "Mercator") {
      activeProjection = WasmProjection.new_web_mercator();
    }

    if (controller && activeProjection) {
      (controller as any).projection = activeProjection;
    }
    
    log(`Projection updated.`);
    if (oldProj) {
      setTimeout(() => oldProj.free(), 100);
    }
  });

  // Toggle Simulation
  const toggleSimBtn = document.getElementById("toggleSimBtn") as HTMLButtonElement;
  toggleSimBtn.addEventListener("click", () => {
    isPaused = !isPaused;
    toggleSimBtn.innerText = isPaused ? "▶️ Resume" : "⏸️ Pause";
    log(isPaused ? "Simulation paused." : "Simulation resumed.");
  });

  // Reset Simulation
  const resetSimBtn = document.getElementById("resetSimBtn") as HTMLButtonElement;
  resetSimBtn.addEventListener("click", () => {
    regenerateTargets();
  });

  // Back to Demo
  const backToDemoBtn = document.getElementById("backToDemoBtn") as HTMLButtonElement;
  backToDemoBtn.addEventListener("click", () => {
    window.location.href = "./index.html";
  });
}

// 60 FPS Render & Interpolation Loop
function renderLoop(timestamp: number) {
  const canvas = document.getElementById("benchmarkCanvas") as HTMLCanvasElement;
  const ctx = canvas.getContext("2d");

  if (!controller || !activeProjection || !cpuRenderer || !canvas || !ctx) {
    requestAnimationFrame(renderLoop);
    return;
  }

  // Calculate FPS
  const elapsed = timestamp - lastFrameTime;
  lastFrameTime = timestamp;
  frameTimes.push(elapsed);
  if (frameTimes.length > 60) {
    frameTimes.shift();
  }

  if (timestamp - lastFpsUpdate > 500) {
    const avgFrameTime = frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
    calculatedFps = 1000 / avgFrameTime;
    lastFpsUpdate = timestamp;

    const fpsMetric = document.getElementById("fpsMetric");
    const performanceToast = document.getElementById("performanceToast") as HTMLDivElement;

    if (fpsMetric) {
      fpsMetric.innerText = `${calculatedFps.toFixed(1)} FPS`;
      if (calculatedFps >= 55) {
        fpsMetric.className = "metric-value highlight-green";
      } else if (calculatedFps >= 45) {
        fpsMetric.className = "metric-value highlight-blue";
      } else if (calculatedFps >= 30) {
        fpsMetric.className = "metric-value highlight-orange";
      } else {
        fpsMetric.className = "metric-value highlight-red";
      }
    }

    if (performanceToast) {
      performanceToast.style.display = calculatedFps < 45 ? "block" : "none";
    }
  }

  const nowSec = (timestamp - startSystemTime) / 1000.0;

  // 1. Step target positions (physics simulated radar ping at 1 Hz)
  if (!isPaused && timestamp - lastRadarUpdate >= 1000) {
    const dt = 1.0;
    const rEarth = 6378137.0;

    for (const t of simulatedTargets) {
      const latOffset = (t.speed * dt * Math.cos(t.heading)) / rEarth;
      const lonOffset = (t.speed * dt * Math.sin(t.heading)) / (rEarth * Math.cos(t.lat));
      t.lat += latOffset;
      t.lon += lonOffset;

      controller.interpolator.update_target(
        t.id,
        t.lat,
        t.lon,
        t.alt,
        t.speed,
        t.heading,
        0.0,
        nowSec
      );
    }
    lastRadarUpdate = timestamp;
  }

  // 2. Dynamic Target Interpolation (WASM Bridge timing)
  const interpStart = performance.now();
  let interpolatedTargets: any[] = [];
  try {
    const jsVal = controller.interpolator.interpolate_all(nowSec);
    if (jsVal) {
      interpolatedTargets = jsVal as any[];
    }
  } catch (err) {
    // Silently ignore interpolation errors in boundary frames
  }
  const interpEnd = performance.now();
  const interpTime = interpEnd - interpStart;

  const interpTimeMetric = document.getElementById("interpTimeMetric");
  if (interpTimeMetric) {
    interpTimeMetric.innerText = `${interpTime.toFixed(2)} ms`;
  }

  // 3. Render Targets on Canvas
  const renderStart = performance.now();
  
  // Clean canvas (Sleek dark radar background)
  ctx.fillStyle = "#090c13";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  // Draw Grid rings
  ctx.strokeStyle = "rgba(0, 230, 118, 0.07)";
  ctx.lineWidth = 1;
  const centerX = canvas.width / 2;
  const centerY = canvas.height / 2;
  for (let r = 100; r < Math.max(canvas.width, canvas.height); r += 100) {
    ctx.beginPath();
    ctx.arc(centerX, centerY, r, 0, Math.PI * 2);
    ctx.stroke();
  }

  // Draw targets
  const camera = controller.getCameraState();
  const viewProjMatrix = controller.currentViewProjMatrix;
  const cxCy = activeProjection.project(camera.center_lat, camera.center_lon, camera.center_height);

  cpuRenderer.beginFrame();

  for (const t of interpolatedTargets) {
    const screenPos = cpuRenderer.projectToScreen(
      activeProjection,
      t.position.lat,
      t.position.lon,
      t.position.height,
      cxCy[0],
      cxCy[1],
      camera.zoom,
      camera.rotation,
      camera.viewport_base_meters,
      canvas.width,
      canvas.height,
      "2D",
      viewProjMatrix,
      camera.center_lat,
      camera.center_lon
    );

    if (screenPos) {
      const speed = simulatedTargets.find(st => st.id === t.id)?.speed || 200;
      
      // Benchmarking uses a lighter custom target draw to focus on raw layout speed
      ctx.save();
      ctx.translate(screenPos.x, screenPos.y);

      // Radar green color scheme
      ctx.fillStyle = "#00e676";
      ctx.strokeStyle = "#00e676";
      ctx.lineWidth = 1.5;

      // Small target dot
      ctx.beginPath();
      ctx.arc(0, 0, 3, 0, 2 * Math.PI);
      ctx.fill();

      // Outer diamond symbol
      ctx.beginPath();
      ctx.moveTo(0, -6);
      ctx.lineTo(6, 0);
      ctx.lineTo(0, 6);
      ctx.lineTo(-6, 0);
      ctx.closePath();
      ctx.stroke();

      ctx.restore();

      // Simple label to prevent full O(N^2) anti-cluttering bottleneck from freezing benchmark.
      // It still draws a label for each aircraft to simulate realistic loads!
      ctx.fillStyle = "#00e676";
      ctx.font = "9px monospace";
      ctx.fillText(t.id, screenPos.x + 8, screenPos.y - 4);
      
      const altitudeFeet = Math.round(t.position.height * 3.28084);
      const fl = Math.round(altitudeFeet / 100);
      ctx.fillStyle = "#b9f6ca";
      ctx.fillText(`FL${fl}`, screenPos.x + 8, screenPos.y + 6);
    }
  }

  // Free camera state struct from WASM memory
  camera.free();

  const renderEnd = performance.now();
  const renderTime = renderEnd - renderStart;

  const renderTimeMetric = document.getElementById("renderTimeMetric");
  if (renderTimeMetric) {
    renderTimeMetric.innerText = `${renderTime.toFixed(1)} ms`;
  }

  requestAnimationFrame(renderLoop);
}

// Start benchmark
start().catch(err => {
  console.error("Failed to start stress benchmark:", err);
  log(`Initialization error: ${err}`);
});
