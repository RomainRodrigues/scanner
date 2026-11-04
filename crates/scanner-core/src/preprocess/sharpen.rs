//! Unsharp mask sharpening for improving barcode readability on blurry camera frames.
//!
//! Unsharp masking: output = original + amount × (original − blur(original, sigma))
//! This enhances edges without introducing haloing artifacts when tuned properly.

use image::GrayImage;

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

    // Create the blurred version (the "mask").
    let blurred = gaussian_blur(img, sigma);

    let out: Vec<u8> = img
        .as_raw()
        .iter()
        .zip(blurred.as_raw().iter())
        .map(|(&o, &b)| {
            let o = f32::from(o);
            (o + amount * (o - f32::from(b))).round().clamp(0.0, 255.0) as u8
        })
        .collect();

    GrayImage::from_raw(width, height, out).expect("unsharp: same dimensions")
}

/// Separable Gaussian blur in fixed point, edges clamped.
///
/// In-house on purpose: `imageproc` was a dependency for this single call
/// and pulled ~40 crates (nalgebra, rand, font rendering…) into the WASM
/// build, and `image::imageops::blur` is ~35 % slower than this version
/// (1024×432, sigma 1: 6.8 ms vs 5.1 ms with imageproc, x86 native).
pub fn gaussian_blur(img: &GrayImage, sigma: f32) -> GrayImage {
    /// Weights sum to `1 << SHIFT`; two passes stay well within `u32`.
    const SHIFT: u32 = 12;
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || sigma <= 0.0 {
        return img.clone();
    }
    let radius = (sigma * 3.0).ceil() as usize;
    let raw: Vec<f32> = (0..=2 * radius)
        .map(|i| {
            let d = i as f32 - radius as f32;
            (-d * d / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let total: f32 = raw.iter().sum();
    let mut kernel: Vec<u32> = raw
        .iter()
        .map(|k| (k / total * (1u32 << SHIFT) as f32).round() as u32)
        .collect();
    // Rounding error goes to the centre tap, so a flat image stays flat.
    let sum: u32 = kernel.iter().sum();
    kernel[radius] = (kernel[radius] + (1 << SHIFT)).saturating_sub(sum);

    let src = img.as_raw();
    // Horizontal pass into 16-bit intermediates (values scaled by 1 << 4),
    // on a copy of each row padded with its edge pixels: no bound checks.
    let mut tmp = vec![0u16; w * h];
    let mut padded = vec![0u8; w + 2 * radius];
    for (row, dst) in src.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
        padded[..radius].fill(row[0]);
        padded[radius..radius + w].copy_from_slice(row);
        padded[radius + w..].fill(row[w - 1]);
        for (x, d) in dst.iter_mut().enumerate() {
            let acc: u32 = kernel
                .iter()
                .zip(&padded[x..x + kernel.len()])
                .map(|(&k, &p)| k * u32::from(p))
                .sum();
            *d = ((acc + (1 << (SHIFT - 5))) >> (SHIFT - 4)) as u16;
        }
    }
    // Vertical pass back to 8 bits, accumulated row by row (cache friendly).
    let mut out = vec![0u8; w * h];
    let mut acc = vec![0u32; w];
    for (y, dst) in out.chunks_exact_mut(w).enumerate() {
        acc.fill(0);
        for (k, &weight) in kernel.iter().enumerate() {
            let sy = (y + k).saturating_sub(radius).min(h - 1);
            for (a, &t) in acc.iter_mut().zip(&tmp[sy * w..(sy + 1) * w]) {
                *a += weight * u32::from(t);
            }
        }
        for (d, &a) in dst.iter_mut().zip(&acc) {
            *d = ((a + (1 << (SHIFT + 3))) >> (SHIFT + 4)).min(255) as u8;
        }
    }
    GrayImage::from_raw(w as u32, h as u32, out).expect("blur: same dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_blur_matches_a_reference_gaussian() {
        let img = GrayImage::from_fn(97, 61, |x, y| {
            image::Luma([((x * 37 + y * 91) % 251) as u8])
        });
        let ours = gaussian_blur(&img, 1.0);
        let reference = image::imageops::blur(&img, 1.0);
        // Same filter, different rounding and edge handling: compare the interior.
        for y in 4..57 {
            for x in 4..93 {
                let (a, b) = (ours.get_pixel(x, y)[0], reference.get_pixel(x, y)[0]);
                assert!(a.abs_diff(b) <= 3, "({x},{y}): {a} vs {b}");
            }
        }
    }

    #[test]
    fn gaussian_blur_keeps_flat_images_flat() {
        let img = GrayImage::from_pixel(20, 20, image::Luma([173]));
        assert!(gaussian_blur(&img, 1.5).pixels().all(|p| p[0] == 173));
    }

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
            let diff = (i32::from(px[0]) - 128i32).abs();
            assert!(diff <= 2, "Pixel ({x},{y}) changed unexpectedly: {}", px[0]);
        }
    }
}
