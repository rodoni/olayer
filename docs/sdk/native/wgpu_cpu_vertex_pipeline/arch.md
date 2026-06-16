# Architecture: wgpu CPU/Vertex Pipeline

This document details the architectural design and technical specification of the **wgpu CPU/Vertex Pipeline** component of the Olayer Native SDK.

---

## 1. Overview

The **wgpu CPU/Vertex Pipeline** is responsible for calculating two-dimensional screen pixel coordinates $(X, Y)$ from three-dimensional geodetic coordinates (latitude, longitude, altitude) of dynamic radar targets, drawing them without three-dimensional perspective distortions ( **Billboard** effect). This component also manages the heading vectors, tactical data labels (labels), and the rendering of the 2.5D flight profile.

```mermaid
graph LR
    Target[Geodetic Target Lat/Lon/Alt] -->|Dead Reckoning| Interp[Interpolated Position]
    Interp -->|CPU Projection| Screen[Screen Coordinates X/Y]
    Screen -->|Billboard Rendering| UI[Radar Overlay and Labels]
```

---

## 2. CPU Projection Algorithm

The `project_lla_to_screen` function in [mod.rs](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/src/wgpu_cpu_vertex_pipeline/mod.rs) processes the conversion according to the configured view mode:

### 2.1 Projection in 3D Mode (Virtual Globe)
1. **ECEF Conversion:** Transforms the LLA point $(\phi, \lambda, h)$ into ECEF rectangular coordinates $(X, Y, Z)$ using the WGS84 ellipsoid.
2. **Horizon Occlusion Culling:** Prevents rendering of targets located behind the Earth's curvature:
   $$\mathbf{x}_{\text{cam}} \cdot \mathbf{x}_{\text{target}} < R_{\text{earth}}^2$$
   If the dot product between the camera vector and the target vector is less than the Earth's radius squared, the target is hidden by the horizon and is discarded.
3. **MVP Multiplication:** Multiplies the ECEF coordinates by the 3D View-Projection matrix and converts homogeneous NDC coordinates to physical screen coordinates.

### 2.2 Projection in 2.5D Mode (Perspective Map)
Projects the target base using the active planar cartographic projection, adds altitude as the Z axis, multiplies by the camera perspective matrix, and converts to screen coordinates.

### 2.3 Projection in 2D Mode (Flat Map)
Projects using the geographic equations (Stereographic, LCC, Mercator), rotates and scales according to the camera azimuth (`bearing`) and zoom.

---

## 3. Target and Tactical Vector Drawing

In the desktop event loop:
* **Icon and Selection Box:** Aircraft are drawn as circles at the projected point, surrounded by rectangles if selected by the operator.
* **Heading Vector (Velocity Vector):** Draws a line segment representing the aircraft's estimated displacement for 1 minute ahead, calculated via speed ($m/s$) and heading (radians).
* **Data Label (Label):** Data box aligned to the target containing CALLSIGN, Altitude (FL - Flight Level), and Speed in knots (KT).

---

## 4. 2.5D Flight Profile Visualization and CFIT Alert

When an aircraft is selected, the SDK activates the 2.5D flight profile visualization at the bottom of the operational panel:
1. **Route Sampling:** Generates geodetic route points ahead and behind the target's current position.
2. **Altitude Profile:** The Core's `TerrainEngine` queries in constant time $O(1)$ the DTED files to extract the ground relief under these points.
3. **CFIT Alert (Controlled Flight Into Terrain):** If the difference between the aircraft's altitude and the ground altitude is less than the tactical safety margin (e.g., 300 meters / 1000 feet), a red alert with visual warning `CFIT HAZARD` is triggered on the flight controller's screen.
