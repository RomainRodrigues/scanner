# @gueshbzh-perso/scanner

Fast QR code & barcode scanning for the web. Rust/WebAssembly decoder, WebGL2
frame pipeline, decoding in a Web Worker, camera management and a ready-made
overlay. No license key, no network calls: everything runs on the device.

⚠️ Work in progress: the API may still change before 1.0.

```bash
npm install @gueshbzh-perso/scanner
```

## Camera scanning

```html
<div id="scanner" style="width: 100%; aspect-ratio: 4 / 3"></div>
```

```ts
import { BarcodeReader } from '@gueshbzh-perso/scanner';

const reader = await BarcodeReader.create({
  host: '#scanner',
  formats: ['qr_code', 'ean_13', 'code_128'], // fewer formats = faster
});

reader.on('detected', (codes) => {
  console.log(codes[0].text, codes[0].format);
});

// Call it from a click handler: browsers require a user gesture for audio (iOS).
await reader.start();

// Later
await reader.stop();     // releases the camera, keeps the reader
await reader.destroy();  // removes the preview, frees the engine
```

### Options

| Option | Default | Description |
| --- | --- | --- |
| `host` | required | Element or CSS selector. It must have a size; the preview fills it. |
| `formats` | all | `qr_code`, `micro_qr`, `data_matrix`, `aztec`, `pdf417`, `code_128`, `code_39`, `code_93`, `ean_13`, `ean_8`, `upc_a`, `upc_e`, `itf`, `codabar`. |
| `regionOfInterest` | `{ x: .1, y: .2, width: .8, height: .6 }` | Part of the **visible** preview to scan (fractions of the host). Smaller = faster. |
| `duplicateIntervalMs` | `1500` | Delay before the same content is reported again. |
| `maxResults` | `1` | `> 1` detects several codes per frame (slower). |
| `tryInvert` | `true` | Also read light-on-dark codes. |
| `preprocessing` | `true` | Contrast / binarization fallbacks for hard frames. |
| `linearConfirmations` | `2` | Agreeing reads before a 1D code is reported. Weak 1D formats (Code 39/93, ITF, Codabar, UPC-E, EAN-8) need one more; 2D codes are reported on the first read. |
| `camera` | `{ facingMode: 'environment', resolution: 'hd' }` | Also `deviceId`, `rememberDevice` (default `true`). |
| `overlay` | all on | `showRegion`, `showDetections`, `showTorchButton`, `showCameraSwitch`, `color`, or `false`. |
| `feedback` | beep + vibrate | `{ beep, vibrate }` or `false`. |
| `processing` | `{ maxDimension: 1024, backend: 'auto' }` | Decoder image size; `'webgl'`, `'canvas'` or `'auto'`. |
| `worker` | `true` | Decode in a Web Worker; falls back to the main thread if unavailable. |
| `wasmUrl`, `workerUrl` | auto | Override asset URLs (CDN, custom bundler setup). |

### Reader API

- `start()`, `stop()`, `pause()`, `resume()`, `destroy()`
- `on('detected' | 'error' | 'started' | 'stopped', listener)` → returns an unsubscribe function
- `configure({ formats, duplicateIntervalMs, … })` — change decoding options live
- `setRegionOfInterest(region)`
- `setTorch(on)`, `torchAvailable`, `setZoom(zoom)`, `switchCamera(deviceId?)`, `BarcodeReader.listCameras()`
- `clearCooldown(text)` — allow a code to be reported again immediately
- `stats` — `{ fps, decodeMs, grabMs, frameWidth, frameHeight, backend, worker, simd }`
- `video` — the underlying `<video>` element

Detection points are in video pixels (`video.videoWidth` space).

## Scanning a still image

```ts
import { scanImage } from '@gueshbzh-perso/scanner';

const codes = await scanImage(file); // File/Blob, ImageData, ImageBitmap, <img>, <canvas>…
```

Still images go through every strategy (rotations, inversion, contrast…) and
up to 8 codes are returned, with points in the source image's pixels.

## Bundlers

The SDK loads its worker and WebAssembly binary with the standard
`new URL('./file', import.meta.url)` pattern, supported by Vite, webpack 5,
Rollup, esbuild-based tools and plain browser ES modules.

**Vite / Nuxt**: exclude the package from dependency pre-bundling, otherwise
the asset URLs are rewritten to a folder where the files do not exist:

```ts
// vite.config.ts
export default { optimizeDeps: { exclude: ['@gueshbzh-perso/scanner'] } };
```

If your setup cannot resolve the assets, copy `node_modules/@gueshbzh-perso/scanner/dist`
to your public folder and pass `workerUrl` / `wasmUrl` explicitly.

## How it works

```
camera ─▶ <video> ─▶ WebGL2: crop to ROI · RGB→luma · box-filter downscale · pack 4 px/texel
                         │  async readback (pixel-pack buffer + fence) — width×height bytes
                         ▼
                Web Worker ─▶ WASM memory (no extra copy) ─▶ Rust engine
                                                             │ fast pass every frame
                                                             │ + 1 fallback per frame (CPU-time share)
                                                             │   (try-harder, inverted, CLAHE, Sauvola)
                                                             │ 1D quiet-zone check · multi-frame confirmation · de-dup
                         ◀────────────── new codes + live detections
overlay (ROI mask, highlights, torch / camera buttons) · beep · vibration
```

- One frame in flight at a time: when the decoder is busy, frames are skipped
  instead of queued, so latency never builds up.
- The SIMD build of the engine is picked automatically when the browser supports it.
- If WebGL2 is missing or the context is lost, the pipeline switches to Canvas 2D.
- If the engine crashes, the worker is restarted transparently.

## Browser support

Any browser with WebAssembly, `getUserMedia` and ES modules in workers:
Chrome/Edge 80+, Firefox 114+, Safari 15+. Camera access requires HTTPS (or `localhost`).

## Development

This package is part of an npm workspace: run commands from the repository root
(see the [root README](../../README.md)). A Nuxt 4 demo lives in [`apps/demo-nuxt`](../../apps/demo-nuxt).

```bash
npm install        # at the repository root
npm run build      # Rust → WASM (baseline + SIMD), then TypeScript
npm run test:e2e   # headless Chromium with a fake camera (needs ffmpeg + Playwright)
```

`npm run build:wasm` only needs Rust (rustup + cargo): the `wasm32-unknown-unknown`
target and the `wasm-bindgen-cli` version pinned in `Cargo.lock` are installed
automatically on first run; `wasm-opt` comes from the `binaryen` devDependency.

## License

MIT
