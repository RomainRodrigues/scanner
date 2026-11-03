# scanner-wasm

Low-level WebAssembly bindings of `scanner-core`.

Most applications should use the SDK in [`packages/scanner-web`](../../packages/scanner-web)
(`@gueshbzh-perso/scanner`), which bundles this module together with camera
handling, the WebGL frame pipeline, a Web Worker and the overlay.

## API

```ts
import init, { WasmScanner, WasmScanSession } from './scanner_wasm.js';
await init({ module_or_path: wasmUrl });

// One-shot scan, every strategy (rotations, inversion, CLAHE, Sauvola)
const scanner = new WasmScanner({ formats: ['qr_code'], maxResults: 4 });
const codes = scanner.scanRgba(imageData.data, imageData.width, imageData.height);
// → [{ text, format: 'qr_code', points: [{ x, y }, …], rawBytes?, timestampMs }]

// Camera stream: write the frame straight into WASM memory
const session = new WasmScanSession({ dedupCooldownMs: 1500, adaptiveThrottle: false });
session.frameBuffer(width * height).set(luma);              // do not keep the view
const fresh = session.processFrameBuffer(width, height, 'luma'); // new codes or null
const live = session.lastDetections;                        // everything seen in that frame
session.free();
```

Config fields (all optional): `formats`, `preprocessing`, `tryRotations`,
`tryInvert`, `maxResults`, `roi`, `dedupCooldownMs`, `weakFormatConfirmations`,
`adaptiveThrottle`. Result `format` uses the same spelling as `formats`.

## Build

```bash
cd packages/scanner-web && npm run build:wasm
```

## Tests

```bash
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p scanner-wasm --target wasm32-unknown-unknown --test web
```
