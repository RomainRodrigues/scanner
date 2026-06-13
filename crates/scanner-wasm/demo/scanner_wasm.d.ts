/* tslint:disable */
/* eslint-disable */

/**
 * A detected barcode result, serialized as a JS object.
 */
export class ScanResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Confidence score [0.0, 1.0].
     */
    confidence: number;
    /**
     * Barcode format (e.g. "QR Code", "EAN-13").
     */
    format: string;
    /**
     * Rotation angle in degrees.
     */
    orientation: number;
    /**
     * Detected corner points as flat [x0,y0, x1,y1, ...] array.
     */
    points: Float32Array;
    /**
     * Decoded text content of the barcode.
     */
    text: string;
    /**
     * Unix timestamp (ms) when the scan completed.
     */
    timestamp_ms: number;
}

/**
 * Stateful scan session for real-time camera streams in the browser.
 *
 * Provides adaptive frame throttling and deduplication.
 *
 * @example
 * ```javascript
 * const session = new WasmScanSession();
 * function onAnimationFrame() {
 *   ctx.drawImage(video, 0, 0);
 *   const { data } = ctx.getImageData(0, 0, width, height);
 *   const result = session.processFrameRgba(data, width, height);
 *   if (result) console.log('Scanned:', result[0].text);
 *   requestAnimationFrame(onAnimationFrame);
 * }
 * ```
 */
export class WasmScanSession {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Clear the cooldown for a specific barcode (allow immediate re-scan).
     */
    clearCooldown(text: string): void;
    /**
     * Create a new scan session.
     *
     * @param {boolean} [preprocessing=true] - Enable adaptive preprocessing.
     * @param {boolean} [try_invert=true] - Try inverted image.
     * @param {number} [roi_x] - ROI left edge (0.0–1.0).
     * @param {number} [roi_y] - ROI top edge (0.0–1.0).
     * @param {number} [roi_w] - ROI width (0.0–1.0).
     * @param {number} [roi_h] - ROI height (0.0–1.0).
     * @param {number} [cooldown_ms=2000] - Dedup cooldown in milliseconds.
     */
    constructor(preprocessing?: boolean | null, try_invert?: boolean | null, roi_x?: number | null, roi_y?: number | null, roi_w?: number | null, roi_h?: number | null, cooldown_ms?: number | null);
    /**
     * Process a camera frame (grayscale, 1 byte/pixel — fastest path).
     *
     * @param {Uint8Array} data - Raw luma bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     */
    processFrameLuma(data: Uint8Array, width: number, height: number): ScanResult[] | undefined;
    /**
     * Process a camera frame (RGBA, 4 bytes/pixel — browser ImageData format).
     *
     * Returns null if the frame was throttled or no new code was detected.
     * Returns an array of ScanResult when new barcodes are found.
     *
     * @param {Uint8ClampedArray} data - The ImageData.data buffer.
     * @param {number} width - Frame width in pixels.
     * @param {number} height - Frame height in pixels.
     * @returns {ScanResult[] | null}
     */
    processFrameRgba(data: Uint8Array, width: number, height: number): ScanResult[] | undefined;
    /**
     * Reset the session (clears dedup cooldowns and all statistics).
     */
    reset(): void;
    /**
     * Average scan time in milliseconds.
     */
    readonly avgScanTimeMs: number;
    /**
     * Total unique barcodes detected (after deduplication).
     */
    readonly codesDetected: number;
    /**
     * Number of frames actually scanned (not throttled).
     */
    readonly framesScanned: number;
    /**
     * Total number of frames submitted (including throttled frames).
     */
    readonly framesSubmitted: number;
    /**
     * Current frame skip interval (1 = every frame, 2 = 1-in-2, etc.).
     */
    readonly skipInterval: number;
}

/**
 * High-performance barcode scanner for the browser.
 *
 * Use this for one-shot scanning of images or captured frames.
 */
export class WasmScanner {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Create a new scanner.
     *
     * @param {boolean} [preprocessing=true] - Enable adaptive preprocessing.
     * @param {boolean} [try_invert=true] - Try inverted image (white-on-black).
     * @param {number} [roi_x] - ROI left (0.0–1.0).
     * @param {number} [roi_y] - ROI top (0.0–1.0).
     * @param {number} [roi_w] - ROI width (0.0–1.0).
     * @param {number} [roi_h] - ROI height (0.0–1.0).
     */
    constructor(preprocessing?: boolean | null, try_invert?: boolean | null, roi_x?: number | null, roi_y?: number | null, roi_w?: number | null, roi_h?: number | null);
    /**
     * Scan from a grayscale buffer (1 byte/pixel, fastest).
     *
     * @param {Uint8Array} data - Raw luma bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     */
    scanLuma(data: Uint8Array, width: number, height: number): ScanResult[];
    /**
     * Scan from a raw RGB buffer (3 bytes/pixel).
     *
     * @param {Uint8Array} data - Raw RGB bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     */
    scanRgb(data: Uint8Array, width: number, height: number): ScanResult[];
    /**
     * Scan from browser ImageData (RGBA, 4 bytes/pixel).
     *
     * @param {Uint8ClampedArray} data - The ImageData.data buffer.
     * @param {number} width - Frame width in pixels.
     * @param {number} height - Frame height in pixels.
     * @returns {ScanResult[]} Array of scan results (empty if not found).
     */
    scanRgba(data: Uint8Array, width: number, height: number): ScanResult[];
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_get_scanresult_confidence: (a: number) => number;
    readonly __wbg_get_scanresult_format: (a: number) => [number, number];
    readonly __wbg_get_scanresult_orientation: (a: number) => number;
    readonly __wbg_get_scanresult_points: (a: number) => [number, number];
    readonly __wbg_get_scanresult_text: (a: number) => [number, number];
    readonly __wbg_get_scanresult_timestamp_ms: (a: number) => number;
    readonly __wbg_scanresult_free: (a: number, b: number) => void;
    readonly __wbg_set_scanresult_confidence: (a: number, b: number) => void;
    readonly __wbg_set_scanresult_format: (a: number, b: number, c: number) => void;
    readonly __wbg_set_scanresult_orientation: (a: number, b: number) => void;
    readonly __wbg_set_scanresult_points: (a: number, b: number, c: number) => void;
    readonly __wbg_set_scanresult_text: (a: number, b: number, c: number) => void;
    readonly __wbg_set_scanresult_timestamp_ms: (a: number, b: number) => void;
    readonly __wbg_wasmscanner_free: (a: number, b: number) => void;
    readonly __wbg_wasmscansession_free: (a: number, b: number) => void;
    readonly wasmscanner_new: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
    readonly wasmscanner_scanLuma: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmscanner_scanRgb: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmscanner_scanRgba: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmscansession_avgScanTimeMs: (a: number) => number;
    readonly wasmscansession_clearCooldown: (a: number, b: number, c: number) => void;
    readonly wasmscansession_codesDetected: (a: number) => number;
    readonly wasmscansession_framesScanned: (a: number) => number;
    readonly wasmscansession_framesSubmitted: (a: number) => number;
    readonly wasmscansession_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => number;
    readonly wasmscansession_processFrameLuma: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmscansession_processFrameRgba: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly wasmscansession_reset: (a: number) => void;
    readonly wasmscansession_skipInterval: (a: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
