// The browser's native barcode decoder (`BarcodeDetector`, Shape Detection API).
//
// Available in Chrome on Android (Google Play Services), macOS (Apple Vision)
// and ChromeOS; not in Safari, Firefox, nor Chrome on Windows / Linux. Where it
// exists it decodes outside the page's threads, so it can replace the engine's
// **fast path**: the engine then only runs its fallback strategies (Linear,
// CLAHE, Sauvola, inverted…) on the frames the native decoder found nothing
// in. The native decoder has no such preprocessing, and those strategies are
// what reads the hard codes (laser-engraved tags, labels in shadow).
import type { Rect } from './grabber.js';
import type { BarcodeFormat, Detection, Point } from './types.js';

// The API is not in TypeScript's `lib.dom` yet: minimal local declarations.

/** A code found by `BarcodeDetector.detect()`. */
export interface NativeBarcode {
  rawValue: string;
  format: string;
  /** Corners in pixels of the source (for a `<video>`: `videoWidth` space). */
  cornerPoints: readonly Point[];
}

interface BarcodeDetectorInstance {
  detect(source: CanvasImageSource): Promise<NativeBarcode[]>;
}

/** The `BarcodeDetector` constructor. */
export interface BarcodeDetectorClass {
  new (options?: { formats?: string[] }): BarcodeDetectorInstance;
  getSupportedFormats(): Promise<string[]>;
}

/**
 * Formats of this SDK, all spelled like the native ones (`qr_code`, `ean_13`…)
 * except `micro_qr`, which the native API does not have. With the engine
 * fallback it is still read by the fallback strategies (`Hard` decodes the
 * raw frame with every format); without it, see `NativeDetector.create`.
 */
const NATIVE_FORMATS: readonly BarcodeFormat[] = [
  'qr_code',
  'data_matrix',
  'aztec',
  'pdf417',
  'code_128',
  'code_39',
  'code_93',
  'ean_13',
  'ean_8',
  'upc_a',
  'upc_e',
  'itf',
  'codabar',
];

/** Native decoder serving one reader configuration. */
export class NativeDetector {
  private constructor(
    private detector: BarcodeDetectorInstance,
    /** Formats the detector looks for. */
    readonly formats: readonly BarcodeFormat[],
    /**
     * Formats of the default list (every format) that this browser's native
     * decoder lacks: the reader turns the engine fallback on to read them.
     */
    readonly missing: readonly BarcodeFormat[] = [],
  ) {}

  /**
   * A detector for `formats` (default: all), or `undefined` when the native
   * decoder cannot replace the engine's fast path: API absent, or one of the
   * requested formats not supported natively (codes of that format would
   * then be read by the slow fallbacks at best).
   *
   * `micro_qr`, absent from the native API: with `engineFallback` the
   * fallbacks read it. Without, a list that names it explicitly keeps the
   * engine (the application asked for it, it must not be lost silently),
   * while the default list (every format) accepts not reading it.
   *
   * The default list only needs **some** native support: native decoders do
   * not all have every format (Safari's, built on Apple's Vision framework,
   * does not offer the same list as Chrome's on Android; requiring all of
   * them kept iPhones on the engine alone). The native detector then reads
   * the formats it has, and `missing` lists the others, for the engine
   * fallback. An explicit list stays strict: every format must be native.
   */
  static async create(
    formats: readonly BarcodeFormat[] | undefined,
    api: BarcodeDetectorClass | undefined = nativeApi(),
    engineFallback = false,
  ): Promise<NativeDetector | undefined> {
    if (!api) return undefined;
    if (!engineFallback && formats?.includes('micro_qr')) return undefined;
    let supported: string[];
    try {
      supported = await api.getSupportedFormats();
    } catch {
      return undefined;
    }
    // On Android the list is empty until Google Play Services has downloaded
    // its barcode module (first use on the device, in the background): the
    // detector is unavailable *for now*, the engine does the whole job.
    const explicit = formats !== undefined && formats.length > 0;
    const requested = (explicit ? formats : NATIVE_FORMATS).filter((f) => f !== 'micro_qr');
    const wanted = requested.filter((f) => supported.includes(f));
    const missing = requested.filter((f) => !supported.includes(f));
    if (wanted.length === 0 || (explicit && missing.length > 0)) return undefined;
    try {
      return new NativeDetector(new api({ formats: [...wanted] }), wanted, missing);
    } catch {
      return undefined;
    }
  }

  /**
   * Codes in `source` whose center lies in `rect` (the region of interest, in
   * source pixels), as detections in source pixels. Throws when the native
   * decoder fails.
   */
  async detect(source: CanvasImageSource, rect: Rect): Promise<Detection[]> {
    const found = await this.detector.detect(source);
    const timestamp = Date.now();
    return found.flatMap((code) => {
      const format = code.format as BarcodeFormat;
      if (!this.formats.includes(format) || code.cornerPoints.length === 0) return [];
      if (!contains(rect, center(code.cornerPoints))) return [];
      const points = code.cornerPoints.map((p) => ({ x: p.x, y: p.y }));
      return [{ text: code.rawValue, format, points, timestamp }];
    });
  }
}

/**
 * The browser's own `BarcodeDetector`, if any. Feature detection only (no
 * list of browsers or devices): any browser that ships the API is used.
 *
 * Polyfills are rejected: packages such as `barcode-detector` install a
 * `BarcodeDetector` that runs ZXing in WebAssembly on the main thread,
 * slower than this SDK's own engine in its workers. A built-in class
 * stringifies as `function BarcodeDetector() { [native code] }`.
 */
function nativeApi(): BarcodeDetectorClass | undefined {
  const api: unknown = Reflect.get(globalThis, 'BarcodeDetector');
  if (typeof api !== 'function' || !isBuiltIn(api)) return undefined;
  return api as BarcodeDetectorClass;
}

/** `true` for a function implemented by the browser, not in JavaScript. */
export function isBuiltIn(fn: object): boolean {
  return /\{\s*\[native code\]\s*\}\s*$/.test(Function.prototype.toString.call(fn));
}

function center(points: readonly Point[]): Point {
  const n = points.length;
  return {
    x: points.reduce((s, p) => s + p.x, 0) / n,
    y: points.reduce((s, p) => s + p.y, 0) / n,
  };
}

function contains(r: Rect, p: Point): boolean {
  return p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
}
