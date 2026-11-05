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

/// Real photo (phone camera, 756×1008) of a French cattle ear tag: laser-engraved
/// Code 128 on orange plastic, uneven lighting along the code, grainy bars that
/// look hollow, and the tag edge only ~3 modules from the code.
/// Content: "0250" (ISO 3166 France) + national number 2239036645.
#[test]
fn decodes_laser_engraved_cattle_ear_tag() {
    let results = scan(
        "real/cattle_ear_tag_code128.jpg",
        ScannerConfig::image_file(),
    )
    .expect("ear tag Code 128 should decode");
    assert_eq!(
        results[0],
        ("02502239036645".to_string(), BarcodeFormat::Code128)
    );
}

/// Same tag in a camera session: found within a few frames thanks to the
/// fallback strategies run in turn.
#[test]
fn session_reads_cattle_ear_tag() {
    let img = luma("real/cattle_ear_tag_code128.jpg");
    let (w, h) = img.dimensions();
    let mut s = session();
    let found = (0..12).find_map(|_| s.process_frame(img.as_raw(), w, h, PixelFormat::Luma));
    assert_eq!(found.expect("ear tag in session")[0].text, "02502239036645");
}

/// The same tag as the SDK sees it from a 1280×720 camera: photo scaled to
/// 720 px high, then cropped to the default region of interest (1024×432).
/// The code is smaller here (~3.4 px per module instead of ~4.7).
#[test]
fn session_reads_cattle_ear_tag_in_camera_roi() {
    let photo = image::open(fixture("real/cattle_ear_tag_code128.jpg")).expect("fixture");
    let photo = photo
        .resize(540, 720, image::imageops::FilterType::CatmullRom)
        .to_luma8();
    let mut frame = image::GrayImage::from_pixel(1280, 720, image::Luma([255]));
    image::imageops::overlay(&mut frame, &photo, (1280 - 540) / 2, 0);
    let roi = image::imageops::crop_imm(&frame, 128, 144, 1024, 432).to_image();
    let mut s = session();
    let (frames, results) = (1..=14)
        .find_map(|i| {
            s.process_frame(roi.as_raw(), 1024, 432, PixelFormat::Luma)
                .map(|r| (i, r))
        })
        .expect("ear tag in camera ROI");
    assert_eq!(results[0].text, "02502239036645");
    // Once a fallback strategy reads the code, the session keeps it for the
    // next frame, so the 2 reads a 1D code needs come back to back.
    println!("ear tag confirmed after {frames} frames");
    assert!(
        frames <= 7,
        "confirmed after {frames} frames (was 12 before sticky rotation)"
    );
}

/// Real phone photo: Code 39 printed on an animal-feed label, partly in the
/// shadow of the phone (grey, uneven background). Content "025925", the end
/// of the lot number printed above it; the same value as zxing-cpp reads.
/// On the whole photo only the Linear strategy reads it.
#[test]
fn decodes_feed_label_code39() {
    let results = scan("real/feed_label_code39.jpg", ScannerConfig::image_file())
        .expect("feed label Code 39");
    assert_eq!(results[0], ("025925".to_string(), BarcodeFormat::Code39));
}

/// The highlight of that Code 39 is the box of the symbol: bars over x
/// 202 → 509 and y 470 → 558 (measured on the pixels; zxing-cpp's box is
/// shorter, it spans the rows it decoded). rxing's end point landed at 256 px,
/// so the highlight covered ~15 % of the code, as a thin bar at the scan row.
#[test]
fn feed_label_code39_points_span_the_symbol() {
    let path = fixture("real/feed_label_code39.jpg");
    let results = Scanner::new(ScannerConfig::image_file())
        .scan_file(path.to_str().expect("utf-8 path"))
        .expect("feed label Code 39");
    let pts = &results[0].points;
    let range = |v: Vec<f32>| {
        v.iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)))
    };
    let (x0, x1) = range(pts.iter().map(|p| p.x).collect());
    let (y0, y1) = range(pts.iter().map(|p| p.y).collect());
    assert_eq!(pts.len(), 4, "{pts:?}");
    assert!(
        (x0 - 202.0).abs() <= 6.0 && (x1 - 509.0).abs() <= 6.0,
        "x {x0}..{x1}, symbol 202..509"
    );
    assert!(
        (y0 - 470.0).abs() <= 4.0 && (y1 - 558.0).abs() <= 4.0,
        "y {y0}..{y1}, symbol 470..558"
    );
}

/// The same label as the SDK's camera sees it on a phone held in portrait
/// (576×576 region of interest) with the code across half of it (~2.6 px
/// per module). Code 39 is a weak format: 3 agreeing reads are required.
#[test]
fn session_reads_feed_label_code39_in_camera_roi() {
    let photo = luma("real/feed_label_code39.jpg");
    // Code bounding box in the fixture: x 201..516, y 480..555.
    let (cx, cy, side) = (358u32, 518u32, 630u32);
    let crop =
        image::imageops::crop_imm(&photo, cx - side / 2, cy - side / 2, side, side).to_image();
    let roi = image::imageops::resize(&crop, 576, 576, image::imageops::FilterType::Triangle);
    let mut s = session();
    let (frames, results) = (1..=12)
        .find_map(|i| {
            s.process_frame(roi.as_raw(), 576, 576, PixelFormat::Luma)
                .map(|r| (i, r))
        })
        .expect("feed label in camera ROI");
    assert_eq!(results[0].text, "025925");
    assert!(frames <= 8, "confirmed after {frames} frames");
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
        let data = vec![200u8; format.buffer_size(w, h).expect("small frame")];
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

// ── Linear strategy: 1D rows only (performance contract) ─────────────────────

/// `decode_rows` (used by the `Linear` strategy) runs the 1D readers on the
/// rows of the image and nothing else: no 2D detectors and no 90° retry,
/// both measured as wasted time on a row-wise binarised image. On clean
/// images that `Hard` reads, it must therefore miss the QR code and the
/// rotated barcode.
#[test]
fn decode_rows_skips_2d_and_rotation() {
    use scanner_core::decode::{Effort, RxingDecoder};
    let decoder = RxingDecoder::new(&ScannerConfig::default());

    let code = luma("barcode_1d/code128.png");
    let results = decoder.decode_rows(&code).expect("horizontal Code 128");
    assert_eq!(results[0].text, "SCANNER-TEST-128");

    for (name, img) in [
        ("rotated Code 128", image::imageops::rotate90(&code)),
        ("QR code", luma("qr/clean_hd.png")),
    ] {
        assert!(
            decoder.decode_with(&img, Effort::Hard).is_ok(),
            "{name}: Hard"
        );
        assert!(
            matches!(decoder.decode_rows(&img), Err(ScanError::NotFound)),
            "{name}: decode_rows must not read it"
        );
    }
}

/// The `Linear` strategy decodes a subset of rows: its points must still be
/// in the coordinates of the input image (not of the sampled rows).
#[test]
fn linear_strategy_points_are_in_image_coordinates() {
    use scanner_core::{LinearScale, Strategy};
    let scanner = Scanner::new(ScannerConfig {
        roi: None,
        ..ScannerConfig::default()
    });
    let code = luma("barcode_1d/code128.png");
    let top = 400;
    let mut img =
        image::GrayImage::from_pixel(code.width(), code.height() + top, image::Luma([255]));
    image::imageops::overlay(&mut img, &code, 0, i64::from(top));

    let results = scanner
        .run_strategy(&img, Strategy::Linear(LinearScale::Fine))
        .expect("Code 128 via Linear");
    assert_eq!(results[0].text, "SCANNER-TEST-128");
    let code_rows = top as f32..img.height() as f32;
    for p in &results[0].points {
        assert!(
            code_rows.contains(&p.y),
            "point {p:?} not on the code rows {code_rows:?}"
        );
    }
}
