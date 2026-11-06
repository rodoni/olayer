pub mod c_ffi_bridge;
pub mod map_provider;
pub mod native_controller;
pub mod native_layer_manager;
pub mod tools;
pub mod wgpu_cpu_vertex_pipeline;
pub mod wgpu_gpu_pipeline;
pub mod wgpu_volumetric_pipeline;

pub use map_provider::decoders::{
    decode_wcs_json_coverage, decode_wcs_raster_coverage, decode_wfs_feature_collection,
    decode_wms_image,
};
pub use map_provider::ogc::{NativeOgcClient, OgcTransport, UreqTransport};
pub use map_provider::sources::{
    WcsCoverageSource, WcsCoverageSourceConfig, WfsFeatureSource, WmsMapSource,
};
pub use map_provider::{GeoserverWmtsSource, MapDataSource, NativeMapDataStack, TerrainDataSource};
pub use native_controller::NativeController;
pub use native_layer_manager::{Layer, NativeLayerManager};
pub use tools::{
    CompassRoseConfig, HistoryDot, HoldingPatternConfig, IlsConeConfig, IlsGeometry, IlsTickMark,
    PplLeader, PplTick, RangeRingsConfig, RblMeasurement, SnailTrailManager, TacticalToolsManager,
    TurnDirection, METERS_PER_NAUTICAL_MILE, STANDARD_RATE_ONE_TURN_DPS,
};
pub use wgpu_cpu_vertex_pipeline::{project_lla_to_screen, rasterize_svg, WgpuCpuVertexPipeline};
pub use wgpu_gpu_pipeline::wms::{build_wms_mesh, WgpuWmsImage, WmsMeshOptions, WmsRenderError};
pub use wgpu_gpu_pipeline::{RasterTileUpload, WgpuGpuPipeline};
pub use wgpu_volumetric_pipeline::{
    GpuRibbonVertex, GpuVolumetricVertex, VolumetricGpuMesh, WgpuVolumetricPipeline,
};

pub use map_provider::coverage_terrain::install_wcs_terrain;
