import {
  WasmTerrainEngine,
  WasmInterpolationEngine,
  WasmProjection,
  WasmCameraState,
} from "olayer-wasm";
import { LayerManager } from "../layers";
import { DataManager } from "../providers";

export type ViewMode = "2D" | "2.5D" | "3D";

export interface OlayerConfig {
  glCanvas: HTMLCanvasElement;
  canvas2D: HTMLCanvasElement;
  projection: WasmProjection;
  initialCenterLatRad?: number;
  initialCenterLonRad?: number;
  initialZoom?: number;
  viewportBaseMeters?: number;
}

export class OlayerController {
  public readonly glCanvas: HTMLCanvasElement;
  public readonly canvas2D: HTMLCanvasElement;
  public readonly gl: WebGL2RenderingContext;
  public readonly ctx2d: CanvasRenderingContext2D;

  // WASM Engines
  public readonly terrainEngine: WasmTerrainEngine;
  public readonly interpolator: WasmInterpolationEngine;
  public readonly projection: WasmProjection;

  // SDK Managers
  public readonly layerManager: LayerManager;
  public readonly dataManager: DataManager;

  // Camera State
  private centerLat: number; // radians
  private centerLon: number; // radians
  private centerHeight: number = 0.0; // meters
  private zoom: number;
  private rotation: number = 0.0; // radians (bearing)
  private pitch: number = 35 * (Math.PI / 180); // radians (tilt / pitch), default 35 degrees for 2.5D
  private roll: number = 0.0; // radians
  private viewportBaseMeters: number;

  // View Mode State
  private viewMode: ViewMode = "2D";
  public currentViewProjMatrix: Float32Array = new Float32Array(16);

  // Interaction State
  private isDragging = false;
  private lastMouseX = 0;
  private lastMouseY = 0;

  // FPS Throttler State
  private isActive = true;
  private lastActiveTime = Date.now();
  private activeTimeoutMs = 1000; // time to remain in active (60 FPS) mode after interaction
  private lastFrameTime = 0;
  private animationFrameId: number | null = null;
  private currentFps = 0;

  constructor(config: OlayerConfig) {
    this.glCanvas = config.glCanvas;
    this.canvas2D = config.canvas2D;

    // Get contexts
    const gl = this.glCanvas.getContext("webgl2");
    if (!gl) {
      throw new Error("WebGL2 is not supported on this browser.");
    }
    this.gl = gl;

    const ctx2d = this.canvas2D.getContext("2d");
    if (!ctx2d) {
      throw new Error("Canvas 2D context is not supported.");
    }
    this.ctx2d = ctx2d;

    // Instantiate WASM wrappers
    this.terrainEngine = new WasmTerrainEngine();
    this.interpolator = new WasmInterpolationEngine();
    this.projection = config.projection;

    // Instantiate Managers
    this.layerManager = new LayerManager();
    this.dataManager = new DataManager(this.terrainEngine);

    // Initial Camera Setup
    this.centerLat = config.initialCenterLatRad ?? 0.0;
    this.centerLon = config.initialCenterLonRad ?? 0.0;
    this.zoom = config.initialZoom ?? 1.0;
    this.viewportBaseMeters = config.viewportBaseMeters ?? 100000.0;

    // Initialize Event Listeners
    this.setupInteractions();
    this.resizeCanvas();
    window.addEventListener("resize", () => this.resizeCanvas());
  }

  /**
   * Resizes canvases to fit their screen dimensions.
   */
  private resizeCanvas(): void {
    const width = this.glCanvas.clientWidth;
    const height = this.glCanvas.clientHeight;

    if (this.glCanvas.width !== width || this.glCanvas.height !== height) {
      this.glCanvas.width = width;
      this.glCanvas.height = height;
      this.canvas2D.width = width;
      this.canvas2D.height = height;
      this.triggerActive();
    }
  }

  /**
   * Triggers active (60 FPS) rendering mode.
   */
  public triggerActive(): void {
    this.isActive = true;
    this.lastActiveTime = Date.now();
  }

  /**
   * Sets the camera center coordinates in radians.
   */
  public setCenter(latRad: number, lonRad: number): void {
    this.centerLat = latRad;
    this.centerLon = lonRad;
    this.triggerActive();
  }

  /**
   * Sets the zoom level.
   */
  public setZoom(zoom: number): void {
    if (zoom > 0) {
      this.zoom = zoom;
      this.triggerActive();
    }
  }

  /**
   * Sets the rotation bearing in radians.
   */
  public setRotation(rotationRad: number): void {
    this.rotation = rotationRad;
    this.triggerActive();
  }

  /**
   * Gets the current FPS.
   */
  public getFPS(): number {
    return this.currentFps;
  }

  /**
   * Returns the current CameraState.
   */
  public getCameraState(): WasmCameraState {
    const aspect = this.glCanvas.width / this.glCanvas.height;
    return new WasmCameraState(
      this.centerLat,
      this.centerLon,
      this.centerHeight,
      this.zoom,
      this.rotation,
      this.pitch,
      this.roll,
      aspect,
      this.viewportBaseMeters
    );
  }

  public getViewMode(): ViewMode {
    return this.viewMode;
  }

  public setViewMode(value: ViewMode): void {
    this.viewMode = value;
    if (value === "2.5D") {
      this.pitch = 35 * (Math.PI / 180);
      this.roll = 0.0;
    } else if (value === "2D") {
      this.pitch = 0.0;
      this.roll = 0.0;
    }
    this.triggerActive();
  }

  public getIs3D(): boolean {
    return this.viewMode === "3D";
  }

  public setIs3D(value: boolean): void {
    this.viewMode = value ? "3D" : "2D";
    this.triggerActive();
  }

  public getCenterLat(): number {
    return this.centerLat;
  }

  public getCenterLon(): number {
    return this.centerLon;
  }

  public getCenterHeight(): number {
    return this.centerHeight;
  }

  public getZoom(): number {
    return this.zoom;
  }

  public getRotation(): number {
    return this.rotation;
  }

  public getPitch(): number {
    return this.pitch;
  }

  public setPitch(pitchRad: number): void {
    this.pitch = Math.max(0, Math.min(180 * Math.PI / 180, pitchRad));
    this.triggerActive();
  }

  public getRoll(): number {
    return this.roll;
  }

  public setRoll(rollRad: number): void {
    this.roll = ((rollRad + Math.PI) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI) - Math.PI;
    this.triggerActive();
  }

  public getViewportBaseMeters(): number {
    return this.viewportBaseMeters;
  }

  /**
   * Starts the animation render loop.
   */
  public startLoop(): void {
    if (this.animationFrameId !== null) return;
    this.lastFrameTime = performance.now();
    const loop = (timestamp: number) => {
      this.tick(timestamp);
      this.animationFrameId = requestAnimationFrame(loop);
    };
    this.animationFrameId = requestAnimationFrame(loop);
  }

  /**
   * Stops the render loop.
   */
  public stopLoop(): void {
    if (this.animationFrameId !== null) {
      cancelAnimationFrame(this.animationFrameId);
      this.animationFrameId = null;
    }
  }

  /**
   * Evaluates a frame, throttling the FPS if camera is inactive.
   */
  private tick(timestamp: number): void {
    const elapsedMs = timestamp - this.lastFrameTime;
    
    // Check if we should drop out of active mode (timeout reached)
    if (this.isActive && Date.now() - this.lastActiveTime > this.activeTimeoutMs) {
      this.isActive = false;
    }

    const targetFps = this.isActive ? 60 : 15;
    const frameIntervalMs = 1000 / targetFps;

    if (elapsedMs < frameIntervalMs) {
      return; // Skip frame to match target FPS
    }

    this.currentFps = Math.round(1000 / elapsedMs);
    this.lastFrameTime = timestamp;

    this.renderFrame();
  }

  /**
   * Renders static and dynamic layers using camera states.
   */
  private renderFrame(): void {
    const camera = this.getCameraState();
    
    // 1. Get View-Projection Matrix from WASM
    let vpMatrix: Float32Array;
    try {
      const flatMatrix = this.viewMode === "3D"
        ? this.projection.get_3d_view_proj_matrix(camera)
        : this.viewMode === "2.5D"
        ? this.projection.get_25d_view_proj_matrix(camera)
        : this.projection.get_view_proj_matrix(camera);
      vpMatrix = new Float32Array(flatMatrix);
      this.currentViewProjMatrix = vpMatrix;
    } catch (err) {
      console.error("Failed to calculate View-Projection matrix:", err);
      camera.free();
      return;
    }

    // 2. Render Static WebGL Layers (Clear and Draw)
    this.gl.viewport(0, 0, this.glCanvas.width, this.glCanvas.height);
    this.gl.clearColor(0.08, 0.09, 0.12, 1.0); // SLEEK DARK MODE BASE
    this.gl.clear(this.gl.COLOR_BUFFER_BIT | this.gl.DEPTH_BUFFER_BIT);

    this.layerManager.renderStaticLayers(this.gl, vpMatrix);

    // 3. Clear and Render Dynamic CPU Layers (Canvas 2D)
    this.ctx2d.clearRect(0, 0, this.canvas2D.width, this.canvas2D.height);
    this.layerManager.renderDynamicLayers(this.ctx2d, Date.now() / 1000);

    // Free camera wrapper in WebAssembly heap
    camera.free();
  }

  /**
   * Configures mouse interactions for map dragging (pan/orbit) and wheel zooming.
   */
  private setupInteractions(): void {
    const getProjectedCoordinates = (latRad: number, lonRad: number) => {
      const xy = this.projection.project(latRad, lonRad, this.centerHeight);
      return { x: xy[0], y: xy[1] };
    };

    // Prevent context menu on canvas to allow smooth right-click dragging
    this.canvas2D.addEventListener("contextmenu", (e) => e.preventDefault());

    this.canvas2D.addEventListener("mousedown", (e) => {
      this.isDragging = true;
      this.lastMouseX = e.clientX;
      this.lastMouseY = e.clientY;
      this.triggerActive();
    });

    window.addEventListener("mouseup", () => {
      this.isDragging = false;
    });

    this.canvas2D.addEventListener("mousemove", (e) => {
      if (!this.isDragging) return;

      this.triggerActive();

      const dx = e.clientX - this.lastMouseX;
      const dy = e.clientY - this.lastMouseY;

      this.lastMouseX = e.clientX;
      this.lastMouseY = e.clientY;

      const isRightClickOrShift = e.buttons === 2 || e.shiftKey;

      if (this.viewMode === "3D") {
        if (isRightClickOrShift) {
          // Adjust rotation (bearing) and pitch (tilt) in 3D
          const dRot = dx * 0.005;
          const dPitch = dy * 0.005;
          this.rotation = (this.rotation - dRot) % (2 * Math.PI);
          this.pitch = Math.max(0, Math.min(180 * Math.PI / 180, this.pitch + dPitch));
        } else {
          // Rotate camera in orbital 3D space
          const lonOffset = dx * 0.005;
          const latOffset = dy * 0.005;
          this.centerLon = (this.centerLon - lonOffset) % (2 * Math.PI);
          // Clamp latitude to avoid pole singularity flip
          this.centerLat = Math.max(-Math.PI / 2 + 0.01, Math.min(Math.PI / 2 - 0.01, this.centerLat + latOffset));
        }
        return;
      }

      if (this.viewMode === "2.5D" && isRightClickOrShift) {
        // Adjust rotation (bearing) and pitch (tilt) in 2.5D
        const dRot = dx * 0.005;
        const dPitch = dy * 0.005;
        this.rotation = (this.rotation - dRot) % (2 * Math.PI);
        this.pitch = Math.max(0, Math.min(180 * Math.PI / 180, this.pitch + dPitch));
        return;
      }

      if (this.viewMode === "2D" && isRightClickOrShift) {
        // Adjust rotation (bearing) in 2D
        const dRot = dx * 0.005;
        this.rotation = (this.rotation - dRot) % (2 * Math.PI);
        return;
      }

      // Project current center to planar meters
      const { x: cx, y: cy } = getProjectedCoordinates(this.centerLat, this.centerLon);

      // Compute scale: map viewport dimensions
      const aspect = this.glCanvas.width / this.glCanvas.height;
      const w = this.viewportBaseMeters / this.zoom;
      const h = w / aspect;

      const metersPerPixelX = w / this.glCanvas.width;
      const metersPerPixelY = h / this.glCanvas.height;

      // Rotate mouse drag offset by negative bearing angle
      const cosTheta = Math.cos(-this.rotation);
      const sinTheta = Math.sin(-this.rotation);

      const rx = dx * cosTheta - dy * sinTheta;
      const ry = dx * sinTheta + dy * cosTheta;

      // New center in planar meters
      const newCx = cx - rx * metersPerPixelX;
      const newCy = cy + ry * metersPerPixelY; // Screen Y goes down, Projected Y goes up

      // Unproject back to geodetic WGS84
      try {
        const lla = this.projection.unproject(newCx, newCy);
        this.centerLat = lla.lat;
        this.centerLon = lla.lon;
        lla.free();
      } catch (err) {
        console.error("Pan unproject failed:", err);
      }
    });

    this.canvas2D.addEventListener("wheel", (e) => {
      e.preventDefault();
      this.triggerActive();

      const factor = e.deltaY < 0 ? 1.1 : 0.9;
      this.zoom = Math.max(0.1, Math.min(this.zoom * factor, 1000.0));
    }, { passive: false });
  }

  /**
   * Destroys contexts and unloads WASM memory allocations.
   */
  public destroy(): void {
    this.stopLoop();
    this.dataManager.clearCache();
    this.terrainEngine.free();
    this.interpolator.free();
    this.projection.free();
  }
}
