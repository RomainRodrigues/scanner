# Scanner — High-Performance QR Code & Barcode Scanner in Rust

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A robust, high-performance barcode and QR code scanning library written in pure Rust, designed for:

- 📷 **Real-time camera stream scanning** (not just static images)
- 🌐 **Node.js native addon** via [napi-rs](https://napi.rs)
- 🦕 **Browser WASM** via [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen)
- ⚡ **Performance target: ≥ Strich.io**

## Supported Formats

| 2D Formats | 1D Formats |
|-----------|-----------|
| QR Code (incl. Micro QR) | Code 128 |
| Data Matrix | Code 39 |
| Aztec Code | Code 93 |
| PDF417 | EAN-13, EAN-8 |
| | UPC-A, UPC-E |
| | ITF (Interleaved 2-of-5) |
| | Codabar |

## Architecture

```
scanner/
├── crates/
│   ├── scanner-core/     # Pure Rust engine (this crate)
│   └── scanner-cli/      # Command-line tool
└── scanner-wasm/         # WASM + browser JS SDK (Phase 4)
```

## Quick Start

### As a Rust library

```toml
[dependencies]
scanner-core = { path = "crates/scanner-core" }
```

```rust
use scanner_core::{Scanner, ScannerConfig, PixelFormat};

// Static image scanning
let scanner = Scanner::with_defaults();
let results = scanner.scan_file("qrcode.png")?;
println!("Found: {} ({})", results[0].text, results[0].format);

// Real-time camera scanning
use scanner_core::ScanSession;

let mut session = ScanSession::new(ScannerConfig::realtime());

// In your camera frame callback:
if let Some(results) = session.process_frame(&rgba_bytes, width, height, PixelFormat::Rgba) {
    println!("New scan: {}", results[0].text);
}
```

### CLI

```bash
# Build
cargo build --release -p scanner-cli

# Scan a file
./target/release/scanner scan image.png

# Scan a directory recursively
./target/release/scanner scan ./images/ --recursive

# JSON output
./target/release/scanner scan image.png --output json

# Specific formats only
./target/release/scanner scan image.jpg --format qr,ean13

# Benchmark
./target/release/scanner bench ./tests/images/ --iterations 20
```

## Performance

The scanner uses a layered pipeline optimized for speed:

1. **Fast path** — Attempt decode on raw grayscale immediately (0 preprocessing)
2. **Contrast path** — CLAHE contrast enhancement only
3. **Full pipeline** — CLAHE + Unsharp Mask + Sauvola adaptive binarization
4. **Rotation retries** — 90°/180°/270° (configurable, off by default for realtime)
5. **Invert retry** — For white-on-black barcodes

For real-time camera frames, the fast path handles most clean captures in **< 10ms**.

### Frame Throttling

`ScanSession` automatically adapts its scanning rate based on the measured scan duration:
- If scanning takes < 15ms → scan every frame
- If scanning takes 30ms → scan 1-in-2 frames
- Capped at 1-in-8 frames minimum

### ROI (Region of Interest)

Restrict scanning to a sub-area of the frame:

```rust
use scanner_core::{ScannerConfig, Roi};

let config = ScannerConfig {
    roi: Some(Roi::centered(0.6)), // Scan only the central 60%
    ..ScannerConfig::realtime()
};
```

A 60% ROI reduces the number of pixels processed by ~64%.

## Running Tests

```bash
cargo test --workspace
```

Place test images in `tests/images/` following the structure documented in the [implementation plan](../implementation_plan.md).

## Roadmap

- [x] Phase 1 — Core library (`scanner-core`)
- [x] Phase 2 — CLI (`scanner-cli`)
- [ ] Phase 3 — Node.js native addon (`scanner-napi`)
- [ ] Phase 4 — WASM + Browser SDK (`scanner-wasm`)

## License

MIT