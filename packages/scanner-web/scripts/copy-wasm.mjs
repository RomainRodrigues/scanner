#!/usr/bin/env node
// Copies the generated wasm-bindgen glue and binaries next to the compiled SDK.
import { cpSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(pkgDir, 'src/wasm');
if (!existsSync(join(src, 'scanner_wasm.js'))) {
  console.error('src/wasm is empty: run `npm run build:wasm` first');
  process.exit(1);
}
cpSync(src, join(pkgDir, 'dist/wasm'), { recursive: true });
