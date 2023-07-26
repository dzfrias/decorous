use std::fs;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use decorous_frontend::parse;

fn parse_bench(c: &mut Criterion) {
    let inputs =
        fs::read_dir("./benches/inputs").expect("should not have problem reading directory");

    for input in inputs.filter_map(|inp| inp.ok()) {
        let path = input.path();
        let contents = fs::read_to_string(&path).expect("should be able to read input file");
        let name = path
            .file_stem()
            .expect("should have stem")
            .to_string_lossy();
        let id = format!("parse: {name}");
        c.bench_function(&id, |b| b.iter(|| parse(black_box(&contents))));
    }
}

criterion_group!(benches, parse_bench);
criterion_main!(benches);
