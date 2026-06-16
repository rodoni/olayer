use criterion::{black_box, criterion_group, criterion_main, Criterion};
use olayer_native::native_controller::NativeController;
use olayer_native::wgpu_gpu_pipeline::WgpuGpuPipeline;

fn bench_generate_grid_vertices_2d(c: &mut Criterion) {
    let controller = NativeController::new(0.0, 0.0);
    c.bench_function("generate_grid_vertices_2d", |b| {
        b.iter(|| {
            let vertices = WgpuGpuPipeline::generate_grid_vertices(black_box(&controller));
            black_box(vertices.len());
        });
    });
}

fn bench_generate_grid_vertices_3d(c: &mut Criterion) {
    let mut controller = NativeController::new(0.0, 0.0);
    controller.view_mode = "3D".to_string();
    c.bench_function("generate_grid_vertices_3d", |b| {
        b.iter(|| {
            let vertices = WgpuGpuPipeline::generate_grid_vertices(black_box(&controller));
            black_box(vertices.len());
        });
    });
}

criterion_group!(benches, bench_generate_grid_vertices_2d, bench_generate_grid_vertices_3d);
criterion_main!(benches);
