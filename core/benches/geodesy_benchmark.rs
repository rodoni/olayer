use criterion::{black_box, criterion_group, criterion_main, Criterion};
use olayer_core::geodesy::{Ellipsoid, LatLon, VincentySolver, HaversineSolver, GeodeticSolver};
use olayer_core::geodesy::conversions::{lla_to_ecef, ecef_to_lla};

fn benchmark_lla_ecef_roundtrip(c: &mut Criterion) {
    let ellipsoid = Ellipsoid::wgs84();
    let point = LatLon::from_degrees(45.0, 45.0, 1000.0);
    c.bench_function("lla_to_ecef", |b| {
        b.iter(|| lla_to_ecef(black_box(&point), black_box(&ellipsoid)))
    });
    let ecef = lla_to_ecef(&point, &ellipsoid);
    c.bench_function("ecef_to_lla", |b| {
        b.iter(|| ecef_to_lla(black_box(&ecef), black_box(&ellipsoid)))
    });
    c.bench_function("lla_ecef_roundtrip", |b| {
        b.iter(|| {
            let ecef = lla_to_ecef(black_box(&point), black_box(&ellipsoid));
            ecef_to_lla(black_box(&ecef), black_box(&ellipsoid))
        })
    });
}

fn benchmark_solvers(c: &mut Criterion) {
    let ellipsoid = Ellipsoid::wgs84();
    let munich = LatLon::from_degrees(48.137154, 11.576124, 0.0);
    let zurich = LatLon::from_degrees(47.376887, 8.541694, 0.0);
    let vincenty = VincentySolver;
    let haversine = HaversineSolver;

    c.bench_function("vincenty_inverse", |b| {
        b.iter(|| vincenty.inverse(black_box(&munich), black_box(&zurich), black_box(&ellipsoid)))
    });
    c.bench_function("haversine_inverse", |b| {
        b.iter(|| haversine.inverse(black_box(&munich), black_box(&zurich), black_box(&ellipsoid)))
    });
    
    let bearing = 0.5_f64;
    let distance = 100_000.0;
    c.bench_function("vincenty_direct", |b| {
        b.iter(|| vincenty.direct(black_box(&munich), black_box(bearing), black_box(distance), black_box(&ellipsoid)))
    });
    c.bench_function("haversine_direct", |b| {
        b.iter(|| haversine.direct(black_box(&munich), black_box(bearing), black_box(distance), black_box(&ellipsoid)))
    });

    // Worst-case: near-antipodal pair forces Vincenty to iterate to the fallback threshold.
    let near_antipodal_1 = LatLon::from_degrees(0.0, 0.0, 0.0);
    let near_antipodal_2 = LatLon::from_degrees(0.0, 179.999, 0.0);
    c.bench_function("vincenty_inverse_near_antipodal", |b| {
        b.iter(|| vincenty.inverse(black_box(&near_antipodal_1), black_box(&near_antipodal_2), black_box(&ellipsoid)))
    });
}

criterion_group!(benches, benchmark_lla_ecef_roundtrip, benchmark_solvers);
criterion_main!(benches);
