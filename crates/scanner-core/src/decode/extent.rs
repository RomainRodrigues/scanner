//! Line ends of 1D codes whose reader reports wrong points.
//!
//! The overlay draws 1D codes as a bar between the two result points, which
//! must therefore be the ends of the symbol. rxing's Code 39 reader computes
//! its end point as `(lastStart + lastPatternSize) / 2` instead of ZXing's
//! `lastStart + lastPatternSize / 2` (rxing 0.9.3): the end lands about half
//! way, and the highlight covered ~15 % of the code (feed-label fixture:
//! points 220 → 256 px for a code spanning 201 → 516 px). Its Codabar end
//! point is set to the end of the row. Correcting the formula would also have
//! to undo rxing's later transforms (reversed rows, 90° retries, crops), so
//! the ends are measured in the image instead.
//!
//! The start point is right (centre of the start pattern), and both points
//! sit on the scan line: rxing's 1D readers only scan rows or, rotated,
//! columns. From the start point the scan line is walked both ways through
//! the bars until a light run longer than `QUIET_FACTOR` × the median bar
//! width: that is the quiet zone. Inter-character gaps (Code 39) and wide
//! spaces are at most 3 modules, a quiet zone at least 10.

use crate::preprocess::linear::dark_profile;
use crate::result::{BarcodeFormat, Point, ScanResult};
use image::GrayImage;

/// A light run longer than this × the median bar width ends the symbol.
const QUIET_FACTOR: usize = 5;

/// `true` for the formats whose rxing reader reports unusable points.
pub(crate) fn has_unreliable_points(format: BarcodeFormat) -> bool {
    matches!(format, BarcodeFormat::Code39 | BarcodeFormat::Codabar)
}

/// Replace the points of `r` by the ends of the symbol, measured on `img`
/// (the image the code was decoded from). `inverted`: light bars on a dark
/// background. Leaves `r` unchanged when the ends cannot be measured.
pub(crate) fn refine_line_ends(img: &GrayImage, r: &mut ScanResult, inverted: bool) {
    if !has_unreliable_points(r.format) {
        return;
    }
    let (Some(&p0), Some(&p1)) = (r.points.first(), r.points.last()) else {
        return;
    };
    let (w, h) = img.dimensions();
    let horizontal = (p0.y - p1.y).abs() < 1.0;
    let vertical = (p0.x - p1.x).abs() < 1.0;
    // Scan line: the row (or column) through the start point.
    let (profile, anchor): (Vec<u8>, f32) = if horizontal {
        let y = (p0.y.round().max(0.0) as u32).min(h.saturating_sub(1));
        ((0..w).map(|x| img.get_pixel(x, y)[0]).collect(), p0.x)
    } else if vertical {
        let x = (p0.x.round().max(0.0) as u32).min(w.saturating_sub(1));
        ((0..h).map(|y| img.get_pixel(x, y)[0]).collect(), p0.y)
    } else {
        return;
    };
    let Some((lo, hi)) = symbol_extent(&profile, anchor, inverted) else {
        return;
    };
    // Keep the start pattern side first.
    let (a, b) = if (anchor - lo).abs() <= (anchor - hi).abs() {
        (lo, hi)
    } else {
        (hi, lo)
    };
    r.points = if horizontal {
        vec![Point::new(a, p0.y), Point::new(b, p0.y)]
    } else {
        vec![Point::new(p0.x, a), Point::new(p0.x, b)]
    };
}

/// First and last bar pixel of the symbol containing `anchor`.
fn symbol_extent(profile: &[u8], anchor: f32, inverted: bool) -> Option<(f32, f32)> {
    let n = profile.len();
    if n < 8 {
        return None;
    }
    // Invert the pixels, not the classification: flat areas must stay
    // background (light) whatever the polarity of the code.
    let radius = (n / 64).clamp(8, 96);
    let dark = if inverted {
        let negative: Vec<u8> = profile.iter().map(|&v| 255 - v).collect();
        dark_profile(&negative, radius)
    } else {
        dark_profile(profile, radius)
    };
    // The start point is the centre of the start pattern: start from the
    // nearest bar pixel.
    let a = (anchor.round().max(0.0) as usize).min(n - 1);
    let start = (0..=8)
        .flat_map(|d| [a.checked_sub(d), Some(a + d)])
        .flatten()
        .find(|&i| i < n && dark[i])?;

    let mut bars = Vec::new();
    let lo = walk(&dark, start, false, &mut bars);
    let hi = walk(&dark, start, true, &mut bars);
    (hi > lo).then_some((lo as f32, hi as f32 + 1.0))
}

/// Walk from `start` (a bar pixel) until a light run longer than
/// `QUIET_FACTOR` × the median bar width; returns the last bar pixel.
/// `bars` collects the bar widths seen (shared by both directions).
fn walk(dark: &[bool], start: usize, forward: bool, bars: &mut Vec<usize>) -> usize {
    let step = |i: usize| -> Option<usize> {
        if forward {
            (i + 1 < dark.len()).then_some(i + 1)
        } else {
            i.checked_sub(1)
        }
    };
    let (mut last_bar, mut bar_run, mut gap) = (start, 0usize, 0usize);
    let mut i = Some(start);
    while let Some(p) = i {
        if dark[p] {
            last_bar = p;
            bar_run += 1;
            gap = 0;
        } else {
            if bar_run > 0 {
                bars.push(bar_run);
                bar_run = 0;
            }
            gap += 1;
            if gap > QUIET_FACTOR * median(bars).max(2) {
                break;
            }
        }
        i = step(p);
    }
    last_bar
}

fn median(values: &[usize]) -> usize {
    let mut v = values.to_vec();
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bars of `widths` (alternating bar / space, starting with a bar) from `x0`.
    fn row(len: usize, x0: usize, widths: &[usize]) -> Vec<u8> {
        let mut p = vec![230u8; len];
        let mut x = x0;
        for (i, &wd) in widths.iter().enumerate() {
            if i % 2 == 0 {
                p[x..x + wd].fill(30);
            }
            x += wd;
        }
        p
    }

    #[test]
    fn finds_both_ends_from_the_start_pattern() {
        // Narrow 3 px, wide 7 px, inter-character gaps of 3 px.
        let widths = [3, 7, 3, 3, 7, 3, 7, 3, 3, 3, 3, 3, 7, 3, 7, 3, 3, 3, 3];
        // An odd number of elements: the symbol starts and ends with a bar.
        let p = row(400, 100, &widths);
        let end = 100 + widths.iter().sum::<usize>();
        let (lo, hi) = symbol_extent(&p, 101.5, false).expect("extent");
        assert_eq!(lo, 100.0);
        assert_eq!(hi, end as f32);
    }

    #[test]
    fn stops_at_the_quiet_zone_not_at_nearby_text() {
        // A blob 40 px after the code (12 narrow modules) is not part of it.
        let mut p = row(400, 100, &[3, 3, 3, 3, 3]);
        p[155..170].fill(30);
        let (_, hi) = symbol_extent(&p, 101.0, false).expect("extent");
        assert_eq!(hi, 115.0);
    }

    #[test]
    fn handles_light_bars_on_dark() {
        let p: Vec<u8> = row(300, 50, &[3, 3, 7, 3, 3])
            .iter()
            .map(|v| 255 - v)
            .collect();
        assert_eq!(symbol_extent(&p, 51.0, true), Some((50.0, 69.0)));
    }

    #[test]
    fn leaves_reliable_formats_alone() {
        let img = GrayImage::new(10, 10);
        let mut r = ScanResult {
            text: String::new(),
            format: BarcodeFormat::Code128,
            raw_bytes: None,
            points: vec![Point::new(1.0, 5.0), Point::new(2.0, 5.0)],
            timestamp_ms: 0,
        };
        let before = r.points.clone();
        refine_line_ends(&img, &mut r, false);
        assert_eq!(r.points, before);
    }
}
