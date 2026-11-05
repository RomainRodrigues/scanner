// Engine core: owns the WebAssembly module and its objects.
//
// Two roles, so that frames can be decoded on several threads while a single
// session keeps the state (strategy scheduling, multi-frame confirmation,
// de-duplication):
// - `SessionCore`: the session. Cheap calls (`plan`, `commit`), main thread.
// - `DecoderCore`: the expensive decoding, one per Web Worker.
// The inline engine (no Worker support) uses both on the main thread.
// Codes read by the browser's native `BarcodeDetector` enter the session with
// `SessionCore.commitExternal` (see `native.ts`).
import init, { WasmFrameDecoder, WasmScanner, WasmScanSession, version } from './wasm/scanner_wasm.js';
import type { FramePlan, FrameReport, PixelFormat, ScanResult, WasmScanConfig } from './wasm/scanner_wasm.js';
import type { Detection, EngineOptions } from './types.js';

export type { FramePlan, FrameReport };

/** Default location of the engine binary, next to this module. */
export function defaultWasmUrl(): URL {
  return new URL('./wasm/scanner_wasm_bg.wasm', import.meta.url);
}

/**
 * Compile the engine once. The compiled module can be posted to any number
 * of workers, which then only instantiate it (no second download or compile).
 */
export async function compileWasm(url: string | URL): Promise<WebAssembly.Module> {
  const response = fetch(url);
  try {
    return await WebAssembly.compileStreaming(response);
  } catch {
    // Servers that do not send `application/wasm` reject streaming compilation.
    return WebAssembly.compile(await (await response).arrayBuffer());
  }
}

let initPromise: Promise<string> | null = null;

/** Instantiate the engine in this realm (once). Returns the engine version. */
export function loadWasm(source: string | URL | WebAssembly.Module): Promise<string> {
  initPromise ??= init({ module_or_path: source }).then(() => version());
  return initPromise;
}

/** Keep only the engine options (drops URLs and other non-cloneable values). */
export function pickEngineOptions(o: EngineOptions): EngineOptions {
  const { formats, preprocessing, tryInvert, maxResults, duplicateIntervalMs, linearConfirmations } = o;
  return { formats, preprocessing, tryInvert, maxResults, duplicateIntervalMs, linearConfirmations };
}

export function toWasmConfig(o: EngineOptions, extra: Partial<WasmScanConfig> = {}): WasmScanConfig {
  const cfg: WasmScanConfig = { ...extra };
  if (o.formats) cfg.formats = o.formats;
  if (o.preprocessing !== undefined) cfg.preprocessing = o.preprocessing;
  if (o.tryInvert !== undefined) cfg.tryInvert = o.tryInvert;
  if (o.maxResults !== undefined) cfg.maxResults = o.maxResults;
  cfg.dedupCooldownMs = o.duplicateIntervalMs ?? 1500;
  if (o.linearConfirmations !== undefined) cfg.linearConfirmations = o.linearConfirmations;
  return cfg;
}

/**
 * Configuration of camera sessions and their decoders: they must be equal,
 * since a plan refers to the decoder's list of fallback strategies.
 */
function streamConfig(o: EngineOptions): WasmScanConfig {
  // Back-pressure is handled by the SDK (frames are skipped, never queued),
  // so the engine's own frame-skipping throttle would only drop useful frames.
  return toWasmConfig(o, { adaptiveThrottle: false });
}

export function toDetection(r: ScanResult): Detection {
  const d: Detection = { text: r.text, format: r.format, points: r.points, timestamp: r.timestampMs };
  if (r.rawBytes) d.rawBytes = r.rawBytes;
  return d;
}

function toScanResult(d: Detection): ScanResult {
  const r: ScanResult = { text: d.text, format: d.format, points: d.points, timestampMs: d.timestamp };
  if (d.rawBytes) r.rawBytes = d.rawBytes;
  return r;
}

export interface FrameOutcome {
  /** New codes, or `null`. */
  results: Detection[] | null;
  /** Every code seen in this frame (including duplicates in cooldown). */
  live: Detection[];
  decodeMs: number;
}

/** The scan session of a camera stream (state only, cheap calls). */
export class SessionCore {
  private session: WasmScanSession | null = null;

  configure(options: EngineOptions): void {
    this.session?.free();
    this.session = new WasmScanSession(streamConfig(options));
  }

  /**
   * Strategy plan of the next frame, for a `DecoderCore` configured the same
   * way. `fastPath: false` when the native `BarcodeDetector` already searched
   * the frame: the decoder then runs the fallback strategy only.
   */
  plan(fastPath = true): FramePlan {
    const session = this.active();
    return fastPath ? session.plan() : session.planFallbackOnly();
  }

  /** Record a decoded frame; returns the new codes and the live ones. */
  commit(report: FrameReport): FrameOutcome {
    const session = this.active();
    const fresh = session.commit(report);
    const results = fresh ? fresh.map(toDetection) : null;
    return { results, live: results ?? session.lastDetections.map(toDetection), decodeMs: report.elapsedMs };
  }

  /**
   * Record codes read by the native `BarcodeDetector`: same format filter,
   * confirmation and de-duplication as decoded frames. Points are kept as
   * given (video pixels).
   */
  commitExternal(codes: Detection[], elapsedMs: number): FrameOutcome {
    const session = this.active();
    const fresh = session.commitExternal(codes.map(toScanResult), elapsedMs);
    const results = fresh ? fresh.map(toDetection) : null;
    return { results, live: results ?? session.lastDetections.map(toDetection), decodeMs: elapsedMs };
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
  }

  private active(): WasmScanSession {
    if (!this.session) throw new Error('Engine not configured');
    return this.session;
  }
}

/** Frame decoding (stateless) and still-image scanning: the expensive part. */
export class DecoderCore {
  private decoder: WasmFrameDecoder | null = null;
  private scanners = new Map<string, WasmScanner>();

  configure(options: EngineOptions): void {
    this.decoder?.free();
    this.decoder = new WasmFrameDecoder(streamConfig(options));
  }

  decode(
    pixels: Uint8Array,
    width: number,
    height: number,
    format: PixelFormat,
    plan: FramePlan,
  ): FrameReport {
    const decoder = this.decoder;
    if (!decoder) throw new Error('Engine not configured');
    decoder.frameBuffer(pixels.length).set(pixels);
    return decoder.decodeFrameBuffer(width, height, format, plan);
  }

  /** One-shot scan of a still image, running every strategy. */
  scan(
    pixels: Uint8Array,
    width: number,
    height: number,
    format: PixelFormat,
    options: EngineOptions,
  ): Detection[] {
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

  destroy(): void {
    this.decoder?.free();
    this.decoder = null;
    for (const s of this.scanners.values()) s.free();
    this.scanners.clear();
  }
}
