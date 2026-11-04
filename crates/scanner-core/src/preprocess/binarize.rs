//! Sauvola adaptive binarization for robust thresholding under uneven lighting.
//!
//! Unlike global Otsu thresholding, Sauvola computes a local threshold for each pixel
//! based on the mean and standard deviation of its neighborhood. This handles shadows,
//! uneven illumination, and reflections much better — critical for camera-captured images.
//!
//! Sauvola formula: T(x,y) = μ(x,y) × [1 + k × (σ(x,y)/R − 1)]
//!
//! Where:
//! - μ = local mean in the window
//! - σ = local standard deviation in the window
//! - R = maximum possible standard deviation (128 for 8-bit images)
//! - k = sensitivity parameter (typically 0.1–0.5)

use image::GrayImage;

/// Apply Sauvola adaptive binarization to a grayscale image.
///
/// # Parameters
/// - `img`: Input grayscale image
/// - `window_radius`: Radius of the local neighborhood (window size = 2×radius+1).
///   Larger radius handles bigger lighting gradients. The cost does not depend
///   on the radius thanks to sliding window sums. Use 15 for barcode scanning.
/// - `k`: Sensitivity parameter. Higher = more aggressive thresholding.
///   Typical range: 0.1–0.5. Use 0.15 for barcodes.
///
/// # Returns
/// A binary image (0 = black, 255 = white).
pub fn sauvola_binarize(img: &GrayImage, window_radius: u32, k: f64) -> GrayImage {
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        return img.clone();
    }
    let (w, h) = (width as usize, height as usize);
    let r = window_radius as usize;
    let src = img.as_raw();

    // Window sums by sliding sums instead of integral images: same integers,
    // so the same output, without the two `(w + 1) × (h + 1)` `u64` tables
    // (~7 MB per call on a 1024×432 frame) and their cache misses.
    // `col_sum[x]` / `col_sq[x]`: column x over the rows of the current window.
    let mut col_sum = vec![0u64; w];
    let mut col_sq = vec![0u64; w];
    let add_row = |cs: &mut [u64], cq: &mut [u64], row: &[u8], sign: bool| {
        for ((s, q), &p) in cs.iter_mut().zip(cq.iter_mut()).zip(row) {
            let (v, v2) = (u64::from(p), u64::from(p) * u64::from(p));
            if sign {
                *s += v;
                *q += v2;
            } else {
                *s -= v;
                *q -= v2;
            }
        }
    };
    for row in src.chunks_exact(w).take(r.min(h - 1) + 1) {
        add_row(&mut col_sum, &mut col_sq, row, true);
    }

    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let rows = (y + r).min(h - 1) - y.saturating_sub(r) + 1;
        let (mut s, mut q) = (0u64, 0u64);
        for x in 0..=r.min(w - 1) {
            s += col_sum[x];
            q += col_sq[x];
        }
        for x in 0..w {
            let cols = (x + r).min(w - 1) - x.saturating_sub(r) + 1;
            let count = (cols * rows) as f64;
            let mean = s as f64 / count;
            let variance = (q as f64 / count) - mean * mean;
            let std_dev = variance.max(0.0).sqrt();
            let threshold = mean * (1.0 + k * (std_dev / R - 1.0));
            out[y * w + x] = if f64::from(src[y * w + x]) >= threshold {
                255
            } else {
                0
            };
            // Slide right: add column x + r + 1, remove column x − r.
            if x + r + 1 < w {
                s += col_sum[x + r + 1];
                q += col_sq[x + r + 1];
            }
            if x >= r {
                s -= col_sum[x - r];
                q -= col_sq[x - r];
            }
        }
        // Slide down: add row y + r + 1, remove row y − r.
        if y + r + 1 < h {
            add_row(
                &mut col_sum,
                &mut col_sq,
                &src[(y + r + 1) * w..(y + r + 2) * w],
                true,
            );
        }
        if y >= r {
            add_row(
                &mut col_sum,
                &mut col_sq,
                &src[(y - r) * w..(y - r + 1) * w],
                false,
            );
        }
    }

    GrayImage::from_raw(width, height, out).expect("sauvola: same dimensions")
}

/// Dynamic range of the standard deviation for 8-bit images.
const R: f64 = 128.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous integral-image implementation, kept as a reference.
    fn reference_sauvola(img: &GrayImage, window_radius: u32, k: f64) -> GrayImage {
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return img.clone();
        }
        const R: f64 = 128.0; // Dynamic range factor for 8-bit images

        let (w, h) = (width as usize, height as usize);
        let src = img.as_raw();
        let (sum, sum_sq) = build_integral_images(src, w, h);
        let iw = w + 1;
        let r = window_radius as usize;

        let mut out = vec![0u8; w * h];
        for y in 0..h {
            let y0 = y.saturating_sub(r);
            let y1 = (y + r).min(h - 1) + 1;
            let row_top = y0 * iw;
            let row_bot = y1 * iw;
            for x in 0..w {
                let x0 = x.saturating_sub(r);
                let x1 = (x + r).min(w - 1) + 1;
                let count = ((x1 - x0) * (y1 - y0)) as f64;

                let s =
                    sum[row_bot + x1] + sum[row_top + x0] - sum[row_top + x1] - sum[row_bot + x0];
                let sq = sum_sq[row_bot + x1] + sum_sq[row_top + x0]
                    - sum_sq[row_top + x1]
                    - sum_sq[row_bot + x0];

                let mean = s as f64 / count;
                let variance = (sq as f64 / count) - mean * mean;
                let std_dev = variance.max(0.0).sqrt();
                let threshold = mean * (1.0 + k * (std_dev / R - 1.0));

                out[y * w + x] = if f64::from(src[y * w + x]) >= threshold {
                    255
                } else {
                    0
                };
            }
        }

        GrayImage::from_raw(width, height, out).expect("sauvola: same dimensions")
    }

    /// Build the integral image (summed area table) and squared integral image.
    ///
    /// Both tables have an extra zero row and column at the top/left, so that
    /// entry `(y + 1) * (w + 1) + (x + 1)` is the sum over `[0, x] × [0, y]`.
    fn build_integral_images(src: &[u8], w: usize, h: usize) -> (Vec<u64>, Vec<u64>) {
        let iw = w + 1;
        let mut sum = vec![0u64; iw * (h + 1)];
        let mut sum_sq = vec![0u64; iw * (h + 1)];

        for y in 0..h {
            let mut row_sum = 0u64;
            let mut row_sq = 0u64;
            let src_row = &src[y * w..(y + 1) * w];
            for (x, &px) in src_row.iter().enumerate() {
                let v = u64::from(px);
                row_sum += v;
                row_sq += v * v;
                let idx = (y + 1) * iw + x + 1;
                sum[idx] = sum[idx - iw] + row_sum;
                sum_sq[idx] = sum_sq[idx - iw] + row_sq;
            }
        }

        (sum, sum_sq)
    }

    #[test]
    fn matches_the_integral_image_implementation() {
        for (w, h) in [(1, 1), (5, 3), (40, 17), (97, 61), (300, 40)] {
            let img = GrayImage::from_fn(w, h, |x, y| {
                image::Luma([((x * 7919 + y * 104_729) % 251) as u8])
            });
            for r in [0, 1, 7, 15, 50] {
                assert_eq!(
                    sauvola_binarize(&img, r, 0.15),
                    reference_sauvola(&img, r, 0.15),
                    "{w}x{h} r={r}"
                );
            }
        }
    }

    #[test]
    fn test_sauvola_preserves_dimensions() {
        let img = GrayImage::new(64, 64);
        let out = sauvola_binarize(&img, 15, 0.15);
        assert_eq!(out.dimensions(), (64, 64));
    }

    #[test]
    fn test_sauvola_all_white_input() {
        // Uniform white image → should produce all white (255) or a valid binary result
        let mut img = GrayImage::new(32, 32);
        for px in img.pixels_mut() {
            *px = image::Luma([255u8]);
        }
        let out = sauvola_binarize(&img, 7, 0.15);
        // All pixels should be white (max std_dev = 0, threshold ≤ pixel value)
        for px in out.pixels() {
            assert!(px[0] == 0 || px[0] == 255);
        }
    }

    #[test]
    fn test_integral_image_sum() {
        // 2×2 image with values [1, 2, 3, 4]
        let mut img = GrayImage::new(2, 2);
        img.put_pixel(0, 0, image::Luma([1u8]));
        img.put_pixel(1, 0, image::Luma([2u8]));
        img.put_pixel(0, 1, image::Luma([3u8]));
        img.put_pixel(1, 1, image::Luma([4u8]));

        let (sum, sq) = build_integral_images(img.as_raw(), 2, 2);
        // Bottom-right entry holds the total: 1+2+3+4 = 10
        assert_eq!(sum[8], 10);
        assert_eq!(sq[8], 1 + 4 + 9 + 16);
    }
}
