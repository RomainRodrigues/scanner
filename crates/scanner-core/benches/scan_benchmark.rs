//! Benchmarks for scanner-core (`cargo bench -p scanner-core`).
#![allow(missing_docs)] // criterion_group! generates undocumented items

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use image::GrayImage;
use scanner_core::preprocess::{clahe, full_pipeline, sauvola_binarize, unsharp_mask};
use scanner_core::{PixelFormat, ScanSession, Scanner, ScannerConfig};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Deterministic textured 640×480 frame without any code.
fn textured_frame() -> GrayImage {
    GrayImage::from_fn(640, 480, |x, y| {
        image::Luma([((x * 7 + y * 13) % 256) as u8])
    })
}

fn bench_preprocessing(c: &mut Criterion) {
    let img = textured_frame();
    c.bench_function("clahe_480p", |b| b.iter(|| clahe(black_box(&img), 8, 3.0)));
    c.bench_function("unsharp_mask_480p", |b| {
        b.iter(|| unsharp_mask(black_box(&img), 1.0, 0.5));
    });
    c.bench_function("sauvola_480p", |b| {
        b.iter(|| sauvola_binarize(black_box(&img), 15, 0.15));
    });
    c.bench_function("full_pipeline_480p", |b| {
        b.iter(|| full_pipeline(black_box(&img)));
    });
}

/// The real-time hot path: frames without any code, which is most frames.
fn bench_session_empty_frame(c: &mut Criterion) {
    let img = textured_frame();
    let mut session = ScanSession::new(ScannerConfig {
        adaptive_throttle: false,
        ..ScannerConfig::default()
    });
    c.bench_function("session_empty_frame_480p", |b| {
        b.iter(|| session.process_frame(black_box(img.as_raw()), 640, 480, PixelFormat::Luma));
    });
}

fn bench_scan_file(c: &mut Criterion) {
    let path = fixture("qr/clean_hd.png");
    let path = path.to_str().expect("utf-8 path");
    let scanner = Scanner::new(ScannerConfig::image_file());
    c.bench_function("scan_file_qr_clean", |b| {
        b.iter(|| scanner.scan_file(black_box(path)));
    });
}

criterion_group!(
    benches,
    bench_preprocessing,
    bench_session_empty_frame,
    bench_scan_file
);
criterion_main!(benches);
