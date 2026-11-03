//! ROI (Region of Interest) cropping.

use crate::config::Roi;
use image::GrayImage;

/// Crop a grayscale image to the given ROI.
///
/// Returns the full image unchanged if the ROI is `None`.
pub fn crop_roi(img: &GrayImage, roi: Option<&Roi>) -> GrayImage {
    match roi_rect(img, roi) {
        None => img.clone(),
        Some((x, y, w, h)) => image::imageops::crop_imm(img, x, y, w, h).to_image(),
    }
}

/// Crop an owned image to the ROI, returning the crop and its top-left offset
/// in the original image. Avoids any copy when there is no ROI.
pub fn crop_roi_owned(img: GrayImage, roi: Option<&Roi>) -> (GrayImage, (u32, u32)) {
    match roi_rect(&img, roi) {
        None => (img, (0, 0)),
        Some((x, y, w, h)) => (
            image::imageops::crop_imm(&img, x, y, w, h).to_image(),
            (x, y),
        ),
    }
}

/// Pixel rectangle of the ROI, or `None` when the whole image should be used.
fn roi_rect(img: &GrayImage, roi: Option<&Roi>) -> Option<(u32, u32, u32, u32)> {
    let roi = roi?;
    let (img_w, img_h) = img.dimensions();
    let (x, y, w, h) = roi.to_pixels(img_w, img_h);
    if w == 0 || h == 0 || (x == 0 && y == 0 && w == img_w && h == img_h) {
        return None;
    }
    // Safe crop: clamp to image bounds
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let w = w.min(img_w - x);
    let h = h.min(img_h - y);
    Some((x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Roi;

    #[test]
    fn test_crop_roi_centered() {
        let img = GrayImage::new(100, 100);
        let roi = Roi::centered(0.5);
        let cropped = crop_roi(&img, Some(&roi));
        assert_eq!(cropped.dimensions(), (50, 50));
    }

    #[test]
    fn test_crop_roi_none() {
        let img = GrayImage::new(100, 100);
        let cropped = crop_roi(&img, None);
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_roi_full() {
        let img = GrayImage::new(100, 100);
        let cropped = crop_roi(&img, Some(&Roi::full()));
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_roi_owned_offset() {
        let img = GrayImage::new(100, 200);
        let (cropped, offset) = crop_roi_owned(img, Some(&Roi::centered(0.5)));
        assert_eq!(cropped.dimensions(), (50, 100));
        assert_eq!(offset, (25, 50));
    }
}
