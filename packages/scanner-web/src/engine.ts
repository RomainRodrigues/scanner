// Decoding engine facade: same API whether decoding runs in Web Workers or inline.
//
// With workers, frames are decoded **in parallel** by a pool of stateless
// decoders (one WebAssembly instance per worker), while the scan session
// (strategy scheduling, multi-frame confirmation, de-duplication) runs on the
// main thread: its calls take microseconds, and a single instance of the
// state is what keeps confirmations and de-duplication correct across
// workers. Decoding is CPU-bound and single-threaded per frame, so on phones
// with many slow cores (Galaxy A14: decode ~125 ms) the pool multiplies the
// frame rate.
import { DecoderCore, SessionCore, compileWasm, defaultWasmUrl, loadWasm } from './core.js';
import type { FramePlan, FrameReport } from './core.js';
import type { WorkerRequest, WorkerResponse } from './protocol.js';
import type { PixelFormat } from './wasm/scanner_wasm.js';
import type { AssetOptions, Detection, EngineOptions } from './types.js';

export interface FrameReply {
  results: Detection[] | null;
  live: Detection[];
  decodeMs: number;
  /** The pixel buffer, handed back for reuse. */
  buffer: ArrayBuffer;
}

export interface Engine {
  readonly inWorker: boolean;
  readonly version: string;
  /** Frames that can be decoded at the same time (number of workers). */
  readonly concurrency: number;
  configure(options: EngineOptions): Promise<void>;
  /** Decode a camera frame. `buffer` is transferred and returned in the reply. */
  processFrame(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat): Promise<FrameReply>;
  /** One-shot scan of a still image, running every strategy. */
  scanImage(
    buffer: ArrayBuffer,
    width: number,
    height: number,
    format: PixelFormat,
    options: EngineOptions,
  ): Promise<Detection[]>;
  reset(): Promise<void>;
  clearCooldown(text: string): Promise<void>;
  destroy(): void;
}

/**
 * Default pool size: one worker per core, minus one for the main thread
 * (camera, GPU, UI), capped at 3. Beyond 3 the frame rate is limited by the
 * frame grab (~55 ms on a Galaxy A14, so ~18 fps) and more workers only cost
 * memory (one WebAssembly instance each).
 */
export function defaultWorkerCount(): number {
  const cores = typeof navigator !== 'undefined' ? navigator.hardwareConcurrency || 2 : 2;
  return Math.min(3, Math.max(1, cores - 1));
}

type Distribute<T> = T extends unknown ? Omit<T, 'id'> : never;
type Request = Distribute<WorkerRequest>;
type OkResponse = Extract<WorkerResponse, { ok: true }>;

/** One decoding worker, restarted if the engine crashes. */
class DecoderWorker {
  version = '';
  /** Requests sent and not answered yet. */
  busy = 0;

  /** `false` once the worker failed for good (script error, crash outside a request). */
  get alive(): boolean {
    return this.dead === null;
  }
  private worker!: Worker;
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: WorkerResponse) => void; reject: (e: Error) => void }>();
  private dead: Error | null = null;
  private options: EngineOptions | null = null;
  private restarting: Promise<void> | null = null;

  constructor(
    private spawn: () => Worker,
    private module: WebAssembly.Module,
  ) {
    this.attach(spawn());
  }

  private attach(worker: Worker) {
    this.worker = worker;
    worker.onmessage = (ev: MessageEvent<WorkerResponse>) => {
      const p = this.pending.get(ev.data.id);
      if (!p) return;
      this.pending.delete(ev.data.id);
      p.resolve(ev.data);
    };
    worker.onerror = (ev) => {
      ev.preventDefault();
      this.fail(new Error(ev.message || 'Scanner worker failed to load'));
    };
  }

  private fail(err: Error) {
    this.dead = err;
    for (const p of this.pending.values()) p.reject(err);
    this.pending.clear();
  }

  private call(req: Request, transfer: Transferable[] = []): Promise<WorkerResponse> {
    if (this.dead) return Promise.reject(this.dead);
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ ...req, id }, transfer);
    });
  }

  /** Send a request; on a fatal engine crash, restart the worker before throwing. */
  async ok(req: Request, transfer?: Transferable[]): Promise<OkResponse> {
    this.busy++;
    try {
      if (this.restarting) await this.restarting;
      const res = await this.call(req, transfer);
      if (!res.ok) {
        if (res.fatal) await this.restart();
        throw Object.assign(new Error(res.error), { buffer: res.buffer });
      }
      return res;
    } finally {
      this.busy--;
    }
  }

  private restart(): Promise<void> {
    this.restarting ??= (async () => {
      console.warn('[scanner] engine crashed, restarting worker');
      this.worker.terminate();
      this.attach(this.spawn());
      await this.init();
      if (this.options) await this.call({ type: 'configure', options: this.options });
    })().finally(() => (this.restarting = null));
    return this.restarting;
  }

  async init() {
    const res = await this.call({ type: 'init', module: this.module });
    if (!res.ok) throw new Error(res.error);
    this.version = res.version ?? '';
  }

  async configure(options: EngineOptions) {
    this.options = options;
    await this.ok({ type: 'configure', options });
  }

  destroy() {
    this.fail(new Error('Engine destroyed'));
    this.worker.terminate();
  }
}

/** A pool of decoding workers serving one scan session on the main thread. */
class WorkerPoolEngine implements Engine {
  readonly inWorker = true;
  readonly version: string;
  private session = new SessionCore();
  private configured = false;
  /** Bumped by `configure`: reports planned with an older configuration are dropped. */
  private generation = 0;

  constructor(private workers: DecoderWorker[]) {
    this.version = workers[0]?.version ?? '';
  }

  get concurrency(): number {
    return this.workers.length;
  }

  async configure(options: EngineOptions) {
    // Session and decoders must share the configuration: a plan is an index
    // in the decoder's list of fallback strategies.
    this.generation++;
    this.session.configure(options);
    this.configured = true;
    await Promise.all(this.workers.map((w) => w.configure(options)));
  }

  async processFrame(
    buffer: ArrayBuffer,
    width: number,
    height: number,
    format: PixelFormat,
  ): Promise<FrameReply> {
    if (!this.configured) throw new Error('Engine not configured');
    const generation = this.generation;
    const plan = this.session.plan();
    let res: OkResponse;
    try {
      res = await this.idleWorker().ok({ type: 'decode', buffer, width, height, format, plan }, [buffer]);
    } catch (err) {
      // Every plan must be committed, or the scheduler keeps counting it as in flight.
      this.session.commit(emptyReport(plan));
      throw err;
    }
    const returned = res.buffer ?? new ArrayBuffer(width * height);
    if (generation !== this.generation || !res.report) {
      return { results: null, live: [], decodeMs: 0, buffer: returned };
    }
    return { ...this.session.commit(res.report), buffer: returned };
  }

  async scanImage(
    buffer: ArrayBuffer,
    width: number,
    height: number,
    format: PixelFormat,
    options: EngineOptions,
  ) {
    const res = await this.idleWorker().ok({ type: 'scan', buffer, width, height, format, options }, [
      buffer,
    ]);
    return res.results ?? [];
  }

  async reset() {
    this.session.reset();
  }

  async clearCooldown(text: string) {
    this.session.clearCooldown(text);
  }

  destroy() {
    for (const w of this.workers) w.destroy();
    this.session.destroy();
  }

  private idleWorker(): DecoderWorker {
    const worker = pickWorker(this.workers);
    if (!worker) throw new Error('Scanner engine unavailable: every decoding worker failed');
    return worker;
  }
}

/**
 * The live worker with the fewest requests in progress. Dead workers are
 * skipped: they reject instantly, so they always look the least busy and
 * would otherwise receive most frames.
 */
export function pickWorker<W extends { alive: boolean; busy: number }>(workers: readonly W[]): W | undefined {
  let best: W | undefined;
  for (const w of workers) {
    if (w.alive && (!best || w.busy < best.busy)) best = w;
  }
  return best;
}

function emptyReport(plan: FramePlan): FrameReport {
  return { plan, results: [], fallbackRan: false, fallbackRead: false, elapsedMs: 0 };
}

/** Everything on the main thread (no Worker support). */
class InlineEngine implements Engine {
  readonly inWorker = false;
  readonly concurrency = 1;
  private session = new SessionCore();
  private decoder = new DecoderCore();

  constructor(readonly version: string) {}

  async configure(options: EngineOptions) {
    this.session.configure(options);
  }

  async processFrame(
    buffer: ArrayBuffer,
    width: number,
    height: number,
    format: PixelFormat,
  ): Promise<FrameReply> {
    return { ...this.session.process(new Uint8Array(buffer), width, height, format), buffer };
  }

  async scanImage(
    buffer: ArrayBuffer,
    width: number,
    height: number,
    format: PixelFormat,
    options: EngineOptions,
  ) {
    return this.decoder.scan(new Uint8Array(buffer), width, height, format, options);
  }

  async reset() {
    this.session.reset();
  }

  async clearCooldown(text: string) {
    this.session.clearCooldown(text);
  }

  destroy() {
    this.session.destroy();
    this.decoder.destroy();
  }
}

/**
 * Load the engine: a pool of Web Workers when possible, else the main thread.
 *
 * Low-level API, for custom pipelines (frames you already have as pixels).
 * Most applications should use `BarcodeReader` or `scanImage`. The `Engine`
 * interface may gain members in minor versions.
 */
export async function createEngine(options: AssetOptions = {}): Promise<Engine> {
  const wasmUrl = new URL(String(options.wasmUrl ?? defaultWasmUrl()), location.href).href;
  // Compiled once here: the main thread needs the engine for the session,
  // and the workers instantiate the same module without compiling it again.
  const module = await compileWasm(wasmUrl);
  const version = await loadWasm(module);

  if (options.worker !== false && typeof Worker !== 'undefined') {
    const workerUrl = options.workerUrl;
    const spawn = () =>
      workerUrl
        ? new Worker(workerUrl, { type: 'module' })
        : new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    const count = Math.max(1, Math.floor(options.workers ?? defaultWorkerCount()));
    const workers: DecoderWorker[] = [];
    try {
      for (let i = 0; i < count; i++) workers.push(new DecoderWorker(spawn, module));
      await Promise.all(workers.map((w) => w.init()));
      return new WorkerPoolEngine(workers);
    } catch (err) {
      for (const w of workers) w.destroy();
      console.warn('[scanner] Web Worker unavailable, decoding on the main thread:', err);
    }
  }

  return new InlineEngine(version);
}

let shared: Promise<Engine> | null = null;

/** Engine shared by `scanImage` calls (one worker is enough for still images). */
export function sharedEngine(options?: AssetOptions): Promise<Engine> {
  shared ??= createEngine({ ...options, workers: 1 }).catch((err) => {
    shared = null;
    throw err;
  });
  return shared;
}
