#!/usr/bin/env node
// Builds the Rust engine into a WebAssembly binary and generates the
// wasm-bindgen JS glue into src/wasm/.
//
// A single baseline build: a second `+simd128` build used to ship too, but
// auto-vectorisation brought no measurable gain on this branchy decoder
// (Node 22, textured 1024×432 frames: 89 ms baseline vs 84-93 ms SIMD) for
// twice the build time and package size.
//
// Requirements: Rust (rustup + cargo). The wasm32 target and the wasm-bindgen
// CLI (pinned to the version in Cargo.lock) are installed automatically.
// wasm-opt comes from the `binaryen` devDependency (npm puts node_modules/.bin
// on PATH); without it the build still works, just ~7% larger.
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeThirdPartyLicenses } from './licenses.mjs';

const pkgDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoDir = resolve(pkgDir, '../..');
const outDir = join(pkgDir, 'src/wasm');
const tmpDir = join(pkgDir, '.wasm-tmp');

const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: repoDir, stdio: 'inherit', env: { ...process.env, ...env } });

const has = (cmd) => {
  try {
    execFileSync(cmd, ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
};

const output = (cmd, args) => {
  try {
    return execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {
    return null;
  }
};

// ── Prerequisites ────────────────────────────────────────────────────────────

function ensureWasmTarget() {
  const installed = output('rustup', ['target', 'list', '--installed']);
  if (installed === null) {
    console.warn('⚠ rustup not found: assuming the wasm32-unknown-unknown target is installed');
    return;
  }
  if (!installed.split('\n').includes('wasm32-unknown-unknown')) {
    console.log('▶ Installing the wasm32-unknown-unknown Rust target');
    run('rustup', ['target', 'add', 'wasm32-unknown-unknown']);
  }
}

/** wasm-bindgen CLI must match the wasm-bindgen crate version exactly. */
function ensureWasmBindgen() {
  const lock = readFileSync(join(repoDir, 'Cargo.lock'), 'utf8');
  const wanted = /name = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)?.[1];
  if (!wanted) throw new Error('wasm-bindgen not found in Cargo.lock');

  const cargoBin = join(process.env.CARGO_HOME ?? join(homedir(), '.cargo'), 'bin');
  const candidates = ['wasm-bindgen', join(cargoBin, 'wasm-bindgen')];
  for (const bin of candidates) {
    const v = output(bin, ['--version'])?.trim().split(/\s+/)[1];
    if (v === wanted) return bin;
    if (v) console.log(`▶ ${bin} is ${v}, the project needs ${wanted}`);
  }

  console.log(`▶ Installing wasm-bindgen-cli ${wanted} (one-time, takes a minute or two)`);
  run('cargo', ['install', 'wasm-bindgen-cli', '--version', wanted, '--locked', '--force']);
  const bin = join(cargoBin, 'wasm-bindgen');
  return existsSync(bin) ? bin : 'wasm-bindgen';
}

ensureWasmTarget();
const wasmBindgen = ensureWasmBindgen();

// ── Build ────────────────────────────────────────────────────────────────────

function build(variant, rustflags) {
  const targetDir = join(repoDir, 'target', `wasm-${variant}`);
  console.log(`\n▶ cargo build (${variant})`);
  run(
    'cargo',
    [
      'build',
      '-p',
      'scanner-wasm',
      '--release',
      '--target',
      'wasm32-unknown-unknown',
      '--target-dir',
      targetDir,
    ],
    { RUSTFLAGS: rustflags },
  );
  const wasm = join(targetDir, 'wasm32-unknown-unknown/release/scanner_wasm.wasm');
  const dest = join(tmpDir, variant);
  run(wasmBindgen, ['--target', 'web', '--weak-refs', '--out-dir', dest, '--out-name', 'scanner_wasm', wasm]);
  return dest;
}

rmSync(tmpDir, { recursive: true, force: true });
// Start from an empty output: a file left by an older build must not ship.
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const base = build('base', '');

for (const f of ['scanner_wasm.js', 'scanner_wasm.d.ts', 'scanner_wasm_bg.wasm.d.ts']) {
  copyFileSync(join(base, f), join(outDir, f));
}
const targets = [[join(base, 'scanner_wasm_bg.wasm'), join(outDir, 'scanner_wasm_bg.wasm'), []]];

const wasmOpt = has('wasm-opt');
if (wasmOpt) console.log(`\n▶ ${output('wasm-opt', ['--version'])?.trim()}`);
if (!wasmOpt)
  console.warn('\n⚠ wasm-opt not found: skipping binary optimisation (run `npm install` to get binaryen)');
for (const [src, dest, flags] of targets) {
  if (wasmOpt) {
    run('wasm-opt', [
      '-O3',
      '--enable-bulk-memory',
      '--enable-nontrapping-float-to-int',
      '--enable-sign-ext',
      '--enable-mutable-globals',
      '--enable-reference-types',
      '--enable-multivalue',
      ...flags,
      src,
      '-o',
      dest,
    ]);
  } else {
    copyFileSync(src, dest);
  }
  console.log(`✓ ${dest.replace(pkgDir + '/', '')} (${(statSync(dest).size / 1024).toFixed(0)} KiB)`);
}

rmSync(tmpDir, { recursive: true, force: true });

// License notices of the Rust crates compiled into the engine (shipped with it).
const notices = writeThirdPartyLicenses(repoDir, join(outDir, 'THIRD_PARTY_LICENSES.md'));
console.log(`✓ src/wasm/THIRD_PARTY_LICENSES.md (${notices.crates} crates, ${notices.texts} license texts)`);
