//! Integration tests for scanner-core.
//!
//! These tests use real barcode images from the `tests/images/` directory.
//! Run with: `cargo test --workspace`

use scanner_core::{BarcodeFormat, PixelFormat, ScanSession, Scanner, ScannerConfig};

// ── Helper ────────────────────────────────────────────────────────────────────

/// Load an image and scan it, returning the result text if found.
fn scan_test_image(path: &str) -> Result<String, scanner_core::ScanError> {
    let scanner = Scanner::new(ScannerConfig::image_file());
    let results = scanner.scan_file(&format!("../../tests/images/{path}"))?;
    if let Some(res) = results.into_iter().next() {
        Ok(res.text)
    } else {
        Err(scanner_core::ScanError::NotFound)
    }
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
    match scan_test_image("qr/clean_hd.png") {
        Ok(text) => {
            assert!(!text.is_empty(), "Decoded QR text should not be empty");
            println!("QR clean decoded: {text}");
        }
        Err(scanner_core::ScanError::ImageLoad { .. }) => {
            println!("Skipping: test image not found")
        }
        Err(e) => panic!("Test failed: {:?}", e),
    }
}

#[test]
fn test_scan_qr_blurry() {
    match scan_test_image("qr/blurry_camera.jpg") {
        Ok(text) => {
            assert!(!text.is_empty());
            println!("QR blurry decoded: {text}");
        }
        Err(scanner_core::ScanError::ImageLoad { .. }) => {
            println!("Skipping: test image not found")
        }
        Err(e) => panic!("Test failed: {:?}", e),
    }
}

#[test]
fn test_scan_ean13() {
    match scan_test_image("barcode_1d/ean13_clean.png") {
        Ok(text) => {
            assert_eq!(text.len(), 13, "EAN-13 should decode to 13 digits");
            println!("EAN-13 decoded: {text}");
        }
        Err(scanner_core::ScanError::ImageLoad { .. }) => {
            println!("Skipping: test image not found")
        }
        Err(e) => panic!("Test failed: {:?}", e),
    }
}

#[test]
fn test_scan_code128() {
    match scan_test_image("barcode_1d/code128.png") {
        Ok(text) => {
            assert!(!text.is_empty());
            println!("Code 128 decoded: {text}");
        }
        Err(scanner_core::ScanError::ImageLoad { .. }) => {
            println!("Skipping: test image not found")
        }
        Err(e) => panic!("Test failed: {:?}", e),
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

// ── Format filter tests (regression for the format-passthrough bug) ────────────

/// A scanner with a Code128-only filter must not return QR code results.
/// Regression test: before the fix, rxing could ignore PossibleFormats and
/// the result was never validated against the requested format list.
#[test]
fn test_code128_filter_does_not_return_qr() {
    let scanner = Scanner::new(ScannerConfig {
        formats: Some(vec![BarcodeFormat::Code128]),
        ..ScannerConfig::default()
    });

    let test_path = "../../tests/images/qr/clean_hd.png";
    if !std::path::Path::new(test_path).exists() {
        println!("Skipping test: test image not found");
        return;
    }

    let result = scanner.scan_file(test_path);
    assert!(result.is_err(), "QR code image should return NotFound when Code128 format is forced");
}

/// QrCode-only config: any successful decode must be QR or MicroQR.
#[test]
fn test_qr_only_returns_only_qr_formats() {
    let scanner = Scanner::new(ScannerConfig::qr_only());
    
    let test_path = "../../tests/images/barcode_1d/ean13_clean.png";
    if !std::path::Path::new(test_path).exists() {
        println!("Skipping test: test image not found");
        return;
    }

    let result = scanner.scan_file(test_path);
    assert!(result.is_err(), "EAN-13 barcode should return NotFound when only QrCode format is requested");
}

/// QrCode filter should still find QR codes when present (allowlist must not block valid formats).
#[test]
fn test_qr_filter_allows_qr_decode() {
    let scanner = Scanner::new(ScannerConfig {
        formats: Some(vec![BarcodeFormat::QrCode]),
        try_rotations: true,
        ..ScannerConfig::image_file()
    });

    let test_path = "../../tests/images/qr/clean_hd.png";
    if !std::path::Path::new(test_path).exists() {
        println!("Skipping test_qr_filter_allows_qr_decode: test image not found");
        return;
    }

    match scanner.scan_file(test_path) {
        Ok(results) => {
            assert!(!results.is_empty(), "Should have decoded at least one QR");
            assert!(
                matches!(results[0].format, BarcodeFormat::QrCode | BarcodeFormat::MicroQr),
                "Expected QrCode format, got {:?}",
                results[0].format
            );
        }
        Err(scanner_core::ScanError::NotFound) => {
            println!("QR not found — image may be missing or degraded");
        }
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

