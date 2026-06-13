//! Node.js native bindings for scanner-core.
//!
//! Exposes the QR/barcode scanning engine to JavaScript via napi-rs.
//!
//! # JavaScript API
//!
//! ```javascript
//! const { Scanner, ScanSession, BarcodeFormat, PixelFormat } = require('./index');
//!
//! // Static image scan
//! const scanner = new Scanner();
//! const results = await scanner.scanFile('./qrcode.png');
//! // results: [{ text, format, rawBytes, points, confidence, orientation, timestampMs }]
//!
//! // Buffer scan (e.g. from sharp)
//! const results = scanner.scanBuffer(buffer, width, height, PixelFormat.Rgba);
//!
//! // Real-time session
//! const session = new ScanSession({ roi: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 } });
//! // In your camera loop:
//! const result = session.processFrame(rgbaBuffer, width, height);
//! if (result) console.log('Scanned:', result[0].text);
//! ```

#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;
use scanner_core::{
    PixelFormat as CorePixelFormat, ScanSession as CoreScanSession,
    ScannerConfig, Scanner as CoreScanner,
};
use scanner_core::config::Roi as CoreRoi;
use scanner_core::result::BarcodeFormat as CoreBarcodeFormat;

// ── Type conversions ──────────────────────────────────────────────────────────

/// Barcode format constants exposed to JavaScript.
#[napi]
pub enum BarcodeFormat {
    QrCode,
    MicroQr,
    DataMatrix,
    Aztec,
    Pdf417,
    Code128,
    Code39,
    Code93,
    Ean13,
    Ean8,
    UpcA,
    UpcE,
    Itf,
    Codabar,
    Unknown,
}

impl From<CoreBarcodeFormat> for BarcodeFormat {
    fn from(f: CoreBarcodeFormat) -> Self {
        match f {
            CoreBarcodeFormat::QrCode => BarcodeFormat::QrCode,
            CoreBarcodeFormat::MicroQr => BarcodeFormat::MicroQr,
            CoreBarcodeFormat::DataMatrix => BarcodeFormat::DataMatrix,
            CoreBarcodeFormat::Aztec => BarcodeFormat::Aztec,
            CoreBarcodeFormat::Pdf417 => BarcodeFormat::Pdf417,
            CoreBarcodeFormat::Code128 => BarcodeFormat::Code128,
            CoreBarcodeFormat::Code39 => BarcodeFormat::Code39,
            CoreBarcodeFormat::Code93 => BarcodeFormat::Code93,
            CoreBarcodeFormat::Ean13 => BarcodeFormat::Ean13,
            CoreBarcodeFormat::Ean8 => BarcodeFormat::Ean8,
            CoreBarcodeFormat::UpcA => BarcodeFormat::UpcA,
            CoreBarcodeFormat::UpcE => BarcodeFormat::UpcE,
            CoreBarcodeFormat::Itf => BarcodeFormat::Itf,
            CoreBarcodeFormat::Codabar => BarcodeFormat::Codabar,
            CoreBarcodeFormat::Unknown => BarcodeFormat::Unknown,
        }
    }
}

impl From<BarcodeFormat> for CoreBarcodeFormat {
    fn from(f: BarcodeFormat) -> Self {
        match f {
            BarcodeFormat::QrCode => CoreBarcodeFormat::QrCode,
            BarcodeFormat::MicroQr => CoreBarcodeFormat::MicroQr,
            BarcodeFormat::DataMatrix => CoreBarcodeFormat::DataMatrix,
            BarcodeFormat::Aztec => CoreBarcodeFormat::Aztec,
            BarcodeFormat::Pdf417 => CoreBarcodeFormat::Pdf417,
            BarcodeFormat::Code128 => CoreBarcodeFormat::Code128,
            BarcodeFormat::Code39 => CoreBarcodeFormat::Code39,
            BarcodeFormat::Code93 => CoreBarcodeFormat::Code93,
            BarcodeFormat::Ean13 => CoreBarcodeFormat::Ean13,
            BarcodeFormat::Ean8 => CoreBarcodeFormat::Ean8,
            BarcodeFormat::UpcA => CoreBarcodeFormat::UpcA,
            BarcodeFormat::UpcE => CoreBarcodeFormat::UpcE,
            BarcodeFormat::Itf => CoreBarcodeFormat::Itf,
            BarcodeFormat::Codabar => CoreBarcodeFormat::Codabar,
            BarcodeFormat::Unknown => CoreBarcodeFormat::Unknown,
        }
    }
}

/// Pixel format constants.
#[napi]
pub enum PixelFormat {
    /// 4 bytes per pixel: Red, Green, Blue, Alpha (browser ImageData)
    Rgba,
    /// 3 bytes per pixel: Red, Green, Blue
    Rgb,
    /// 1 byte per pixel: luminance (fastest)
    Luma,
    /// Android camera format (NV21)
    Nv21,
    /// Common webcam format (YUY2/YUYV)
    Yuy2,
}

impl From<PixelFormat> for CorePixelFormat {
    fn from(f: PixelFormat) -> Self {
        match f {
            PixelFormat::Rgba => CorePixelFormat::Rgba,
            PixelFormat::Rgb => CorePixelFormat::Rgb,
            PixelFormat::Luma => CorePixelFormat::Luma,
            PixelFormat::Nv21 => CorePixelFormat::Nv21,
            PixelFormat::Yuy2 => CorePixelFormat::Yuy2,
        }
    }
}

// ── JavaScript types ──────────────────────────────────────────────────────────

/// A 2D point (corner of a detected barcode).
#[napi(object)]
pub struct JsPoint {
    pub x: f64,
    pub y: f64,
}

/// A scan result returned to JavaScript.
#[napi(object)]
pub struct JsScanResult {
    /// Decoded text content of the barcode.
    pub text: String,
    /// Barcode format as a string (e.g. "qr_code", "ean_13").
    pub format: String,
    /// Raw bytes if available (e.g. for binary QR codes).
    pub raw_bytes: Option<Buffer>,
    /// Corner points of the detected barcode.
    pub points: Vec<JsPoint>,
    /// Confidence score [0.0, 1.0].
    pub confidence: f64,
    /// Detected rotation angle in degrees.
    pub orientation: f64,
    /// Unix timestamp in milliseconds when the scan completed.
    pub timestamp_ms: f64,
}

impl From<scanner_core::ScanResult> for JsScanResult {
    fn from(r: scanner_core::ScanResult) -> Self {
        Self {
            text: r.text,
            format: format!("{}", r.format),
            raw_bytes: r.raw_bytes.map(|b| Buffer::from(b.as_slice())),
            points: r.points.iter().map(|p| JsPoint { x: p.x as f64, y: p.y as f64 }).collect(),
            confidence: r.confidence as f64,
            orientation: r.orientation as f64,
            timestamp_ms: r.timestamp_ms as f64,
        }
    }
}

/// Scanner configuration object (all fields optional).
#[napi(object)]
#[derive(Default)]
pub struct JsScanConfig {
    /// Formats to scan. Null/undefined = scan all formats.
    pub formats: Option<Vec<BarcodeFormat>>,
    /// Enable adaptive preprocessing pipeline. Default: true.
    pub preprocessing: Option<bool>,
    /// Retry with 90°/180°/270° rotations. Default: false.
    pub try_rotations: Option<bool>,
    /// Retry with inverted image. Default: true.
    pub try_invert: Option<bool>,
    /// Max number of barcodes per image. Default: 1.
    pub max_results: Option<u32>,
    /// Region of Interest (fractions 0.0–1.0).
    pub roi: Option<JsRoi>,
    /// Cooldown before the same code is reported again (ms). Default: 2000.
    pub dedup_cooldown_ms: Option<f64>,
}

/// Region of Interest (all values are 0.0–1.0 fractions of image size).
#[napi(object)]
pub struct JsRoi {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn js_config_to_core(js: Option<JsScanConfig>) -> ScannerConfig {
    let js = js.unwrap_or_default();

    let formats = js.formats.map(|fmts| {
        fmts.into_iter().map(CoreBarcodeFormat::from).collect()
    });

    let roi = js.roi.map(|r| CoreRoi {
        x: r.x as f32,
        y: r.y as f32,
        width: r.width as f32,
        height: r.height as f32,
    });

    ScannerConfig {
        formats,
        preprocessing: js.preprocessing.unwrap_or(true),
        try_rotations: js.try_rotations.unwrap_or(false),
        try_invert: js.try_invert.unwrap_or(true),
        max_results: js.max_results.unwrap_or(1) as usize,
        roi,
        dedup_cooldown_ms: js.dedup_cooldown_ms.map(|v| v as u64).unwrap_or(2000),
    }
}

// ── Session stats ─────────────────────────────────────────────────────────────

/// Performance statistics for a ScanSession.
#[napi(object)]
pub struct JsSessionStats {
    pub frames_submitted: f64,
    pub frames_scanned: f64,
    pub frames_with_detection: f64,
    pub codes_detected: f64,
    pub avg_scan_time_ms: f64,
    pub last_scan_time_ms: f64,
}

// ── Scanner class ─────────────────────────────────────────────────────────────

/// High-performance QR code and barcode scanner.
///
/// Use `Scanner` for one-shot image scanning (files or buffers).
/// For real-time camera streams, use `ScanSession` instead.
///
/// @example
/// ```javascript
/// const { Scanner } = require('./index');
/// const scanner = new Scanner();
/// const results = await scanner.scanFile('./qrcode.png');
/// console.log(results[0].text);
/// ```
#[napi]
pub struct Scanner {
    inner: CoreScanner,
    config: ScannerConfig,
}

// Private helpers — must NOT be inside #[napi] impl
impl Scanner {
    fn clone_config(&self) -> ScannerConfig {
        self.config.clone()
    }
}

#[napi]
impl Scanner {
    /// Create a new scanner with optional configuration.
    #[napi(constructor)]
    pub fn new(config: Option<JsScanConfig>) -> Self {
        let core_config = js_config_to_core(config);
        Self {
            inner: CoreScanner::new(core_config.clone()),
            config: core_config,
        }
    }

    /// Scan an image file from disk (async, non-blocking).
    ///
    /// @param path - Absolute or relative path to the image file.
    /// @returns Array of scan results (empty array if nothing detected).
    #[napi]
    pub async fn scan_file(&self, path: String) -> Result<Vec<JsScanResult>> {
        let cfg = self.clone_config();
        let results = tokio::task::spawn_blocking(move || {
            CoreScanner::new(cfg).scan_file(&path)
        })
        .await
        .map_err(|e| napi::Error::from_reason(format!("Task error: {e}")))? ;

        match results {
            Ok(r) => Ok(r.into_iter().map(JsScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(napi::Error::from_reason(format!("{e}"))),
        }
    }

    /// Scan from a raw pixel buffer (synchronous).
    ///
    /// @param data - Raw pixel bytes.
    /// @param width - Image width in pixels.
    /// @param height - Image height in pixels.
    /// @param format - Pixel format (default: Rgba).
    #[napi]
    pub fn scan_buffer(
        &self,
        data: Buffer,
        width: u32,
        height: u32,
        format: Option<PixelFormat>,
    ) -> Result<Vec<JsScanResult>> {
        let pixel_fmt = format.map(CorePixelFormat::from).unwrap_or(CorePixelFormat::Rgba);
        match self.inner.scan_frame(&data, width, height, pixel_fmt) {
            Ok(r) => Ok(r.into_iter().map(JsScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(napi::Error::from_reason(format!("{e}"))),
        }
    }

    /// Scan from a raw pixel buffer (async, non-blocking).
    ///
    /// @param data - Raw pixel bytes.
    /// @param width - Image width in pixels.
    /// @param height - Image height in pixels.
    /// @param format - Pixel format (default: Rgba).
    #[napi]
    pub async fn scan_buffer_async(
        &self,
        data: Buffer,
        width: u32,
        height: u32,
        format: Option<PixelFormat>,
    ) -> Result<Vec<JsScanResult>> {
        let cfg = self.clone_config();
        let pixel_fmt = format.map(CorePixelFormat::from).unwrap_or(CorePixelFormat::Rgba);
        let data_vec: Vec<u8> = data.to_vec();

        let results = tokio::task::spawn_blocking(move || {
            CoreScanner::new(cfg).scan_frame(&data_vec, width, height, pixel_fmt)
        })
        .await
        .map_err(|e| napi::Error::from_reason(format!("Task error: {e}")))? ;

        match results {
            Ok(r) => Ok(r.into_iter().map(JsScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(napi::Error::from_reason(format!("{e}"))),
        }
    }
}


// ── ScanSession class ─────────────────────────────────────────────────────────

/// Stateful real-time scan session for camera streams.
///
/// Maintains state between frames to provide:
/// - Adaptive frame throttling (auto-skips frames when CPU is busy)
/// - Deduplication (same code not reported twice within cooldown period)
/// - Performance statistics
///
/// @example
/// ```javascript
/// const { ScanSession, PixelFormat } = require('./index');
///
/// const session = new ScanSession({
///   roi: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 },
///   dedupCooldownMs: 3000,
/// });
///
/// // In your camera frame callback:
/// function onFrame(rgbaBuffer, width, height) {
///   const results = session.processFrame(rgbaBuffer, width, height);
///   if (results && results.length > 0) {
///     console.log('Scanned:', results[0].text);
///   }
/// }
/// ```
#[napi]
pub struct ScanSession {
    inner: CoreScanSession,
}

#[napi]
impl ScanSession {
    /// Create a new scan session.
    #[napi(constructor)]
    pub fn new(config: Option<JsScanConfig>) -> Self {
        let core_config = js_config_to_core(config);
        Self { inner: CoreScanSession::new(core_config) }
    }

    /// Process a camera frame.
    ///
    /// Call this for every frame from the camera.
    /// Returns null if no new code was detected (or frame was skipped).
    /// Returns an array of ScanResult if new codes are detected.
    ///
    /// @param data - Raw pixel bytes (RGBA by default).
    /// @param width - Frame width in pixels.
    /// @param height - Frame height in pixels.
    /// @param format - Pixel format (default: PixelFormat.Rgba).
    #[napi]
    pub fn process_frame(
        &mut self,
        data: Buffer,
        width: u32,
        height: u32,
        format: Option<PixelFormat>,
    ) -> Option<Vec<JsScanResult>> {
        let pixel_fmt = format.map(CorePixelFormat::from).unwrap_or(CorePixelFormat::Rgba);
        self.inner
            .process_frame(&data, width, height, pixel_fmt)
            .map(|results| results.into_iter().map(JsScanResult::from).collect())
    }

    /// Get current session statistics.
    #[napi]
    pub fn get_stats(&self) -> JsSessionStats {
        let s = self.inner.stats();
        JsSessionStats {
            frames_submitted: s.frames_submitted as f64,
            frames_scanned: s.frames_scanned as f64,
            frames_with_detection: s.frames_with_detection as f64,
            codes_detected: s.codes_detected as f64,
            avg_scan_time_ms: s.avg_scan_time_ms,
            last_scan_time_ms: s.last_scan_time_ms,
        }
    }

    /// Reset the session (clears dedup state and stats).
    ///
    /// Call this when starting a new scanning context.
    #[napi]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Clear the cooldown for a specific barcode text.
    ///
    /// Use this to allow the same code to be re-scanned immediately
    /// after a user-initiated "scan again" action.
    #[napi]
    pub fn clear_cooldown(&mut self, text: String) {
        self.inner.clear_cooldown(&text);
    }

    /// Current frame skip interval (1 = scan every frame, 2 = 1 in 2, etc.).
    #[napi(getter)]
    pub fn skip_interval(&self) -> u32 {
        self.inner.skip_interval()
    }
}
