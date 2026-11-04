#!/usr/bin/env node
// Copies the generated wasm-bindgen glue, binaries and third-party notices next
// to the compiled SDK, and the project LICENSE into the package.
import { copyFileSync, cpSync, existsSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(pkgDir, 'src/wasm');
if (!existsSync(join(src, 'scanner_wasm.js'))) {
  console.error('src/wasm is empty: run `npm run build:wasm` first');
  process.exit(1);
}
// Replace, not merge: a file left by an older build must not ship.
rmSync(join(pkgDir, 'dist/wasm'), { recursive: true, force: true });
cpSync(src, join(pkgDir, 'dist/wasm'), { recursive: true });
// The project license, at the package root where npm always includes it.
copyFileSync(join(pkgDir, '../../LICENSE'), join(pkgDir, 'LICENSE'));
