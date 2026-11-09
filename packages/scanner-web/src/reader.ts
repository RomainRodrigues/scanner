// BarcodeReader: camera preview + frame pipeline + engine, wired together.
import { capabilities, listCameras, openCamera, rememberCamera, setTorch, setZoom } from './camera.js';
import { pickEngineOptions } from './core.js';
import { createEngine, type Engine, type FrameReply } from './engine.js';
import { Feedback } from './feedback.js';
import {
  clampRegion,
  coverLayout,
  decoderToVideo,
  outputSize,
  regionToVideoRect,
  videoToElement,
} from './geometry.js';
import { CanvasGrabber, createGrabber, type FrameGrabber, type Rect } from './grabber.js';
import type { FrameOutcome } from './core.js';
import { NativeDetector } from './native.js';
import { Preview } from './overlay.js';
import type { Detection, EngineOptions, ReaderEvents, ReaderOptions, ReaderStats, Region } from './types.js';

const DEFAULT_REGION: Region = { x: 0.1, y: 0.2, width: 0.8, height: 0.6 };

/**
 * With the native detector and `processing.engineFallback`, delay between two engine frames while the
 * native decoder finds nothing (~3 fallbacks per second). A code that only
 * the fallbacks read is still found within a couple of seconds (the
 * strategies share CPU time, the cheap Linear one runs most often), while
 * the workers stay idle most of the time. After a frame where the engine saw
 * a code, the next one is immediate: a 1D code needs a second read.
 */
const FALLBACK_INTERVAL_MS = 300;

/** A frame read from the video, waiting for (or being decoded by) the engine. */
interface GrabbedFrame {
  buffer: ArrayBuffer;
  width: number;
  height: number;
  /** Region of the video the frame was cropped from (to map points back). */
  rect: Rect;
  grabMs: number;
  /** Grab order: decodes finish out of order when several workers run. */
  seq: number;
  /** Run the engine's fast path (`false`: the native detector already searched this frame). */
  fastPath: boolean;
}

type Listener<T> = (payload: T) => void;

/**
 * Real-time barcode reader bound to a host element.
 *
 * ```ts
 * const reader = await BarcodeReader.create({ host: '#scanner', formats: ['qr_code', 'ean_13'] });
 * reader.on('detected', (codes) => console.log(codes[0].text));
 * await reader.start();
 * ```
 */
export class BarcodeReader {
  private preview: Preview;
  private grabber: FrameGrabber;
  private feedback: Feedback;
  private stream: MediaStream | null = null;
  private listeners: { [K in keyof ReaderEvents]: Set<Listener<ReaderEvents[K]>> } = {
    detected: new Set(),
    error: new Set(),
    started: new Set(),
    stopped: new Set(),
  };
  private region: Region;
  private engineOptions: EngineOptions;
  private running = false;
  private paused = false;
  /** A frame is being acquired: grab (GPU readback) and native detection. */
  private grabbing = false;
  /** Native `BarcodeDetector` used as the fast path, when enabled and available. */
  private native: NativeDetector | undefined;
  /** Bumped when the native detector is replaced: results of the old one are dropped. */
  private nativeGeneration = 0;
  /** The native detector failed once: not used again by this reader. */
  private nativeFailed = false;
  /** Native detector active: a fallback-only frame is being grabbed or decoded. */
  private fallbackInFlight = false;
  /** Native detector active: earliest time of the next fallback-only frame. */
  private nextFallbackAt = 0;
  /** Which decoder drew the current highlights: each one only clears its own. */
  private highlightSource: 'native' | 'engine' | null = null;
  /** Frames being decoded (at most `engine.concurrency`). */
  private decoding = 0;
  /** Sequence number of the last grabbed frame, and of the last one drawn. */
  private grabbed = 0;
  private drawn = 0;
  /** Grabbed frame waiting for the engine (at most one). */
  private pending: GrabbedFrame | null = null;
  private destroyed = false;
  /** Start in progress, shared by concurrent `start()` calls. */
  private starting: Promise<void> | null = null;
  /** Bumped by `stop()`: a start that sees another value was cancelled. */
  private lifecycle = 0;
  /** Consecutive failed frames: only the first error of a streak is reported. */
  private failures = 0;
  /** Camera released while the page is hidden (see `onVisibilityChange`). */
  private suspended = false;
  private readonly onVisibility = () => void this.onVisibilityChange();
  private torchOn = false;
  private buffers: ArrayBuffer[] = [];
  private frameHandle = 0;
  private perf = {
    frames: 0,
    engineFrames: 0,
    since: 0,
    fps: 0,
    engineFps: 0,
    decodeMs: 0,
    grabMs: 0,
    nativeMs: 0,
    w: 0,
    h: 0,
  };

  private constructor(
    private engine: Engine,
    private options: ReaderOptions,
    host: HTMLElement,
    native: NativeDetector | undefined,
  ) {
    this.native = native;
    this.region = clampRegion(options.regionOfInterest ?? DEFAULT_REGION);
    this.engineOptions = pickEngineOptions(options);
    this.preview = new Preview(host, options.overlay);
    this.preview.setRegion(this.region);
    this.grabber = createGrabber(options.processing?.backend);
    this.feedback = new Feedback(options.feedback ?? {});
    if (options.camera?.releaseWhenHidden !== false && typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.onVisibility);
    }
  }

  /** Load the engine (WebAssembly, in a Web Worker) and build the preview inside `options.host`. */
  static async create(options: ReaderOptions): Promise<BarcodeReader> {
    const host =
      typeof options.host === 'string' ? document.querySelector<HTMLElement>(options.host) : options.host;
    if (!host) throw new Error(`BarcodeReader: host element not found (${String(options.host)})`);
    const engine = await createEngine(options);
    const engineOptions = pickEngineOptions(options);
    await engine.configure(engineOptions);
    const native = await createNative(options, engineOptions);
    return new BarcodeReader(engine, options, host, native);
  }

  /** Video input devices (labels are empty until permission is granted). */
  static listCameras(): Promise<MediaDeviceInfo[]> {
    return listCameras();
  }

  on<K extends keyof ReaderEvents>(event: K, listener: Listener<ReaderEvents[K]>): () => void {
    this.listeners[event].add(listener);
    return () => this.listeners[event].delete(listener);
  }

  /** The `<video>` element showing the camera feed. */
  get video(): HTMLVideoElement {
    return this.preview.video;
  }

  get isRunning(): boolean {
    return this.running;
  }

  get stats(): ReaderStats {
    return {
      fps: this.perf.fps,
      decodeMs: this.perf.decodeMs,
      grabMs: this.perf.grabMs,
      frameWidth: this.perf.w,
      frameHeight: this.perf.h,
      backend: this.grabber.kind,
      worker: this.engine.inWorker,
      workers: this.engine.concurrency,
      native: this.native !== undefined,
      nativeMs: this.perf.nativeMs,
      engineFps: this.perf.engineFps,
    };
  }

  get engineVersion(): string {
    return this.engine.version;
  }

  /** Open the camera and start scanning. Call it from a user gesture to allow the beep on iOS. */
  start(): Promise<void> {
    this.assertAlive();
    if (this.running) return Promise.resolve();
    // Concurrent calls (double tap, React strict mode…) share one start: a
    // second getUserMedia would open a second stream that is never released.
    this.starting ??= this.open().finally(() => (this.starting = null));
    return this.starting;
  }

  private async open(): Promise<void> {
    const lifecycle = this.lifecycle;
    const cancelled = () => lifecycle !== this.lifecycle || this.destroyed;
    // Synchronous, before the first await: must run inside the user gesture.
    this.feedback.unlock();
    if (!(await this.acquireCamera(cancelled))) return;

    this.running = true;
    this.suspended = false;
    this.paused = false;
    this.failures = 0;
    this.perf.since = performance.now();
    this.perf.frames = 0;
    this.emit('started', undefined);
    this.schedule();
  }

  /** Open the camera into the preview. `false` if `cancelled()` became true meanwhile. */
  private async acquireCamera(cancelled: () => boolean): Promise<boolean> {
    const stream = await openCamera(this.options.camera);
    if (cancelled()) {
      // stop() or destroy() was called while the camera was opening.
      stream.getTracks().forEach((t) => t.stop());
      return false;
    }
    this.stream = stream;
    const video = this.preview.video;
    video.srcObject = stream;
    await video.play().catch(() => undefined);
    await waitForVideo(video);
    if (cancelled()) return false; // stop() already released `this.stream`.

    const track = this.track();
    if (this.options.camera?.rememberDevice !== false) rememberCamera(track?.getSettings().deviceId);
    this.torchOn = false;
    await this.refreshButtons();
    return !cancelled();
  }

  private releaseCamera(): void {
    this.cancelSchedule();
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.preview.video.srcObject = null;
    this.preview.setDetections([]);
    this.torchOn = false;
  }

  /**
   * Release the camera while the page is hidden, reopen it when it comes back.
   *
   * Decoding already stops on its own in a hidden page (no video frame
   * callbacks), but the camera stayed on: camera indicator lit, battery
   * drained, while the user cannot see the page. The reader stays "running"
   * through the suspension (no `stopped` / `started` events), so the host
   * application's UI state does not change. Opt out with
   * `camera.releaseWhenHidden: false`.
   */
  private async onVisibilityChange(): Promise<void> {
    if (document.hidden) {
      if (this.running && !this.suspended) {
        this.suspended = true;
        this.releaseCamera();
      }
      return;
    }
    if (!this.suspended || !this.running) return;
    this.suspended = false;
    const lifecycle = this.lifecycle;
    const cancelled = () => lifecycle !== this.lifecycle || this.destroyed || document.hidden;
    try {
      if (!(await this.acquireCamera(cancelled))) {
        // Hidden again while reopening: wait for the next visibility change.
        if (lifecycle === this.lifecycle && !this.destroyed) {
          this.suspended = true;
          this.releaseCamera();
        }
        return;
      }
    } catch (err) {
      // Camera taken by another app, permission revoked…: stop for good.
      this.emit('error', err instanceof Error ? err : new Error(String(err)));
      await this.stop();
      return;
    }
    this.failures = 0;
    this.schedule();
  }

  /** Stop scanning and release the camera. Also cancels a `start()` in progress. */
  async stop(): Promise<void> {
    this.lifecycle++;
    if (!this.running && !this.stream) return;
    const wasRunning = this.running;
    this.running = false;
    this.suspended = false;
    this.releaseCamera();
    if (wasRunning) this.emit('stopped', undefined);
  }

  /** Keep the camera running but stop decoding. */
  pause(): void {
    this.paused = true;
  }

  resume(): void {
    this.paused = false;
  }

  /** Stop everything and remove the preview from the page. The reader cannot be reused. */
  async destroy(): Promise<void> {
    if (this.destroyed) return;
    await this.stop();
    this.destroyed = true;
    document.removeEventListener('visibilitychange', this.onVisibility);
    this.grabber.destroy();
    this.preview.destroy();
    this.feedback.destroy();
    this.engine.destroy();
    for (const set of Object.values(this.listeners)) set.clear();
  }

  setRegionOfInterest(region: Region): void {
    this.region = clampRegion(region);
    this.preview.setRegion(this.region);
  }

  /** Change decoding options (formats, duplicate interval…) on the fly. Resets de-duplication. */
  async configure(options: EngineOptions): Promise<void> {
    this.assertAlive();
    this.engineOptions = { ...this.engineOptions, ...options };
    // Detections in flight were made for the old formats: dropped.
    const generation = ++this.nativeGeneration;
    await this.engine.configure(this.engineOptions);
    const native = this.nativeFailed ? undefined : await createNative(this.options, this.engineOptions);
    if (generation === this.nativeGeneration) this.native = native;
  }

  /** Allow `text` to be reported again right away. */
  clearCooldown(text: string): Promise<void> {
    return this.engine.clearCooldown(text);
  }

  get torchAvailable(): boolean {
    return !!capabilities(this.track()).torch;
  }

  async setTorch(on: boolean): Promise<void> {
    const track = this.track();
    if (!track) throw new Error('Camera is not started');
    await setTorch(track, on);
    this.torchOn = on;
    await this.refreshButtons();
  }

  async setZoom(zoom: number): Promise<void> {
    const track = this.track();
    if (!track) throw new Error('Camera is not started');
    await setZoom(track, zoom);
  }

  /** Switch to another camera (next one in the list when `deviceId` is omitted). */
  async switchCamera(deviceId?: string): Promise<void> {
    this.assertAlive();
    if (!deviceId) {
      const cams = await listCameras();
      const current = this.track()?.getSettings().deviceId;
      const idx = cams.findIndex((c) => c.deviceId === current);
      deviceId = cams[(idx + 1) % cams.length]?.deviceId;
    }
    const wasRunning = this.running;
    await this.stop();
    this.options = { ...this.options, camera: { ...this.options.camera, deviceId } };
    if (wasRunning) await this.start();
  }

  // ── Frame loop ───────────────────────────────────────────────────────────

  private schedule(): void {
    if (!this.running) return;
    const video = this.preview.video;
    if ('requestVideoFrameCallback' in video) {
      this.frameHandle = video.requestVideoFrameCallback(() => this.tick());
    } else {
      this.frameHandle = requestAnimationFrame(() => this.tick());
    }
  }

  private cancelSchedule(): void {
    const video = this.preview.video;
    if ('cancelVideoFrameCallback' in video) video.cancelVideoFrameCallback(this.frameHandle);
    cancelAnimationFrame(this.frameHandle);
  }

  /**
   * Frame pipeline, two stages that run **concurrently**: the grab (GPU crop +
   * readback, main thread) of the next frame overlaps the decoding of the
   * previous ones, which itself runs on `engine.concurrency` workers in
   * parallel. Running grab and decode one after the other made the frame
   * time `grab + decode`; overlapped it is `max(grab, decode / workers)`
   * (Galaxy A14: grab ~55 ms, decode ~125 ms → ~180 ms per frame before).
   *
   * Back-pressure: at most one grab in flight and one frame waiting for a
   * free worker. While a frame waits, no new grab starts, so frames are
   * skipped, never queued, and the waiting frame is at most one decode old.
   * The native detection is part of the grab stage, so the same rule keeps
   * at most one `detect()` in flight.
   */
  private tick(): void {
    if (!this.running) return;
    if (!this.grabbing && !this.pending && !this.paused && this.preview.video.videoWidth > 0) {
      this.grabbing = true;
      this.acquire()
        .catch((err) => this.handleFrameError(err))
        .finally(() => (this.grabbing = false));
    }
    this.preview.render();
    this.schedule();
  }

  /**
   * Grab stage. With the native `BarcodeDetector`, the frame is searched
   * natively first, straight from the `<video>` (Chrome reads it outside the
   * page's threads): no grab of our own. Codes it finds are committed right
   * away. When it finds nothing and `processing.engineFallback` is on, the
   * engine gets a frame **from time to time** (`FALLBACK_INTERVAL_MS`),
   * without its fast path, for one fallback strategy: a safety net for what
   * the native decoder has no preprocessing for (laser engraving,
   * light-on-dark codes, `micro_qr`…). Off by default (choice of the project
   * after testing on a Galaxy A14, where the native decoder read codes very
   * well): the net mostly costs CPU and battery.
   * Grabbing and decoding every frame as well would double the reads of the
   * frame and keep the workers busy on frames the native decoder handles,
   * costing battery and CPU it also needs.
   */
  private async acquire(): Promise<void> {
    const native = this.native;
    const rect = this.videoRect();
    if (!native) {
      this.enqueue(await this.grabFrame(rect, true));
      return;
    }
    const generation = this.nativeGeneration;
    const t0 = performance.now();
    let codes: Detection[];
    try {
      codes = await native.detect(this.preview.video, rect);
    } catch (err) {
      this.disableNative(err);
      this.enqueue(await this.grabFrame(rect, true));
      return;
    }
    const nativeMs = performance.now() - t0;
    // Replaced by `configure()` meanwhile: results made for the old formats.
    if (generation !== this.nativeGeneration || !this.running || this.paused) return;

    if (codes.length === 0 && this.engineFallback(native) && this.fallbackDue()) {
      this.fallbackInFlight = true;
      this.updatePerf({ nativeMs, counted: false });
      try {
        this.enqueue(await this.grabFrame(rect, false));
      } catch (err) {
        this.fallbackInFlight = false;
        throw err;
      }
      return;
    }
    const outcome = await this.engine.commitExternal(codes, nativeMs);
    if (generation !== this.nativeGeneration || !this.running || this.paused) return;
    this.failures = 0;
    this.updatePerf({ nativeMs });
    // Native points are already in video pixels.
    this.present(outcome, ++this.grabbed, 'native', (d) => d);
  }

  /**
   * Engine fallback behind the native detector: when asked for, and also
   * when the native decoder lacks formats of the default list (Safari on
   * iPhone), so that codes of those formats are still read.
   */
  private engineFallback(native: NativeDetector): boolean {
    return this.options.processing?.engineFallback === true || native.missing.length > 0;
  }

  /** The engine may decode a frame for its fallbacks (native detector active). */
  private fallbackDue(): boolean {
    return !this.fallbackInFlight && performance.now() >= this.nextFallbackAt;
  }

  /** The native decoder threw: decode with the engine only from now on. */
  private disableNative(err: unknown): void {
    if (this.nativeFailed) return;
    // Like the WebGL → Canvas 2D fallback: the reader keeps working, so this
    // is a warning, not an `error` event.
    console.warn('[scanner] native BarcodeDetector failed, decoding with the engine only:', err);
    this.nativeFailed = true;
    this.native = undefined;
    this.nativeGeneration++;
  }

  /** The region of interest, in video pixels. */
  private videoRect(): Rect {
    const video = this.preview.video;
    const ew = this.preview.width || video.videoWidth;
    const eh = this.preview.height || video.videoHeight;
    return regionToVideoRect(this.region, video.videoWidth, video.videoHeight, ew, eh);
  }

  private async grabFrame(rect: Rect, fastPath: boolean): Promise<GrabbedFrame> {
    const video = this.preview.video;
    const { width, height } = outputSize(rect, this.options.processing?.maxDimension ?? 1024);

    const t0 = performance.now();
    const buffer = this.takeBuffer(width * height);
    try {
      await this.grabber.grab(video, rect, width, height, new Uint8Array(buffer));
    } catch (err) {
      this.buffers.push(buffer);
      throw new GrabError(err);
    }
    return {
      buffer,
      width,
      height,
      rect,
      grabMs: performance.now() - t0,
      seq: ++this.grabbed,
      fastPath,
    };
  }

  private enqueue(frame: GrabbedFrame): void {
    if (!this.running || this.paused) {
      this.buffers.push(frame.buffer);
      if (!frame.fastPath) this.fallbackInFlight = false;
    } else if (this.decoding >= this.engine.concurrency) {
      this.pending = frame;
    } else {
      this.decode(frame);
    }
  }

  private decode(frame: GrabbedFrame): void {
    this.decoding++;
    this.decodeFrame(frame)
      .catch((err) => this.handleFrameError(err))
      .finally(() => {
        this.decoding--;
        const next = this.pending;
        this.pending = null;
        if (next) this.enqueue(next);
      });
  }

  private async decodeFrame(frame: GrabbedFrame): Promise<void> {
    const { width, height, rect } = frame;
    let reply: FrameReply;
    try {
      reply = await this.engine.processFrame(frame.buffer, width, height, 'luma', frame.fastPath);
    } finally {
      if (!frame.fastPath) {
        this.fallbackInFlight = false;
        this.nextFallbackAt = performance.now() + FALLBACK_INTERVAL_MS;
      }
    }
    this.buffers.push(reply.buffer);
    if (!this.running) return;
    this.failures = 0;
    if (!frame.fastPath && reply.live.length > 0) this.nextFallbackAt = 0;

    this.updatePerf({ grab: frame, decodeMs: reply.decodeMs });
    // Fallback-only frames (native detector active) are decoded one at a
    // time, so they cannot finish out of order: numbered when they finish,
    // after the native frames that ran meanwhile.
    const seq = frame.fastPath ? frame.seq : ++this.grabbed;
    this.present(reply, seq, 'engine', (d) => ({
      ...d,
      points: decoderToVideo(d.points, rect, width, height),
    }));
  }

  /** Draw the live codes of frame `seq` and report the new ones, mapped to video pixels by `toVideo`. */
  private present(
    outcome: Pick<FrameOutcome, 'results' | 'live'>,
    seq: number,
    source: 'native' | 'engine',
    toVideo: (d: Detection) => Detection,
  ): void {
    // An older frame finishing after a newer one must not move the highlights
    // back. With the native detector, a decoder that found nothing does not
    // erase what the other one drew: the native decoder would otherwise
    // erase, ~30 times per second, a code only the fallbacks can read.
    const owns = outcome.live.length > 0 || this.highlightSource === source || !this.highlightSource;
    if (seq > this.drawn && owns) {
      this.drawn = seq;
      this.highlightSource = outcome.live.length > 0 ? source : null;
      const video = this.preview.video;
      const vw = video.videoWidth;
      const vh = video.videoHeight;
      const layout = coverLayout(vw, vh, this.preview.width || vw, this.preview.height || vh);
      this.preview.setDetections(
        outcome.live.map((d) => toVideo(d).points.map((p) => videoToElement(p, layout))),
      );
    }

    if (outcome.results && outcome.results.length > 0) {
      this.feedback.play();
      this.emit('detected', outcome.results.map(toVideo));
    }
  }

  private handleFrameError(err: unknown): void {
    // Frames still in flight when the reader was stopped or destroyed.
    if (!this.running || this.destroyed) return;
    if (err instanceof GrabError && this.grabber.kind === 'webgl') {
      // GPU pipeline failed (context lost, driver issue…): continue on Canvas 2D.
      console.warn('[scanner] WebGL pipeline failed, falling back to Canvas 2D:', err.cause);
      this.grabber.destroy();
      this.grabber = new CanvasGrabber();
      return;
    }
    // A persistent failure (engine unavailable…) would otherwise emit one error
    // per frame, ~30 per second: report the first one of each streak.
    if (this.failures++ > 0) return;
    const error = err instanceof GrabError ? err.cause : err;
    this.emit('error', error instanceof Error ? error : new Error(String(error)));
  }

  private takeBuffer(size: number): ArrayBuffer {
    // Buffers of another size (ROI or resolution changed) are dropped.
    for (let b = this.buffers.pop(); b; b = this.buffers.pop()) {
      if (b.byteLength === size) return b;
    }
    return new ArrayBuffer(size);
  }

  /**
   * Account for a processed frame. `counted: false` for a native miss handed
   * to the engine: that frame is counted once, when the engine is done.
   */
  private updatePerf(s: {
    grab?: GrabbedFrame;
    decodeMs?: number;
    nativeMs?: number;
    counted?: boolean;
  }): void {
    const p = this.perf;
    const ema = (avg: number, v: number) => (avg ? avg * 0.9 + v * 0.1 : v);
    if (s.counted !== false) p.frames++;
    if (s.grab) {
      p.grabMs = ema(p.grabMs, s.grab.grabMs);
      p.w = s.grab.width;
      p.h = s.grab.height;
    }
    if (s.decodeMs !== undefined) {
      p.decodeMs = ema(p.decodeMs, s.decodeMs);
      p.engineFrames++;
    }
    if (s.nativeMs !== undefined) p.nativeMs = ema(p.nativeMs, s.nativeMs);
    const now = performance.now();
    if (now - p.since >= 1000) {
      p.fps = (p.frames * 1000) / (now - p.since);
      p.engineFps = (p.engineFrames * 1000) / (now - p.since);
      // With the native detector the engine may decode nothing for long
      // stretches: its averages would show stale values, as if it were busy.
      if (p.engineFrames === 0) {
        p.decodeMs = 0;
        p.grabMs = 0;
      }
      p.frames = 0;
      p.engineFrames = 0;
      p.since = now;
    }
  }

  // ── Helpers ──────────────────────────────────────────────────────────────

  private track(): MediaStreamTrack | undefined {
    return this.stream?.getVideoTracks()[0];
  }

  private async refreshButtons(): Promise<void> {
    this.preview.setTorchButton(this.torchAvailable, this.torchOn, () => {
      this.setTorch(!this.torchOn).catch((e) => this.emit('error', e));
    });
    const cams = await listCameras().catch(() => []);
    this.preview.setSwitchButton(cams.length > 1, () => {
      this.switchCamera().catch((e) => this.emit('error', e));
    });
  }

  private emit<K extends keyof ReaderEvents>(event: K, payload: ReaderEvents[K]): void {
    for (const l of this.listeners[event]) {
      try {
        l(payload);
      } catch (err) {
        console.error('[scanner] listener error:', err);
      }
    }
  }

  private assertAlive(): void {
    if (this.destroyed) throw new Error('BarcodeReader has been destroyed');
  }
}

/**
 * Failure of the frame pipeline (GPU / canvas), as opposed to the engine.
 * Only these trigger the WebGL → Canvas 2D fallback: an engine error must not
 * be mistaken for a WebGL problem.
 */
class GrabError extends Error {
  constructor(override readonly cause: unknown) {
    super(cause instanceof Error ? cause.message : String(cause));
  }
}

/** The native detector unless `processing.nativeDetector` is `'off'`, when it can serve `engineOptions`. */
function createNative(
  options: ReaderOptions,
  engineOptions: EngineOptions,
): Promise<NativeDetector | undefined> {
  // On by default: on a Galaxy A14 (Chrome, Play Services) it read codes very
  // well whatever the orientation of the phone, where the engine's fast pass
  // only reads 1D codes roughly aligned with the image rows.
  if (options.processing?.nativeDetector === 'off') return Promise.resolve(undefined);
  return NativeDetector.create(engineOptions.formats, undefined, options.processing?.engineFallback === true);
}

function waitForVideo(video: HTMLVideoElement): Promise<void> {
  if (video.readyState >= 2 && video.videoWidth > 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Camera did not deliver any frame')), 10_000);
    video.addEventListener(
      'loadeddata',
      () => {
        clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
  });
}
