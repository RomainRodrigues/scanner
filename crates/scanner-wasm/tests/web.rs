//! Runs inside a real WebAssembly runtime (Node.js):
//!
//! ```bash
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   cargo test -p scanner-wasm --target wasm32-unknown-unknown
//! ```
#![cfg(target_arch = "wasm32")]
// `allow-unwrap-in-tests` does not cover `#[wasm_bindgen_test]` functions.
#![allow(clippy::unwrap_used)]

use rxing::{BarcodeFormat, MultiFormatWriter, Writer};
use scanner_wasm::{WasmScanSession, WasmScanner};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

/// Render `text` as a QR code into a white luma frame of `size`×`size`.
fn qr_frame(text: &str, size: u32) -> Vec<u8> {
    let matrix = MultiFormatWriter
        .encode(
            text,
            &BarcodeFormat::QR_CODE,
            (size / 2) as i32,
            (size / 2) as i32,
        )
        .unwrap();
    let mut frame = vec![255u8; (size * size) as usize];
    let off = size / 4;
    for y in 0..matrix.getHeight() {
        for x in 0..matrix.getWidth() {
            if matrix.get(x, y) {
                frame[((y + off) * size + x + off) as usize] = 0;
            }
        }
    }
    frame
}

fn first_text(results: &JsValue) -> Option<String> {
    let first = js_sys::Reflect::get(results, &0.into()).ok()?;
    js_sys::Reflect::get(&first, &"text".into())
        .ok()?
        .as_string()
}

/// Regression: rxing calls `chrono::Utc::now()` on every decode, which used
/// to panic with "time not implemented on this platform".
#[wasm_bindgen_test]
fn scanner_decodes_qr_without_panicking() {
    let frame = qr_frame("hello wasm", 200);
    let scanner = WasmScanner::new(None);
    let results = scanner
        .scan_luma(&frame, 200, 200)
        .map_err(JsValue::from)
        .unwrap();
    assert_eq!(first_text(&results).as_deref(), Some("hello wasm"));
}

#[wasm_bindgen_test]
fn session_frame_buffer_roundtrip() {
    let frame = qr_frame("zero copy", 200);
    let mut session = WasmScanSession::new(None);
    session.frame_buffer(frame.len()).copy_from(&frame);
    let results = session
        .process_frame_buffer(200, 200, "luma")
        .map_err(JsValue::from)
        .unwrap();
    assert_eq!(first_text(&results).as_deref(), Some("zero copy"));
    // Same code again: de-duplicated, but still visible as a live detection.
    session.frame_buffer(frame.len()).copy_from(&frame);
    let again = session
        .process_frame_buffer(200, 200, "luma")
        .map_err(JsValue::from)
        .unwrap();
    assert!(again.is_null());
    assert_eq!(
        first_text(&session.last_detections()).as_deref(),
        Some("zero copy")
    );
}
