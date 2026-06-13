# @scanner/native

High-performance QR code and barcode scanner for Node.js, powered by a native Rust engine via napi-rs.

## Features

- 🚀 **Native Rust engine** — zero-copy, no V8 GC overhead
- 📷 **Real-time camera scanning** via stateful `ScanSession`
- 🔍 **Wide format support**: QR Code, DataMatrix, Aztec, PDF417, Code128/39/93, EAN-13/8, UPC-A/E, ITF, Codabar
- 🖼️ **Multi-format pixel input**: RGBA, RGB, Luma, NV21 (Android), YUY2 (webcam)
- 🎯 **ROI (Region of Interest)** for faster scanning in camera streams
- 🔄 **Adaptive preprocessing** pipeline: CLAHE + Unsharp Mask + Sauvola binarization
- ⚡ **Async/sync** APIs for both file scanning and buffer scanning

## Installation

```bash
npm install @scanner/native
```

## Quick Start

### Scan a file

```javascript
const { Scanner } = require('@scanner/native');

const scanner = new Scanner();

// Async
const results = await scanner.scanFile('./qrcode.png');
if (results.length > 0) {
  console.log(results[0].text);   // decoded text
  console.log(results[0].format); // "QR Code", "EAN-13", etc.
}

// Sync (from a buffer, e.g. from sharp)
const { createReadStream } = require('fs');
const sharp = require('sharp');

const { data, info } = await sharp('./image.jpg')
  .raw()
  .toBuffer({ resolveWithObject: true });

const results = scanner.scanBuffer(data, info.width, info.height, PixelFormat.Rgb);
```

### Real-time camera scanning

```javascript
const { ScanSession, PixelFormat } = require('@scanner/native');

const session = new ScanSession({
  // Restrict scanning to central 60% of frame (faster)
  roi: { x: 0.2, y: 0.2, width: 0.6, height: 0.6 },
  // Don't report the same code twice within 3 seconds
  dedupCooldownMs: 3000,
});

// In your camera frame callback (e.g. from node-webcam, v4l2, etc.):
function onCameraFrame(rgbaBuffer, width, height) {
  const results = session.processFrame(rgbaBuffer, width, height, PixelFormat.Rgba);
  if (results && results.length > 0) {
    console.log(`Scanned: ${results[0].text} (${results[0].format})`);
    
    // Print scan performance
    const stats = session.getStats();
    console.log(`Avg scan: ${stats.avgScanTimeMs.toFixed(1)}ms | Skip 1/${session.skipInterval}`);
  }
}
```

### With a browser via WebSocket

```javascript
// server.js — Node.js side
const ws = require('ws');
const { ScanSession, PixelFormat } = require('@scanner/native');

const wss = new ws.Server({ port: 3000 });

wss.on('connection', (socket) => {
  const session = new ScanSession({ dedupCooldownMs: 2000 });

  socket.on('message', (buffer) => {
    // buffer contains RGBA frame from browser ImageData
    const { width, height } = JSON.parse(buffer.slice(0, 16)); // header
    const pixels = buffer.slice(16);

    const results = session.processFrame(pixels, width, height, PixelFormat.Rgba);
    if (results) {
      socket.send(JSON.stringify(results));
    }
  });
});
```

## API Reference

### `new Scanner(config?)`

Creates a scanner for one-shot image scanning.

| Method | Returns | Description |
|--------|---------|-------------|
| `scanFile(path)` | `Promise<ScanResult[]>` | Scan an image file (async) |
| `scanBuffer(buf, w, h, fmt?)` | `ScanResult[]` | Scan from pixel buffer (sync) |
| `scanBufferAsync(buf, w, h, fmt?)` | `Promise<ScanResult[]>` | Scan from pixel buffer (async) |

### `new ScanSession(config?)`

Stateful scanner for real-time camera streams.

| Method/Property | Returns | Description |
|----------------|---------|-------------|
| `processFrame(buf, w, h, fmt?)` | `ScanResult[] \| null` | Process a camera frame |
| `getStats()` | `SessionStats` | Get performance statistics |
| `reset()` | `void` | Reset state (dedup + stats) |
| `clearCooldown(text)` | `void` | Allow re-scanning a specific code |
| `skipInterval` | `number` | Current frame skip ratio |

### `ScanConfig`

```typescript
interface ScanConfig {
  formats?: BarcodeFormat[]       // Filter to specific formats (default: all)
  preprocessing?: boolean          // Enable adaptive preprocessing (default: true)
  tryRotations?: boolean           // Retry with 90°/180°/270° (default: false)
  tryInvert?: boolean              // Retry inverted (white-on-black, default: true)
  maxResults?: number              // Max barcodes per scan (default: 1)
  roi?: { x, y, width, height }   // Region of interest (0.0–1.0 fractions)
  dedupCooldownMs?: number         // Same-code cooldown for ScanSession (default: 2000)
}
```

### `ScanResult`

```typescript
interface ScanResult {
  text: string          // Decoded content
  format: string        // Human-readable format name
  rawBytes: Buffer | null  // Raw binary data (binary QR codes)
  points: Point[]       // Barcode corner points
  confidence: number    // Confidence score [0.0, 1.0]
  orientation: number   // Rotation angle in degrees
  timestampMs: number   // Unix timestamp (ms)
}
```

## Supported Formats

| Format | Constant |
|--------|----------|
| QR Code | `BarcodeFormat.QrCode` |
| Micro QR | `BarcodeFormat.MicroQr` |
| Data Matrix | `BarcodeFormat.DataMatrix` |
| Aztec Code | `BarcodeFormat.Aztec` |
| PDF417 | `BarcodeFormat.Pdf417` |
| Code 128 | `BarcodeFormat.Code128` |
| Code 39 | `BarcodeFormat.Code39` |
| Code 93 | `BarcodeFormat.Code93` |
| EAN-13 | `BarcodeFormat.Ean13` |
| EAN-8 | `BarcodeFormat.Ean8` |
| UPC-A | `BarcodeFormat.UpcA` |
| UPC-E | `BarcodeFormat.UpcE` |
| ITF | `BarcodeFormat.Itf` |
| Codabar | `BarcodeFormat.Codabar` |

## Building from Source

```bash
# Prerequisites: Rust stable, Node.js >= 18
git clone https://github.com/romrd/scanner
cd scanner/crates/scanner-napi
npm install
npm run build
node test.mjs
```

## License

MIT
