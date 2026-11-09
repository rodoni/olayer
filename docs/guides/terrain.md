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

`Absolute` is the default and ignores ground and mesh heights without querying terrain. For `RelativeToMesh`, an explicitly supplied finite mesh height takes precedence and requires no ground sample; a supplied nonfinite mesh height is rejected, not treated as missing. When mesh height is absent, sampled ground is used as a compatibility fallback, not as an inferred rendered-mesh height. Unknown policies apply when the selected surface is absent: `Reject`/`reject` errors, `UseAbsolute`/`use-absolute` returns the input height, and `UseZero`/`use-zero` uses a zero base (zero for clamping, input height for relative modes). Nonfinite input or selected surface heights and overflowing relative sums are errors under every policy. Choose a policy explicitly when missing data affects a consumer. True mesh-relative placement still requires an explicit mesh-height value/provider; it is not inferred from terrain or visual exaggeration.

The TypeScript controller exposes default mode/policy setters and `resolveAltitude`. `NativeController::resolve_altitude`, WASM `resolve_altitude`, and the C resolver provide the corresponding contract. WFS layers currently offer absolute, clamped and relative-to-ground placement with a height offset; do not assume every layer supports every controller mode. See [binding exposure](../reference/bindings.md) and [WFS](maps/wfs.md).

## Appearance modes

Browser `TerrainLayer` and `TerrainContourLayer` render in 2.5D/3D; they submit no geometry in 2D. Native hosts control their terrain pass/layer visibility.

After a successful mesh rebuild, browser `TerrainLayer.getMeshHeightAt(latRad, lonRad)` and native `WgpuGpuPipeline::terrain_mesh_height_at(latRad, lonRad)` sample the actual retained draw triangles on the CPU. They return `null`/`None` for 2D, outside the local patch or a hole. In 2.5D the returned value is the interpolated rendered Z (including visual exaggeration); in 3D it is the ellipsoidal height of the interpolated ECEF triangle. They do not query the GPU or substitute source-grid terrain for a missing rendered mesh. Results correspond to the last successfully built mesh until the next rebuild.

Browser `TerrainLayer` and native `WgpuGpuPipeline` offer opt-in bounded **whole-grid** LOD. Existing fixed-resolution output remains the default. Configure a finite nonnegative sampled position-error threshold, maximum grid level (base resolution multiplied by a power of two, capped at 64), and source-sample budget (capped at 4,225). Browser `TerrainLayer.setLod(...)` and native `set_terrain_lod(...)` expose an optional quality report with selected subdivisions, sample count, maximum observed residual, whether the sampled threshold was satisfied, unknown-sample and budget-limit flags. The report describes only tested lattice positions. A narrow ridge or nodata island between samples can be missed; this is not a conservative global terrain-error bound or complete globe coverage. Known nodata encountered on the sampled lattice remains a hole rather than being bridged.

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

Native `rebuild_terrain_buffers` samples terrain and computes positions, normals and slope; `set_terrain_lod(Some(options))` selects the opt-in sampled whole-grid LOD builder and `terrain_lod_report()` exposes its diagnostics. Browser `TerrainLayer.setLod(options)` does the equivalent and `getLodReport()` returns its last successful report. `set_terrain_style` updates the 32-byte style uniform (mode, elevation range, aircraft altitude, light direction and contour interval). Browser globe normals rotate into ECEF. Missing neighbors use available samples rather than inventing zero ground for slope calculation.

Visual exaggeration affects rendered geometry only. Keep exaggeration consistent across terrain and draped overlays; the shared-depth integration fixtures use exaggeration 1. Terrain is opaque at its default opacity, and partially transparent surfaces require suitable ordering.

## Limits and follow-ups

The compatibility default remains 32 native and configurable browser grid resolution. Opt-in whole-grid LOD doubles a uniform mesh up to a bounded level using cached sampled XYZ residuals and an aggregate source-sample budget. Elevation queries in a local patch wrap longitude at ±π, but patch geometry/imagery are not split into coordinated tiled-globe seams. LOD is not per-cell adaptive, does not guarantee a global error bound or detect arbitrary off-lattice ridges, and does not yet provide complete tiled-globe coverage. Texture CORS/network requirements depend on the selected imagery source.

The native demo's equivalent altitude placement controls remain a [roadmap item](../plans/map-core.md#altitude-follow-ups). Shared external palettes, multi-tile terrain imagery/atlas support and contour labels are further extensions, not current guarantees.
