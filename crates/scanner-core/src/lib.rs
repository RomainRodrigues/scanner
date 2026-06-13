//! # scanner-core
//!
//! High-performance QR code and barcode scanning engine written in pure Rust.
//!
//! ## Features
//!
//! - Multi-format support: QR Code, Data Matrix, Aztec, PDF417, Code 128/39/93,
//!   EAN-13/8, UPC-A/E, ITF, Codabar, and more
//! - Adaptive preprocessing pipeline for robust scanning (blur, low-light, angles)
//! - Real-time scanning with stateful `ScanSession` (frame dedup, multi-frame validation)
//! - Region of Interest (ROI) for faster scanning on camera streams
//! - Designed to be embedded into Node.js (napi-rs) and WASM (wasm-bindgen)
//!
//! ## Quick Start
//!
//! ```no_run
//! use scanner_core::{Scanner, ScannerConfig};
//!
//! let scanner = Scanner::with_defaults();
//! let results = scanner.scan_file("qrcode.png").unwrap();
//! for r in results {
//!     println!("Found: {} ({})", r.text, r.format);
//! }
//! ```
//!
//! ## Real-Time Scanning
//!
//! ```no_run
//! use scanner_core::{ScanSession, ScannerConfig, PixelFormat};
//!
//! let mut session = ScanSession::new(ScannerConfig::default());
//! let frame_bytes = vec![0u8; 640 * 480 * 4];
//! let (width, height) = (640u32, 480u32);
//!
//! // Call this for every camera frame
//! if let Some(results) = session.process_frame(&frame_bytes, width, height, PixelFormat::Rgba) {
//!     println!("New code detected: {}", results[0].text);
//! }
//! ```

pub mod config;
pub mod decode;
pub mod error;
pub mod frame;
pub mod preprocess;
pub mod realtime;
pub mod result;
pub mod scanner;

// Re-export the primary public API
pub use config::{Roi, ScannerConfig};
pub use error::ScanError;
pub use frame::PixelFormat;
pub use realtime::session::{ScanSession, SessionStats};
pub use result::{BarcodeFormat, Point, ScanResult};
pub use scanner::Scanner;
