//! WebAssembly bindings for scanner-core.
//!
//! This is the low-level engine. Most applications should use the
//! `@gueshbzh-perso/scanner` TypeScript SDK (`packages/scanner-web`), which
//! wraps this module with camera management, a WebGL frame pipeline, a Web
//! Worker and a UI overlay.
//!
//! # Browser Usage
//!
//! ```javascript
//! import init, { WasmScanner, WasmScanSession } from './scanner_wasm.js';
//!
//! await init();
//!
//! // One-shot scan of an ImageData
//! const scanner = new WasmScanner({ formats: ["qr_code", "code_128"] });
//! const results = scanner.scanRgba(imageData.data, imageData.width, imageData.height);
//!
//! // Real-time: write pixels straight into WASM memory (no extra copy)
//! const session = new WasmScanSession({ adaptiveThrottle: false });
//! const view = session.frameBuffer(width * height);   // luma, 1 byte / px
//! view.set(lumaPixels);
//! const fresh = session.processFrameBuffer(width, height, "luma");
//! ```

#![deny(clippy::all)]
use scanner_core::config::Roi as CoreRoi;
use scanner_core::{
    BarcodeFormat, PixelFormat, ScanError, ScanSession as CoreScanSession, Scanner as CoreScanner,
    ScannerConfig,
};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

// Report Rust panics to the browser console as soon as the module is loaded.
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

/// Version of the scanner engine.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
/** Barcode formats supported by the scanner. */
export type BarcodeFormat =
  | "qr_code"
  | "micro_qr"
  | "data_matrix"
  | "aztec"
  | "pdf417"
  | "code_128"
  | "code_39"
  | "code_93"
  | "ean_13"
  | "ean_8"
  | "upc_a"
  | "upc_e"
  | "itf"
  | "codabar"
  | "unknown";

/** Layout of a raw pixel buffer. */
export type PixelFormat = "rgba" | "rgb" | "luma";
"#;

// ── JS-facing types ───────────────────────────────────────────────────────────

/// A point in frame pixel coordinates.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A detected barcode.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    /// Decoded text content.
    pub text: String,
    /// Barcode format, same spelling as the `formats` config option.
    #[tsify(type = "BarcodeFormat")]
    pub format: BarcodeFormat,
    /// Location of the code in the coordinates of the submitted frame.
    pub points: Vec<Point>,
    /// Raw payload bytes, when available (binary QR codes…).
    #[serde(with = "serde_bytes")]
    #[tsify(type = "Uint8Array | undefined", optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<Vec<u8>>,
    /// Unix timestamp (ms) of the detection.
    pub timestamp_ms: f64,
}

impl From<&scanner_core::ScanResult> for ScanResult {
    fn from(r: &scanner_core::ScanResult) -> Self {
        Self {
            text: r.text.clone(),
            format: r.format,
            points: r.points.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
            raw_bytes: r.raw_bytes.clone(),
            timestamp_ms: r.timestamp_ms as f64,
        }
    }
}

/// Performance statistics of a scan session.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub frames_submitted: f64,
    pub frames_scanned: f64,
    pub frames_with_detection: f64,
    pub codes_detected: f64,
    pub avg_scan_time_ms: f64,
    pub last_scan_time_ms: f64,
    pub skip_interval: u32,
}

fn results_to_js(results: &[scanner_core::ScanResult]) -> JsValue {
    let out: Vec<ScanResult> = results.iter().map(ScanResult::from).collect();
    out.serialize(&serde_wasm_bindgen::Serializer::new())
        .unwrap_or(JsValue::NULL)
}

fn scan_to_js(r: Result<Vec<scanner_core::ScanResult>, ScanError>) -> Result<JsValue, JsError> {
    match r {
        Ok(results) => Ok(results_to_js(&results)),
        Err(ScanError::NotFound) => Ok(js_sys::Array::new().into()),
        Err(e) => Err(JsError::new(&format!("Scanner error: {e}"))),
    }
}

fn parse_pixel_format(format: &str) -> Result<PixelFormat, JsError> {
    match format {
        "rgba" => Ok(PixelFormat::Rgba),
        "rgb" => Ok(PixelFormat::Rgb),
        "luma" => Ok(PixelFormat::Luma),
        other => Err(JsError::new(&format!(
            "Unsupported pixel format '{other}' (expected \"rgba\", \"rgb\" or \"luma\")"
        ))),
    }
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Scanner configuration. Every field is optional.
#[derive(Deserialize, Default, Tsify)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase", default)]
pub struct WasmScanConfig {
    /// Formats to look for. Default: all. Fewer formats = faster.
    #[tsify(optional)]
    pub formats: Option<Vec<BarcodeFormat>>,
    /// Enable CLAHE / Sauvola fallback strategies. Default: `true`.
    #[tsify(optional)]
    pub preprocessing: Option<bool>,
    /// Retry rotated images (static scans only). Default: `false`, `true` for `WasmScanner`.
    #[tsify(optional)]
    pub try_rotations: Option<bool>,
    /// Also look for inverted (light-on-dark) codes. Default: `true`.
    #[tsify(optional)]
    pub try_invert: Option<bool>,
    /// Maximum codes per frame. `> 1` enables multi-code detection. Default: `1`.
    #[tsify(optional)]
    pub max_results: Option<u32>,
    /// Region of interest, as fractions of the frame (0.0–1.0).
    #[tsify(optional)]
    pub roi: Option<WasmRoi>,
    /// Delay before the same content is reported again. Default: `2000`.
    #[tsify(optional)]
    pub dedup_cooldown_ms: Option<f64>,
    /// Agreeing reads required for Code 39/93, ITF and Codabar. Default: `2`.
    #[tsify(optional)]
    pub weak_format_confirmations: Option<u32>,
    /// Skip frames when scanning gets slow. Disable it when the caller
    /// already applies back-pressure (Web Worker). Default: `true`.
    #[tsify(optional)]
    pub adaptive_throttle: Option<bool>,
}

#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct WasmRoi {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn js_config_to_core(js_cfg: Option<WasmScanConfig>, base: ScannerConfig) -> ScannerConfig {
    let mut config = base;
    let Some(c) = js_cfg else { return config };

    if let Some(formats) = c.formats {
        config.formats = if formats.is_empty() {
            None
        } else {
            Some(formats)
        };
    }
    if let Some(v) = c.preprocessing {
        config.preprocessing = v;
    }
    if let Some(v) = c.try_rotations {
        config.try_rotations = v;
    }
    if let Some(v) = c.try_invert {
        config.try_invert = v;
    }
    if let Some(v) = c.max_results {
        config.max_results = v.max(1) as usize;
    }
    if let Some(roi) = c.roi {
        config.roi = Some(CoreRoi {
            x: roi.x,
            y: roi.y,
            width: roi.width,
            height: roi.height,
        });
    }
    if let Some(v) = c.dedup_cooldown_ms {
        config.dedup_cooldown_ms = v.max(0.0) as u64;
    }
    if let Some(v) = c.weak_format_confirmations {
        config.weak_format_confirmations = v;
    }
    if let Some(v) = c.adaptive_throttle {
        config.adaptive_throttle = v;
    }
    config
}

/// Return a `Uint8Array` view of `buf`, resized to `len` bytes.
///
/// The view aliases WASM linear memory: it is invalidated as soon as the
/// memory grows, so callers must request it right before writing and must not
/// keep it across calls into the module.
fn buffer_view(buf: &mut Vec<u8>, len: usize) -> js_sys::Uint8Array {
    if buf.len() != len {
        buf.resize(len, 0);
    }
    // SAFETY: the view is only valid until the next allocation in the module.
    // This is documented on `frameBuffer` and the SDK never retains it.
    unsafe { js_sys::Uint8Array::view(buf) }
}

// ── WasmScanner ───────────────────────────────────────────────────────────────

/// One-shot scanner for images and captured frames. Runs every enabled
/// strategy until a code is found.
#[wasm_bindgen]
pub struct WasmScanner {
    inner: CoreScanner,
    buf: Vec<u8>,
}

#[wasm_bindgen]
impl WasmScanner {
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<WasmScanConfig>) -> WasmScanner {
        let base = ScannerConfig {
            try_rotations: true,
            ..ScannerConfig::default()
        };
        WasmScanner {
            inner: CoreScanner::new(js_config_to_core(config, base)),
            buf: Vec::new(),
        }
    }

    /// Scan an RGBA buffer (e.g. `ImageData.data`).
    #[wasm_bindgen(js_name = scanRgba, unchecked_return_type = "ScanResult[]")]
    pub fn scan_rgba(&self, data: &[u8], width: u32, height: u32) -> Result<JsValue, JsError> {
        scan_to_js(
            self.inner
                .scan_frame(data, width, height, PixelFormat::Rgba),
        )
    }

    /// Scan an RGB buffer (3 bytes / pixel).
    #[wasm_bindgen(js_name = scanRgb, unchecked_return_type = "ScanResult[]")]
    pub fn scan_rgb(&self, data: &[u8], width: u32, height: u32) -> Result<JsValue, JsError> {
        scan_to_js(self.inner.scan_frame(data, width, height, PixelFormat::Rgb))
    }

    /// Scan a grayscale buffer (1 byte / pixel, fastest).
    #[wasm_bindgen(js_name = scanLuma, unchecked_return_type = "ScanResult[]")]
    pub fn scan_luma(&self, data: &[u8], width: u32, height: u32) -> Result<JsValue, JsError> {
        scan_to_js(
            self.inner
                .scan_frame(data, width, height, PixelFormat::Luma),
        )
    }

    /// Get a view of an internal buffer of `len` bytes to write pixels into,
    /// then call `scanFrameBuffer`. **Do not keep the view** across calls.
    #[wasm_bindgen(js_name = frameBuffer)]
    pub fn frame_buffer(&mut self, len: usize) -> js_sys::Uint8Array {
        buffer_view(&mut self.buf, len)
    }

    /// Scan the pixels previously written into `frameBuffer`.
    #[wasm_bindgen(js_name = scanFrameBuffer, unchecked_return_type = "ScanResult[]")]
    pub fn scan_frame_buffer(
        &self,
        width: u32,
        height: u32,
        #[wasm_bindgen(unchecked_param_type = "PixelFormat")] format: &str,
    ) -> Result<JsValue, JsError> {
        let format = parse_pixel_format(format)?;
        scan_to_js(self.inner.scan_frame(&self.buf, width, height, format))
    }
}

// ── WasmScanSession ───────────────────────────────────────────────────────────

/// Stateful session for camera streams: strategy rotation across frames,
/// deduplication, multi-frame confirmation and optional throttling.
#[wasm_bindgen]
pub struct WasmScanSession {
    inner: CoreScanSession,
    buf: Vec<u8>,
}

#[wasm_bindgen]
impl WasmScanSession {
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<WasmScanConfig>) -> WasmScanSession {
        WasmScanSession {
            inner: CoreScanSession::new(js_config_to_core(config, ScannerConfig::default())),
            buf: Vec::new(),
        }
    }

    /// Process an RGBA camera frame. Returns the **new** codes, or `null`.
    #[wasm_bindgen(js_name = processFrameRgba, unchecked_return_type = "ScanResult[] | null")]
    pub fn process_frame_rgba(&mut self, data: &[u8], width: u32, height: u32) -> JsValue {
        self.process(data, width, height, PixelFormat::Rgba)
    }

    /// Process a grayscale camera frame (fastest). Returns the **new** codes, or `null`.
    #[wasm_bindgen(js_name = processFrameLuma, unchecked_return_type = "ScanResult[] | null")]
    pub fn process_frame_luma(&mut self, data: &[u8], width: u32, height: u32) -> JsValue {
        self.process(data, width, height, PixelFormat::Luma)
    }

    /// Get a view of the internal frame buffer resized to `len` bytes.
    ///
    /// Write the frame into it, then call `processFrameBuffer`. This avoids
    /// the extra copy made when passing a JS array. **Do not keep the view**:
    /// it becomes invalid whenever WASM memory grows.
    #[wasm_bindgen(js_name = frameBuffer)]
    pub fn frame_buffer(&mut self, len: usize) -> js_sys::Uint8Array {
        buffer_view(&mut self.buf, len)
    }

    /// Process the frame previously written into `frameBuffer`.
    #[wasm_bindgen(js_name = processFrameBuffer, unchecked_return_type = "ScanResult[] | null")]
    pub fn process_frame_buffer(
        &mut self,
        width: u32,
        height: u32,
        #[wasm_bindgen(unchecked_param_type = "PixelFormat")] format: &str,
    ) -> Result<JsValue, JsError> {
        let format = parse_pixel_format(format)?;
        let buf = std::mem::take(&mut self.buf);
        let out = self.process(&buf, width, height, format);
        self.buf = buf;
        Ok(out)
    }

    /// Every code decoded in the last scanned frame, including duplicates
    /// still in cooldown. Use it to draw live highlights.
    #[wasm_bindgen(getter, js_name = lastDetections, unchecked_return_type = "ScanResult[]")]
    pub fn last_detections(&self) -> JsValue {
        results_to_js(self.inner.last_detections())
    }

    /// Performance statistics.
    #[wasm_bindgen(getter)]
    pub fn stats(&self) -> SessionStats {
        let s = self.inner.stats();
        SessionStats {
            frames_submitted: s.frames_submitted as f64,
            frames_scanned: s.frames_scanned as f64,
            frames_with_detection: s.frames_with_detection as f64,
            codes_detected: s.codes_detected as f64,
            avg_scan_time_ms: s.avg_scan_time_ms,
            last_scan_time_ms: s.last_scan_time_ms,
            skip_interval: self.inner.skip_interval(),
        }
    }

    /// Reset the session (clears dedup cooldowns and all statistics).
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Allow `text` to be reported again immediately.
    #[wasm_bindgen(js_name = clearCooldown)]
    pub fn clear_cooldown(&mut self, text: &str) {
        self.inner.clear_cooldown(text);
    }

    fn process(&mut self, data: &[u8], width: u32, height: u32, format: PixelFormat) -> JsValue {
        match self.inner.process_frame(data, width, height, format) {
            Some(results) => results_to_js(&results),
            None => JsValue::NULL,
        }
    }
}
