use compress::{compress_content, CompressionConfig, ContentKind};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn sample_json() -> String {
    let items: Vec<serde_json::Value> = (0..200)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "status": if i == 137 { "failed" } else { "ok" },
                "message": if i == 137 { "timeout while fetching" } else { "all good" },
            })
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap()
}

fn sample_log() -> String {
    (0..400)
        .map(|i| {
            if i == 251 {
                format!("{i}: ERROR task failed with timeout")
            } else {
                format!("{i}: info request completed")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sample_code() -> String {
    let mut out = String::new();
    out.push_str("use std::fmt;\n\n");
    for i in 0..80 {
        out.push_str(&format!("pub fn generated_{i}() {{\n"));
        out.push_str("    let x = 1;\n");
        out.push_str("    println!(\"{}\", x);\n");
        out.push_str("}\n\n");
    }
    out
}

fn bench_json(c: &mut Criterion) {
    let input = sample_json();
    c.bench_function("compress_json", |b| {
        b.iter(|| {
            let result = compress_content(
                black_box(&input),
                Some(ContentKind::Json),
                CompressionConfig::default(),
            );
            black_box(result)
        })
    });
}

fn bench_log(c: &mut Criterion) {
    let input = sample_log();
    c.bench_function("compress_log", |b| {
        b.iter(|| {
            let result = compress_content(
                black_box(&input),
                Some(ContentKind::Log),
                CompressionConfig::default(),
            );
            black_box(result)
        })
    });
}

fn bench_code(c: &mut Criterion) {
    let input = sample_code();
    c.bench_function("compress_code", |b| {
        b.iter(|| {
            let result = compress_content(
                black_box(&input),
                Some(ContentKind::Code),
                CompressionConfig::default(),
            );
            black_box(result)
        })
    });
}

criterion_group!(compress_baseline, bench_json, bench_log, bench_code);
criterion_main!(compress_baseline);
