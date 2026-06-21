use criterion::{black_box, criterion_group, criterion_main, Criterion};
use olayer_core::sld::StyleRegistry;
use olayer_core::symbol_registry::{IcaoProvider, NatoProvider, SymbolRegistry};

fn benchmark_symbol_resolution(c: &mut Criterion) {
    let mut registry = SymbolRegistry::new();
    registry.register_provider(Box::new(NatoProvider::new()));
    registry.register_provider(Box::new(IcaoProvider::new()));
    let style = StyleRegistry::default();

    c.bench_function("resolve_nato_fighter", |b| {
        b.iter(|| registry.resolve_symbol(black_box("nato:friend:fighter"), black_box(&style)))
    });

    c.bench_function("resolve_icao_vor", |b| {
        b.iter(|| registry.resolve_symbol(black_box("icao:vor"), black_box(&style)))
    });
}

criterion_group!(benches, benchmark_symbol_resolution);
criterion_main!(benches);
