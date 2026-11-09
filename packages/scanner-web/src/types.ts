import type { BarcodeFormat } from './wasm/scanner_wasm.js';

export type { BarcodeFormat };

/** A point in pixels. */
export interface Point {
  x: number;
  y: number;
}

/** A rectangle expressed as fractions (0–1) of a reference box. */
export interface Region {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A decoded barcode. */
export interface Detection {
  /** Decoded text content. */
  text: string;
  /** Symbology, e.g. `"qr_code"`, `"ean_13"`. */
  format: BarcodeFormat;
  /**
   * Location of the code: corners of the symbol (2D codes: their finder
   * points; 1D codes: start-top, end-top, end-bottom, start-bottom). For
   * camera scans: video pixels (`video.videoWidth` space). For `scanImage`:
   * pixels of the source image.
   */
  points: Point[];
  /** Raw payload bytes when the symbology provides them (binary QR codes…). */
  rawBytes?: Uint8Array;
  /** `Date.now()`-style timestamp of the detection. */
  timestamp: number;
}

/** Options of the decoding engine, shared by the camera reader and `scanImage`. */
export interface EngineOptions {
  /** Formats to look for. Default: all. Listing only what you need is much faster. */
  formats?: BarcodeFormat[];
  /** Enable CLAHE / binarization fallbacks for hard frames. Default: `true`. */
  preprocessing?: boolean;
  /** Also look for light-on-dark codes. Default: `true`. */
  tryInvert?: boolean;
  /** Maximum codes per frame. `> 1` enables multi-code detection (slower). Default: `1`. */
  maxResults?: number;
  /** Delay before the same content is reported again. Default: `1500` ms. */
  duplicateIntervalMs?: number;
  /**
   * Agreeing reads required before a 1D code is reported. Weak 1D formats
   * (Code 39/93, ITF, Codabar, UPC-E, EAN-8) need one more; 2D codes are
   * reported on the first read. Default: `2`.
   */
  linearConfirmations?: number;
}

/** Where to load the engine from. Only needed when the bundler cannot resolve `import.meta.url`. */
export interface AssetOptions {
  /** URL of `scanner_wasm_bg.wasm`. Default: next to the SDK. */
  wasmUrl?: string | URL;
  /** URL of the SDK `worker.js`. Default: next to the SDK. */
  workerUrl?: string | URL;
  /** Decode in Web Workers (recommended). Default: `true`; falls back to the main thread if workers are unavailable. */
  worker?: boolean;
  /**
   * Number of decoding Web Workers: frames are decoded in parallel, which
   * multiplies the frame rate on phones with several slow cores. Default:
   * cores − 1, between 1 and 3.
   */
  workers?: number;
}

export type CameraResolution = 'sd' | 'hd' | 'fhd';

export interface CameraOptions {
  /** Default: `'environment'` (rear camera). */
  facingMode?: 'environment' | 'user';
  /** Use a specific camera (see `BarcodeReader.listCameras()`). */
  deviceId?: string;
  /** Requested capture resolution. Default: `'hd'` (1280×720). */
  resolution?: CameraResolution;
  /** Remember the last selected camera in `localStorage`. Default: `true`. */
  rememberDevice?: boolean;
  /**
   * Release the camera while the page is hidden (tab in the background,
   * screen locked) and reopen it when the page is visible again. The reader
   * stays running in between. Default: `true`.
   */
  releaseWhenHidden?: boolean;
}

export interface OverlayOptions {
  /** Dim everything outside the region of interest and draw its corners. Default: `true`. */
  showRegion?: boolean;
  /** Highlight detected codes. Default: `true`. */
  showDetections?: boolean;
  /** Show a flashlight button when the camera supports it. Default: `true`. */
  showTorchButton?: boolean;
  /** Show a camera switch button when several cameras are available. Default: `true`. */
  showCameraSwitch?: boolean;
  /** Accent color. Default: `'#00d4aa'`. */
  color?: string;
}

export interface FeedbackOptions {
  /** Short beep on detection. Default: `true`. */
  beep?: boolean;
  /** Vibrate on detection (when supported). Default: `true`. */
  vibrate?: boolean;
}

export interface ProcessingOptions {
  /**
   * Largest side, in pixels, of the image handed to the decoder. The region
   * of interest is downscaled on the GPU to this size. Default: `1024`.
   */
  maxDimension?: number;
  /** Frame pipeline. `'auto'` uses WebGL2 and falls back to Canvas 2D. Default: `'auto'`. */
  backend?: 'auto' | 'webgl' | 'canvas';
  /**
   * Use the browser's native `BarcodeDetector` (Chrome on Android, macOS and
   * ChromeOS) as the fast pass, when it supports every requested format
   * (`micro_qr` excepted). The engine's fallback strategies keep running on
   * the frames it finds nothing in: hard codes (laser engraving, shadows…)
   * are still read. Codes get the same confirmation and de-duplication.
   * It reads codes whatever their orientation. Elsewhere (Safari, Firefox,
   * Chrome on Windows / Linux), or if it fails, the engine does everything.
   * `'off'`: always the engine alone. Default: `'auto'`.
   */
  nativeDetector?: 'auto' | 'off';
  /**
   * With the native detector: while it finds nothing, also hand a frame to
   * the engine's fallback strategies from time to time (~3 per second), a
   * safety net for codes the native decoder may miss (laser engraving,
   * shadows, light-on-dark codes, `micro_qr`). Costs a little CPU and
   * battery. Without it, `micro_qr` listed in `formats` keeps the engine
   * instead of the native detector. Default: `false`.
   */
  engineFallback?: boolean;
}

export interface ReaderOptions extends EngineOptions, AssetOptions {
  /** Element (or CSS selector) that will host the camera preview. It should have a size. */
  host: HTMLElement | string;
  /**
   * Area of the **visible** preview that is scanned, as fractions of the host
   * element. Smaller = faster. Default: `{ x: 0.1, y: 0.2, width: 0.8, height: 0.6 }`.
   */
  regionOfInterest?: Region;
  camera?: CameraOptions;
  /** UI drawn on top of the preview, or `false` to draw nothing. */
  overlay?: OverlayOptions | false;
  feedback?: FeedbackOptions | false;
  processing?: ProcessingOptions;
}

/** Live statistics of a running reader. */
export interface ReaderStats {
  /** Frames decoded per second. */
  fps: number;
  /** Average decode time in the engine, in ms (`0` when it decoded nothing in the last second). */
  decodeMs: number;
  /** Average GPU/canvas frame preparation time, in ms (`0` when nothing was grabbed in the last second). */
  grabMs: number;
  /** Size of the image handed to the decoder. */
  frameWidth: number;
  frameHeight: number;
  /** Active frame pipeline. */
  backend: 'webgl' | 'canvas';
  /** `true` when decoding runs in Web Workers. */
  worker: boolean;
  /** Frames decoded in parallel (number of workers, `1` on the main thread). */
  workers: number;
  /** `true` when the native `BarcodeDetector` does the fast pass (see `processing.nativeDetector`). */
  native: boolean;
  /** Average native detection time, in ms (`0` when not used). */
  nativeMs: number;
  /**
   * Frames decoded by the engine per second. With the native detector: `0`
   * while it reads everything, a few per second with `engineFallback`.
   */
  engineFps: number;
}

export interface ReaderEvents {
  /** New codes (already de-duplicated). */
  detected: Detection[];
  error: Error;
  started: undefined;
  stopped: undefined;
}
