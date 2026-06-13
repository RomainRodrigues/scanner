//! Scanner configuration types.

use crate::result::BarcodeFormat;
use serde::{Deserialize, Serialize};

/// Region of Interest — restricts scanning to a sub-area of the frame.
///
/// All values are in the range [0.0, 1.0] as fractions of image dimensions.
///
/// # Example
/// A centered 60%×60% ROI:
/// ```
/// use scanner_core::Roi;
/// let roi = Roi { x: 0.2, y: 0.2, width: 0.6, height: 0.6 };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Roi {
    /// Left edge as fraction of image width (0.0 = left, 1.0 = right)
    pub x: f32,
    /// Top edge as fraction of image height (0.0 = top, 1.0 = bottom)
    pub y: f32,
    /// Width as fraction of image width
    pub width: f32,
    /// Height as fraction of image height
    pub height: f32,
}

impl Roi {
    /// A full-frame ROI (no cropping).
    pub fn full() -> Self {
        Self { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }

    /// A centered ROI with the given size fraction.
    ///
    /// `size` must be in (0.0, 1.0]. E.g. 0.6 creates a centered 60%×60% ROI.
    pub fn centered(size: f32) -> Self {
        let offset = (1.0 - size) / 2.0;
        Self { x: offset, y: offset, width: size, height: size }
    }

    /// Convert ROI fractions to pixel coordinates for a given image size.
    pub fn to_pixels(&self, img_width: u32, img_height: u32) -> (u32, u32, u32, u32) {
        let x = (self.x * img_width as f32) as u32;
        let y = (self.y * img_height as f32) as u32;
        let w = (self.width * img_width as f32).min((img_width - x) as f32) as u32;
        let h = (self.height * img_height as f32).min((img_height - y) as f32) as u32;
        (x, y, w, h)
    }
}

/// Configuration for the scanner and scan sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    /// Barcode formats to look for. `None` means try all supported formats.
    ///
    /// For best performance, specify only the formats you need.
    pub formats: Option<Vec<BarcodeFormat>>,

    /// Enable the adaptive preprocessing pipeline (CLAHE + sharpening + Sauvola binarization).
    ///
    /// The scanner always attempts a fast scan first. Preprocessing is applied
    /// only if the fast scan fails, so enabling this has low cost for clean images.
    ///
    /// Default: `true`
    pub preprocessing: bool,

    /// If the first scan attempt fails, retry with 90°, 180°, and 270° rotations.
    ///
    /// Useful for images that are known to be rotated (e.g., uploaded photos).
    /// For real-time camera scanning this is rarely needed.
    ///
    /// Default: `false` (set to `true` for static image scanning)
    pub try_rotations: bool,

    /// If all scan attempts fail, retry with an inverted image (white-on-black barcodes).
    ///
    /// Default: `true`
    pub try_invert: bool,

    /// Maximum number of barcodes to detect per frame.
    ///
    /// Default: `1` (set higher for multi-code images)
    pub max_results: usize,

    /// Region of Interest — limits scanning to a sub-area of the frame.
    ///
    /// `None` means scan the entire frame.
    /// For real-time camera scanning, a centered ROI (e.g., 60% of the frame)
    /// dramatically improves performance.
    ///
    /// Default: `None` (full frame)
    pub roi: Option<Roi>,

    /// Cooldown period (in milliseconds) before the same barcode content
    /// is reported again by a `ScanSession`.
    ///
    /// Default: `2000` (2 seconds)
    pub dedup_cooldown_ms: u64,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            formats: None,
            preprocessing: true,
            try_rotations: false,
            try_invert: true,
            max_results: 1,
            roi: None,
            dedup_cooldown_ms: 2000,
        }
    }
}

impl ScannerConfig {
    /// Configuration optimized for real-time camera scanning.
    ///
    /// - Centered 70% ROI (reduces pixel processing by ~50%)
    /// - No rotation retries (camera handles physical rotation)
    /// - Invert retry enabled
    /// - 2 second dedup cooldown
    pub fn realtime() -> Self {
        Self {
            roi: Some(Roi::centered(0.7)),
            try_rotations: false,
            ..Self::default()
        }
    }

    /// Configuration optimized for scanning static images from disk.
    ///
    /// - Full frame
    /// - Rotation retries enabled
    /// - More preprocessing
    pub fn image_file() -> Self {
        Self {
            try_rotations: true,
            max_results: 10,
            ..Self::default()
        }
    }

    /// Configuration for QR code only scanning (faster).
    pub fn qr_only() -> Self {
        Self {
            formats: Some(vec![BarcodeFormat::QrCode, BarcodeFormat::MicroQr]),
            roi: Some(Roi::centered(0.7)),
            ..Self::default()
        }
    }
}
