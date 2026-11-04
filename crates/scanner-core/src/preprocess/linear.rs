//! Enhancement for hard 1D barcodes: uneven lighting and grainy bars.
//!
//! Motivating case (`tests/fixtures/real/cattle_ear_tag_code128.jpg`): a
//! laser-engraved Code 128 on an orange ear tag. Along the code the spaces go
//! from grey (left) to white (right), so a single threshold cannot separate
//! bars from spaces everywhere, and the engraving is grainy: some bars look
//! hollow and get split in two. rxing's `HybridBinarizer` and the Sauvola
//! pipeline both fail on it, although the symbol itself is intact.
//!
//! Two cheap, separable passes (O(width × height), no allocation per pixel):
//!
//! 1. **Smooth along the bars** (vertical box blur). Bars of a horizontal 1D
//!    code are vertical, so averaging vertically removes the grain without
//!    merging neighbouring bars. The radius stays small so that a few degrees
//!    of tilt do not blur bar edges (2° over ±7 px moves an edge by 0.5 px).
//! 2. **Threshold locally along each row**: every pixel is compared with the
//!    midpoint of the darkest and lightest values within ±`radius` pixels,
//!    which cancels the lighting gradient. Flat areas (no contrast in the
//!    window) become white, so noise is not amplified into fake bars.
//!
//! The output is binary (0 / 255) on purpose. With a normalised but still
//! grey image, rxing's `HybridBinarizer` re-thresholds 8×8 blocks and loses
//! the code again (measured on the ear-tag photo: grey output not found,
//! binary output decoded).
//!
//! Only roughly horizontal codes survive the row-wise threshold: vertical
//! codes are left to the other strategies (rxing's try-harder mode also scans
//! the image rotated by 90°).
//!
//! After the vertical blur, neighbouring rows are nearly identical, so the
//! decoder only needs one row every [`row_step`] rows ([`sample_rows`]).

use image::GrayImage;
use std::collections::VecDeque;

/// Below this local contrast (in grey levels) a window is considered flat.
const MIN_LOCAL_CONTRAST: u8 = 24;

/// Size of the thresholding window relative to the image width.
///
/// The window works best at ~6 modules of the code (measured on the ear-tag
/// fixture: it decodes for windows of ~2.5 to ~9 modules). The module size is
/// unknown, so two scales are tried in turn; together they cover codes from
/// ~2 to ~16 px per module in a 1024 px wide image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearScale {
    /// Window of width / 64 (16 px at 1024 px): small or distant codes.
    Fine,
    /// Window of width / 30 (34 px at 1024 px): large or close codes.
    Coarse,
}

/// Enhance an image for 1D decoding (see the module documentation).
pub fn linear_enhance(img: &GrayImage, scale: LinearScale) -> GrayImage {
    let (w, h) = img.dimensions();
    if w < 8 || h < 8 {
        return img.clone();
    }
    let smooth_radius = smooth_radius(h) as usize;
    let divisor = match scale {
        LinearScale::Fine => 64,
        LinearScale::Coarse => 30,
    };
    let window_radius = (w / divisor).clamp(8, 96) as usize;
    threshold_rows(&vertical_box_blur(img, smooth_radius), window_radius)
}

/// Radius of the vertical blur: ~1/120 of the height. Larger radii start to
/// blur tilted bars; smaller ones leave the engraving grain (both measured).
fn smooth_radius(height: u32) -> u32 {
    (height / 120).clamp(3, 8)
}

/// Distance between the rows worth decoding in a [`linear_enhance`] output.
///
/// Each output row averages `2 × radius + 1` input rows, so rows `radius`
/// apart still overlap by half: a code at least ~2 radii tall (7 px at 720p)
/// is always crossed by a sampled row.
pub fn row_step(height: u32) -> u32 {
    smooth_radius(height)
}

/// Keep one row every `step` rows (row `i` of the output is row `i × step`).
pub fn sample_rows(img: &GrayImage, step: u32) -> GrayImage {
    let (w, h) = img.dimensions();
    let step = step.max(1);
    let rows = h.div_ceil(step);
    let mut out = Vec::with_capacity((w * rows) as usize);
    for row in img.as_raw().chunks_exact(w as usize).step_by(step as usize) {
        out.extend_from_slice(row);
    }
    GrayImage::from_raw(w, rows, out).expect("sample_rows: exact size")
}

/// Box blur along columns, using a running sum per column.
fn vertical_box_blur(img: &GrayImage, radius: usize) -> GrayImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let src = img.as_raw();
    let mut out = vec![0u8; w * h];
    let mut sums = vec![0u32; w];
    // Prime the window with rows [0, radius].
    for y in 0..=radius.min(h - 1) {
        for (s, &p) in sums.iter_mut().zip(&src[y * w..(y + 1) * w]) {
            *s += u32::from(p);
        }
    }
    for y in 0..h {
        let top = y.saturating_sub(radius);
        let bottom = (y + radius).min(h - 1);
        let count = (bottom - top + 1) as u32;
        for (o, &s) in out[y * w..(y + 1) * w].iter_mut().zip(&sums) {
            *o = (s / count) as u8;
        }
        // Slide: add row bottom+1, remove row top (when the window moves).
        if bottom + 1 < h {
            for (s, &p) in sums
                .iter_mut()
                .zip(&src[(bottom + 1) * w..(bottom + 2) * w])
            {
                *s += u32::from(p);
            }
        }
        if y >= radius {
            for (s, &p) in sums.iter_mut().zip(&src[top * w..(top + 1) * w]) {
                *s -= u32::from(p);
            }
        }
    }
    GrayImage::from_raw(w as u32, h as u32, out).expect("blur: same dimensions")
}

/// Binarise each pixel against the midpoint of the min and max of its row
/// neighbourhood. Sliding min / max use monotonic deques: O(1) amortised per pixel.
fn threshold_rows(img: &GrayImage, radius: usize) -> GrayImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let src = img.as_raw();
    let mut out = vec![0u8; w * h];
    let mut mins: VecDeque<usize> = VecDeque::with_capacity(2 * radius + 1);
    let mut maxs: VecDeque<usize> = VecDeque::with_capacity(2 * radius + 1);

    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        mins.clear();
        maxs.clear();
        let mut next = 0; // next index to enter the window
        for x in 0..w {
            let right = (x + radius).min(w - 1);
            while next <= right {
                while mins.back().is_some_and(|&i| row[i] >= row[next]) {
                    mins.pop_back();
                }
                mins.push_back(next);
                while maxs.back().is_some_and(|&i| row[i] <= row[next]) {
                    maxs.pop_back();
                }
                maxs.push_back(next);
                next += 1;
            }
            let left = x.saturating_sub(radius);
            while mins.front().is_some_and(|&i| i < left) {
                mins.pop_front();
            }
            while maxs.front().is_some_and(|&i| i < left) {
                maxs.pop_front();
            }
            let (lo, hi) = match (mins.front(), maxs.front()) {
                (Some(&a), Some(&b)) => (row[a], row[b]),
                _ => (0, 255),
            };
            let flat = hi - lo < MIN_LOCAL_CONTRAST;
            let midpoint = (u16::from(lo) + u16::from(hi)) / 2;
            out[y * w + x] = if flat || u16::from(row[x]) > midpoint {
                255
            } else {
                0
            };
        }
    }
    GrayImage::from_raw(w as u32, h as u32, out).expect("threshold: same dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn preserves_dimensions() {
        let img = GrayImage::new(120, 40);
        assert_eq!(
            linear_enhance(&img, LinearScale::Fine).dimensions(),
            (120, 40)
        );
    }

    #[test]
    fn cancels_a_lighting_gradient() {
        // Bars every 8 px; the space level rises from 90 (left) to 230 (right).
        let img = GrayImage::from_fn(400, 60, |x, _| {
            let space = 90 + (x * 140 / 400) as u8;
            Luma([if (x / 8) % 2 == 0 { 40 } else { space }])
        });
        let out = linear_enhance(&img, LinearScale::Coarse);
        for x in [20u32, 200, 380] {
            let bar_x = x - x % 16 + 4; // middle of a bar
            let space_x = bar_x + 8; // middle of the next space
            assert_eq!(out.get_pixel(bar_x, 30)[0], 0, "bar at {bar_x}");
            assert_eq!(out.get_pixel(space_x, 30)[0], 255, "space at {space_x}");
        }
    }

    #[test]
    fn flat_areas_become_white() {
        let img = GrayImage::from_fn(100, 30, |x, _| Luma([(120 + (x % 3) * 5) as u8]));
        for scale in [LinearScale::Fine, LinearScale::Coarse] {
            assert!(linear_enhance(&img, scale).pixels().all(|p| p[0] == 255));
        }
    }

    #[test]
    fn sample_rows_keeps_every_nth_row() {
        let img = GrayImage::from_fn(4, 10, |_, y| Luma([y as u8]));
        let out = sample_rows(&img, 3);
        assert_eq!(out.dimensions(), (4, 4));
        let rows: Vec<u8> = (0..4).map(|y| out.get_pixel(0, y)[0]).collect();
        assert_eq!(rows, [0, 3, 6, 9]);
    }

    #[test]
    fn vertical_blur_keeps_flat_columns() {
        let img = GrayImage::from_fn(10, 30, |x, _| Luma([(x * 20) as u8]));
        let out = vertical_box_blur(&img, 4);
        for (x, _, p) in out.enumerate_pixels() {
            assert_eq!(p[0], (x * 20) as u8);
        }
    }
}
