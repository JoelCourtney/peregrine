use peregrine::{
    graph::series::resource::Resource,
    op,
    plan::{Duration, Time},
    run,
};

fn run_line(n: usize) -> i32 {
    let mut time = Time::from_tai_seconds(0.0);

    let x = Resource::new(1);

    for _ in 0..n {
        time += Duration::from_seconds(1.0);
        x.mutate_at(time, |x| op! { i!(x) + 1 });
    }

    run(x.get_at_inc(time))
}

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("straight-line 20", |b| b.iter(|| run_line(black_box(20))));
    c.bench_function("straight-line 1000", |b| {
        b.iter(|| run_line(black_box(1000)))
    });
    c.bench_function("straight-line 10000", |b| {
        b.iter(|| run_line(black_box(10000)))
    });
    c.bench_function("straight-line 60000", |b| {
        b.iter(|| run_line(black_box(60000)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
