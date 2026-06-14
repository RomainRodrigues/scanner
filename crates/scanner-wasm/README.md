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

## Usage

### 1. Static Image Scanning

```javascript
import { WasmScanner } from '@gueshbzh-perso/scanner-wasm';

const scanner = new WasmScanner({
  formats: ["qr_code", "code_128"],
  preprocessing: true,
  tryInvert: true
});

// data is an RGBA Uint8ClampedArray (e.g. from ctx.getImageData)
const results = scanner.scanRgba(data, width, height);

if (results.length > 0) {
  console.log('Found barcode:', results[0].text);
}
```

### 2. Real-Time Camera Stream

For real-time streams, `WasmScanSession` automatically manages performance.

```javascript
import { WasmScanSession } from '@gueshbzh-perso/scanner-wasm';

// Define a Region of Interest (e.g., center 70% of the screen)
const session = new WasmScanSession({
  preprocessing: true,
  tryInvert: true,
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
```

## Configuration Options

```typescript
interface JsWasmScanConfig {
  formats?: string[];
  preprocessing?: boolean;
  tryRotations?: boolean;
  tryInvert?: boolean;
  maxResults?: number;
  roi?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  dedupCooldownMs?: number;
}
```

## License

MIT
