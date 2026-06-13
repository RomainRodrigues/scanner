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

    let mut output = GrayImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels() {
        let original = pixel[0] as f32;
        let blur = blurred.get_pixel(x, y)[0] as f32;

        // Unsharp mask formula
        let sharpened = original + amount * (original - blur);
        let val = sharpened.round().clamp(0.0, 255.0) as u8;
        output.put_pixel(x, y, image::Luma([val]));
    }

    output
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
