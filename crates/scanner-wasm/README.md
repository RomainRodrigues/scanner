# Scanner WASM

⚠️ **Work in Progress**: This package is under active development. APIs and configurations are experimental and subject to change.

High-performance QR code and barcode scanner for the browser, powered by Rust and WebAssembly.

## Features

- **Fast & Efficient**: Built in Rust and compiled to WebAssembly for near-native performance.
- **Real-time camera scanning**: Optimized for video streams with adaptive frame throttling.
- **Image scanning**: Scan static files or `ImageData` objects.
- **Deduplication**: Built-in cooldowns for the same barcode to prevent spam.
- **Multiple formats**: Supports QR Code, Code 128, EAN-13, and more.

## Installation

```bash
npm install @gueshbzh-perso/scanner-wasm
```

## Usage (Vite / Nuxt 3 / Modern Bundlers)

By default, this package is built with `--target web`. This means it exposes a standard ES Module that needs to be initialized asynchronously before use.

If you are using **Vite** (or **Nuxt 3**), you can import the WASM file as a URL and pass it to the init function:

```javascript
import init, { WasmScanner, WasmScanSession } from '@gueshbzh-perso/scanner-wasm';
// The `?url` suffix tells Vite to treat the WASM file as an asset URL
import wasmUrl from '@gueshbzh-perso/scanner-wasm/scanner_wasm_bg.wasm?url';

// 1. Initialize the WebAssembly module (do this once)
await init(wasmUrl);

// 2. Initialize the scanner
const scanner = new WasmScanner({
  formats: ["qr_code", "code_128"],
  preprocessing: true,
  tryInvert: true
});

// 3. Scan a static image (data is an RGBA Uint8ClampedArray)
const results = scanner.scanRgba(imageData.data, imageData.width, imageData.height);

if (results.length > 0) {
  console.log('Found barcode:', results[0].text);
}

// 4. Recommended: Free memory for immediate release
scanner.free();
```

### Real-Time Camera Stream

For real-time streams (like a webcam), `WasmScanSession` automatically manages performance and frame throttling.

```javascript
// Define a Region of Interest (e.g., center 70% of the screen)
const session = new WasmScanSession({
  preprocessing: true,
  roi: { x: 0.15, y: 0.15, width: 0.7, height: 0.7 },
  dedupCooldownMs: 2000
});

function onAnimationFrame() {
  ctx.drawImage(video, 0, 0);
  const { data, width, height } = ctx.getImageData(0, 0, vw, vh);
  
  const results = session.processFrameRgba(data, width, height);
  if (results && results.length > 0) {
    console.log('Detected:', results[0].text);
  }
  
  requestAnimationFrame(onAnimationFrame);
}

// When you stop the camera:
// session.free(); // Recommended: Free memory immediately when the session is no longer needed
```

## 🛡️ Memory Management

WebAssembly does not have automatic garbage collection for Rust objects. However, because this package is compiled with the `--weak-refs` flag, the generated bindings use the JavaScript `FinalizationRegistry` to automatically call the Rust `.free()` method when the JS wrapper object is garbage collected.

While this acts as a great safety net against memory leaks, the JavaScript garbage collector is non-deterministic. For objects that allocate large buffers (like `WasmScanSession` handling camera frames), it is **highly recommended to manually call `.free()`** as soon as you are done with them (e.g. when stopping the camera). This ensures the memory is released immediately rather than waiting for the GC, avoiding temporary memory spikes in the browser.

## Supported Formats

You can filter which barcodes to detect using the `formats` array in the configuration. The supported formats in `snake_case` are:

- **2D**: `qr_code`, `micro_qr`, `data_matrix`, `aztec`, `pdf417`
- **1D**: `code_128`, `code_39`, `code_93`, `ean_13`, `ean_8`, `upc_a`, `upc_e`, `itf`, `codabar`

## Configuration Options

```typescript
export interface WasmRoi {
    x: number;
    y: number;
    width: number;
    height: number;
}

export interface WasmScanConfig {
    formats: BarcodeFormat[] | null;
    preprocessing: boolean | null;
    tryRotations: boolean | null;
    tryInvert: boolean | null;
    maxResults: number | null;
    roi: WasmRoi | null;
    dedupCooldownMs: number | null;
}

export class ScanResult {
    text: string;
    format: string;
    points: Float32Array;
    confidence: number;
    orientation: number;
    timestamp_ms: number;
}
```

## License

MIT
