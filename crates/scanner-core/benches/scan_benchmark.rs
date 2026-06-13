//! Benchmarks for scanner-core using criterion.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use image::GrayImage;
use scanner_core::preprocess::{clahe, full_pipeline, sauvola_binarize, unsharp_mask};
use scanner_core::{PixelFormat, Scanner, ScannerConfig};

fn bench_grayscale_conversion(c: &mut Criterion) {
    // Simulate a 720p camera frame in RGBA
    let width = 1280u32;
    let height = 720u32;
    let rgba_data = vec![128u8; (width * height * 4) as usize];

    c.bench_function("rgba_to_luma_720p", |b| {
        b.iter(|| {
            let scanner = Scanner::with_defaults();
            let _ = scanner.scan_frame(
                black_box(&rgba_data),
                black_box(width),
                black_box(height),
                PixelFormat::Rgba,
            );
        });
    });
}

fn bench_preprocessing(c: &mut Criterion) {
    // Simulate a 640×480 grayscale frame
    let width = 640u32;
    let height = 480u32;
    let luma_data: Vec<u8> = (0..(width * height) as usize)
        .map(|i| (i % 256) as u8)
        .collect();
    let img = GrayImage::from_raw(width, height, luma_data).unwrap();

    c.bench_function("clahe_480p", |b| {
        b.iter(|| {
            let _ = clahe(black_box(&img), 8, 3.0);
        });
    });

    c.bench_function("unsharp_mask_480p", |b| {
        b.iter(|| {
            let _ = unsharp_mask(black_box(&img), 1.0, 0.5);
        });
    });

    c.bench_function("sauvola_binarize_480p", |b| {
        b.iter(|| {
            let _ = sauvola_binarize(black_box(&img), 15, 0.15);
        });
    });

    c.bench_function("full_pipeline_480p", |b| {
        b.iter(|| {
            let _ = full_pipeline(black_box(&img));
        });
    });
}

fn bench_scan_file(c: &mut Criterion) {
    // Only run if a test image is available
    let test_image = "../../tests/images/qr/clean_hd.png";
    if !std::path::Path::new(test_image).exists() {
        eprintln!("Skipping scan benchmark: test image not found at {test_image}");
        return;
    }

    let scanner = Scanner::new(ScannerConfig::image_file());
    c.bench_function("scan_qr_clean_hd", |b| {
        b.iter(|| {
            let _ = scanner.scan_file(black_box(test_image));
        });
    });
}

criterion_group!(benches, bench_grayscale_conversion, bench_preprocessing, bench_scan_file);
criterion_main!(benches);
