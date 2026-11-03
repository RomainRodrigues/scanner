//! Real-time detection of every 1D format, and absence of false positives on
//! barcode-like textures. Codes are synthesised with rxing's encoders, then
//! blurred and noised like a camera frame.

use image::{GrayImage, Luma};
use rxing::{BarcodeFormat as RxFormat, MultiFormatWriter, Writer};
use scanner_core::{BarcodeFormat, PixelFormat, ScanSession, ScannerConfig};

fn lcg(seed: &mut u64) -> u32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 33) as u32
}

/// A 640×480 frame with `content` encoded as `format`, `px` pixels per module,
/// a 10-module quiet zone, a light grey background.
fn frame(format: RxFormat, content: &str, px: u32) -> GrayImage {
    let m = MultiFormatWriter
        .encode(content, &format, 0, 0)
        .expect("encode");
    let (mw, mh) = (m.getWidth(), m.getHeight().max(1));
    let (cw, ch) = (mw * px, 160);
    let mut img = GrayImage::from_pixel(640, 480, Luma([190]));
    let (ox, oy) = ((640 - cw) / 2, (480 - ch) / 2);
    // White label with quiet zones around the bars.
    for y in oy - 20..oy + ch + 20 {
        for x in ox.saturating_sub(10 * px)..(ox + cw + 10 * px).min(640) {
            img.put_pixel(x, y, Luma([235]));
        }
    }
    for y in 0..ch {
        for x in 0..cw {
            if m.get(x / px, (y * mh / ch).min(mh - 1)) {
                img.put_pixel(ox + x, oy + y, Luma([30]));
            }
        }
    }
    imageproc::filter::gaussian_blur_f32(&img, 0.8)
}

fn noisy(img: &GrayImage, seed: &mut u64) -> GrayImage {
    let mut o = img.clone();
    for p in o.pixels_mut() {
        let n = (lcg(seed) % 13) as i32 - 6;
        p.0[0] = (p.0[0] as i32 + n).clamp(0, 255) as u8;
    }
    o
}

fn session() -> ScanSession {
    ScanSession::new(ScannerConfig {
        adaptive_throttle: false,
        ..ScannerConfig::default()
    })
}

#[test]
fn every_linear_format_is_detected_within_a_few_frames() {
    let cases = [
        (RxFormat::UPC_E, "01234565", BarcodeFormat::UpcE),
        (RxFormat::EAN_8, "96385074", BarcodeFormat::Ean8),
        (RxFormat::EAN_13, "5901234123457", BarcodeFormat::Ean13),
        (RxFormat::UPC_A, "036000291452", BarcodeFormat::UpcA),
        (RxFormat::CODE_128, "SCANNER-128", BarcodeFormat::Code128),
        (RxFormat::CODE_39, "CODE39", BarcodeFormat::Code39),
        (RxFormat::CODE_93, "CODE93", BarcodeFormat::Code93),
        (RxFormat::ITF, "12345678", BarcodeFormat::Itf),
        (RxFormat::CODABAR, "A123456B", BarcodeFormat::Codabar),
    ];
    let mut seed = 1;
    let mut failures = vec![];
    for (rx, content, expected) in cases {
        for px in [2u32, 3] {
            let img = frame(rx, content, px);
            if img.width() == 0 {
                continue;
            }
            let mut s = session();
            let found = (1..=12).find_map(|i| {
                s.process_frame(noisy(&img, &mut seed).as_raw(), 640, 480, PixelFormat::Luma)
                    .map(|r| (i, r))
            });
            match found {
                Some((_, r)) if r[0].format == expected => {}
                other => failures.push(format!("{expected:?} @ {px}px/module: {other:?}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "missed codes:\n{}",
        failures.join("\n")
    );
}

/// Textures without any barcode (blurred noise, stripes, wavy noise, random
/// bars). Before the quiet-zone check, one of them produced a stable UPC-E
/// false positive on every frame, which multi-frame confirmation cannot stop.
fn textures() -> Vec<GrayImage> {
    let (w, h) = (640u32, 480u32);
    let mut seed = 42u64;
    (0..40)
        .map(|i| {
            let kind = i % 4;
            let f = 2 + (lcg(&mut seed) % 12);
            let mut img = GrayImage::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let v = match kind {
                        0 => (lcg(&mut seed) % 256) as u8,
                        1 => {
                            (128.0 + 60.0 * ((x / f) as f32).sin() * ((y / (f + 3)) as f32).cos())
                                as u8
                        }
                        2 => {
                            ((x as f32 * 0.05 + (y / 7) as f32).sin() * 40.0
                                + 120.0
                                + (lcg(&mut seed) % 30) as f32) as u8
                        }
                        _ => {
                            if ((x / f) + (lcg(&mut seed) % 2)).is_multiple_of(2) {
                                40
                            } else {
                                200
                            }
                        }
                    };
                    img.put_pixel(x, y, Luma([v]));
                }
            }
            if kind == 0 {
                imageproc::filter::gaussian_blur_f32(&img, 1.5)
            } else {
                img
            }
        })
        .collect()
}

#[test]
fn no_false_positives_on_barcode_like_textures() {
    let mut seed = 7;
    let mut reports = vec![];
    for (t, tex) in textures().iter().enumerate() {
        let mut s = session();
        for _ in 0..15 {
            if let Some(r) =
                s.process_frame(noisy(tex, &mut seed).as_raw(), 640, 480, PixelFormat::Luma)
            {
                reports.extend(
                    r.into_iter()
                        .map(|r| format!("texture {t}: {} {:?}", r.text, r.format)),
                );
            }
        }
    }
    assert!(
        reports.is_empty(),
        "false positives:\n{}",
        reports.join("\n")
    );
}
