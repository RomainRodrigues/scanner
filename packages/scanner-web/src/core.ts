// Engine core: owns the WebAssembly module and the scan session.
// Runs either inside the Web Worker (default) or on the main thread (fallback).
import init, { WasmScanner, WasmScanSession, version } from './wasm/scanner_wasm.js';
import type { ScanResult, WasmScanConfig, PixelFormat } from './wasm/scanner_wasm.js';
import type { Detection, EngineOptions } from './types.js';

/** Feature-detects WebAssembly SIMD (same probe as `wasm-feature-detect`). */
export function simdSupported(): boolean {
  try {
    return WebAssembly.validate(
      new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11]),
    );
  } catch {
    return false;
  }
}

/** Default location of the engine binary, next to this module. */
export function defaultWasmUrl(simd = simdSupported()): URL {
  return simd
    ? new URL('./wasm/scanner_wasm_simd_bg.wasm', import.meta.url)
    : new URL('./wasm/scanner_wasm_bg.wasm', import.meta.url);
}

/** Keep only the engine options (drops URLs and other non-cloneable values). */
export function pickEngineOptions(o: EngineOptions): EngineOptions {
  const { formats, preprocessing, tryInvert, maxResults, duplicateIntervalMs, weakFormatConfirmations } = o;
  return { formats, preprocessing, tryInvert, maxResults, duplicateIntervalMs, weakFormatConfirmations };
}

export function toWasmConfig(o: EngineOptions, extra: Partial<WasmScanConfig> = {}): WasmScanConfig {
  const cfg: WasmScanConfig = { ...extra };
  if (o.formats) cfg.formats = o.formats;
  if (o.preprocessing !== undefined) cfg.preprocessing = o.preprocessing;
  if (o.tryInvert !== undefined) cfg.tryInvert = o.tryInvert;
  if (o.maxResults !== undefined) cfg.maxResults = o.maxResults;
  cfg.dedupCooldownMs = o.duplicateIntervalMs ?? 1500;
  if (o.weakFormatConfirmations !== undefined) cfg.weakFormatConfirmations = o.weakFormatConfirmations;
  return cfg;
}

export function toDetection(r: ScanResult): Detection {
  const d: Detection = { text: r.text, format: r.format, points: r.points, timestamp: r.timestampMs };
  if (r.rawBytes) d.rawBytes = r.rawBytes;
  return d;
}

export interface FrameOutcome {
  /** New codes, or `null`. */
  results: Detection[] | null;
  /** Every code seen in this frame (including duplicates in cooldown). */
  live: Detection[];
  decodeMs: number;
}

let initPromise: Promise<unknown> | null = null;

export class EngineCore {
  private session: WasmScanSession | null = null;
  private scanners = new Map<string, WasmScanner>();

  private constructor(readonly version: string) {}

  static async create(wasmUrl?: string | URL): Promise<EngineCore> {
    initPromise ??= init({ module_or_path: wasmUrl ?? defaultWasmUrl() });
    await initPromise;
    return new EngineCore(version());
  }

  configure(options: EngineOptions): void {
    this.session?.free();
    // Back-pressure is handled by the SDK (one frame in flight), so the
    // engine's own frame-skipping throttle would only drop useful frames.
    this.session = new WasmScanSession(toWasmConfig(options, { adaptiveThrottle: false }));
  }

  process(pixels: Uint8Array, width: number, height: number, format: PixelFormat): FrameOutcome {
    const session = this.session;
    if (!session) throw new Error('Engine not configured');
    // Write straight into WASM memory: avoids one more copy of the frame.
    session.frameBuffer(pixels.length).set(pixels);
    const t0 = performance.now();
    const results = session.processFrameBuffer(width, height, format);
    const decodeMs = performance.now() - t0;
    const fresh = results ? results.map(toDetection) : null;
    return { results: fresh, live: fresh ?? session.lastDetections.map(toDetection), decodeMs };
  }

  scan(pixels: Uint8Array, width: number, height: number, format: PixelFormat, options: EngineOptions): Detection[] {
    const config = toWasmConfig(options);
    const key = JSON.stringify(config);
    let scanner = this.scanners.get(key);
    if (!scanner) {
      if (this.scanners.size >= 4) {
        const [oldKey, old] = this.scanners.entries().next().value as [string, WasmScanner];
        old.free();
        this.scanners.delete(oldKey);
      }
      scanner = new WasmScanner(config);
      this.scanners.set(key, scanner);
    }
    scanner.frameBuffer(pixels.length).set(pixels);
    return scanner.scanFrameBuffer(width, height, format).map(toDetection);
  }

  reset(): void {
    this.session?.reset();
  }

  clearCooldown(text: string): void {
    this.session?.clearCooldown(text);
  }

  destroy(): void {
    this.session?.free();
    this.session = null;
    for (const s of this.scanners.values()) s.free();
    this.scanners.clear();
  }
}
