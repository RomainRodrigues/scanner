//! The main Scanner — orchestrates preprocessing and decoding for static images / single frames.
//!
//! For real-time camera scanning use `ScanSession` instead: it runs the same
//! strategies but spreads the expensive ones over successive frames.

use crate::config::ScannerConfig;
use crate::decode::{verify, Effort, RxingDecoder};
use crate::error::ScanError;
use crate::frame::{Frame, PixelFormat};
use crate::preprocess::{self, roi::crop_roi_owned, LinearScale};
use crate::result::{BarcodeFormat, Point, ScanResult};
use image::{DynamicImage, GrayImage};
use log::debug;
use web_time::Instant;

/// One way of looking at a frame. The scanner tries several of them, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Raw grayscale, rxing default effort. Solves the vast majority of frames.
    Fast,
    /// Raw grayscale, rxing `TryHarder` (more scan lines, rotated 1D codes).
    Hard,
    /// Inverted grayscale (light code on dark background).
    Inverted,
    /// CLAHE contrast enhancement (low light, glare, low contrast print).
    Contrast,
    /// CLAHE + unsharp mask + Sauvola binarization (shadows, blur).
    Binarized,
    /// Inverted + binarized.
    InvertedBinarized,
    /// Smoothing along the bars + row-wise local thresholding, for 1D codes
    /// under uneven lighting or with grainy print (laser-engraved tags), at
    /// two window sizes. Only enabled when 1D formats are requested.
    Linear(LinearScale),
}

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

    /// The configuration this scanner was built with.
    pub fn config(&self) -> &ScannerConfig {
        &self.config
    }

    /// Scan a raw pixel buffer, trying every enabled strategy until one succeeds.
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
        self.scan_luma_owned(frame.to_luma())
    }

    /// Scan an image file from disk.
    ///
    /// Supports PNG, JPEG, GIF and WebP.
    #[cfg(feature = "file-io")]
    pub fn scan_file(&self, path: &str) -> Result<Vec<ScanResult>, ScanError> {
        let img = image::open(path).map_err(|e| ScanError::ImageLoad {
            path: path.to_string(),
            source: e,
        })?;
        self.scan_dynamic_image(&img)
    }

    /// Scan from an already-loaded `DynamicImage`.
    pub fn scan_dynamic_image(&self, img: &DynamicImage) -> Result<Vec<ScanResult>, ScanError> {
        self.scan_luma_owned(img.to_luma8())
    }

    // ── Building blocks (shared with ScanSession) ────────────────────────────

    /// Fallback strategies enabled by the configuration, in order of cost.
    /// [`Strategy::Fast`] is not included: it always runs first.
    pub fn fallback_strategies(&self) -> Vec<Strategy> {
        let mut s = vec![Strategy::Hard];
        if self.config.try_invert {
            s.push(Strategy::Inverted);
        }
        if self.config.preprocessing {
            s.push(Strategy::Contrast);
            s.push(Strategy::Binarized);
        }
        if self.config.preprocessing && self.wants_linear() {
            s.push(Strategy::Linear(LinearScale::Fine));
            s.push(Strategy::Linear(LinearScale::Coarse));
        }
        if self.config.try_invert && self.config.preprocessing {
            s.push(Strategy::InvertedBinarized);
        }
        s
    }

    /// `true` when at least one 1D format may be decoded.
    fn wants_linear(&self) -> bool {
        match &self.config.formats {
            None => true,
            Some(f) => f.is_empty() || f.iter().any(BarcodeFormat::is_linear),
        }
    }

    /// Run a single strategy on an (already cropped) grayscale image.
    pub fn run_strategy(
        &self,
        img: &GrayImage,
        strategy: Strategy,
    ) -> Result<Vec<ScanResult>, ScanError> {
        let (results, enhanced) = self.decode_strategy(img, strategy)?;
        // The Linear strategy is verified on its normalised image, where the
        // lighting gradient (and e.g. the edge of a tag next to the code) no
        // longer looks like bars.
        let reference = enhanced.as_ref().unwrap_or(img);
        // Drop 1D reads that are not surrounded by a quiet zone (textures).
        let results: Vec<ScanResult> = results
            .into_iter()
            .filter(|r| verify::plausible_linear(reference, r))
            .collect();
        if results.is_empty() {
            Err(ScanError::NotFound)
        } else {
            Ok(results)
        }
    }

    /// Decode with one strategy, without verification. Also returns the image
    /// the results must be verified on, when it is not `img`.
    fn decode_strategy(
        &self,
        img: &GrayImage,
        strategy: Strategy,
    ) -> Result<(Vec<ScanResult>, Option<GrayImage>), ScanError> {
        let results = match strategy {
            Strategy::Linear(scale) => {
                let enhanced = preprocess::linear_enhance(img, scale);
                let results = self.decode_linear(&enhanced)?;
                return Ok((results, Some(enhanced)));
            }
            Strategy::Fast => self.decoder.decode_with(img, Effort::Fast),
            Strategy::Hard => self.decoder.decode_with(img, Effort::Hard),
            Strategy::Inverted => self.decoder.decode_inverted(img, Effort::Hard),
            Strategy::Contrast => self
                .decoder
                .decode_with(&preprocess::contrast_pipeline(img), Effort::Fast),
            Strategy::Binarized => self
                .decoder
                .decode_with(&preprocess::full_pipeline(img), Effort::Hard),
            Strategy::InvertedBinarized => self
                .decoder
                .decode_inverted(&preprocess::full_pipeline(img), Effort::Hard),
        }?;
        Ok((results, None))
    }

    /// Decode a [`preprocess::linear_enhance`] output: 1D readers only, on one
    /// row every [`row_step`](preprocess::linear::row_step) rows (the blur made
    /// the others redundant). Points are returned in the coordinates of `enhanced`.
    fn decode_linear(&self, enhanced: &GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let step = preprocess::linear::row_step(enhanced.height());
        let rows = preprocess::linear::sample_rows(enhanced, step);
        let mut results = self.decoder.decode_rows(&rows)?;
        for p in results.iter_mut().flat_map(|r| r.points.iter_mut()) {
            p.y *= step as f32;
        }
        Ok(results)
    }

    /// Convert a frame to grayscale and crop it to the ROI.
    /// Returns the image and the ROI top-left offset in frame pixels.
    pub(crate) fn prepare(&self, frame: &Frame<'_>) -> (GrayImage, (u32, u32)) {
        crop_roi_owned(frame.to_luma(), self.config.roi.as_ref())
    }

    // ── Internal pipeline ────────────────────────────────────────────────────

    /// Core scan logic on a grayscale image.
    ///
    /// Pipeline:
    /// 1. ROI crop
    /// 2. Fast scan, then every fallback strategy
    /// 3. Rotation retries (if configured and still failed)
    fn scan_luma_owned(&self, luma: GrayImage) -> Result<Vec<ScanResult>, ScanError> {
        let t0 = Instant::now();
        let (cropped, origin) = crop_roi_owned(luma, self.config.roi.as_ref());
        let offset = |results: Vec<ScanResult>| offset_results(results, origin);

        let strategies = std::iter::once(Strategy::Fast).chain(self.fallback_strategies());
        for strategy in strategies {
            if let Ok(results) = self.run_strategy(&cropped, strategy) {
                debug!("{strategy:?} scan success in {:?}", t0.elapsed());
                return Ok(offset(results));
            }
        }

        if self.config.try_rotations {
            let (w, h) = cropped.dimensions();
            for angle in [90u32, 180, 270] {
                let rotated = rotate_luma(&cropped, angle);
                for strategy in [Strategy::Hard, Strategy::Binarized] {
                    if strategy == Strategy::Binarized && !self.config.preprocessing {
                        continue;
                    }
                    if let Ok(mut results) = self.run_strategy(&rotated, strategy) {
                        debug!(
                            "Rotation {angle}° {strategy:?} success in {:?}",
                            t0.elapsed()
                        );
                        for r in &mut results {
                            r.map_points(|p| unrotate_point(p, angle, w, h));
                        }
                        return Ok(offset(results));
                    }
                }
            }
        }

        debug!("All scan attempts failed in {:?}", t0.elapsed());
        Err(ScanError::NotFound)
    }
}

/// Translate result points from ROI space back to full-frame space.
pub(crate) fn offset_results(
    mut results: Vec<ScanResult>,
    (ox, oy): (u32, u32),
) -> Vec<ScanResult> {
    if ox != 0 || oy != 0 {
        for r in &mut results {
            r.map_points(|p| Point::new(p.x + ox as f32, p.y + oy as f32));
        }
    }
    results
}

/// Rotate a grayscale image clockwise by a multiple of 90°.
fn rotate_luma(img: &GrayImage, angle_deg: u32) -> GrayImage {
    match angle_deg % 360 {
        90 => image::imageops::rotate90(img),
        180 => image::imageops::rotate180(img),
        270 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}

/// Map a point found in an image rotated clockwise by `angle_deg` back to the
/// original image of size `w`×`h`.
fn unrotate_point(p: Point, angle_deg: u32, w: u32, h: u32) -> Point {
    let (w, h) = (w as f32, h as f32);
    match angle_deg % 360 {
        // rotate90: (x, y) -> (h - 1 - y, x)
        90 => Point::new(p.y, h - 1.0 - p.x),
        180 => Point::new(w - 1.0 - p.x, h - 1.0 - p.y),
        // rotate270: (x, y) -> (y, w - 1 - x)
        270 => Point::new(w - 1.0 - p.y, p.x),
        _ => p,
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

    #[test]
    fn test_unrotate_roundtrip() {
        // A marker pixel at (3, 1) in a 10x5 image must map back after rotation.
        let (w, h) = (10u32, 5u32);
        let mut img = GrayImage::new(w, h);
        img.put_pixel(3, 1, image::Luma([255]));
        for angle in [90, 180, 270] {
            let rotated = rotate_luma(&img, angle);
            let (rx, ry) = rotated
                .enumerate_pixels()
                .find(|(_, _, p)| p[0] == 255)
                .map(|(x, y, _)| (x, y))
                .unwrap();
            let back = unrotate_point(Point::new(rx as f32, ry as f32), angle, w, h);
            assert_eq!((back.x, back.y), (3.0, 1.0), "angle {angle}");
        }
    }

    #[test]
    fn test_fallback_strategies_follow_config() {
        let s = Scanner::new(ScannerConfig {
            preprocessing: false,
            try_invert: false,
            ..ScannerConfig::default()
        });
        assert_eq!(s.fallback_strategies(), vec![Strategy::Hard]);
        assert_eq!(Scanner::with_defaults().fallback_strategies().len(), 7);
        let qr_only = Scanner::new(ScannerConfig {
            formats: Some(vec![BarcodeFormat::QrCode]),
            ..ScannerConfig::default()
        });
        assert!(!qr_only
            .fallback_strategies()
            .iter()
            .any(|s| matches!(s, Strategy::Linear(_))));
    }
}
