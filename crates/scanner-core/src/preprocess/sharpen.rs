//! Unsharp mask sharpening for improving barcode readability on blurry camera frames.
//!
//! Unsharp masking: output = original + amount × (original − blur(original, sigma))
//! This enhances edges without introducing haloing artifacts when tuned properly.

use image::GrayImage;
use imageproc::filter::gaussian_blur_f32;

/// Apply unsharp mask to a grayscale image.
///
/// # Parameters
/// - `img`: Input grayscale image
/// - `sigma`: Gaussian blur sigma. Higher = more blurring in the mask, wider sharpening radius.
///   Typical range: 0.5–2.0. Use ~1.0 for camera frames.
/// - `amount`: Sharpening strength. Higher = more aggressive.
///   Typical range: 0.3–1.0. Use ~0.5 to avoid over-sharpening noise.
pub fn unsharp_mask(img: &GrayImage, sigma: f32, amount: f32) -> GrayImage {
    let (width, height) = img.dimensions();

    // Create the blurred version (the "mask")
    let blurred = gaussian_blur_f32(img, sigma);

    let out: Vec<u8> = img
        .as_raw()
        .iter()
        .zip(blurred.as_raw().iter())
        .map(|(&o, &b)| {
            let o = o as f32;
            (o + amount * (o - b as f32)).round().clamp(0.0, 255.0) as u8
        })
        .collect();

    GrayImage::from_raw(width, height, out).expect("unsharp: same dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsharp_mask_preserves_dimensions() {
        let img = GrayImage::new(64, 64);
        let out = unsharp_mask(&img, 1.0, 0.5);
        assert_eq!(out.dimensions(), (64, 64));
    }

    #[test]
    fn test_unsharp_mask_uniform_image_unchanged() {
        // A uniform image has no edges, so unsharp mask should not change it
        let mut img = GrayImage::new(8, 8);
        for px in img.pixels_mut() {
            *px = image::Luma([128u8]);
        }
        let out = unsharp_mask(&img, 1.0, 0.5);
        for (x, y, px) in out.enumerate_pixels() {
            // Allow ±2 for floating point rounding
            let diff = (px[0] as i32 - 128i32).abs();
            assert!(diff <= 2, "Pixel ({x},{y}) changed unexpectedly: {}", px[0]);
        }
    }
}
