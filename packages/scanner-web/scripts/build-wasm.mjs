#!/usr/bin/env node
// Builds the Rust engine into two WebAssembly binaries (baseline + SIMD)
// and generates the wasm-bindgen JS glue into src/wasm/.
//
// Requirements: Rust (rustup + cargo). The wasm32 target and the wasm-bindgen
// CLI (pinned to the version in Cargo.lock) are installed automatically.
// Optional:     wasm-opt (binaryen) on PATH for a smaller/faster binary.
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
mkdirSync(outDir, { recursive: true });

const base = build('base', '');
const simd = build('simd', '-C target-feature=+simd128');

// Both builds export the exact same API, so a single JS glue serves both.
const glue = (dir) => readFileSync(join(dir, 'scanner_wasm.js'), 'utf8');
if (glue(base) !== glue(simd)) {
  throw new Error('wasm-bindgen glue differs between baseline and SIMD builds');
}

for (const f of ['scanner_wasm.js', 'scanner_wasm.d.ts', 'scanner_wasm_bg.wasm.d.ts']) {
  copyFileSync(join(base, f), join(outDir, f));
}
const targets = [
  [join(base, 'scanner_wasm_bg.wasm'), join(outDir, 'scanner_wasm_bg.wasm'), []],
  [join(simd, 'scanner_wasm_bg.wasm'), join(outDir, 'scanner_wasm_simd_bg.wasm'), ['--enable-simd']],
];

const wasmOpt = has('wasm-opt');
if (!wasmOpt)
  console.warn(
    '\n⚠ wasm-opt not found: skipping binary optimisation (install binaryen for ~20% smaller output)',
  );
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
