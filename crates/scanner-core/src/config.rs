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
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }

    /// A centered ROI with the given size fraction.
    ///
    /// `size` is clamped to (0.0, 1.0]. E.g. 0.6 creates a centered 60%×60% ROI.
    pub fn centered(size: f32) -> Self {
        let size = size.clamp(0.01, 1.0);
        let offset = (1.0 - size) / 2.0;
        Self {
            x: offset,
            y: offset,
            width: size,
            height: size,
        }
    }

    /// Convert ROI fractions to pixel coordinates for a given image size.
    ///
    /// All outputs are guaranteed to fit within `[0, img_width)` and `[0, img_height)`.
    pub fn to_pixels(&self, img_width: u32, img_height: u32) -> (u32, u32, u32, u32) {
        // Clamp fractions to valid range before converting
        let x_frac = self.x.clamp(0.0, 1.0);
        let y_frac = self.y.clamp(0.0, 1.0);
        let w_frac = self.width.clamp(0.0, 1.0);
        let h_frac = self.height.clamp(0.0, 1.0);

        let x = (x_frac * img_width as f32) as u32;
        let y = (y_frac * img_height as f32) as u32;
        // Ensure w/h don't extend beyond the image boundary
        let w = (w_frac * img_width as f32).min((img_width.saturating_sub(x)) as f32) as u32;
        let h = (h_frac * img_height as f32).min((img_height.saturating_sub(y)) as f32) as u32;
        (x, y, w, h)
    }
}

/// Configuration for the scanner and scan sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScannerConfig {
    /// Barcode formats to look for. `None` means try all supported formats.
    ///
    /// For best performance, specify only the formats you need: every extra
    /// format is an extra decoder run on each frame.
    pub formats: Option<Vec<BarcodeFormat>>,

    /// Enable the adaptive preprocessing strategies (CLAHE contrast boost and
    /// CLAHE + unsharp mask + Sauvola binarization).
    ///
    /// They only run after the fast path failed. In a [`ScanSession`](crate::ScanSession)
    /// they are spread over successive frames, so their cost is amortised.
    ///
    /// Default: `true`
    pub preprocessing: bool,

    /// If the first scan attempt fails, retry with 90°, 180°, and 270° rotations.
    ///
    /// Useful for images that are known to be rotated (e.g., uploaded photos).
    /// Ignored by `ScanSession` (camera frames are scanned in "try harder" mode
    /// every few frames instead, which already covers rotated 1D codes).
    ///
    /// Default: `false` (set to `true` for static image scanning)
    pub try_rotations: bool,

    /// Also try to decode an inverted image (white-on-black barcodes).
    ///
    /// Default: `true`
    pub try_invert: bool,

    /// Maximum number of barcodes to detect per frame.
    ///
    /// `1` uses the single-code fast path. Values `> 1` enable rxing's
    /// multi-barcode reader, which is noticeably slower.
    ///
    /// Default: `1`
    pub max_results: usize,

    /// Region of Interest — limits scanning to a sub-area of the frame.
    ///
    /// `None` means scan the entire frame. Result points are always reported
    /// in the coordinate system of the **full** frame.
    ///
    /// Default: `None` (full frame)
    pub roi: Option<Roi>,

    /// Cooldown period (in milliseconds) before the same barcode content
    /// is reported again by a `ScanSession`.
    ///
    /// Default: `2000` (2 seconds)
    pub dedup_cooldown_ms: u64,

    /// Agreeing reads required within ~1 s before a `ScanSession` reports a
    /// 1D code. Weak 1D formats (Code 39/93, ITF, Codabar, UPC-E, EAN-8) need
    /// one more; 2D codes (error-corrected) are reported on the first read.
    /// Protects against false positives on noise and textures. `1` disables
    /// it for strong formats.
    ///
    /// Default: `2`
    pub linear_confirmations: u32,

    /// Enable the adaptive frame throttle in `ScanSession`.
    ///
    /// Useful when frames are pushed synchronously on the UI thread. Disable
    /// it when the caller already applies back-pressure (e.g. a Web Worker
    /// that only receives a new frame once the previous one is processed).
    ///
    /// Default: `true`
    pub adaptive_throttle: bool,
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
            linear_confirmations: 2,
            adaptive_throttle: true,
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
    /// - Up to 10 codes per image
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
