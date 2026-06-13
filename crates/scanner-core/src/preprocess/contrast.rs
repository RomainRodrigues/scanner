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

    let tile_w = tile_size.max(1);
    let tile_h = tile_size.max(1);

    // Number of tiles in each direction (at least 1)
    let nx = width.div_ceil(tile_w).max(1);
    let ny = height.div_ceil(tile_h).max(1);

    // Compute a LUT for each tile
    let mut tile_luts: Vec<Vec<u8>> = Vec::with_capacity((nx * ny) as usize);

    for ty in 0..ny {
        for tx in 0..nx {
            let x0 = tx * tile_w;
            let y0 = ty * tile_h;
            let x1 = (x0 + tile_w).min(width);
            let y1 = (y0 + tile_h).min(height);

            let tile = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
            let lut = compute_clahe_lut(&tile, clip_limit);
            tile_luts.push(lut);
        }
    }

    // Bilinear interpolation between the four surrounding tile LUTs
    let mut output = GrayImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let pixel_val = img.get_pixel(x, y)[0] as usize;

            // Find which tile context this pixel falls into (using tile centers)
            let tx_f = (x as f32 / tile_w as f32) - 0.5;
            let ty_f = (y as f32 / tile_h as f32) - 0.5;

            let tx0 = (tx_f.floor() as i64).clamp(0, nx as i64 - 1) as u32;
            let ty0 = (ty_f.floor() as i64).clamp(0, ny as i64 - 1) as u32;
            let tx1 = (tx0 + 1).min(nx - 1);
            let ty1 = (ty0 + 1).min(ny - 1);

            let fx = (tx_f - tx0 as f32).clamp(0.0, 1.0);
            let fy = (ty_f - ty0 as f32).clamp(0.0, 1.0);

            let lut00 = &tile_luts[(ty0 * nx + tx0) as usize];
            let lut10 = &tile_luts[(ty0 * nx + tx1) as usize];
            let lut01 = &tile_luts[(ty1 * nx + tx0) as usize];
            let lut11 = &tile_luts[(ty1 * nx + tx1) as usize];

            // Bilinear interpolation
            let v00 = lut00[pixel_val] as f32;
            let v10 = lut10[pixel_val] as f32;
            let v01 = lut01[pixel_val] as f32;
            let v11 = lut11[pixel_val] as f32;

            let top = v00 + fx * (v10 - v00);
            let bot = v01 + fx * (v11 - v01);
            let val = (top + fy * (bot - top)).round().clamp(0.0, 255.0) as u8;

            output.put_pixel(x, y, image::Luma([val]));
        }
    }

    output
}

/// Compute a clipped-and-redistributed histogram equalization LUT for one tile.
fn compute_clahe_lut(tile: &GrayImage, clip_limit: f32) -> Vec<u8> {
    let n_pixels = (tile.width() * tile.height()) as usize;

    // Build histogram
    let mut hist = [0usize; 256];
    for pixel in tile.pixels() {
        hist[pixel[0] as usize] += 1;
    }

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
    let mut lut = vec![0u8; 256];
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
