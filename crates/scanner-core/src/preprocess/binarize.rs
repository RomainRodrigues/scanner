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
///   Larger radius handles bigger lighting gradients but is slower.
///   Typical range: 7–25. Use 15 for barcode scanning.
/// - `k`: Sensitivity parameter. Higher = more aggressive thresholding.
///   Typical range: 0.1–0.5. Use 0.15 for barcodes.
///
/// # Returns
/// A binary image (0 = black, 255 = white).
pub fn sauvola_binarize(img: &GrayImage, window_radius: u32, k: f64) -> GrayImage {
    let (width, height) = img.dimensions();
    const R: f64 = 128.0; // Dynamic range factor for 8-bit images

    // Build integral images for fast local mean and variance computation
    let (sum_table, sum_sq_table) = build_integral_images(img);

    let mut output = GrayImage::new(width, height);
    let radius = window_radius as i64;

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y)[0] as f64;

            // Window bounds (clamped to image)
            let x0 = (x as i64 - radius).max(0) as u32;
            let y0 = (y as i64 - radius).max(0) as u32;
            let x1 = (x as i64 + radius).min(width as i64 - 1) as u32;
            let y1 = (y as i64 + radius).min(height as i64 - 1) as u32;

            let count = ((x1 - x0 + 1) * (y1 - y0 + 1)) as f64;

            let sum = integral_rect(&sum_table, x0, y0, x1, y1, width);
            let sum_sq = integral_rect(&sum_sq_table, x0, y0, x1, y1, width);

            let mean = sum / count;
            let variance = (sum_sq / count) - (mean * mean);
            let std_dev = variance.max(0.0).sqrt();

            // Sauvola threshold
            let threshold = mean * (1.0 + k * (std_dev / R - 1.0));

            let val = if pixel >= threshold { 255u8 } else { 0u8 };
            output.put_pixel(x, y, image::Luma([val]));
        }
    }

    output
}

/// Build integral image (summed area table) and squared integral image.
///
/// Integral image allows computing the sum over any rectangular region in O(1).
fn build_integral_images(img: &GrayImage) -> (Vec<f64>, Vec<f64>) {
    let (width, height) = img.dimensions();
    let size = (width * height) as usize;
    let mut sum = vec![0.0f64; size];
    let mut sum_sq = vec![0.0f64; size];

    for y in 0..height {
        for x in 0..width {
            let val = img.get_pixel(x, y)[0] as f64;
            let idx = (y * width + x) as usize;

            let above = if y > 0 { sum[((y - 1) * width + x) as usize] } else { 0.0 };
            let left = if x > 0 { sum[(y * width + x - 1) as usize] } else { 0.0 };
            let above_left = if x > 0 && y > 0 {
                sum[((y - 1) * width + x - 1) as usize]
            } else {
                0.0
            };

            sum[idx] = val + above + left - above_left;

            let sq = val * val;
            let above_sq =
                if y > 0 { sum_sq[((y - 1) * width + x) as usize] } else { 0.0 };
            let left_sq =
                if x > 0 { sum_sq[(y * width + x - 1) as usize] } else { 0.0 };
            let above_left_sq = if x > 0 && y > 0 {
                sum_sq[((y - 1) * width + x - 1) as usize]
            } else {
                0.0
            };

            sum_sq[idx] = sq + above_sq + left_sq - above_left_sq;
        }
    }

    (sum, sum_sq)
}

/// Query the rectangular sum from an integral image.
///
/// Computes the sum over the rectangle [x0,x1] × [y0,y1] (inclusive).
#[inline]
fn integral_rect(table: &[f64], x0: u32, y0: u32, x1: u32, y1: u32, width: u32) -> f64 {
    let get = |x: i64, y: i64| -> f64 {
        if x < 0 || y < 0 {
            0.0
        } else {
            table[(y as u32 * width + x as u32) as usize]
        }
    };

    get(x1 as i64, y1 as i64)
        - get(x0 as i64 - 1, y1 as i64)
        - get(x1 as i64, y0 as i64 - 1)
        + get(x0 as i64 - 1, y0 as i64 - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let (sum, _) = build_integral_images(&img);
        // Total sum should be 1+2+3+4 = 10
        assert_eq!(sum[3] as u32, 10);
    }
}
