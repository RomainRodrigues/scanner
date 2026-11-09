// The browser's native barcode decoder (`BarcodeDetector`, Shape Detection API).
//
// Available in Chrome on Android (Google Play Services), macOS (Apple Vision)
// and ChromeOS; not in Firefox nor Chrome on Windows / Linux. Safari only has
// it behind its experimental "Shape Detection API" flag, and there it finds
// nothing (iPhone, tested on the demo: active, no code ever read); left as
// is, the flag is off by default. Where it
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
 * Every format of this SDK. The native API spells them the same way
 * (`qr_code`, `ean_13`…) but has no `micro_qr`, and a given browser may lack
 * others (Safari, built on Apple's Vision framework, has no `upc_a`).
 */
const ALL_FORMATS: readonly BarcodeFormat[] = [
  'qr_code',
  'micro_qr',
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
    /** Requested formats the detector reads. */
    readonly formats: readonly BarcodeFormat[],
    /**
     * Requested formats the native decoder cannot read. They are read only
     * with `processing.engineFallback`.
     */
    readonly missing: readonly BarcodeFormat[],
    /** UPC-A codes come back as EAN-13 and are converted (see `create`). */
    private upcFromEan: boolean,
  ) {}

  /**
   * A detector for `formats` (default: all), or `undefined` when the engine
   * must keep the job: API absent, no requested format readable natively, or
   * an **explicit** list naming a format the native decoder lacks while
   * `engineFallback` is off (a format the application asked for must not be
   * dropped silently). With the default list, missing formats (always
   * `micro_qr`, sometimes others) are accepted: the native decoder reads the
   * rest, and the missing ones are read only with `engineFallback`. The
   * engine fallback never turns itself on: it costs CPU and battery, that is
   * the application's call.
   *
   * UPC-A is EAN-13 with a leading `0` (GTIN-12 inside GTIN-13), so a native
   * decoder without `upc_a` but with `ean_13` covers it: it is asked for
   * EAN-13 and those codes are reported as UPC-A, as this SDK's engine (and
   * ZXing) do when UPC-A is requested.
   */
  static async create(
    formats: readonly BarcodeFormat[] | undefined,
    api: BarcodeDetectorClass | undefined = nativeApi(),
    engineFallback = false,
  ): Promise<NativeDetector | undefined> {
    if (!api) return undefined;
    let supported: string[];
    try {
      supported = await api.getSupportedFormats();
    } catch {
      return undefined;
    }
    const explicit = formats !== undefined && formats.length > 0;
    const requested = explicit ? formats : ALL_FORMATS;
    const upcFromEan =
      requested.includes('upc_a') && !supported.includes('upc_a') && supported.includes('ean_13');
    const covered = (f: BarcodeFormat) => supported.includes(f) || (f === 'upc_a' && upcFromEan);
    const readable = requested.filter(covered);
    const missing = requested.filter((f) => !covered(f));
    // On Android the list is empty until Google Play Services has downloaded
    // its barcode module (first use on the device, in the background): the
    // detector is unavailable *for now*, the engine does the whole job.
    if (readable.length === 0 || (explicit && missing.length > 0 && !engineFallback)) return undefined;
    const native = readable.map((f) => (f === 'upc_a' && upcFromEan ? 'ean_13' : f));
    try {
      return new NativeDetector(new api({ formats: [...new Set(native)] }), readable, missing, upcFromEan);
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
      let format = code.format as BarcodeFormat;
      let text = code.rawValue;
      if (this.upcFromEan && format === 'ean_13' && /^0\d{12}$/.test(text)) {
        format = 'upc_a';
        text = text.slice(1);
      }
      if (!this.formats.includes(format) || code.cornerPoints.length === 0) return [];
      if (!contains(rect, center(code.cornerPoints))) return [];
      const points = code.cornerPoints.map((p) => ({ x: p.x, y: p.y }));
      return [{ text, format, points, timestamp }];
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
