use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use olayer_core::geodesy::LatLon;
use olayer_core::interpolator::{InterpolationEngine, TargetState};

fn benchmark_interpolation(c: &mut Criterion) {
    let mut engine = InterpolationEngine::new();

    // Register 1000 simulated targets
    for i in 0..1000 {
        let id = format!("TGT{i:04}");
        let lat = 0.0 + (i as f64) * 0.001;
        let lon = 0.0 + (i as f64) * 0.001;
        let _ = engine.update_target(TargetState {
            id: Arc::from(id),
            last_position: LatLon::new(lat, lon, 1000.0 + (i as f64) * 10.0),
            speed_mps: 200.0,
            track_heading_rad: 0.5,
            vertical_rate_mps: 0.0,
            last_ping_time: 0.0,
        });
    }

    c.bench_function("interpolate_all_1000_targets", |b| {
        b.iter(|| engine.interpolate_all(black_box(5.0)))
    });
}

criterion_group!(benches, benchmark_interpolation);
criterion_main!(benches);
