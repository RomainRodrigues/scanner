// Coordinate mapping between the preview element, the video and the decoder image.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clampRegion, decoderToVideo, outputSize, regionToVideoRect } from '../../dist/geometry.js';

test('outputSize keeps the width a multiple of 4 and never upscales', () => {
  assert.deepEqual(outputSize({ x: 0, y: 0, width: 1023, height: 431 }, 1024), { width: 1020, height: 431 });
  assert.deepEqual(outputSize({ x: 0, y: 0, width: 2048, height: 1024 }, 1024), { width: 1024, height: 512 });
  assert.equal(outputSize({ x: 0, y: 0, width: 2, height: 2 }, 1024).width, 4);
});

test('regionToVideoRect accounts for object-fit: cover cropping', () => {
  // 1280×720 video in a 360×640 portrait element: the video is cropped on the sides.
  const rect = regionToVideoRect({ x: 0, y: 0, width: 1, height: 1 }, 1280, 720, 360, 640);
  assert.ok(Math.abs(rect.width - 405) < 1, `width ${rect.width}`);
  assert.ok(Math.abs(rect.x - (1280 - 405) / 2) < 1, `x ${rect.x}`);
  assert.equal(rect.height, 720);
});

test('decoderToVideo maps pixel centres back through the crop and the downscale', () => {
  const rect = { x: 100, y: 50, width: 800, height: 400 };
  const [p] = decoderToVideo([{ x: 0, y: 0 }], rect, 400, 200);
  assert.deepEqual(p, { x: 101, y: 51 });
});

test('clampRegion keeps the region inside the element', () => {
  assert.deepEqual(clampRegion({ x: -1, y: 0.5, width: 3, height: 0.9 }), {
    x: 0,
    y: 0.5,
    width: 1,
    height: 0.5,
  });
});
