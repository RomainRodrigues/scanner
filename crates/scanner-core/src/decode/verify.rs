//! Post-decode plausibility checks for 1D barcodes.
//!
//! 1D readers accept any run of bars and spaces whose widths fit a symbology.
//! Textures (fabric, keyboards, printed text, wood…) regularly contain such
//! runs, especially for short symbologies like UPC-E (6 digits) or EAN-8.
//! A real barcode is surrounded by a **quiet zone**: a blank margin at least
//! 7–10 modules wide (ISO/IEC 15420, 15417…). rxing only checks a few pixels
//! of it, so we verify it here on the grayscale image.

use crate::result::{BarcodeFormat, ScanResult};
use image::GrayImage;

/// Quiet zone width required for weak formats, in modules (the specs ask for
/// 7–10; we stay tolerant to blur and tight framing).
const QUIET_MODULES: f32 = 5.0;
/// Quiet zone width required for checksummed formats (Code 128, EAN-13, UPC-A).
/// Real labels and tags are often printed with much less than the spec asks:
/// the cattle ear-tag fixture has the tag edge ~3 modules from the code. The
/// checksum already makes texture false positives rare for these formats.
const STRONG_QUIET_MODULES: f32 = 2.0;
/// A quiet zone may vary by at most this fraction of the barcode's own contrast.
const QUIET_MAX_RELATIVE_CONTRAST: f32 = 0.4;
/// Minimum quiet zone that must be visible inside the image, in modules.
const MIN_VISIBLE_MODULES: f32 = 3.0;

/// Symbol geometry, in modules: total width, and how far inside the symbol
/// rxing places the first and last result points (they sit in the middle of
/// the start / stop patterns, which differ per symbology).
struct Geometry {
    modules: f32,
    inner_start: f32,
    inner_end: f32,
}

fn geometry(format: BarcodeFormat, text_len: usize) -> Option<Geometry> {
    let n = text_len as f32;
    let g = |modules, inner_start, inner_end| Geometry {
        modules,
        inner_start,
        inner_end,
    };
    Some(match format {
        BarcodeFormat::UpcE => g(51.0, 1.5, 3.0),
        BarcodeFormat::Ean8 => g(67.0, 1.5, 1.5),
        BarcodeFormat::Ean13 | BarcodeFormat::UpcA => g(95.0, 1.5, 1.5),
        BarcodeFormat::Code128 => g(11.0 * n + 35.0, 5.5, 6.5),
        BarcodeFormat::Code93 => g(9.0 * (n + 4.0) + 1.0, 4.5, 5.5),
        BarcodeFormat::Itf => g(9.0 * n + 9.0, 4.0, 5.0),
        // Code 39 and Codabar: rxing's result points do not mark the symbol
        // ends (the Code 39 end point is miscomputed, the Codabar one is the
        // row end; see `extent.rs`, which measures the real ends afterwards
        // for the overlay), so they cannot be checked here. Their readers
        // already verify a quiet zone around the start/stop characters,
        // unlike the UPC/EAN readers.
        _ => return None,
    })
}

/// `false` when a decoded 1D code does not look like a real printed barcode.
///
/// - Weak formats (see [`BarcodeFormat::is_weak`]) must show a quiet zone on
///   both sides, visible inside the image.
/// - Strong formats are only rejected when a quiet zone is visible *and*
///   clearly not blank (they are often framed tightly by the user).
pub fn plausible_linear(img: &GrayImage, r: &ScanResult) -> bool {
    if !r.format.is_linear() || r.points.len() < 2 {
        return true;
    }
    let Some(geo) = geometry(r.format, r.text.chars().count()) else {
        return true;
    };
    let strict = r.format.is_weak();

    // Scan line: from the first to the last point (any orientation: rxing's
    // try-harder mode also reads codes rotated by 90°).
    let (p0, p1) = (r.points[0], r.points[r.points.len() - 1]);
    let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
    let span = (dx * dx + dy * dy).sqrt();
    let module = span / (geo.modules - geo.inner_start - geo.inner_end).max(1.0);
    if span < 12.0 || module < 0.5 {
        return !strict;
    }
    let (ux, uy) = (dx / span, dy / span); // along the line
    let (nx, ny) = (-uy, ux); // across the bars

    let (w, h) = img.dimensions();
    let sample = |t: f32, o: f32| -> Option<u8> {
        let x = p0.x + ux * t + nx * o;
        let y = p0.y + uy * t + ny * o;
        if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
            return None;
        }
        Some(img.get_pixel(x.round() as u32, y.round() as u32)[0])
    };
    // Pixels of segment [t0, t1] along the line at offset `o`; `None` values are off-image.
    let segment = |t0: f32, t1: f32, o: f32| -> Vec<Option<u8>> {
        let steps = (t1 - t0).abs().ceil().max(1.0) as usize;
        (0..=steps)
            .map(|i| sample(t0 + (t1 - t0) * i as f32 / steps as f32, o))
            .collect()
    };

    // Skip the rest of the start/stop pattern plus a 2-module safety margin.
    let skip_start = (geo.inner_start + 2.0) * module;
    let skip_end = (geo.inner_end + 2.0) * module;
    let quiet = if strict {
        QUIET_MODULES
    } else {
        STRONG_QUIET_MODULES
    } * module;
    let offset = (module * 2.0).max(2.0);
    let mut votes_ok = 0;
    let mut votes_total = 0;

    for o in [-offset, 0.0, offset] {
        let code: Vec<u8> = segment(0.0, span, o).into_iter().flatten().collect();
        if code.len() < 8 {
            continue;
        }
        let contrast = f32::from(percentile_range(&code));
        if contrast < 16.0 {
            continue; // unreadable line, no opinion
        }

        let mut line_ok = true;
        for (a, b) in [
            (-skip_start - quiet, -skip_start),
            (span + skip_end, span + skip_end + quiet),
        ] {
            let zone: Vec<u8> = segment(a, b, o).into_iter().flatten().collect();
            if (zone.len() as f32) < MIN_VISIBLE_MODULES * module {
                // Quiet zone cut by the image border: cannot verify.
                if strict {
                    line_ok = false;
                }
                continue;
            }
            let (min, max) = zone
                .iter()
                .fold((u8::MAX, u8::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            let range = max - min;
            if f32::from(range) > QUIET_MAX_RELATIVE_CONTRAST * contrast {
                line_ok = false;
            }
        }
        votes_total += 1;
        if line_ok {
            votes_ok += 1;
        }
    }

    if votes_total == 0 {
        return !strict;
    }
    votes_ok * 2 > votes_total
}

/// Robust contrast: difference between the 90th and 10th percentile.
fn percentile_range(values: &[u8]) -> u8 {
    let mut hist = [0usize; 256];
    for &v in values {
        hist[v as usize] += 1;
    }
    let n = values.len();
    let pick = |q: f32| {
        let target = ((n as f32) * q) as usize;
        let mut acc = 0;
        for (i, &c) in hist.iter().enumerate() {
            acc += c;
            if acc > target {
                return i as u8;
            }
        }
        255
    };
    pick(0.9).saturating_sub(pick(0.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::Point;
    use image::Luma;

    fn result(format: BarcodeFormat, text: &str, x0: f32, x1: f32, y: f32) -> ScanResult {
        ScanResult {
            text: text.into(),
            format,
            raw_bytes: None,
            points: vec![Point::new(x0, y), Point::new(x1, y)],
            timestamp_ms: 0,
        }
    }

    /// Bars between 200 and 302 (51 modules × 2 px), on a white or striped background.
    fn image(background_stripes: bool) -> GrayImage {
        GrayImage::from_fn(500, 60, |x, _| {
            let in_code = (200..302).contains(&x);
            let dark = if in_code || background_stripes {
                (x / 2) % 2 == 0
            } else {
                false
            };
            Luma([if dark { 20 } else { 230 }])
        })
    }

    #[test]
    fn accepts_code_with_quiet_zone() {
        let r = result(BarcodeFormat::UpcE, "01234565", 203.0, 299.0, 30.0);
        assert!(plausible_linear(&image(false), &r));
    }

    #[test]
    fn rejects_weak_code_inside_texture() {
        let r = result(BarcodeFormat::UpcE, "01234565", 203.0, 299.0, 30.0);
        assert!(!plausible_linear(&image(true), &r));
    }

    #[test]
    fn weak_code_against_border_is_rejected_strong_is_kept() {
        let img = image(false);
        let weak = result(BarcodeFormat::UpcE, "01234565", 1.0, 97.0, 30.0);
        assert!(!plausible_linear(&img, &weak));
        let strong = result(BarcodeFormat::Ean13, "5901234123457", 1.0, 97.0, 30.0);
        assert!(plausible_linear(&img, &strong));
    }

    #[test]
    fn vertical_code_is_checked_along_its_line() {
        // Same symbol rotated by 90°: bars are horizontal, line is vertical.
        let img = GrayImage::from_fn(60, 500, |_, y| {
            let dark = (200..302).contains(&y) && (y / 2) % 2 == 0;
            Luma([if dark { 20 } else { 230 }])
        });
        let r = ScanResult {
            points: vec![Point::new(30.0, 203.0), Point::new(30.0, 299.0)],
            ..result(BarcodeFormat::UpcE, "01234565", 0.0, 0.0, 0.0)
        };
        assert!(plausible_linear(&img, &r));
    }

    #[test]
    fn two_d_codes_are_not_checked() {
        let r = result(BarcodeFormat::QrCode, "x", 203.0, 299.0, 30.0);
        assert!(plausible_linear(&image(true), &r));
    }
}
