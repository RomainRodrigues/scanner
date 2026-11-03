// Decoding engine facade: same API whether decoding runs in a Web Worker or inline.
import { EngineCore, defaultWasmUrl, simdSupported } from './core.js';
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
  readonly simd: boolean;
  readonly version: string;
  configure(options: EngineOptions): Promise<void>;
  /** Decode a camera frame. `buffer` is transferred and returned in the reply. */
  processFrame(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat): Promise<FrameReply>;
  /** One-shot scan of a still image, running every strategy. */
  scanImage(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat, options: EngineOptions): Promise<Detection[]>;
  reset(): Promise<void>;
  clearCooldown(text: string): Promise<void>;
  destroy(): void;
}

type Distribute<T> = T extends unknown ? Omit<T, 'id'> : never;
type Request = Distribute<WorkerRequest>;

class WorkerEngine implements Engine {
  readonly inWorker = true;
  version = '';
  private worker!: Worker;
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: WorkerResponse) => void; reject: (e: Error) => void }>();
  private dead: Error | null = null;
  private options: EngineOptions | null = null;
  private restarting: Promise<void> | null = null;

  constructor(
    private spawn: () => Worker,
    private wasmUrl: string,
    readonly simd: boolean,
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
  private async ok(req: Request, transfer?: Transferable[]) {
    if (this.restarting) await this.restarting;
    const res = await this.call(req, transfer);
    if (!res.ok) {
      if (res.fatal) await this.restart();
      throw Object.assign(new Error(res.error), { buffer: res.buffer });
    }
    return res;
  }

  private restart(): Promise<void> {
    this.restarting ??= (async () => {
      console.warn('[scanner] engine crashed, restarting worker');
      this.worker.terminate();
      this.attach(this.spawn());
      await this.init();
      if (this.options) await this.ok({ type: 'configure', options: this.options });
    })().finally(() => (this.restarting = null));
    return this.restarting;
  }

  async init() {
    const res = await this.call({ type: 'init', wasmUrl: this.wasmUrl });
    if (!res.ok) throw new Error(res.error);
    this.version = res.version ?? '';
  }

  async configure(options: EngineOptions) {
    this.options = options;
    await this.ok({ type: 'configure', options });
  }

  async processFrame(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat): Promise<FrameReply> {
    const res = await this.ok({ type: 'frame', buffer, width, height, format }, [buffer]);
    return {
      results: (res.ok && res.results) || null,
      live: (res.ok && res.live) || [],
      decodeMs: (res.ok && res.decodeMs) || 0,
      buffer: res.buffer ?? new ArrayBuffer(width * height),
    };
  }

  async scanImage(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat, options: EngineOptions) {
    const res = await this.ok({ type: 'scan', buffer, width, height, format, options }, [buffer]);
    return (res.ok && res.results) || [];
  }

  async reset() {
    await this.ok({ type: 'reset' });
  }

  async clearCooldown(text: string) {
    await this.ok({ type: 'clearCooldown', text });
  }

  destroy() {
    this.fail(new Error('Engine destroyed'));
    this.worker.terminate();
  }
}

class InlineEngine implements Engine {
  readonly inWorker = false;
  readonly version: string;

  constructor(private core: EngineCore, readonly simd: boolean) {
    this.version = core.version;
  }

  async configure(options: EngineOptions) {
    this.core.configure(options);
  }

  async processFrame(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat): Promise<FrameReply> {
    return { ...this.core.process(new Uint8Array(buffer), width, height, format), buffer };
  }

  async scanImage(buffer: ArrayBuffer, width: number, height: number, format: PixelFormat, options: EngineOptions) {
    return this.core.scan(new Uint8Array(buffer), width, height, format, options);
  }

  async reset() {
    this.core.reset();
  }

  async clearCooldown(text: string) {
    this.core.clearCooldown(text);
  }

  destroy() {
    this.core.destroy();
  }
}

/** Load the engine, in a Web Worker when possible. */
export async function createEngine(options: AssetOptions = {}): Promise<Engine> {
  const simd = simdSupported();
  const wasmUrl = new URL(String(options.wasmUrl ?? defaultWasmUrl(simd)), location.href).href;

  if (options.worker !== false && typeof Worker !== 'undefined') {
    const workerUrl = options.workerUrl;
    const spawn = () =>
      workerUrl
        ? new Worker(workerUrl, { type: 'module' })
        : new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    let engine: WorkerEngine | null = null;
    try {
      engine = new WorkerEngine(spawn, wasmUrl, simd);
      await engine.init();
      return engine;
    } catch (err) {
      engine?.destroy();
      console.warn('[scanner] Web Worker unavailable, decoding on the main thread:', err);
    }
  }

  return new InlineEngine(await EngineCore.create(wasmUrl), simd);
}

let shared: Promise<Engine> | null = null;

/** Engine shared by `scanImage` calls. */
export function sharedEngine(options?: AssetOptions): Promise<Engine> {
  shared ??= createEngine(options).catch((err) => {
    shared = null;
    throw err;
  });
  return shared;
}
