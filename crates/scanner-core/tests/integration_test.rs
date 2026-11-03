//! Integration tests on the committed fixture images (`tests/fixtures/`).
//!
//! Fixtures are versioned so these tests always run: a missing file is a test
//! failure, never a silent skip. Regenerate them with `tests/fixtures/generate.py`.

use std::path::PathBuf;

use scanner_core::{
    BarcodeFormat, PixelFormat, Roi, ScanError, ScanSession, Scanner, ScannerConfig,
};

const LONG_QR: &str = "Hello, Scanner! This is a blurry QR code test.";

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    assert!(path.exists(), "missing fixture {}", path.display());
    path
}

fn luma(name: &str) -> image::GrayImage {
    image::open(fixture(name))
        .expect("readable fixture")
        .to_luma8()
}

/// Scan a fixture with the static-image configuration and return `(text, format)` pairs.
fn scan(name: &str, config: ScannerConfig) -> Result<Vec<(String, BarcodeFormat)>, ScanError> {
    let path = fixture(name);
    let results = Scanner::new(config).scan_file(path.to_str().expect("utf-8 path"))?;
    Ok(results.into_iter().map(|r| (r.text, r.format)).collect())
}

fn single(name: &str) -> (String, BarcodeFormat) {
    let mut results =
        scan(name, ScannerConfig::image_file()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(results.len(), 1, "{name}: expected exactly one code");
    results.remove(0)
}

// ── Decoding of every fixture ─────────────────────────────────────────────────

#[test]
fn decodes_qr_fixtures() {
    let cases = [
        ("qr/clean_hd.png", "https://github.com/romrd/scanner"),
        ("qr/tiny_in_large.png", "Tiny code"),
        ("qr/rotated_45deg.png", LONG_QR),
        ("qr/inverted.png", LONG_QR),
        ("qr/dark_low_light.jpg", LONG_QR),
        ("qr/blurry_camera.jpg", LONG_QR),
    ];
    for (name, expected) in cases {
        assert_eq!(
            single(name),
            (expected.to_string(), BarcodeFormat::QrCode),
            "{name}"
        );
    }
}

#[test]
fn decodes_linear_fixtures() {
    let cases = [
        (
            "barcode_1d/ean13_clean.png",
            "5901234123457",
            BarcodeFormat::Ean13,
        ),
        ("barcode_1d/upc_a.png", "012345678905", BarcodeFormat::UpcA),
        (
            "barcode_1d/code128.png",
            "SCANNER-TEST-128",
            BarcodeFormat::Code128,
        ),
    ];
    for (name, text, format) in cases {
        assert_eq!(single(name), (text.to_string(), format), "{name}");
    }
}

#[test]
fn photo_without_code_is_not_found() {
    assert!(matches!(
        scan("edge_cases/no_code.jpg", ScannerConfig::image_file()),
        Err(ScanError::NotFound)
    ));
}

#[test]
fn decodes_multiple_codes_in_one_image() {
    let mut texts: Vec<String> = scan("qr/multiple.png", ScannerConfig::image_file())
        .expect("two codes")
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    texts.sort();
    assert_eq!(texts, ["Code A", "Code B"]);
}

// ── Format filter (regression: rxing does not strictly honour PossibleFormats) ─

#[test]
fn format_filter_rejects_other_symbologies() {
    let code128_only = ScannerConfig {
        formats: Some(vec![BarcodeFormat::Code128]),
        ..ScannerConfig::image_file()
    };
    assert!(matches!(
        scan("qr/clean_hd.png", code128_only),
        Err(ScanError::NotFound)
    ));

    let qr_only = ScannerConfig {
        formats: Some(vec![BarcodeFormat::QrCode, BarcodeFormat::MicroQr]),
        ..ScannerConfig::image_file()
    };
    assert!(matches!(
        scan("barcode_1d/ean13_clean.png", qr_only.clone()),
        Err(ScanError::NotFound)
    ));
    assert_eq!(
        scan("qr/clean_hd.png", qr_only).unwrap()[0].1,
        BarcodeFormat::QrCode
    );
}

// ── Geometry ──────────────────────────────────────────────────────────────────

/// Result points must be expressed in full-frame coordinates, even with a ROI.
#[test]
fn roi_points_are_in_frame_coordinates() {
    let img = luma("qr/tiny_in_large.png");
    let (w, h) = img.dimensions();
    let scanner = Scanner::new(ScannerConfig {
        roi: Some(Roi::centered(0.5)),
        ..ScannerConfig::default()
    });
    let results = scanner
        .scan_frame(img.as_raw(), w, h, PixelFormat::Luma)
        .expect("tiny QR inside the ROI should decode");
    assert_eq!(results[0].text, "Tiny code");
    // The code is pasted at (440..560, 440..560) in the full image.
    for p in &results[0].points {
        assert!(
            (430.0..570.0).contains(&p.x),
            "x = {} not in frame space",
            p.x
        );
        assert!(
            (430.0..570.0).contains(&p.y),
            "y = {} not in frame space",
            p.y
        );
    }
}

// ── Input validation ──────────────────────────────────────────────────────────

#[test]
fn blank_frame_is_not_found() {
    let scanner = Scanner::with_defaults();
    let data = vec![255u8; 200 * 200 * 4];
    assert!(matches!(
        scanner.scan_frame(&data, 200, 200, PixelFormat::Rgba),
        Err(ScanError::NotFound)
    ));
}

#[test]
fn undersized_buffer_is_rejected() {
    let scanner = Scanner::with_defaults();
    assert!(matches!(
        scanner.scan_frame(&[0u8; 10], 640, 480, PixelFormat::Rgba),
        Err(ScanError::InvalidImage(_))
    ));
}

#[test]
fn every_pixel_format_is_accepted() {
    let scanner = Scanner::with_defaults();
    let (w, h) = (100u32, 100u32);
    for format in [
        PixelFormat::Rgba,
        PixelFormat::Rgb,
        PixelFormat::Luma,
        PixelFormat::Nv21,
        PixelFormat::Yuy2,
    ] {
        let data = vec![200u8; format.buffer_size(w, h)];
        assert!(
            matches!(
                scanner.scan_frame(&data, w, h, format),
                Err(ScanError::NotFound)
            ),
            "{format:?}"
        );
    }
}

// ── Real-time session ─────────────────────────────────────────────────────────

fn session() -> ScanSession {
    ScanSession::new(ScannerConfig {
        adaptive_throttle: false,
        ..ScannerConfig::default()
    })
}

#[test]
fn session_stats_and_reset() {
    let mut s = session();
    let data = vec![200u8; 100 * 100 * 4];
    for _ in 0..5 {
        assert!(s
            .process_frame(&data, 100, 100, PixelFormat::Rgba)
            .is_none());
    }
    assert_eq!(s.stats().frames_submitted, 5);
    assert_eq!(s.stats().frames_scanned, 5);
    s.reset();
    assert_eq!(s.stats().frames_submitted, 0);
    assert_eq!(s.stats().codes_detected, 0);
}

/// The session spreads fallback strategies over frames: an inverted code that
/// the fast path cannot read must still be found within a few frames, and is
/// then reported only once during the cooldown.
#[test]
fn session_finds_inverted_code_and_deduplicates_it() {
    let img = luma("qr/inverted.png");
    let (w, h) = img.dimensions();
    let mut s = session();
    let found = (0..6).find_map(|_| s.process_frame(img.as_raw(), w, h, PixelFormat::Luma));
    assert_eq!(found.expect("inverted QR")[0].text, LONG_QR);
    for _ in 0..6 {
        assert!(s
            .process_frame(img.as_raw(), w, h, PixelFormat::Luma)
            .is_none());
    }
    assert_eq!(s.stats().codes_detected, 1);
}
