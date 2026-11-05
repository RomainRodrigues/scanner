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
//! 2. **Height** across the scan line: the bars of the scan line are followed
//!    line by line on both sides until most of them end. Comparing whole rows
//!    to the previous one instead drifted: on the ear tag the bars blend into
//!    the digits printed above and below over a few rows, each close enough
//!    to the previous one, and the box covered the whole frame (0 → 1008 px);
//!    comparing them to the scan line only stopped half way down the
//!    feed-label bars (print variations).

use super::verify::geometry;
use crate::preprocess::linear::dark_profile;
use crate::result::{BarcodeFormat, Point, ScanResult};
use image::GrayImage;

/// A light run longer than this × the median bar width ends the symbol.
/// Spaces inside a symbol are at most 4 modules (Code 128, EAN), Code 39
/// inter-character gaps at most 3; a quiet zone is at least 10.
const QUIET_FACTOR: usize = 5;
/// A line belongs to the symbol while at least this fraction of the bar
/// pixels of the scan line continue through it.
const ALIVE: f32 = 0.6;
/// Light lines a bar may cross (scratches, specks) and still continue.
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
    let dark = line.dark_at(line.pos)?;
    if !has_unreliable_points(r.format) {
        // Points in the middle of the start / stop patterns, at a known
        // number of modules inside the symbol. The module is measured on the
        // bars: deriving it from the text length assumes one character per
        // symbol, wrong for Code 128 set C (two digits per symbol), which
        // left the outer bars of the ear tag out of the box.
        let geo = geometry(r.format, r.text.chars().count())?;
        let (from, to) = (
            a0.min(a1).max(0.0) as usize,
            (a0.max(a1) as usize).min(dark.len()),
        );
        let module = module_width(&dark[from..to])?;
        let dir = (a1 - a0).signum();
        return Some((
            a0 - dir * geo.inner_start * module,
            a1 + dir * geo.inner_end * module,
        ));
    }
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

/// Width of one module: the narrowest element width that is common (first
/// quartile of the bar and space widths, which are 1 to 4 modules).
fn module_width(dark: &[bool]) -> Option<f32> {
    let mut runs: Vec<usize> = dark.chunk_by(|a, b| a == b).map(<[bool]>::len).collect();
    // The outer runs may be cut by the span: ignore them.
    if runs.len() < 6 {
        return None;
    }
    runs.pop();
    runs.remove(0);
    runs.sort_unstable();
    Some(runs[runs.len() / 4] as f32)
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

    /// Lines on each side of the scan line through which the bars of the scan
    /// line continue: each bar pixel of the scan line is followed line by
    /// line, moving by one pixel to follow a tilted code and surviving
    /// `MAX_MISSES` light lines (scratches). The symbol ends when fewer than
    /// `ALIVE` of them continue: text or digits touching the bars only extend
    /// the bars under their strokes.
    fn thickness(&self, lo: usize, hi: usize) -> (usize, usize) {
        let hi = hi.min(self.len);
        let Some(reference) = self.dark_span(self.pos, lo, hi) else {
            return (0, 0);
        };
        let start: Vec<usize> = (0..reference.len()).filter(|&i| reference[i]).collect();
        if start.is_empty() {
            return (0, 0);
        }
        let max = (hi - lo).max(16) * 2;
        let extend = |step: isize| {
            // (position in the span, consecutive misses)
            let mut tracks: Vec<(usize, usize)> = start.iter().map(|&x| (x, 0)).collect();
            let mut extent = 0;
            for d in 1..=max {
                let index = self.pos as isize + step * d as isize;
                let Ok(index) = usize::try_from(index) else {
                    break;
                };
                let Some(row) = self.dark_span(index, lo, hi) else {
                    break;
                };
                let mut on = 0;
                for (x, misses) in &mut tracks {
                    let next = [*x, x.saturating_sub(1), *x + 1]
                        .into_iter()
                        .find(|&c| row.get(c).copied().unwrap_or(false));
                    if let Some(c) = next {
                        *x = c;
                        *misses = 0;
                        on += 1;
                    } else {
                        *misses += 1;
                    }
                }
                let alive = tracks.iter().filter(|(_, m)| *m <= MAX_MISSES).count();
                if on as f32 >= ALIVE * start.len() as f32 {
                    extent = d;
                }
                if (alive as f32) < ALIVE * start.len() as f32 {
                    break;
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

    /// The ear-tag layout: big digits printed right above and below the bars,
    /// touching them. Comparing rows to the previous one drifted into them.
    #[test]
    fn box_stops_at_digits_touching_the_bars() {
        let img = GrayImage::from_fn(300, 300, |x, y| {
            let bar = (100..160).contains(&y) && (100..190).contains(&x) && (x / 3) % 2 == 0;
            // "Digits": thick strokes 20 px wide every 40 px, with a soft edge
            // into the bars (2 rows half-way between both patterns).
            let digit = (!(100..160).contains(&y) && (40..280).contains(&y) && (x / 20) % 2 == 0)
                || ((98..100).contains(&y) || (160..162).contains(&y))
                    && (x / 3) % 2 == 0
                    && (x / 20) % 2 == 0;
            Luma([if bar || digit { 30 } else { 230 }])
        });
        let mut r = code39(vec![Point::new(103.0, 130.0), Point::new(130.0, 130.0)]);
        refine_linear_box(&img, &mut r, false);
        let ys: Vec<f32> = r.points.iter().map(|p| p.y).collect();
        let (y0, y1) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(
            (96.0..=100.0).contains(&y0) && (160.0..=164.0).contains(&y1),
            "{ys:?}"
        );
    }

    #[test]
    fn box_follows_a_slightly_tilted_code() {
        // Bars shifted by one pixel every 12 rows (~5°).
        let img = GrayImage::from_fn(300, 200, |x, y| {
            let xs = i64::from(x) - (i64::from(y) - 100) / 12;
            let bar = (60..140).contains(&y) && (100..190).contains(&xs) && (xs / 3) % 2 == 0;
            Luma([if bar { 30 } else { 230 }])
        });
        let mut r = code39(vec![Point::new(103.0, 100.0), Point::new(130.0, 100.0)]);
        refine_linear_box(&img, &mut r, false);
        let ys: Vec<f32> = r.points.iter().map(|p| p.y).collect();
        let (y0, y1) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(y0 <= 62.0 && y1 >= 138.0, "{ys:?}");
    }

    #[test]
    fn leaves_non_scan_line_points_alone() {
        let mut r = code39(vec![Point::new(1.0, 5.0), Point::new(20.0, 9.0)]);
        let before = r.points.clone();
        refine_linear_box(&GrayImage::new(30, 30), &mut r, false);
        assert_eq!(r.points, before);
    }
}
