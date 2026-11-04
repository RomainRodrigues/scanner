// The 'auto' frame backend: WebGL, unless Canvas 2D is measured much faster.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { AdaptiveGrabber } from '../../dist/grabber.js';

/** Fake grabbers on a fake clock: each grab "takes" `cost` ms. */
function setup(webglMs, canvasMs) {
  const clock = { t: 0 };
  const destroyed = [];
  const fake = (kind, cost) => ({
    kind,
    grab: async () => {
      clock.t += cost;
    },
    destroy: () => destroyed.push(kind),
  });
  const grabber = new AdaptiveGrabber(
    fake('webgl', webglMs),
    () => fake('canvas', canvasMs),
    () => clock.t,
  );
  const run = async (frames) => {
    for (let i = 0; i < frames; i++) await grabber.grab(null, null, 4, 1, new Uint8Array(4));
  };
  return { grabber, run, destroyed };
}

test('fast WebGL is kept without ever trying Canvas 2D', async () => {
  const { grabber, run, destroyed } = setup(8, 2);
  await run(100);
  assert.equal(grabber.kind, 'webgl');
  assert.deepEqual(destroyed, []);
});

test('slow WebGL readback switches to a clearly faster Canvas 2D', async () => {
  const { grabber, run, destroyed } = setup(55, 10);
  await run(15);
  assert.equal(grabber.kind, 'canvas', 'probing');
  await run(15);
  assert.equal(grabber.kind, 'canvas');
  assert.deepEqual(destroyed, ['webgl']);
  await run(50);
  assert.equal(grabber.kind, 'canvas', 'decision is final');
});

test('Canvas 2D only slightly faster: back to WebGL', async () => {
  const { grabber, run, destroyed } = setup(40, 30);
  await run(30);
  assert.equal(grabber.kind, 'webgl');
  assert.deepEqual(destroyed, ['canvas']);
});
