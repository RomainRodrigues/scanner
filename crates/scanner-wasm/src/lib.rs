//! WebAssembly bindings for scanner-core.
//!
//! Exposes the QR/barcode scanning engine to the browser via wasm-bindgen.
//!
//! # Browser Usage
//!
//! ```javascript
//! import init, { WasmScanner, WasmScanSession } from './scanner_wasm.js';
//!
//! await init(); // loads the WASM module
//!
//! // Static image scan (from canvas ImageData)
//! const scanner = new WasmScanner({ formats: ["qr_code", "code_128"] });
//! const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
//! const results = scanner.scanRgba(imageData.data, canvas.width, canvas.height);
//! if (results.length > 0) console.log(results[0].text);
//!
//! // Real-time camera scanning (from requestAnimationFrame)
//! const session = new WasmScanSession({ roi: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 } });
//! function tick() {
//!   ctx.drawImage(video, 0, 0);
//!   const { data, width, height } = ctx.getImageData(0, 0, vw, vh);
//!   const result = session.processFrameRgba(data, width, height);
//!   if (result) showResult(result);
//!   requestAnimationFrame(tick);
//! }
//! ```

#![deny(clippy::all)]
use scanner_core::config::Roi as CoreRoi;
use scanner_core::{
    PixelFormat, ScanSession as CoreScanSession, Scanner as CoreScanner, ScannerConfig,
};
use wasm_bindgen::prelude::*;

// Initialise the panic hook as soon as the WASM module is loaded.
// This ensures Rust panics are reported to the browser console even if no
// WasmScanner or WasmScanSession has been constructed yet.
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

// ── JS-facing result type ─────────────────────────────────────────────────────

/// A detected barcode result, serialized as a JS object.
#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct ScanResult {
    /// Decoded text content of the barcode.
    pub text: String,
    /// Barcode format (e.g. "QR Code", "EAN-13").
    pub format: String,
    /// Detected corner points as flat [x0,y0, x1,y1, ...] array.
    pub points: Vec<f32>,
    /// Confidence score [0.0, 1.0].
    pub confidence: f32,
    /// Rotation angle in degrees.
    pub orientation: f32,
    /// Unix timestamp (ms) when the scan completed.
    pub timestamp_ms: f64,
}

impl From<scanner_core::ScanResult> for ScanResult {
    fn from(r: scanner_core::ScanResult) -> Self {
        let points: Vec<f32> = r.points.iter().flat_map(|p| [p.x, p.y]).collect();
        Self {
            text: r.text,
            format: format!("{}", r.format),
            points,
            confidence: r.confidence,
            orientation: r.orientation,
            timestamp_ms: r.timestamp_ms as f64,
        }
    }
}

// ── Config helper ─────────────────────────────────────────────────────────────

use serde::Deserialize;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JsWasmScanConfig {
    pub formats: Option<Vec<scanner_core::result::BarcodeFormat>>,
    pub preprocessing: Option<bool>,
    pub try_rotations: Option<bool>,
    pub try_invert: Option<bool>,
    pub max_results: Option<u32>,
    pub roi: Option<JsWasmRoi>,
    pub dedup_cooldown_ms: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsWasmRoi {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn js_config_to_core(js_val: JsValue, is_session: bool) -> ScannerConfig {
    let mut config = ScannerConfig::default();
    if is_session {
        config.try_rotations = false;
    }

    if js_val.is_null() || js_val.is_undefined() {
        return config;
    }

    if let Ok(js_cfg) = serde_wasm_bindgen::from_value::<JsWasmScanConfig>(js_val) {
        if let Some(formats) = js_cfg.formats {
            config.formats = Some(formats);
        }
        if let Some(pre) = js_cfg.preprocessing {
            config.preprocessing = pre;
        }
        if let Some(rot) = js_cfg.try_rotations {
            config.try_rotations = rot;
        }
        if let Some(inv) = js_cfg.try_invert {
            config.try_invert = inv;
        }
        if let Some(max) = js_cfg.max_results {
            config.max_results = max as usize;
        }
        if let Some(roi) = js_cfg.roi {
            config.roi = Some(CoreRoi {
                x: roi.x,
                y: roi.y,
                width: roi.width,
                height: roi.height,
            });
        }
        if let Some(cd) = js_cfg.dedup_cooldown_ms {
            config.dedup_cooldown_ms = cd as u64;
        }
    }
    
    config
}

// ── WasmScanner ───────────────────────────────────────────────────────────────

/// High-performance barcode scanner for the browser.
///
/// Use this for one-shot scanning of images or captured frames.
#[wasm_bindgen]
pub struct WasmScanner {
    inner: CoreScanner,
}

#[wasm_bindgen]
impl WasmScanner {
    /// Create a new scanner.
    ///
    /// @param {Object} [config] - Configuration object.
    /// @param {string[]} [config.formats] - Formats to scan (e.g. ["qr_code", "code_128"]).
    /// @param {boolean} [config.preprocessing=true] - Enable adaptive preprocessing.
    /// @param {boolean} [config.tryRotations=false] - Try rotated images.
    /// @param {boolean} [config.tryInvert=true] - Try inverted image (white-on-black).
    /// @param {Object} [config.roi] - Region of Interest {x, y, width, height} (0.0–1.0).
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> WasmScanner {
        let core_config = js_config_to_core(config, false);
        WasmScanner {
            inner: CoreScanner::new(core_config),
        }
    }

    /// Scan from browser ImageData (RGBA, 4 bytes/pixel).
    ///
    /// @param {Uint8ClampedArray} data - The ImageData.data buffer.
    /// @param {number} width - Frame width in pixels.
    /// @param {number} height - Frame height in pixels.
    /// @returns {ScanResult[]} Array of scan results (empty if not found).
    #[wasm_bindgen(js_name = scanRgba)]
    pub fn scan_rgba(&self, data: &[u8], width: u32, height: u32) -> Result<Vec<ScanResult>, JsValue> {
        match self
            .inner
            .scan_frame(data, width, height, PixelFormat::Rgba)
        {
            Ok(results) => Ok(results.into_iter().map(ScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(JsValue::from_str(&format!("Scanner error: {e}"))),
        }
    }

    /// Scan from a raw RGB buffer (3 bytes/pixel).
    ///
    /// @param {Uint8Array} data - Raw RGB bytes.
    /// @param {number} width - Frame width.
    /// @param {number} height - Frame height.
    #[wasm_bindgen(js_name = scanRgb)]
    pub fn scan_rgb(&self, data: &[u8], width: u32, height: u32) -> Result<Vec<ScanResult>, JsValue> {
        match self.inner.scan_frame(data, width, height, PixelFormat::Rgb) {
            Ok(results) => Ok(results.into_iter().map(ScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(JsValue::from_str(&format!("Scanner error: {e}"))),
        }
    }

    /// Scan from a grayscale buffer (1 byte/pixel, fastest).
    ///
    /// @param {Uint8Array} data - Raw luma bytes.
    /// @param {number} width - Frame width.
    /// @param {number} height - Frame height.
    #[wasm_bindgen(js_name = scanLuma)]
    pub fn scan_luma(&self, data: &[u8], width: u32, height: u32) -> Result<Vec<ScanResult>, JsValue> {
        match self
            .inner
            .scan_frame(data, width, height, PixelFormat::Luma)
        {
            Ok(results) => Ok(results.into_iter().map(ScanResult::from).collect()),
            Err(scanner_core::ScanError::NotFound) => Ok(vec![]),
            Err(e) => Err(JsValue::from_str(&format!("Scanner error: {e}"))),
        }
    }
}

// ── WasmScanSession ───────────────────────────────────────────────────────────

/// Stateful scan session for real-time camera streams in the browser.
///
/// Provides adaptive frame throttling and deduplication.
///
/// @example
/// ```javascript
/// const session = new WasmScanSession();
/// function onAnimationFrame() {
///   ctx.drawImage(video, 0, 0);
///   const { data } = ctx.getImageData(0, 0, width, height);
///   const result = session.processFrameRgba(data, width, height);
///   if (result) console.log('Scanned:', result[0].text);
///   requestAnimationFrame(onAnimationFrame);
/// }
/// ```
#[wasm_bindgen]
pub struct WasmScanSession {
    inner: CoreScanSession,
}

#[wasm_bindgen]
impl WasmScanSession {
    /// Create a new scan session.
    ///
    /// @param {Object} [config] - Configuration object.
    /// @param {string[]} [config.formats] - Formats to scan.
    /// @param {boolean} [config.preprocessing=true] - Enable adaptive preprocessing.
    /// @param {boolean} [config.tryInvert=true] - Try inverted image.
    /// @param {Object} [config.roi] - Region of Interest {x, y, width, height} (0.0–1.0).
    /// @param {number} [config.dedupCooldownMs=2000] - Dedup cooldown in milliseconds.
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> WasmScanSession {
        let core_config = js_config_to_core(config, true);
        WasmScanSession {
            inner: CoreScanSession::new(core_config),
        }
    }

    /// Process a camera frame (RGBA, 4 bytes/pixel — browser ImageData format).
    ///
    /// Returns null if the frame was throttled or no new code was detected.
    /// Returns an array of ScanResult when new barcodes are found.
    ///
    /// @param {Uint8ClampedArray} data - The ImageData.data buffer.
    /// @param {number} width - Frame width in pixels.
    /// @param {number} height - Frame height in pixels.
    /// @returns {ScanResult[] | null}
    #[wasm_bindgen(js_name = processFrameRgba)]
    pub fn process_frame_rgba(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Option<Vec<ScanResult>> {
        self.inner
            .process_frame(data, width, height, PixelFormat::Rgba)
            .map(|results| results.into_iter().map(ScanResult::from).collect())
    }

    /// Process a camera frame (grayscale, 1 byte/pixel — fastest path).
    ///
    /// @param {Uint8Array} data - Raw luma bytes.
    /// @param {number} width - Frame width.
    /// @param {number} height - Frame height.
    #[wasm_bindgen(js_name = processFrameLuma)]
    pub fn process_frame_luma(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Option<Vec<ScanResult>> {
        self.inner
            .process_frame(data, width, height, PixelFormat::Luma)
            .map(|results| results.into_iter().map(ScanResult::from).collect())
    }

    /// Total number of frames submitted (including throttled frames).
    #[wasm_bindgen(getter, js_name = framesSubmitted)]
    pub fn frames_submitted(&self) -> u32 {
        self.inner.stats().frames_submitted as u32
    }

    /// Number of frames actually scanned (not throttled).
    #[wasm_bindgen(getter, js_name = framesScanned)]
    pub fn frames_scanned(&self) -> u32 {
        self.inner.stats().frames_scanned as u32
    }

    /// Total unique barcodes detected (after deduplication).
    #[wasm_bindgen(getter, js_name = codesDetected)]
    pub fn codes_detected(&self) -> u32 {
        self.inner.stats().codes_detected as u32
    }

    /// Average scan time in milliseconds.
    #[wasm_bindgen(getter, js_name = avgScanTimeMs)]
    pub fn avg_scan_time_ms(&self) -> f64 {
        self.inner.stats().avg_scan_time_ms
    }

    /// Current frame skip interval (1 = every frame, 2 = 1-in-2, etc.).
    #[wasm_bindgen(getter, js_name = skipInterval)]
    pub fn skip_interval(&self) -> u32 {
        self.inner.skip_interval()
    }

    /// Reset the session (clears dedup cooldowns and all statistics).
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Clear the cooldown for a specific barcode (allow immediate re-scan).
    #[wasm_bindgen(js_name = clearCooldown)]
    pub fn clear_cooldown(&mut self, text: &str) {
        self.inner.clear_cooldown(text);
    }
}

// ── Panic hook ────────────────────────────────────────────────────────────────
// Initialisation happens automatically via #[wasm_bindgen(start)] on `wasm_init`.
// No manual call needed.
