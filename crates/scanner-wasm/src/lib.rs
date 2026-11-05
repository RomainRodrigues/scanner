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
//!
//! // Parallel decoding: one session (state) + one decoder per worker
//! const plan = session.plan();                         // main thread
//! const decoder = new WasmFrameDecoder(config);        // in each worker
//! decoder.frameBuffer(width * height).set(lumaPixels);
//! const report = decoder.decodeFrameBuffer(width, height, "luma", plan);
//! const fresh2 = session.commit(report);               // main thread
//!
//! // Codes read by the native BarcodeDetector: same confirmation and dedup
//! const fresh3 = session.commitExternal(nativeResults, elapsedMs);
//! const fallbackOnly = session.planFallbackOnly();     // frames it missed
//! ```

use scanner_core::config::Roi as CoreRoi;
use scanner_core::{
    BarcodeFormat, FrameDecoder as CoreFrameDecoder, FramePlan as CoreFramePlan,
    FrameReport as CoreFrameReport, PixelFormat, ScanError, ScanSession as CoreScanSession,
    Scanner as CoreScanner, ScannerConfig,
};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// Runs when the module is instantiated: reports Rust panics to the browser
/// console instead of an opaque `unreachable` trap.
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
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Point {
    /// Pixels from the left edge of the frame.
    pub x: f32,
    /// Pixels from the top edge of the frame.
    pub y: f32,
}

/// A detected barcode.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
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

impl From<ScanResult> for scanner_core::ScanResult {
    fn from(r: ScanResult) -> Self {
        Self {
            text: r.text,
            format: r.format,
            raw_bytes: r.raw_bytes,
            points: r
                .points
                .into_iter()
                .map(|p| scanner_core::Point { x: p.x, y: p.y })
                .collect(),
            // Timestamps are positive milliseconds, well within f64's exact range.
            timestamp_ms: r.timestamp_ms.max(0.0) as u64,
        }
    }
}

/// What a frame must run, from `WasmScanSession.plan()`. Opaque: pass it
/// unchanged to `WasmFrameDecoder.decodeFrameBuffer()`.
#[derive(Serialize, Deserialize, Tsify, Clone, Copy)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct FramePlan {
    /// Index of the fallback strategy to run when the fast path finds nothing.
    #[tsify(optional)]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fallback: Option<u32>,
    /// Run the fallback only (plans from `planFallbackOnly()`).
    #[tsify(optional)]
    #[serde(
        rename = "skipFastPath",
        skip_serializing_if = "std::ops::Not::not",
        default
    )]
    pub skip_fast_path: bool,
}

impl From<CoreFramePlan> for FramePlan {
    fn from(p: CoreFramePlan) -> Self {
        Self {
            fallback: p.fallback.and_then(|i| u32::try_from(i).ok()),
            skip_fast_path: p.skip_fast_path,
        }
    }
}

impl From<FramePlan> for CoreFramePlan {
    fn from(p: FramePlan) -> Self {
        Self {
            fallback: p.fallback.map(|i| i as usize),
            skip_fast_path: p.skip_fast_path,
        }
    }
}

/// Outcome of `WasmFrameDecoder.decodeFrameBuffer()`, to pass to
/// `WasmScanSession.commit()`.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FrameReport {
    /// The plan the frame was decoded with.
    pub plan: FramePlan,
    /// Codes read in the frame (duplicates included), in frame coordinates.
    pub results: Vec<ScanResult>,
    /// The planned fallback ran (the fast path found nothing).
    pub fallback_ran: bool,
    /// The fallback read the codes.
    pub fallback_read: bool,
    /// Decoding time of the frame, in ms.
    pub elapsed_ms: f64,
}

impl From<CoreFrameReport> for FrameReport {
    fn from(r: CoreFrameReport) -> Self {
        Self {
            plan: r.plan.into(),
            results: r.results.iter().map(ScanResult::from).collect(),
            fallback_ran: r.fallback_ran,
            fallback_read: r.fallback_read,
            elapsed_ms: r.elapsed_ms,
        }
    }
}

impl From<FrameReport> for CoreFrameReport {
    fn from(r: FrameReport) -> Self {
        Self {
            plan: r.plan.into(),
            results: r.results.into_iter().map(Into::into).collect(),
            fallback_ran: r.fallback_ran,
            fallback_read: r.fallback_read,
            elapsed_ms: r.elapsed_ms,
        }
    }
}

/// Performance statistics of a scan session.
#[derive(Serialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    /// Frames passed to the session.
    pub frames_submitted: f64,
    /// Frames actually decoded (not skipped by the throttle).
    pub frames_scanned: f64,
    /// Frames in which at least one code was decoded (duplicates included).
    pub frames_with_detection: f64,
    /// Codes reported as new.
    pub codes_detected: f64,
    /// Average decode time per scanned frame, in ms.
    pub avg_scan_time_ms: f64,
    /// Decode time of the last scanned frame, in ms.
    pub last_scan_time_ms: f64,
    /// Throttle interval: `1` = every frame is scanned, `n` = one in `n`.
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
    /// Agreeing reads required for 1D codes (weak 1D formats: one more,
    /// 2D codes: always 1). Default: `2`.
    #[tsify(optional)]
    pub linear_confirmations: Option<u32>,
    /// Skip frames when scanning gets slow. Disable it when the caller
    /// already applies back-pressure (Web Worker). Default: `true`.
    #[tsify(optional)]
    pub adaptive_throttle: Option<bool>,
}

/// Region of interest, as fractions (0.0–1.0) of the frame size.
#[derive(Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct WasmRoi {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
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
    if let Some(v) = c.linear_confirmations {
        config.linear_confirmations = v;
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
    #[allow(unsafe_code)]
    unsafe {
        js_sys::Uint8Array::view(buf)
    }
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
    /// Create a scanner. Rotations are tried by default for still images.
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
    /// Create a session for a camera stream.
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

    /// Decide what the next frame runs, when frames are decoded elsewhere
    /// (`WasmFrameDecoder` in Web Workers). Every plan must come back to
    /// `commit`, in any order. The adaptive throttle does not apply.
    pub fn plan(&mut self) -> FramePlan {
        self.inner.plan().into()
    }

    /// Record a frame decoded by a `WasmFrameDecoder`. Returns the **new**
    /// codes, or `null`, like `processFrameBuffer`.
    #[wasm_bindgen(unchecked_return_type = "ScanResult[] | null")]
    pub fn commit(&mut self, report: FrameReport) -> JsValue {
        match self.inner.commit(report.into()) {
            Some(results) => results_to_js(&results),
            None => JsValue::NULL,
        }
    }

    /// Like `plan`, for a frame the browser's native `BarcodeDetector` already
    /// searched without finding anything: the decoder runs the planned
    /// fallback strategy only, without its own fast path.
    #[wasm_bindgen(js_name = planFallbackOnly)]
    pub fn plan_fallback_only(&mut self) -> FramePlan {
        self.inner.plan_fallback_only().into()
    }

    /// Record codes decoded by another engine (the browser's native
    /// `BarcodeDetector`), in whatever coordinates the caller uses. They get
    /// the same format filter, multi-frame confirmation and de-duplication
    /// as decoded frames; the strategy scheduling is not affected. Returns
    /// the **new** codes, or `null`.
    ///
    /// # Errors
    /// When `results` is not an array of `ScanResult`.
    #[wasm_bindgen(js_name = commitExternal, unchecked_return_type = "ScanResult[] | null")]
    pub fn commit_external(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "ScanResult[]")] results: JsValue,
        #[wasm_bindgen(js_name = elapsedMs)] elapsed_ms: f64,
    ) -> Result<JsValue, JsError> {
        let results: Vec<ScanResult> = serde_wasm_bindgen::from_value(results)
            .map_err(|e| JsError::new(&format!("Invalid results: {e}")))?;
        let results = results.into_iter().map(Into::into).collect();
        Ok(match self.inner.commit_external(results, elapsed_ms) {
            Some(fresh) => results_to_js(&fresh),
            None => JsValue::NULL,
        })
    }

    fn process(&mut self, data: &[u8], width: u32, height: u32, format: PixelFormat) -> JsValue {
        match self.inner.process_frame(data, width, height, format) {
            Some(results) => results_to_js(&results),
            None => JsValue::NULL,
        }
    }
}

// ── WasmFrameDecoder ──────────────────────────────────────────────────────────

/// Stateless frame decoder for parallel decoding: one per Web Worker, all
/// serving a single `WasmScanSession` (created with the same configuration)
/// that plans the frames and commits the reports.
#[wasm_bindgen]
pub struct WasmFrameDecoder {
    inner: CoreFrameDecoder,
    buf: Vec<u8>,
}

#[wasm_bindgen]
impl WasmFrameDecoder {
    /// Create a decoder. Use the configuration of the session it serves.
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<WasmScanConfig>) -> WasmFrameDecoder {
        WasmFrameDecoder {
            inner: CoreFrameDecoder::new(js_config_to_core(config, ScannerConfig::default())),
            buf: Vec::new(),
        }
    }

    /// Get a view of the internal frame buffer resized to `len` bytes, then
    /// call `decodeFrameBuffer`. **Do not keep the view** across calls.
    #[wasm_bindgen(js_name = frameBuffer)]
    pub fn frame_buffer(&mut self, len: usize) -> js_sys::Uint8Array {
        buffer_view(&mut self.buf, len)
    }

    /// Decode the frame previously written into `frameBuffer`, following `plan`.
    #[wasm_bindgen(js_name = decodeFrameBuffer)]
    pub fn decode_frame_buffer(
        &self,
        width: u32,
        height: u32,
        #[wasm_bindgen(unchecked_param_type = "PixelFormat")] format: &str,
        plan: FramePlan,
    ) -> Result<FrameReport, JsError> {
        let format = parse_pixel_format(format)?;
        self.inner
            .decode(&self.buf, width, height, format, plan.into())
            .map(FrameReport::from)
            .map_err(|e| JsError::new(&format!("Scanner error: {e}")))
    }
}
