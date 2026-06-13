//! ROI (Region of Interest) cropping.

use crate::config::Roi;
use image::GrayImage;

/// Crop a grayscale image to the given ROI.
///
/// Returns the full image unchanged if the ROI is `None`.
pub fn crop_roi(img: &GrayImage, roi: Option<&Roi>) -> GrayImage {
    match roi {
        None => img.clone(),
        Some(roi) => {
            let (img_w, img_h) = img.dimensions();
            let (x, y, w, h) = roi.to_pixels(img_w, img_h);

            if w == 0 || h == 0 {
                return img.clone();
            }

            // Safe crop: clamp to image bounds
            let x = x.min(img_w.saturating_sub(1));
            let y = y.min(img_h.saturating_sub(1));
            let w = w.min(img_w - x);
            let h = h.min(img_h - y);

            image::imageops::crop_imm(img, x, y, w, h).to_image()
        }
    }
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
}
