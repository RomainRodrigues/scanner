//! Image preprocessing pipeline for robust barcode scanning.
//!
//! The pipeline is designed to be layered — each step is applied only when needed:
//!
//! 1. Grayscale conversion (always)
//! 2. ROI crop (if configured)
//! 3. Fast scan attempt (no further preprocessing)
//! 4. If failed → CLAHE contrast enhancement
//! 5. If failed → Unsharp mask sharpening
//! 6. If failed → Sauvola adaptive binarization
//! 7. Retry scan on enhanced image

pub mod binarize;
pub mod contrast;
pub mod roi;
pub mod sharpen;

use image::GrayImage;

pub use binarize::sauvola_binarize;
pub use contrast::clahe;
pub use roi::crop_roi;
pub use sharpen::unsharp_mask;

/// Apply the full preprocessing pipeline to a grayscale image.
///
/// Steps: CLAHE → Unsharp Mask → Sauvola Binarization
///
/// This is the "heavy" path used only when the fast scan fails.
pub fn full_pipeline(img: &GrayImage) -> GrayImage {
    let enhanced = clahe(img, 8, 3.0);
    let sharpened = unsharp_mask(&enhanced, 1.0, 0.5);
    sauvola_binarize(&sharpened, 15, 0.15)
}

/// Apply only contrast enhancement (lighter pipeline for moderately difficult images).
pub fn contrast_pipeline(img: &GrayImage) -> GrayImage {
    clahe(img, 8, 3.0)
}
