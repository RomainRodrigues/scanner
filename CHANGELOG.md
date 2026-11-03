# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0]

### Added
- TypeScript SDK `@gueshbzh-perso/scanner` (`packages/scanner-web`): `BarcodeReader`
  (camera, torch, zoom, camera switch, overlay, beep / vibration) and `scanImage()`.
- WebGL2 frame pipeline (ROI crop, luma, box-filtered downscale, 4 px per texel,
  asynchronous readback) with Canvas 2D fallback.
- Decoding in a Web Worker, one frame in flight, automatic restart after an engine crash.
- Baseline and SIMD WebAssembly builds, selected at runtime.
- Real multi-code detection (`maxResults > 1`).
- Quiet-zone verification of 1D results and multi-frame confirmation
  (`linearConfirmations`) against false positives.
- Nuxt 4 demo (`apps/demo-nuxt`), deployed to GitHub Pages; `npm run tunnel` for phones.
- CI: Rust, WASM, lint, type checks, end-to-end tests with a fake camera; Pages deployment.

### Changed
- `ScanSession` runs the fast path plus one rotating fallback strategy per frame
  (~6.6× cheaper empty frames).
- Result `format` uses the same snake_case names as the `formats` option; points are
  `{ x, y }` objects in full-frame coordinates (ROI and rotations undone).
- rxing default features disabled; only the decoders in use are compiled.
- npm workspaces at the repository root; test images are committed in `tests/fixtures/`.

### Removed
- `scanner-cli` and `scanner-napi` crates.
- `confidence` and `orientation` result fields (they were constant).

### Fixed
- Result points were relative to the ROI crop.
- `Unknown` results (RSS, Telepen…) were returned when no format was configured.
- UPC-E false positives on textures.
