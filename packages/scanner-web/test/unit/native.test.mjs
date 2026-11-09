// NativeDetector with a fake `BarcodeDetector` (Node has no native one).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { NativeDetector, isBuiltIn } from '../../dist/native.js';

/** A fake `BarcodeDetector` class supporting `supported`, returning `codes`. */
function fakeApi(supported, codes = []) {
  return class {
    static created = [];
    static getSupportedFormats = async () => supported;
    constructor(options) {
      this.constructor.created.push(options);
    }
    async detect() {
      return codes;
    }
  };
}

const ALL = [
  'aztec',
  'code_128',
  'code_39',
  'code_93',
  'codabar',
  'data_matrix',
  'ean_13',
  'ean_8',
  'itf',
  'pdf417',
  'qr_code',
  'upc_a',
  'upc_e',
];

const square = (x, y, s) => [
  { x, y },
  { x: x + s, y },
  { x: x + s, y: y + s },
  { x, y: y + s },
];

test('no BarcodeDetector API: unavailable', async () => {
  assert.equal(await NativeDetector.create(['qr_code'], undefined), undefined);
});

test('empty supported list (Play Services module not downloaded yet): unavailable', async () => {
  assert.equal(await NativeDetector.create(undefined, fakeApi([])), undefined);
  assert.equal(await NativeDetector.create(['qr_code'], fakeApi([])), undefined);
});

test('getSupportedFormats() failing: unavailable', async () => {
  const api = fakeApi(ALL);
  api.getSupportedFormats = async () => {
    throw new Error('boom');
  };
  assert.equal(await NativeDetector.create(['qr_code'], api), undefined);
});

test('a requested format missing natively: unavailable', async () => {
  const api = fakeApi(['qr_code', 'ean_13']);
  assert.equal(await NativeDetector.create(['qr_code', 'code_39'], api), undefined);
  // An explicit list is strict: every format must be native.
  assert.equal(await NativeDetector.create(['ean_13', 'code_39'], api), undefined);
});

test('requested formats are passed to the native detector, micro_qr excepted', async () => {
  const api = fakeApi(ALL);
  // With the engine fallback, which reads micro_qr.
  const det = await NativeDetector.create(['qr_code', 'micro_qr', 'ean_13'], api, true);
  assert.deepEqual(det?.formats, ['qr_code', 'ean_13']);
  assert.deepEqual(api.created, [{ formats: ['qr_code', 'ean_13'] }]);
  // micro_qr alone: nothing for the native detector to do.
  assert.equal(await NativeDetector.create(['micro_qr'], api, true), undefined);
  // Default: every format it has.
  assert.equal((await NativeDetector.create(undefined, api))?.formats.length, 13);
});

test('detect keeps the codes inside the region and of the requested formats', async () => {
  const codes = [
    { rawValue: 'inside', format: 'qr_code', cornerPoints: square(100, 100, 50) },
    { rawValue: 'outside', format: 'qr_code', cornerPoints: square(500, 100, 50) },
    { rawValue: 'other format', format: 'ean_13', cornerPoints: square(120, 120, 20) },
    { rawValue: 'unknown', format: 'unknown', cornerPoints: square(120, 120, 20) },
    { rawValue: 'no points', format: 'qr_code', cornerPoints: [] },
  ];
  const det = await NativeDetector.create(['qr_code'], fakeApi(ALL, codes));
  const found = await det.detect({}, { x: 50, y: 50, width: 200, height: 200 });
  assert.deepEqual(
    found.map((d) => d.text),
    ['inside'],
  );
  assert.equal(found[0].format, 'qr_code');
  assert.deepEqual(found[0].points, square(100, 100, 50));
  assert.equal(typeof found[0].timestamp, 'number');
});

test('detect propagates native failures', async () => {
  const api = fakeApi(ALL);
  api.prototype.detect = async () => {
    throw new DOMException('Barcode detection service unavailable.', 'NotSupportedError');
  };
  const det = await NativeDetector.create(['qr_code'], api);
  await assert.rejects(det.detect({}, { x: 0, y: 0, width: 10, height: 10 }), /unavailable/);
});

test('only a built-in BarcodeDetector counts, not a JavaScript polyfill', async () => {
  assert.equal(isBuiltIn(Math.max), true);
  assert.equal(isBuiltIn(Map), true);
  assert.equal(isBuiltIn(fakeApi(ALL)), false);
  assert.equal(
    isBuiltIn(function BarcodeDetector() {
      return undefined;
    }),
    false,
  );
  // Default lookup on globalThis: a polyfill installed there is ignored.
  globalThis.BarcodeDetector = fakeApi(ALL);
  try {
    assert.equal(await NativeDetector.create(['qr_code']), undefined);
  } finally {
    delete globalThis.BarcodeDetector;
  }
});

test('without the engine fallback, an explicit micro_qr keeps the engine', async () => {
  const api = fakeApi(ALL);
  // Nothing would read micro_qr: the native detector is not used.
  assert.equal(await NativeDetector.create(['qr_code', 'micro_qr'], api), undefined);
  assert.equal(await NativeDetector.create(['qr_code', 'micro_qr'], api, false), undefined);
  // The default list (every format) accepts not reading micro_qr.
  assert.equal((await NativeDetector.create(undefined, api))?.formats.length, 13);
  assert.equal((await NativeDetector.create([], api))?.formats.length, 13);
});

test('default list: the native formats it has, the others listed as missing', async () => {
  // A native decoder without Codabar and UPC-A (possible on Safari / Vision).
  const api = fakeApi(ALL.filter((f) => f !== 'codabar' && f !== 'upc_a'));
  const det = await NativeDetector.create(undefined, api);
  assert.equal(det?.formats.length, 11);
  assert.deepEqual([...(det?.missing ?? [])].sort(), ['codabar', 'upc_a']);
  assert.deepEqual(api.created.at(-1).formats, det?.formats);
  // Everything native: nothing missing, no engine fallback needed.
  assert.deepEqual((await NativeDetector.create(undefined, fakeApi(ALL)))?.missing, []);
  // Explicit list naming a missing format: the engine keeps the job.
  assert.equal(await NativeDetector.create(['qr_code', 'codabar'], api), undefined);
  // Explicit list of native formats only: fine.
  assert.deepEqual((await NativeDetector.create(['qr_code'], api))?.missing, []);
});
