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
tests/fixtures/     Committed test images (+ generate.py to regenerate them)
```

**Live demo:** https://romainrodrigues.github.io/scanner/ (deployed from `main`).

## Development

Requirements: Rust (rustup) and Node.js 20+. Everything else (wasm32 target,
`wasm-bindgen-cli` pinned by `Cargo.lock`) is installed on the first build.

```bash
npm install          # once, at the root (npm workspaces: SDK + demo)
npm run build        # Rust → WASM (baseline + SIMD) + TypeScript
npm run dev          # demo on http://localhost:3000
npm run tunnel       # demo + Cloudflare tunnel (https URL + QR code) for your phone

npm run lint && npm run typecheck
npm run test:e2e     # headless Chromium with a fake camera (ffmpeg + Playwright)
cargo test --workspace --release
```

## Continuous integration

- **CI** (`.github/workflows/ci.yml`, every push to `main` and every pull request):
  Rust fmt / clippy / tests / docs, WASM tests in Node.js, ESLint + Prettier,
  SDK build, type checks, end-to-end tests with a fake camera, demo build.
- **Pages** (`.github/workflows/pages.yml`, every push to `main`): builds the Nuxt
  demo and deploys it to GitHub Pages.

Contributing rules, known pitfalls and conventions: [CLAUDE.md](CLAUDE.md).
French documentation: [DOCUMENTATION_FR.md](DOCUMENTATION_FR.md). Changes: [CHANGELOG.md](CHANGELOG.md).

## License

MIT
