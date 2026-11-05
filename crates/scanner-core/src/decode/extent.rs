//! Box of a decoded 1D code, for the overlay.
//!
//! rxing reports a 1D code as two points on the **scan line** that decoded
//! it. Drawn as is, the highlight is a thin bar whose height jumps from frame
//! to frame (each frame may decode another row), and for two formats it does
//! not even span the code:
//! - Code 39: the end point is computed as `(lastStart + lastPatternSize) / 2`
//!   instead of ZXing's `lastStart + lastPatternSize / 2` (rxing 0.9.3), so
//!   it lands about half way (feed-label fixture: 220 → 256 px for a symbol
//!   spanning 201 → 516 px);
//! - Codabar: the end point is the end of the row.
//!
//! This module replaces the two points by the four corners of the symbol:
//! 1. **Ends** along the scan line (rxing's 1D readers only scan rows or,
//!    rotated, columns): from the symbology geometry when the points are
//!    reliable (they sit at a known number of modules inside the symbol),
//!    else measured on the image from the start point, which is correct,
//!    through the bars until the quiet zone. Correcting rxing's formulas
//!    instead would have to undo its later transforms (reversed rows, 90°
//!    retries, crops).
//! 2. **Height** across the scan line: rows (or columns) are added on both
//!    sides while their bar pattern matches the previous one, which also
//!    follows slightly tilted codes.

use super::verify::geometry;
use crate::preprocess::linear::dark_profile;
use crate::result::{BarcodeFormat, Point, ScanResult};
use image::GrayImage;

/// A light run longer than this × the median bar width ends the symbol.
/// Spaces inside a symbol are at most 4 modules (Code 128, EAN), Code 39
/// inter-character gaps at most 3; a quiet zone is at least 10.
const QUIET_FACTOR: usize = 5;
/// A row belongs to the symbol when this fraction of its pixels have the
/// same dark / light value as the previous row (text or background: ~50 %).
const SAME_PATTERN: f32 = 0.8;
/// Rows that may fail the comparison in a row (scratches, specks) before the
/// symbol is considered to end.
const MAX_MISSES: usize = 2;

/// `true` for the formats whose rxing reader reports unusable end points.
fn has_unreliable_points(format: BarcodeFormat) -> bool {
    matches!(format, BarcodeFormat::Code39 | BarcodeFormat::Codabar)
}

/// Replace the points of a 1D result by the corners of the symbol, measured
/// on `img` (the image it was decoded from): start-top, end-top, end-bottom,
/// start-bottom. `inverted`: light bars on a dark background. Leaves `r`
/// unchanged when the box cannot be measured.
pub(crate) fn refine_linear_box(img: &GrayImage, r: &mut ScanResult, inverted: bool) {
    if !r.format.is_linear() || r.points.len() != 2 {
        return;
    }
    let (p0, p1) = (r.points[0], r.points[1]);
    let horizontal = (p0.y - p1.y).abs() < 1.0;
    if !horizontal && (p0.x - p1.x).abs() >= 1.0 {
        return; // Not a row nor a column: not a scan line.
    }
    let line = Line::new(
        img,
        horizontal,
        if horizontal { p0.y } else { p0.x },
        inverted,
    );
    let (a0, a1) = if horizontal {
        (p0.x, p1.x)
    } else {
        (p0.y, p1.y)
    };

    let Some((start, end)) = symbol_ends(&line, r, a0, a1) else {
        return;
    };
    let (lo, hi) = (start.min(end), start.max(end));
    let (before, after) = line.thickness(lo as usize, hi as usize);
    let (c0, c1) = (
        line.pos as f32 - before as f32,
        line.pos as f32 + after as f32 + 1.0,
    );
    let corner = |along: f32, across: f32| {
        if horizontal {
            Point::new(along, across)
        } else {
            Point::new(across, along)
        }
    };
    r.points = vec![
        corner(start, c0),
        corner(end, c0),
        corner(end, c1),
        corner(start, c1),
    ];
}

/// Ends of the symbol along the scan line, start pattern side first.
fn symbol_ends(line: &Line<'_>, r: &ScanResult, a0: f32, a1: f32) -> Option<(f32, f32)> {
    if !has_unreliable_points(r.format) {
        // Points in the middle of the start / stop patterns, at a known
        // number of modules inside the symbol.
        let geo = geometry(r.format, r.text.chars().count())?;
        let module = (a1 - a0).abs() / (geo.modules - geo.inner_start - geo.inner_end).max(1.0);
        let dir = (a1 - a0).signum();
        return Some((
            a0 - dir * geo.inner_start * module,
            a1 + dir * geo.inner_end * module,
        ));
    }
    let dark = line.dark_at(line.pos)?;
    let (lo, hi) = symbol_extent(&dark, a0)?;
    // Keep the start pattern side first.
    Some(if (a0 - lo).abs() <= (a0 - hi).abs() {
        (lo, hi)
    } else {
        (hi, lo)
    })
}

/// First and last bar pixel (end exclusive) of the symbol containing `anchor`.
fn symbol_extent(dark: &[bool], anchor: f32) -> Option<(f32, f32)> {
    let n = dark.len();
    if n < 8 {
        return None;
    }
    // The start point is the centre of the start pattern: start from the
    // nearest bar pixel.
    let a = (anchor.round().max(0.0) as usize).min(n - 1);
    let start = (0..=8)
        .flat_map(|d| [a.checked_sub(d), Some(a + d)])
        .flatten()
        .find(|&i| i < n && dark[i])?;
    let mut bars = Vec::new();
    let lo = walk(dark, start, false, &mut bars);
    let hi = walk(dark, start, true, &mut bars);
    (hi > lo).then_some((lo as f32, hi as f32 + 1.0))
}

/// Walk from `start` (a bar pixel) until a light run longer than
/// `QUIET_FACTOR` × the median bar width; returns the last bar pixel.
/// `bars` collects the bar widths seen (shared by both directions).
fn walk(dark: &[bool], start: usize, forward: bool, bars: &mut Vec<usize>) -> usize {
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
        i = if forward {
            (p + 1 < dark.len()).then_some(p + 1)
        } else {
            p.checked_sub(1)
        };
    }
    last_bar
}

fn median(values: &[usize]) -> usize {
    let mut v = values.to_vec();
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(1)
}

/// The image seen as lines parallel to the scan line: rows for a horizontal
/// code, columns for a vertical one.
struct Line<'a> {
    img: &'a GrayImage,
    horizontal: bool,
    inverted: bool,
    /// Index of the scan line.
    pos: usize,
    /// Number of lines, and pixels per line.
    count: usize,
    len: usize,
}

impl<'a> Line<'a> {
    fn new(img: &'a GrayImage, horizontal: bool, pos: f32, inverted: bool) -> Self {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let (count, len) = if horizontal { (h, w) } else { (w, h) };
        let pos = (pos.round().max(0.0) as usize).min(count.saturating_sub(1));
        Self {
            img,
            horizontal,
            inverted,
            pos,
            count,
            len,
        }
    }

    /// Thresholding window, as for the Linear strategy.
    fn radius(&self) -> usize {
        (self.len / 64).clamp(8, 96)
    }

    /// Dark / light pixels of line `index`, over the whole line.
    fn dark_at(&self, index: usize) -> Option<Vec<bool>> {
        self.dark_span(index, 0, self.len)
    }

    /// Dark / light pixels of line `index` over `[from, to)`, thresholded with
    /// some context on both sides.
    fn dark_span(&self, index: usize, from: usize, to: usize) -> Option<Vec<bool>> {
        if index >= self.count || from >= to || to > self.len {
            return None;
        }
        let r = self.radius();
        let (a, b) = (from.saturating_sub(r), (to + r).min(self.len));
        let pixel = |i: usize| {
            let (x, y) = if self.horizontal {
                (i, index)
            } else {
                (index, i)
            };
            let v = self.img.get_pixel(x as u32, y as u32)[0];
            // Invert the pixels, not the classification: flat areas must stay
            // background (light) whatever the polarity of the code.
            if self.inverted {
                255 - v
            } else {
                v
            }
        };
        let profile: Vec<u8> = (a..b).map(pixel).collect();
        let dark = dark_profile(&profile, r);
        Some(dark[from - a..to - a].to_vec())
    }

    /// Lines on each side of the scan line whose bars over `[lo, hi)` continue
    /// the pattern of the previous line.
    fn thickness(&self, lo: usize, hi: usize) -> (usize, usize) {
        let hi = hi.min(self.len);
        let Some(reference) = self.dark_span(self.pos, lo, hi) else {
            return (0, 0);
        };
        let max = (hi - lo).max(16) * 2;
        let extend = |step: isize| {
            let (mut prev, mut extent, mut misses) = (reference.clone(), 0usize, 0usize);
            for d in 1..=max {
                let index = self.pos as isize + step * d as isize;
                let Ok(index) = usize::try_from(index) else {
                    break;
                };
                let Some(row) = self.dark_span(index, lo, hi) else {
                    break;
                };
                let same = prev.iter().zip(&row).filter(|(a, b)| a == b).count();
                if same as f32 >= SAME_PATTERN * row.len() as f32 {
                    extent = d;
                    prev = row;
                    misses = 0;
                } else {
                    misses += 1;
                    if misses > MAX_MISSES {
                        break;
                    }
                }
            }
            extent
        };
        (extend(-1), extend(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    /// Bars of `widths` (alternating bar / space, starting with a bar) from `x0`.
    fn row(len: usize, x0: usize, widths: &[usize]) -> Vec<bool> {
        let mut p = vec![false; len];
        let mut x = x0;
        for (i, &wd) in widths.iter().enumerate() {
            if i % 2 == 0 {
                p[x..x + wd].fill(true);
            }
            x += wd;
        }
        p
    }

    #[test]
    fn finds_both_ends_from_the_start_pattern() {
        // Narrow 3 px, wide 7 px, inter-character gaps of 3 px; an odd number
        // of elements: the symbol starts and ends with a bar.
        let widths = [3, 7, 3, 3, 7, 3, 7, 3, 3, 3, 3, 3, 7, 3, 7, 3, 3, 3, 3];
        let p = row(400, 100, &widths);
        let end = 100 + widths.iter().sum::<usize>();
        assert_eq!(symbol_extent(&p, 101.5), Some((100.0, end as f32)));
    }

    #[test]
    fn stops_at_the_quiet_zone_not_at_nearby_text() {
        // A blob 40 px after the code (12 narrow modules) is not part of it.
        let mut p = row(400, 100, &[3, 3, 3, 3, 3]);
        p[155..170].fill(true);
        assert_eq!(symbol_extent(&p, 101.0), Some((100.0, 115.0)));
    }

    /// A 300×200 image with vertical bars over rows 60..140 and some text-like
    /// blobs above and below.
    fn labelled_code(inverted: bool) -> GrayImage {
        let (bg, ink) = if inverted { (25, 225) } else { (230, 30) };
        GrayImage::from_fn(300, 200, |x, y| {
            let bar = (60..140).contains(&y) && (100..190).contains(&x) && (x / 3) % 2 == 0;
            let text = (40..52).contains(&y) && (x / 7) % 3 == 0;
            Luma([if bar || text { ink } else { bg }])
        })
    }

    fn code39(points: Vec<Point>) -> ScanResult {
        ScanResult {
            text: "X".into(),
            format: BarcodeFormat::Code39,
            raw_bytes: None,
            points,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn box_covers_the_bars_height_and_stops_at_text() {
        for inverted in [false, true] {
            let mut r = code39(vec![Point::new(101.0, 100.0), Point::new(130.0, 100.0)]);
            refine_linear_box(&labelled_code(inverted), &mut r, inverted);
            let ys: Vec<f32> = r.points.iter().map(|p| p.y).collect();
            let xs: Vec<f32> = r.points.iter().map(|p| p.x).collect();
            assert_eq!(r.points.len(), 4, "inverted={inverted}");
            assert_eq!(
                ys.iter().copied().fold(f32::MAX, f32::min),
                60.0,
                "inverted={inverted}"
            );
            assert_eq!(
                ys.iter().copied().fold(f32::MIN, f32::max),
                140.0,
                "inverted={inverted}"
            );
            // Bars where (x / 3) is even inside 100..190: the first at 102,
            // the last ends at 189.
            assert_eq!(
                xs.iter().copied().fold(f32::MAX, f32::min),
                102.0,
                "inverted={inverted}"
            );
            assert_eq!(
                xs.iter().copied().fold(f32::MIN, f32::max),
                189.0,
                "inverted={inverted}"
            );
        }
    }

    #[test]
    fn box_does_not_depend_on_the_scan_line() {
        let img = labelled_code(false);
        let boxes: Vec<Vec<Point>> = [70.0, 100.0, 130.0]
            .iter()
            .map(|&y| {
                let mut r = code39(vec![Point::new(101.0, y), Point::new(130.0, y)]);
                refine_linear_box(&img, &mut r, false);
                r.points
            })
            .collect();
        assert!(boxes.windows(2).all(|w| w[0] == w[1]), "{boxes:?}");
    }

    #[test]
    fn leaves_non_scan_line_points_alone() {
        let mut r = code39(vec![Point::new(1.0, 5.0), Point::new(20.0, 9.0)]);
        let before = r.points.clone();
        refine_linear_box(&GrayImage::new(30, 30), &mut r, false);
        assert_eq!(r.points, before);
    }
}
