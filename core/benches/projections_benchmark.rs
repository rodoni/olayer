use criterion::{black_box, criterion_group, criterion_main, Criterion};
use olayer_core::geodesy::{Ellipsoid, LatLon};
use olayer_core::projections::{LambertConformalConic, Projection, Stereographic, WebMercator};

fn benchmark_projections(c: &mut Criterion) {
    let ellipsoid = Ellipsoid::wgs84();
    let stereo = Stereographic::new(-23.55_f64.to_radians(), -46.63_f64.to_radians(), ellipsoid);
    let lcc = LambertConformalConic::new(
        33.0_f64.to_radians(),
        45.0_f64.to_radians(),
        0.0_f64.to_radians(),
        -96.0_f64.to_radians(),
        ellipsoid,
    );
    let wm = WebMercator::new(ellipsoid);

    let point = LatLon::from_degrees(-23.55, -46.63, 0.0);

    c.bench_function("stereographic_project", |b| {
        b.iter(|| stereo.project(black_box(&point)))
    });
    c.bench_function("lcc_project", |b| b.iter(|| lcc.project(black_box(&point))));
    c.bench_function("web_mercator_project", |b| b.iter(|| wm.project(black_box(&point))));

    c.bench_function("stereographic_roundtrip", |b| {
        b.iter(|| {
            let (x, y) = stereo.project(black_box(&point)).unwrap();
            stereo.unproject(black_box(x), black_box(y))
        })
    });
    c.bench_function("lcc_roundtrip", |b| {
        b.iter(|| {
            let (x, y) = lcc.project(black_box(&point)).unwrap();
            lcc.unproject(black_box(x), black_box(y))
        })
    });
    c.bench_function("web_mercator_roundtrip", |b| {
        b.iter(|| {
            let (x, y) = wm.project(black_box(&point)).unwrap();
            wm.unproject(black_box(x), black_box(y))
        })
    });
}

criterion_group!(benches, benchmark_projections);
criterion_main!(benches);
