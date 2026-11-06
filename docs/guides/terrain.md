# Terrain, altitude and rendering modes

`TerrainEngine` owns elevation data and status-aware sampling. SDK terrain layers choose its visual representation. Appearance settings do not convert vertical datums or change the physical altitude contract.

## Data and unknown ground

Supported engine paths include DTED, Mapbox/Terrarium RGB tiles, COG/GeoTIFF and validated scalar elevation grids. The [WCS adapter](maps/wcs-terrain.md) installs north-up JSON node grids with explicit CRS, registration and ellipsoidal metre heights.

`get_elevation_status(latRad, lonRad)` distinguishes a known zero from unknown elevation. A contributing nodata sample stays unknown; no loaded covering source is an error. Legacy scalar elevation getters retain their zero fallback for compatibility. Explicit scalar grids take precedence over older terrain formats, including when their sample is nodata.

A wrapping terrain `revision()` token invalidates sampled meshes after mutation. Unknown cells emit no terrain triangles/contour segments and cannot hide other layers behind an invented sea-level surface. The [depth guide](maps/depth.md) explains sampling extent, resource lifecycle and compatibility changes.

## Altitude modes

| Mode | Placement |
|---|---|
| `Absolute` / `absolute` | Use the supplied ellipsoidal height |
| `ClampToGround` / `clamp-to-ground` | Use sampled terrain height |
| `RelativeToGround` / `relative-to-ground` | Add supplied height to sampled terrain |
| `RelativeToMesh` / `relative-to-mesh` | Add supplied height to an explicitly provided effective mesh height |

`Absolute` is the default. Unknown policies are `Reject`/`reject`, `UseAbsolute`/`use-absolute`, and `UseZero`/`use-zero`. Choose one explicitly when missing data affects a consumer. Resolving against a mesh requires a mesh-height value/provider; it is not inferred from terrain or visual exaggeration.

The TypeScript controller exposes default mode/policy setters and `resolveAltitude`. `NativeController::resolve_altitude`, WASM `resolve_altitude`, and the C resolver provide the corresponding contract. WFS layers currently offer absolute, clamped and relative-to-ground placement with a height offset; do not assume every layer supports every controller mode. See [binding exposure](../reference/bindings.md) and [WFS](maps/wfs.md).

## Appearance modes

Browser `TerrainLayer` and `TerrainContourLayer` render in 2.5D/3D; they submit no geometry in 2D. Native hosts control their terrain pass/layer visibility.

| Mode | Appearance | Implementation |
|---|---|---|
| `hypsometric` | Elevation-range colors | WebGL/WGPU |
| `hillshade` | Normal/light-based shading | WebGL/WGPU |
| `slope` | Local slope colors | WebGL/WGPU |
| `hybrid` | Elevation tint plus shading | WebGL/WGPU |
| `textured` | XYZ image draped on terrain | Browser terrain imagery path |
| `taws` | Terrain relative to aircraft reference height | WebGL/WGPU |
| Procedural contours | Shader isolines via derivatives | WebGL/WGPU |

```ts
terrainLayer.setRenderMode("taws");
terrainLayer.setTawsReferenceAltitude(1200);
terrainLayer.setElevationRange(0, 2500);
terrainLayer.setHillshade({ azimuthDeg: 315, altitudeDeg: 45 });
terrainLayer.setContours(true, 100);
```

For browser XYZ imagery, set an imagery template and enable imagery on the layer. That path currently samples one XYZ tile at a time; bounded WMS draping is a separate [WMS layer](maps/wms.md).

Native `rebuild_terrain_buffers` samples terrain and computes positions, normals and slope. `set_terrain_style` updates the 32-byte style uniform (mode, elevation range, aircraft altitude, light direction and contour interval). Browser globe normals rotate into ECEF. Missing neighbors use available samples rather than inventing zero ground for slope calculation.

Visual exaggeration affects rendered geometry only. Keep exaggeration consistent across terrain and draped overlays; the shared-depth integration fixtures use exaggeration 1. Terrain is opaque at its default opacity, and partially transparent surfaces require suitable ordering.

## Limits and follow-ups

Meshes are fixed-resolution local patches: native terrain uses 32 cells per axis, while the browser is configurable up to 64. These are not complete globe LOD/adaptive terrain systems. Features smaller than sampling spacing can be missed. Texture CORS/network requirements depend on the selected imagery source.

The native demo's altitude controls and a first-class effective mesh-height provider remain [roadmap items](../plans/map-core.md#altitude-follow-ups). Shared external palettes, multi-tile terrain imagery/atlas support and contour labels are further extensions, not current guarantees.
