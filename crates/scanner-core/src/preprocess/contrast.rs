//! Adaptive contrast enhancement via CLAHE (Contrast Limited Adaptive Histogram Equalization).
//!
//! CLAHE divides the image into tiles and equalizes each tile's histogram independently,
//! with a clip limit to prevent noise amplification. This is significantly better than
//! global histogram equalization for images with uneven lighting (camera flash shadows, etc.)

use image::GrayImage;

/// Apply CLAHE to a grayscale image.
///
/// # Parameters
/// - `img`: Input grayscale image
/// - `tile_size`: Size of each tile in pixels (e.g., 8 means 8×8 tiles).
///   Smaller tiles = more local adaptation but more computation.
/// - `clip_limit`: Maximum slope of the CDF (contrast limit). Higher = more contrast.
///   Typical range: 2.0–4.0. Use 3.0 for barcode scanning.
///
/// # Returns
/// A new contrast-enhanced grayscale image.
pub fn clahe(img: &GrayImage, tile_size: u32, clip_limit: f32) -> GrayImage {
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        return img.clone();
    }

    let (w, h) = (width as usize, height as usize);
    let src = img.as_raw();
    let tile = tile_size.max(1) as usize;
    let nx = w.div_ceil(tile).max(1);
    let ny = h.div_ceil(tile).max(1);

    // One LUT per tile, computed straight from the source buffer (no tile copies).
    let mut luts: Vec<[u8; 256]> = Vec::with_capacity(nx * ny);
    for ty in 0..ny {
        for tx in 0..nx {
            let (x0, y0) = (tx * tile, ty * tile);
            let (x1, y1) = ((x0 + tile).min(w), (y0 + tile).min(h));
            let mut hist = [0usize; 256];
            for y in y0..y1 {
                for &px in &src[y * w + x0..y * w + x1] {
                    hist[px as usize] += 1;
                }
            }
            luts.push(lut_from_histogram(hist, (x1 - x0) * (y1 - y0), clip_limit));
        }
    }

    // Interpolation coordinates only depend on the column / row: precompute them.
    let axis = |len: usize, n: usize| -> Vec<(usize, usize, f32)> {
        (0..len)
            .map(|i| {
                let f = (i as f32 / tile as f32) - 0.5;
                let i0 = (f.floor() as i64).clamp(0, n as i64 - 1) as usize;
                let i1 = (i0 + 1).min(n - 1);
                (i0, i1, (f - i0 as f32).clamp(0.0, 1.0))
            })
            .collect()
    };
    let cols = axis(w, nx);
    let rows = axis(h, ny);

    let mut out = vec![0u8; w * h];
    for (y, &(ty0, ty1, fy)) in rows.iter().enumerate() {
        let src_row = &src[y * w..(y + 1) * w];
        let out_row = &mut out[y * w..(y + 1) * w];
        for (x, &(tx0, tx1, fx)) in cols.iter().enumerate() {
            let v = src_row[x] as usize;
            let v00 = f32::from(luts[ty0 * nx + tx0][v]);
            let v10 = f32::from(luts[ty0 * nx + tx1][v]);
            let v01 = f32::from(luts[ty1 * nx + tx0][v]);
            let v11 = f32::from(luts[ty1 * nx + tx1][v]);
            let top = v00 + fx * (v10 - v00);
            let bot = v01 + fx * (v11 - v01);
            out_row[x] = (top + fy * (bot - top)).round().clamp(0.0, 255.0) as u8;
        }
    }

    GrayImage::from_raw(width, height, out).expect("clahe: same dimensions")
}

/// Compute a clipped-and-redistributed histogram equalization LUT for one tile.
#[cfg(test)]
fn compute_clahe_lut(tile: &GrayImage, clip_limit: f32) -> [u8; 256] {
    let mut hist = [0usize; 256];
    for pixel in tile.pixels() {
        hist[pixel[0] as usize] += 1;
    }
    lut_from_histogram(hist, (tile.width() * tile.height()) as usize, clip_limit)
}

/// Clip a histogram, redistribute the excess and turn its CDF into a LUT.
fn lut_from_histogram(mut hist: [usize; 256], n_pixels: usize, clip_limit: f32) -> [u8; 256] {
    let n_pixels = n_pixels.max(1);
    // Compute clip threshold (in pixels)
    let clip_threshold = ((clip_limit * n_pixels as f32) / 256.0).max(1.0) as usize;

    // Clip histogram and redistribute excess
    let mut excess = 0usize;
    for count in hist.iter_mut() {
        if *count > clip_threshold {
            excess += *count - clip_threshold;
            *count = clip_threshold;
        }
    }

    // Redistribute excess uniformly
    let per_bin = excess / 256;
    let leftover = excess % 256;
    for (i, count) in hist.iter_mut().enumerate() {
        *count += per_bin;
        if i < leftover {
            *count += 1;
        }
    }

    // Build cumulative distribution function (CDF) → LUT
    let mut lut = [0u8; 256];
    let mut cdf = 0usize;
    let scale = 255.0 / n_pixels as f32;

    for (i, &h) in hist.iter().enumerate() {
        cdf += h;
        lut[i] = (cdf as f32 * scale).round().clamp(0.0, 255.0) as u8;
    }

    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clahe_preserves_dimensions() {
        let img = GrayImage::new(64, 64);
        let out = clahe(&img, 8, 3.0);
        assert_eq!(out.dimensions(), (64, 64));
    }

    #[test]
    fn test_clahe_small_image() {
        let img = GrayImage::new(4, 4);
        let out = clahe(&img, 8, 3.0);
        assert_eq!(out.dimensions(), (4, 4));
    }

    #[test]
    fn test_clahe_lut_monotone() {
        // LUT should be non-decreasing
        let img = GrayImage::new(16, 16);
        let lut = compute_clahe_lut(&img, 3.0);
        for i in 1..256 {
            assert!(lut[i] >= lut[i - 1]);
        }
    }
}
