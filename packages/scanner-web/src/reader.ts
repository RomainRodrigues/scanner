// BarcodeReader: camera preview + frame pipeline + engine, wired together.
import { capabilities, listCameras, openCamera, rememberCamera, setTorch, setZoom } from './camera.js';
import { pickEngineOptions } from './core.js';
import { createEngine, type Engine } from './engine.js';
import { Feedback } from './feedback.js';
import {
  clampRegion,
  coverLayout,
  decoderToVideo,
  outputSize,
  regionToVideoRect,
  videoToElement,
} from './geometry.js';
import { CanvasGrabber, createGrabber, type FrameGrabber } from './grabber.js';
import { Preview } from './overlay.js';
import type { Detection, EngineOptions, ReaderEvents, ReaderOptions, ReaderStats, Region } from './types.js';

const DEFAULT_REGION: Region = { x: 0.1, y: 0.2, width: 0.8, height: 0.6 };

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
  private busy = false;
  private destroyed = false;
  private torchOn = false;
  private buffers: ArrayBuffer[] = [];
  private frameHandle = 0;
  private perf = { frames: 0, since: 0, fps: 0, decodeMs: 0, grabMs: 0, w: 0, h: 0 };

  private constructor(
    private engine: Engine,
    private options: ReaderOptions,
    host: HTMLElement,
  ) {
    this.region = clampRegion(options.regionOfInterest ?? DEFAULT_REGION);
    this.engineOptions = pickEngineOptions(options);
    this.preview = new Preview(host, options.overlay);
    this.preview.setRegion(this.region);
    this.grabber = createGrabber(options.processing?.backend);
    this.feedback = new Feedback(options.feedback ?? {});
  }

  /** Load the engine (WebAssembly, in a Web Worker) and build the preview inside `options.host`. */
  static async create(options: ReaderOptions): Promise<BarcodeReader> {
    const host =
      typeof options.host === 'string' ? document.querySelector<HTMLElement>(options.host) : options.host;
    if (!host) throw new Error(`BarcodeReader: host element not found (${String(options.host)})`);
    const engine = await createEngine(options);
    await engine.configure(pickEngineOptions(options));
    return new BarcodeReader(engine, options, host);
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
      simd: this.engine.simd,
    };
  }

  get engineVersion(): string {
    return this.engine.version;
  }

  /** Open the camera and start scanning. Call it from a user gesture to allow the beep on iOS. */
  async start(): Promise<void> {
    this.assertAlive();
    if (this.running) return;
    this.feedback.unlock();
    this.stream = await openCamera(this.options.camera);
    const video = this.preview.video;
    video.srcObject = this.stream;
    await video.play().catch(() => undefined);
    await waitForVideo(video);

    const track = this.track();
    if (this.options.camera?.rememberDevice !== false) rememberCamera(track?.getSettings().deviceId);
    this.torchOn = false;
    await this.refreshButtons();

    this.running = true;
    this.paused = false;
    this.perf.since = performance.now();
    this.perf.frames = 0;
    this.emit('started', undefined);
    this.schedule();
  }

  /** Stop scanning and release the camera. */
  async stop(): Promise<void> {
    if (!this.running && !this.stream) return;
    this.running = false;
    this.cancelSchedule();
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.preview.video.srcObject = null;
    this.preview.setDetections([]);
    this.emit('stopped', undefined);
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
    await this.engine.configure(this.engineOptions);
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

  private tick(): void {
    if (!this.running) return;
    // One frame in flight at a time: natural back-pressure, no frame queue.
    if (!this.busy && !this.paused && this.preview.video.videoWidth > 0) {
      this.busy = true;
      this.processFrame()
        .catch((err) => this.handleFrameError(err))
        .finally(() => (this.busy = false));
    }
    this.preview.render();
    this.schedule();
  }

  private async processFrame(): Promise<void> {
    const video = this.preview.video;
    const vw = video.videoWidth;
    const vh = video.videoHeight;
    const ew = this.preview.width || vw;
    const eh = this.preview.height || vh;
    const rect = regionToVideoRect(this.region, vw, vh, ew, eh);
    const { width, height } = outputSize(rect, this.options.processing?.maxDimension ?? 1024);

    const t0 = performance.now();
    const buffer = this.takeBuffer(width * height);
    try {
      await this.grabber.grab(video, rect, width, height, new Uint8Array(buffer));
    } catch (err) {
      this.buffers.push(buffer);
      throw new GrabError(err);
    }
    const t1 = performance.now();

    const reply = await this.engine.processFrame(buffer, width, height, 'luma');
    this.buffers.push(reply.buffer);
    if (!this.running) return;

    this.updatePerf(t1 - t0, reply.decodeMs, width, height);

    const toVideo = (d: Detection): Detection => ({
      ...d,
      points: decoderToVideo(d.points, rect, width, height),
    });
    const live = reply.live.map(toVideo);
    const layout = coverLayout(vw, vh, ew, eh);
    this.preview.setDetections(live.map((d) => d.points.map((p) => videoToElement(p, layout))));

    if (reply.results && reply.results.length > 0) {
      this.feedback.play();
      this.emit('detected', reply.results.map(toVideo));
    }
  }

  private handleFrameError(err: unknown): void {
    if (err instanceof GrabError && this.grabber.kind === 'webgl') {
      // GPU pipeline failed (context lost, driver issue…): continue on Canvas 2D.
      console.warn('[scanner] WebGL pipeline failed, falling back to Canvas 2D:', err.cause);
      this.grabber.destroy();
      this.grabber = new CanvasGrabber();
      return;
    }
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

  private updatePerf(grabMs: number, decodeMs: number, w: number, h: number): void {
    const p = this.perf;
    p.frames++;
    p.grabMs = p.grabMs ? p.grabMs * 0.9 + grabMs * 0.1 : grabMs;
    p.decodeMs = p.decodeMs ? p.decodeMs * 0.9 + decodeMs * 0.1 : decodeMs;
    p.w = w;
    p.h = h;
    const now = performance.now();
    if (now - p.since >= 1000) {
      p.fps = (p.frames * 1000) / (now - p.since);
      p.frames = 0;
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
