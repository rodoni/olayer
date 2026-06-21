use criterion::{black_box, criterion_group, criterion_main, Criterion};
use olayer_core::terrain::DtedTile;

fn create_mock_dted0(num_cols: usize, num_rows: usize) -> Vec<u8> {
    let mut data = vec![b' '; 3428];
    data[0..4].copy_from_slice(b"UHL1");
    let lon_bytes = format!("{: <8}", "0480000W");
    data[4..12].copy_from_slice(lon_bytes.as_bytes());
    let lat_bytes = format!("{: <8}", "230000S");
    data[12..20].copy_from_slice(lat_bytes.as_bytes());
    data[20..24].copy_from_slice(b"0300");
    data[24..28].copy_from_slice(b"0300");
    let cols_str = format!("{:0>4}", num_cols);
    data[47..51].copy_from_slice(cols_str.as_bytes());
    let rows_str = format!("{:0>4}", num_rows);
    data[51..55].copy_from_slice(rows_str.as_bytes());

    let col_size = 11 + num_rows * 2;
    for c in 0..num_cols {
        let mut col = vec![0u8; col_size];
        col[0] = 0xAA;
        col[1..4].copy_from_slice(&[0, 0, c as u8]);
        col[4..7].copy_from_slice(&[0, 0, 0]);
        for r in 0..num_rows {
            let height = (c * 10 + r) as i16;
            let be = height.to_be_bytes();
            let idx = 7 + r * 2;
            col[idx] = be[0];
            col[idx + 1] = be[1];
        }
        data.extend_from_slice(&col);
    }
    data
}

fn benchmark_dted_parse(c: &mut Criterion) {
    let data_100 = create_mock_dted0(100, 100);
    let data_1200 = create_mock_dted0(1200, 1200);

    c.bench_function("dted_parse_100x100", |b| {
        b.iter(|| DtedTile::from_bytes(black_box(&data_100)).unwrap())
    });

    c.bench_function("dted_parse_1200x1200", |b| {
        b.iter(|| DtedTile::from_bytes(black_box(&data_1200)).unwrap())
    });
}

criterion_group!(benches, benchmark_dted_parse);
criterion_main!(benches);
