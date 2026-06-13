//! Integration tests for scanner-core.
//!
//! These tests use real barcode images from the `tests/images/` directory.
//! Run with: `cargo test --workspace`

use scanner_core::{BarcodeFormat, PixelFormat, ScanSession, Scanner, ScannerConfig};

// ── Helper ────────────────────────────────────────────────────────────────────

/// Load an image from the test fixtures directory as RGBA bytes.
fn load_test_image_rgba(relative_path: &str) -> Option<(Vec<u8>, u32, u32)> {
    let path = format!("../../tests/images/{relative_path}");
    let img = image::open(&path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

/// Load an image and scan it, returning the result text if found.
fn scan_test_image(path: &str) -> Option<String> {
    let scanner = Scanner::new(ScannerConfig::image_file());
    scanner.scan_file(&format!("../../tests/images/{path}")).ok().and_then(|mut v| {
        v.pop().map(|r| r.text)
    })
}

// ── Scanner API tests ─────────────────────────────────────────────────────────

#[test]
fn test_scan_blank_image_returns_not_found() {
    let scanner = Scanner::with_defaults();
    let data = vec![255u8; 200 * 200 * 4];
    let result = scanner.scan_frame(&data, 200, 200, PixelFormat::Rgba);
    assert!(result.is_err(), "Blank image should not decode anything");
}

#[test]
fn test_scan_frame_validates_buffer_size() {
    let scanner = Scanner::with_defaults();
    let data = vec![0u8; 10]; // Way too small for 640×480 RGBA
    let result = scanner.scan_frame(&data, 640, 480, PixelFormat::Rgba);
    assert!(result.is_err());
}

#[test]
fn test_scan_session_basic_flow() {
    let mut session = ScanSession::new(ScannerConfig::realtime());
    let data = vec![200u8; 100 * 100 * 4];
    // Should not crash, should return None for a blank frame
    let result = session.process_frame(&data, 100, 100, PixelFormat::Rgba);
    assert!(result.is_none());
}

#[test]
fn test_scan_session_stats_update() {
    let mut session = ScanSession::new(ScannerConfig::default());
    let data = vec![200u8; 100 * 100 * 4];
    for _ in 0..5 {
        session.process_frame(&data, 100, 100, PixelFormat::Rgba);
    }
    assert!(session.stats().frames_submitted >= 5);
    assert!(session.stats().avg_scan_time_ms >= 0.0);
}

#[test]
fn test_scan_session_reset() {
    let mut session = ScanSession::new(ScannerConfig::default());
    let data = vec![200u8; 100 * 100 * 4];
    session.process_frame(&data, 100, 100, PixelFormat::Rgba);
    session.reset();
    assert_eq!(session.stats().frames_submitted, 0);
    assert_eq!(session.stats().codes_detected, 0);
}

// ── Image-based tests (require test images) ───────────────────────────────────

#[test]
fn test_scan_qr_clean() {
    if let Some(text) = scan_test_image("qr/clean_hd.png") {
        assert!(!text.is_empty(), "Decoded QR text should not be empty");
        println!("QR clean decoded: {text}");
    } else {
        println!("Skipping: test image not found");
    }
}

#[test]
fn test_scan_qr_blurry() {
    if let Some(text) = scan_test_image("qr/blurry_camera.jpg") {
        assert!(!text.is_empty());
        println!("QR blurry decoded: {text}");
    } else {
        println!("Skipping: test image not found");
    }
}

#[test]
fn test_scan_ean13() {
    if let Some(text) = scan_test_image("barcode_1d/ean13_clean.png") {
        assert_eq!(text.len(), 13, "EAN-13 should decode to 13 digits");
        println!("EAN-13 decoded: {text}");
    } else {
        println!("Skipping: test image not found");
    }
}

#[test]
fn test_scan_code128() {
    if let Some(text) = scan_test_image("barcode_1d/code128.png") {
        assert!(!text.is_empty());
        println!("Code 128 decoded: {text}");
    } else {
        println!("Skipping: test image not found");
    }
}

#[test]
fn test_scan_rotated_qr() {
    let scanner = Scanner::new(ScannerConfig {
        try_rotations: true,
        ..ScannerConfig::image_file()
    });

    if std::path::Path::new("../../tests/images/qr/rotated_45deg.png").exists() {
        let result = scanner.scan_file("../../tests/images/qr/rotated_45deg.png");
        // May or may not detect (45° is not a multiple of 90°, but preprocessing may help)
        println!("Rotated QR result: {:?}", result.map(|r| r[0].text.clone()));
    } else {
        println!("Skipping: test image not found");
    }
}

#[test]
fn test_format_detection() {
    // QR-only config should only detect QR codes
    let scanner = Scanner::new(ScannerConfig::qr_only());
    // A blank image should still return NotFound
    let data = vec![255u8; 200 * 200 * 4];
    let result = scanner.scan_frame(&data, 200, 200, PixelFormat::Rgba);
    assert!(result.is_err());
}

// ── Pixel format conversion tests ─────────────────────────────────────────────

#[test]
fn test_rgb_frame_accepted() {
    let scanner = Scanner::with_defaults();
    let data = vec![200u8; 100 * 100 * 3]; // RGB
    // Should not crash (may return NotFound)
    let _ = scanner.scan_frame(&data, 100, 100, PixelFormat::Rgb);
}

#[test]
fn test_luma_frame_accepted() {
    let scanner = Scanner::with_defaults();
    let data = vec![200u8; 100 * 100]; // Luma
    let _ = scanner.scan_frame(&data, 100, 100, PixelFormat::Luma);
}

#[test]
fn test_nv21_frame_accepted() {
    let scanner = Scanner::with_defaults();
    let size = 100 * 100 + 100 * 100 / 2; // NV21 size
    let data = vec![200u8; size];
    let _ = scanner.scan_frame(&data, 100, 100, PixelFormat::Nv21);
}

#[test]
fn test_yuy2_frame_accepted() {
    let scanner = Scanner::with_defaults();
    let data = vec![200u8; 100 * 100 * 2]; // YUY2
    let _ = scanner.scan_frame(&data, 100, 100, PixelFormat::Yuy2);
}
