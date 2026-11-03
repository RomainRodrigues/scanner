# Scanner — QR code & barcode scanning for the web

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

⚠️ **Work in progress**: APIs are still experimental.

A barcode and QR code scanner for web apps: a Rust engine compiled to
WebAssembly, driven by a TypeScript SDK that handles the camera, a WebGL2
frame pipeline, a Web Worker and the scanning UI. Everything runs on the
device, with no license key and no network calls.

```ts
import { BarcodeReader } from '@gueshbzh-perso/scanner';

const reader = await BarcodeReader.create({ host: '#scanner', formats: ['qr_code', 'ean_13'] });
reader.on('detected', (codes) => console.log(codes[0].text));
await reader.start();
```

See [`packages/scanner-web/README.md`](packages/scanner-web/README.md) for the full SDK documentation.

## Supported formats

| 2D | 1D |
| --- | --- |
| QR Code (incl. Micro QR), Data Matrix, Aztec, PDF417 | Code 128, Code 39, Code 93, EAN-13, EAN-8, UPC-A, UPC-E, ITF, Codabar |

## Repository layout

```
crates/
  scanner-core/     Rust engine: preprocessing, decoding strategies, real-time session
  scanner-wasm/     wasm-bindgen bindings (low-level engine API)
packages/
  scanner-web/      TypeScript SDK published to npm
apps/
  demo-nuxt/        Nuxt 4 demo (phone testing through a Cloudflare tunnel)
tests/images/       Test image generator
```

## Development

```bash
# Rust engine
python3 tests/images/generate_test_images.py   # pip install qrcode pillow python-barcode
cargo test --workspace --release

# SDK (needs Rust; the wasm target and wasm-bindgen-cli are installed automatically)
cd packages/scanner-web
npm install
npm run build
npm run test:e2e    # headless Chromium with a fake camera

# Demo (Nuxt 4)
cd apps/demo-nuxt
npm install && npm run dev          # http://localhost:3000
npm run phone                       # dev server + https tunnel for your phone (cloudflared)
```

French documentation: [DOCUMENTATION_FR.md](DOCUMENTATION_FR.md).

## License

MIT
