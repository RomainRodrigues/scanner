//! The main Scanner — orchestrates preprocessing and decoding for static images / single frames.
//!
//! For real-time camera scanning use `ScanSession` instead.

use crate::config::ScannerConfig;
use crate::decode::RxingDecoder;
use crate::error::ScanError;
use crate::frame::{Frame, PixelFormat};
use crate::preprocess::{self, roi::crop_roi};
use crate::result::ScanResult;
use image::DynamicImage;
use log::debug;
use web_time::Instant;

/// High-level scanner for one-shot image scanning.
///
/// # Examples
/// ```no_run
/// use scanner_core::{Scanner, ScannerConfig};
///
/// let scanner = Scanner::with_defaults();
/// match scanner.scan_file("test.png") {
///     Ok(results) => println!("Found: {}", results[0].text),
///     Err(e) => eprintln!("Error: {e}"),
/// }
/// ```
pub struct Scanner {
    config: ScannerConfig,
    decoder: RxingDecoder,
}

impl Scanner {
    /// Create a scanner with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ScannerConfig::default())
    }

    /// Create a scanner with a custom configuration.
    pub fn new(config: ScannerConfig) -> Self {
        let decoder = RxingDecoder::new(&config);
        Self { config, decoder }
    }

    /// Scan a raw pixel buffer.
    ///
    /// # Parameters
    /// - `data`: Raw pixel bytes
    /// - `width` / `height`: Frame dimensions in pixels
    /// - `format`: Pixel format of the data
    pub fn scan_frame(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let frame = Frame::new(data, width, height, format)?;
        self.scan_frame_internal(&frame)
    }

    /// Scan an image file from disk.
    ///
    /// Supports PNG, JPEG, GIF, WebP, and any format supported by the `image` crate.
    pub fn scan_file(&self, path: &str) -> Result<Vec<ScanResult>, ScanError> {
        let img = image::open(path).map_err(|e| ScanError::ImageLoad {
            path: path.to_string(),
            source: e,
        })?;
        self.scan_dynamic_image(&img)
    }

    /// Scan from an already-loaded `DynamicImage`.
    pub fn scan_dynamic_image(&self, img: &DynamicImage) -> Result<Vec<ScanResult>, ScanError> {
        let luma = img.to_luma8();
        self.scan_luma(&luma)
    }

    // ── Internal pipeline ────────────────────────────────────────────────────

    fn scan_frame_internal(&self, frame: &Frame<'_>) -> Result<Vec<ScanResult>, ScanError> {
        let luma = frame.to_luma();
        self.scan_luma(&luma)
    }

    /// Core scan logic on a grayscale image.
    ///
    /// Pipeline:
    /// 1. ROI crop
    /// 2. Fast scan (no preprocessing)
    /// 3. Preprocessing + scan (if fast scan failed)
    /// 4. Rotation retries (if configured and still failed)
    /// 5. Invert retry (if configured and still failed)
    pub(crate) fn scan_luma(
        &self,
        luma: &image::GrayImage,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let t0 = Instant::now();

        // Step 1: ROI crop
        let cropped = crop_roi(luma, self.config.roi.as_ref());

        // Step 2: Fast scan — no preprocessing, works for clean images
        debug!("Attempting fast scan ({:?})", luma.dimensions());
        if let Ok(results) = self.decoder.decode(&cropped) {
            debug!("Fast scan success in {:?}", t0.elapsed());
            return Ok(results);
        }

        // Step 3: Preprocessing pipeline
        if self.config.preprocessing {
            debug!("Fast scan failed, applying contrast enhancement");
            let contrasted = preprocess::contrast_pipeline(&cropped);
            if let Ok(results) = self.decoder.decode(&contrasted) {
                debug!("Contrast scan success in {:?}", t0.elapsed());
                return Ok(results);
            }

            debug!("Contrast scan failed, applying full preprocessing pipeline");
            let preprocessed = preprocess::full_pipeline(&cropped);
            if let Ok(results) = self.decoder.decode(&preprocessed) {
                debug!("Full preprocessing scan success in {:?}", t0.elapsed());
                return Ok(results);
            }
        }

        // Step 4: Rotation retries
        if self.config.try_rotations {
            debug!("Trying rotations");
            for angle in [90u32, 180, 270] {
                let rotated = rotate_luma(&cropped, angle);
                if let Ok(results) = self.decoder.decode(&rotated) {
                    debug!("Rotation {angle}° scan success in {:?}", t0.elapsed());
                    return Ok(results);
                }
                if self.config.preprocessing {
                    let preprocessed = preprocess::full_pipeline(&rotated);
                    if let Ok(results) = self.decoder.decode(&preprocessed) {
                        debug!("Rotation {angle}° + preprocess success in {:?}", t0.elapsed());
                        return Ok(results);
                    }
                }
            }
        }

        // Step 5: Invert (white-on-black barcodes)
        if self.config.try_invert {
            debug!("Trying inversion");
            if let Ok(results) = self.decoder.decode_inverted(&cropped) {
                debug!("Invert scan success in {:?}", t0.elapsed());
                return Ok(results);
            }
            if self.config.preprocessing {
                let preprocessed = preprocess::full_pipeline(&cropped);
                if let Ok(results) = self.decoder.decode_inverted(&preprocessed) {
                    debug!("Invert + preprocess success in {:?}", t0.elapsed());
                    return Ok(results);
                }
            }
        }

        debug!("All scan attempts failed in {:?}", t0.elapsed());
        Err(ScanError::NotFound)
    }
}

/// Rotate a grayscale image by a multiple of 90°.
fn rotate_luma(img: &image::GrayImage, angle_deg: u32) -> image::GrayImage {
    match angle_deg % 360 {
        90 => image::imageops::rotate90(img),
        180 => image::imageops::rotate180(img),
        270 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_not_found_on_blank_image() {
        let scanner = Scanner::with_defaults();
        let data = vec![255u8; 100 * 100 * 4]; // White RGBA image
        let result = scanner.scan_frame(&data, 100, 100, PixelFormat::Rgba);
        assert!(matches!(result, Err(ScanError::NotFound)));
    }

    #[test]
    fn test_scanner_invalid_frame() {
        let scanner = Scanner::with_defaults();
        let data = vec![0u8; 10]; // Too small
        let result = scanner.scan_frame(&data, 100, 100, PixelFormat::Rgba);
        assert!(matches!(result, Err(ScanError::InvalidImage(_))));
    }
}
