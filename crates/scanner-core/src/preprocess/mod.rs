//! Image preprocessing used by the fallback decoding strategies.
//!
//! None of this runs on the fast path: see [`Strategy`](crate::Strategy) for
//! when each pipeline is used.
//!
//! - [`contrast_pipeline`]: CLAHE, for low light / glare / faded print.
//! - [`full_pipeline`]: CLAHE → unsharp mask → Sauvola binarization, for
//!   shadows and blur.
//! - [`linear_enhance`]: smoothing along the bars + local contrast
//!   normalisation, for 1D codes under uneven lighting or grainy print.

pub mod binarize;
pub mod contrast;
pub mod linear;
pub mod roi;
pub mod sharpen;

use image::GrayImage;

pub use binarize::sauvola_binarize;
pub use contrast::clahe;
pub use linear::{linear_enhance, LinearScale};
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
