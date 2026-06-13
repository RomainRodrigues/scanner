/* @ts-self-types="./scanner_wasm.d.ts" */

/**
 * A detected barcode result, serialized as a JS object.
 */
export class ScanResult {
    static __wrap(ptr) {
        const obj = Object.create(ScanResult.prototype);
        obj.__wbg_ptr = ptr;
        ScanResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ScanResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_scanresult_free(ptr, 0);
    }
    /**
     * Confidence score [0.0, 1.0].
     * @returns {number}
     */
    get confidence() {
        const ret = wasm.__wbg_get_scanresult_confidence(this.__wbg_ptr);
        return ret;
    }
    /**
     * Barcode format (e.g. "QR Code", "EAN-13").
     * @returns {string}
     */
    get format() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_scanresult_format(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Rotation angle in degrees.
     * @returns {number}
     */
    get orientation() {
        const ret = wasm.__wbg_get_scanresult_orientation(this.__wbg_ptr);
        return ret;
    }
    /**
     * Detected corner points as flat [x0,y0, x1,y1, ...] array.
     * @returns {Float32Array}
     */
    get points() {
        const ret = wasm.__wbg_get_scanresult_points(this.__wbg_ptr);
        var v1 = getArrayF32FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Decoded text content of the barcode.
     * @returns {string}
     */
    get text() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_scanresult_text(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Unix timestamp (ms) when the scan completed.
     * @returns {number}
     */
    get timestamp_ms() {
        const ret = wasm.__wbg_get_scanresult_timestamp_ms(this.__wbg_ptr);
        return ret;
    }
    /**
     * Confidence score [0.0, 1.0].
     * @param {number} arg0
     */
    set confidence(arg0) {
        wasm.__wbg_set_scanresult_confidence(this.__wbg_ptr, arg0);
    }
    /**
     * Barcode format (e.g. "QR Code", "EAN-13").
     * @param {string} arg0
     */
    set format(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_scanresult_format(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Rotation angle in degrees.
     * @param {number} arg0
     */
    set orientation(arg0) {
        wasm.__wbg_set_scanresult_orientation(this.__wbg_ptr, arg0);
    }
    /**
     * Detected corner points as flat [x0,y0, x1,y1, ...] array.
     * @param {Float32Array} arg0
     */
    set points(arg0) {
        const ptr0 = passArrayF32ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_scanresult_points(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Decoded text content of the barcode.
     * @param {string} arg0
     */
    set text(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_scanresult_text(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Unix timestamp (ms) when the scan completed.
     * @param {number} arg0
     */
    set timestamp_ms(arg0) {
        wasm.__wbg_set_scanresult_timestamp_ms(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) ScanResult.prototype[Symbol.dispose] = ScanResult.prototype.free;

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
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmScanSessionFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmscansession_free(ptr, 0);
    }
    /**
     * Average scan time in milliseconds.
     * @returns {number}
     */
    get avgScanTimeMs() {
        const ret = wasm.wasmscansession_avgScanTimeMs(this.__wbg_ptr);
        return ret;
    }
    /**
     * Clear the cooldown for a specific barcode (allow immediate re-scan).
     * @param {string} text
     */
    clearCooldown(text) {
        const ptr0 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.wasmscansession_clearCooldown(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Total unique barcodes detected (after deduplication).
     * @returns {number}
     */
    get codesDetected() {
        const ret = wasm.wasmscansession_codesDetected(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Number of frames actually scanned (not throttled).
     * @returns {number}
     */
    get framesScanned() {
        const ret = wasm.wasmscansession_framesScanned(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Total number of frames submitted (including throttled frames).
     * @returns {number}
     */
    get framesSubmitted() {
        const ret = wasm.wasmscansession_framesSubmitted(this.__wbg_ptr);
        return ret >>> 0;
    }
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
     * @param {boolean | null} [preprocessing]
     * @param {boolean | null} [try_invert]
     * @param {number | null} [roi_x]
     * @param {number | null} [roi_y]
     * @param {number | null} [roi_w]
     * @param {number | null} [roi_h]
     * @param {number | null} [cooldown_ms]
     */
    constructor(preprocessing, try_invert, roi_x, roi_y, roi_w, roi_h, cooldown_ms) {
        const ret = wasm.wasmscansession_new(isLikeNone(preprocessing) ? 0xFFFFFF : preprocessing ? 1 : 0, isLikeNone(try_invert) ? 0xFFFFFF : try_invert ? 1 : 0, isLikeNone(roi_x) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_x), isLikeNone(roi_y) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_y), isLikeNone(roi_w) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_w), isLikeNone(roi_h) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_h), isLikeNone(cooldown_ms) ? Number.MAX_SAFE_INTEGER : (cooldown_ms) >>> 0);
        this.__wbg_ptr = ret;
        WasmScanSessionFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Process a camera frame (grayscale, 1 byte/pixel — fastest path).
     *
     * @param {Uint8Array} data - Raw luma bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     * @param {Uint8Array} data
     * @param {number} width
     * @param {number} height
     * @returns {ScanResult[] | undefined}
     */
    processFrameLuma(data, width, height) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmscansession_processFrameLuma(this.__wbg_ptr, ptr0, len0, width, height);
        let v2;
        if (ret[0] !== 0) {
            v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        }
        return v2;
    }
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
     * @param {Uint8Array} data
     * @param {number} width
     * @param {number} height
     * @returns {ScanResult[] | undefined}
     */
    processFrameRgba(data, width, height) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmscansession_processFrameRgba(this.__wbg_ptr, ptr0, len0, width, height);
        let v2;
        if (ret[0] !== 0) {
            v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        }
        return v2;
    }
    /**
     * Reset the session (clears dedup cooldowns and all statistics).
     */
    reset() {
        wasm.wasmscansession_reset(this.__wbg_ptr);
    }
    /**
     * Current frame skip interval (1 = every frame, 2 = 1-in-2, etc.).
     * @returns {number}
     */
    get skipInterval() {
        const ret = wasm.wasmscansession_skipInterval(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) WasmScanSession.prototype[Symbol.dispose] = WasmScanSession.prototype.free;

/**
 * High-performance barcode scanner for the browser.
 *
 * Use this for one-shot scanning of images or captured frames.
 */
export class WasmScanner {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmScannerFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmscanner_free(ptr, 0);
    }
    /**
     * Create a new scanner.
     *
     * @param {boolean} [preprocessing=true] - Enable adaptive preprocessing.
     * @param {boolean} [try_invert=true] - Try inverted image (white-on-black).
     * @param {number} [roi_x] - ROI left (0.0–1.0).
     * @param {number} [roi_y] - ROI top (0.0–1.0).
     * @param {number} [roi_w] - ROI width (0.0–1.0).
     * @param {number} [roi_h] - ROI height (0.0–1.0).
     * @param {boolean | null} [preprocessing]
     * @param {boolean | null} [try_invert]
     * @param {number | null} [roi_x]
     * @param {number | null} [roi_y]
     * @param {number | null} [roi_w]
     * @param {number | null} [roi_h]
     */
    constructor(preprocessing, try_invert, roi_x, roi_y, roi_w, roi_h) {
        const ret = wasm.wasmscanner_new(isLikeNone(preprocessing) ? 0xFFFFFF : preprocessing ? 1 : 0, isLikeNone(try_invert) ? 0xFFFFFF : try_invert ? 1 : 0, isLikeNone(roi_x) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_x), isLikeNone(roi_y) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_y), isLikeNone(roi_w) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_w), isLikeNone(roi_h) ? Number.MAX_SAFE_INTEGER : Math.fround(roi_h));
        this.__wbg_ptr = ret;
        WasmScannerFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Scan from a grayscale buffer (1 byte/pixel, fastest).
     *
     * @param {Uint8Array} data - Raw luma bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     * @param {Uint8Array} data
     * @param {number} width
     * @param {number} height
     * @returns {ScanResult[]}
     */
    scanLuma(data, width, height) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmscanner_scanLuma(this.__wbg_ptr, ptr0, len0, width, height);
        var v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
    /**
     * Scan from a raw RGB buffer (3 bytes/pixel).
     *
     * @param {Uint8Array} data - Raw RGB bytes.
     * @param {number} width - Frame width.
     * @param {number} height - Frame height.
     * @param {Uint8Array} data
     * @param {number} width
     * @param {number} height
     * @returns {ScanResult[]}
     */
    scanRgb(data, width, height) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmscanner_scanRgb(this.__wbg_ptr, ptr0, len0, width, height);
        var v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
    /**
     * Scan from browser ImageData (RGBA, 4 bytes/pixel).
     *
     * @param {Uint8ClampedArray} data - The ImageData.data buffer.
     * @param {number} width - Frame width in pixels.
     * @param {number} height - Frame height in pixels.
     * @returns {ScanResult[]} Array of scan results (empty if not found).
     * @param {Uint8Array} data
     * @param {number} width
     * @param {number} height
     * @returns {ScanResult[]}
     */
    scanRgba(data, width, height) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmscanner_scanRgba(this.__wbg_ptr, ptr0, len0, width, height);
        var v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
}
if (Symbol.dispose) WasmScanner.prototype[Symbol.dispose] = WasmScanner.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_is_undefined_721f8decd50c87a3: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_throw_ea4887a5f8f9a9db: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_getTime_7a770f8a2ec8d634: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_new_0_1b32bedde98fef4b: function() {
            const ret = new Date();
            return ret;
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_now_d2e0afbad4edbe82: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_now_e7c6795a7f81e10f: function(arg0) {
            const ret = arg0.now();
            return ret;
        },
        __wbg_performance_3fcf6e32a7e1ed0a: function(arg0) {
            const ret = arg0.performance;
            return ret;
        },
        __wbg_scanresult_new: function(arg0) {
            const ret = ScanResult.__wrap(arg0);
            return ret;
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_THIS_2fee5048bcca5938: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_ce44e66a4935da8c: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_44f6e0cb5e67cdad: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_168f178805d978fe: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./scanner_wasm_bg.js": import0,
    };
}

const ScanResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_scanresult_free(ptr, 1));
const WasmScanSessionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmscansession_free(ptr, 1));
const WasmScannerFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmscanner_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayF32ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 4, 4) >>> 0;
    getFloat32ArrayMemory0().set(arg, ptr / 4);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('scanner_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
