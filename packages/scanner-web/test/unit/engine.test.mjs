// Unit tests of the pure parts of the SDK (no browser needed), run on `dist/`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultWorkerCount, pickWorker } from '../../dist/engine.js';

test('pickWorker takes the least busy live worker', () => {
  const workers = [
    { id: 'a', alive: true, busy: 2 },
    { id: 'b', alive: true, busy: 0 },
    { id: 'c', alive: true, busy: 1 },
  ];
  assert.equal(pickWorker(workers)?.id, 'b');
});

test('pickWorker skips dead workers, which reject instantly and look idle', () => {
  const workers = [
    { id: 'dead', alive: false, busy: 0 },
    { id: 'live', alive: true, busy: 3 },
  ];
  assert.equal(pickWorker(workers)?.id, 'live');
});

test('pickWorker returns undefined when every worker is dead', () => {
  assert.equal(pickWorker([{ alive: false, busy: 0 }]), undefined);
  assert.equal(pickWorker([]), undefined);
});

test('defaultWorkerCount: cores − 1, between 1 and 3', () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  try {
    for (const [cores, expected] of [
      [1, 1],
      [2, 1],
      [4, 3],
      [8, 3],
      [undefined, 1],
    ]) {
      Object.defineProperty(globalThis, 'navigator', {
        value: { hardwareConcurrency: cores },
        configurable: true,
      });
      assert.equal(defaultWorkerCount(), expected, `${cores} cores`);
    }
  } finally {
    if (original) Object.defineProperty(globalThis, 'navigator', original);
  }
});
