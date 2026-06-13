/**
 * @scanner/web — High-performance QR code and barcode scanner for browsers
 *
 * A thin JavaScript SDK that wraps the WASM engine with:
 * - Automatic camera stream management (getUserMedia)
 * - requestAnimationFrame scanning loop
 * - Event-based API
 *
 * @example
 * ```javascript
 * import { BrowserScanner } from './scanner-web.js';
 *
 * const scanner = new BrowserScanner({
 *   onDetect: (result) => console.log('Scanned:', result.text),
 *   roi: 0.7, // scan central 70% of frame
 * });
 *
 * await scanner.start(videoElement);
 * // ... later:
 * scanner.stop();
 * ```
 */

// ── WASM loader ───────────────────────────────────────────────────────────────

let wasmModule = null;

async function loadWasm(wasmPath = './scanner_wasm_bg.wasm') {
  if (wasmModule) return wasmModule;
  const mod = await import('./scanner_wasm.js');
  await mod.default(wasmPath);
  wasmModule = mod;
  return mod;
}

// ── BrowserScanner ────────────────────────────────────────────────────────────

/**
 * @typedef {Object} ScanResult
 * @property {string} text - Decoded barcode content.
 * @property {string} format - Barcode format (e.g. "QR Code", "EAN-13").
 * @property {number[]} points - Corner points as flat [x0,y0, x1,y1, ...] array.
 * @property {number} confidence - Confidence score [0.0, 1.0].
 * @property {number} orientation - Detected rotation in degrees.
 * @property {number} timestampMs - Unix timestamp (ms).
 */

/**
 * @typedef {Object} BrowserScannerOptions
 * @property {function(ScanResult): void} onDetect - Called when a barcode is detected.
 * @property {function(Error): void} [onError] - Called on unrecoverable errors.
 * @property {number} [roi=0.7] - Fraction of frame to scan (0.1–1.0). Default: 0.7.
 * @property {number} [cooldownMs=2000] - Cooldown before same code reported again (ms).
 * @property {boolean} [preprocessing=true] - Enable adaptive preprocessing.
 * @property {string} [wasmPath='./scanner_wasm_bg.wasm'] - Path to .wasm file.
 * @property {MediaStreamConstraints} [cameraConstraints] - getUserMedia constraints.
 */

export class BrowserScanner {
  /** @type {BrowserScannerOptions} */
  #opts;
  /** @type {HTMLVideoElement | null} */
  #video = null;
  /** @type {HTMLCanvasElement} */
  #canvas;
  /** @type {CanvasRenderingContext2D} */
  #ctx;
  /** @type {MediaStream | null} */
  #stream = null;
  /** @type {number | null} */
  #rafId = null;
  /** @type {import('./scanner_wasm.js').WasmScanSession | null} */
  #session = null;
  #running = false;

  /**
   * @param {BrowserScannerOptions} options
   */
  constructor(options) {
    if (!options.onDetect) throw new Error('BrowserScanner: onDetect callback is required');
    this.#opts = {
      roi: 0.7,
      cooldownMs: 2000,
      preprocessing: true,
      wasmPath: './scanner_wasm_bg.wasm',
      cameraConstraints: { video: { facingMode: 'environment', width: { ideal: 1280 }, height: { ideal: 720 } } },
      ...options,
    };

    this.#canvas = document.createElement('canvas');
    this.#ctx = this.#canvas.getContext('2d', { willReadFrequently: true });
  }

  /**
   * Start scanning from the given video element or open the camera automatically.
   *
   * @param {HTMLVideoElement} [videoElement] - Existing video element. If omitted, creates one.
   * @returns {Promise<void>}
   */
  async start(videoElement) {
    if (this.#running) return;

    // Load WASM
    const { WasmScanSession } = await loadWasm(this.#opts.wasmPath);

    // Setup scan session
    const roiMargin = (1 - this.#opts.roi) / 2;
    this.#session = new WasmScanSession(
      this.#opts.preprocessing,   // preprocessing
      true,                        // try_invert
      roiMargin,                   // roi_x
      roiMargin,                   // roi_y
      this.#opts.roi,              // roi_w
      this.#opts.roi,              // roi_h
      this.#opts.cooldownMs,       // cooldown_ms
    );

    // Setup video
    this.#video = videoElement ?? document.createElement('video');
    this.#video.playsInline = true;
    this.#video.muted = true;

    try {
      this.#stream = await navigator.mediaDevices.getUserMedia(
        this.#opts.cameraConstraints
      );
      this.#video.srcObject = this.#stream;
      await this.#video.play();
    } catch (err) {
      this.#opts.onError?.(err);
      throw err;
    }

    this.#running = true;
    this.#rafId = requestAnimationFrame(this.#tick.bind(this));
  }

  /**
   * Stop scanning and release the camera.
   */
  stop() {
    this.#running = false;
    if (this.#rafId !== null) {
      cancelAnimationFrame(this.#rafId);
      this.#rafId = null;
    }
    if (this.#stream) {
      this.#stream.getTracks().forEach(t => t.stop());
      this.#stream = null;
    }
    if (this.#video) {
      this.#video.srcObject = null;
    }
  }

  /**
   * Pause scanning (keeps camera open).
   */
  pause() {
    if (!this.#running) return;
    this.#running = false;
    if (this.#rafId !== null) {
      cancelAnimationFrame(this.#rafId);
      this.#rafId = null;
    }
  }

  /**
   * Resume scanning after pause().
   */
  resume() {
    if (this.#running || !this.#stream) return;
    this.#running = true;
    this.#rafId = requestAnimationFrame(this.#tick.bind(this));
  }

  /**
   * Allow the same barcode to be scanned again immediately.
   * @param {string} text - The barcode text to clear cooldown for.
   */
  clearCooldown(text) {
    this.#session?.clearCooldown(text);
  }

  /**
   * Reset the session (clear all cooldowns and statistics).
   */
  reset() {
    this.#session?.reset();
  }

  /**
   * Get current performance statistics.
   * @returns {{ framesSubmitted, framesScanned, codesDetected, avgScanTimeMs, skipInterval }}
   */
  get stats() {
    if (!this.#session) return null;
    return {
      framesSubmitted: this.#session.framesSubmitted,
      framesScanned: this.#session.framesScanned,
      codesDetected: this.#session.codesDetected,
      avgScanTimeMs: this.#session.avgScanTimeMs,
      skipInterval: this.#session.skipInterval,
    };
  }

  // ── Private ──────────────────────────────────────────────────────────────

  #tick() {
    if (!this.#running) return;

    const video = this.#video;
    if (video && video.readyState >= video.HAVE_CURRENT_DATA) {
      const vw = video.videoWidth;
      const vh = video.videoHeight;

      if (vw > 0 && vh > 0) {
        this.#canvas.width = vw;
        this.#canvas.height = vh;
        this.#ctx.drawImage(video, 0, 0, vw, vh);

        const { data } = this.#ctx.getImageData(0, 0, vw, vh);
        const results = this.#session?.processFrameRgba(data, vw, vh);
        if (results && results.length > 0) {
          for (const r of results) {
            this.#opts.onDetect(r);
          }
        }
      }
    }

    this.#rafId = requestAnimationFrame(this.#tick.bind(this));
  }
}

// ── Single-frame scanner (for file/image scanning) ────────────────────────────

/**
 * Scan a single image element, blob, or URL for barcodes.
 *
 * @param {HTMLImageElement | Blob | string} source - Image to scan.
 * @param {object} [opts] - Options: roi (0.1–1.0), preprocessing (bool).
 * @returns {Promise<ScanResult[]>}
 *
 * @example
 * ```javascript
 * const results = await scanImage(document.querySelector('img'));
 * console.log(results[0]?.text);
 * ```
 */
export async function scanImage(source, opts = {}) {
  const { WasmScanner } = await loadWasm(opts.wasmPath);
  const roi = opts.roi ?? 1.0;
  const roiMargin = (1 - roi) / 2;

  const scanner = new WasmScanner(
    opts.preprocessing ?? true, // preprocessing
    true,                        // try_invert
    roi < 1.0 ? roiMargin : undefined,
    roi < 1.0 ? roiMargin : undefined,
    roi < 1.0 ? roi : undefined,
    roi < 1.0 ? roi : undefined,
  );

  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  const img = await loadImage(source);

  canvas.width = img.naturalWidth || img.width;
  canvas.height = img.naturalHeight || img.height;
  ctx.drawImage(img, 0, 0);

  const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
  return scanner.scanRgba(data, canvas.width, canvas.height);
}

async function loadImage(source) {
  if (source instanceof HTMLImageElement) {
    if (source.complete) return source;
    return new Promise((resolve, reject) => {
      source.onload = () => resolve(source);
      source.onerror = reject;
    });
  }

  const url = source instanceof Blob ? URL.createObjectURL(source) : source;
  const img = new Image();
  img.src = url;
  return new Promise((resolve, reject) => {
    img.onload = () => {
      if (source instanceof Blob) URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = reject;
  });
}
