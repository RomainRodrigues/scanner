#!/usr/bin/env node
// End-to-end test in headless Chromium with a fake camera.
//
// Requirements: `npm run build`, Playwright + Chromium and ffmpeg.
// Set CHROMIUM_PATH to use an existing Chromium binary.
// Camera feeds are rendered from the committed images in tests/fixtures/.
//
// For each scenario a synthetic camera feed (.y4m) is rendered with ffmpeg and
// fed to Chromium as its webcam, then the real SDK is driven in the page.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { cpus, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { serve } from './serve.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const pkgDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const images = resolve(pkgDir, '../../tests/fixtures');
const tmp = mkdtempSync(join(tmpdir(), 'scanner-e2e-'));
// Optional: use an existing Chromium instead of Playwright's own download.
const executablePath = process.env.CHROMIUM_PATH || undefined;

/** Render a 1280×720 camera feed with `image` pasted at the center, scaled to `size` px wide. */
function fakeCamera(name, image, size, { invert = false, photo = false } = {}) {
  const out = join(tmp, `${name}.y4m`);
  const neg = invert ? ',negate' : '';
  // Synthetic codes keep sharp modules; real photos are resampled smoothly.
  const filter = photo ? 'bicubic' : 'neighbor';
  execFileSync('ffmpeg', [
    '-loglevel',
    'error',
    '-y',
    '-f',
    'lavfi',
    '-i',
    `color=${invert ? 'black' : 'white'}:s=1280x720:r=30`,
    '-i',
    image,
    '-filter_complex',
    `[1]scale=${size}:-1:flags=${filter}${neg}[c];[0][c]overlay=(W-w)/2:(H-h)/2`,
    '-frames:v',
    '30',
    '-pix_fmt',
    'yuv420p',
    out,
  ]);
  return out;
}

const scenarios = [
  {
    // WebGL forced: in headless Chromium the GPU is emulated (SwiftShader) and
    // slow, so the 'auto' backend would switch to Canvas 2D and leave the
    // WebGL path untested.
    name: 'qr-webgl-worker',
    image: 'qr/clean_hd.png',
    size: 330,
    expect: 'https://github.com/romrd/scanner',
    options: { processing: { backend: 'webgl' } },
  },
  {
    name: 'ean13-webgl-worker',
    image: 'barcode_1d/ean13_clean.png',
    size: 700,
    expect: '5901234123457',
    options: { formats: ['ean_13'], processing: { backend: 'webgl' } },
  },
  {
    name: 'code128-canvas-inline',
    image: 'barcode_1d/code128.png',
    size: 800,
    expect: 'SCANNER-TEST-128',
    options: { worker: false, processing: { backend: 'canvas' } },
  },
  {
    name: 'qr-inverted',
    image: 'qr/clean_hd.png',
    size: 330,
    invert: true,
    expect: 'https://github.com/romrd/scanner',
    options: { processing: { backend: 'webgl' } },
  },
  {
    // Real phone photo: laser-engraved Code 128 on a cattle ear tag. Default
    // ('auto') backend.
    name: 'ear-tag-photo',
    image: 'real/cattle_ear_tag_code128.jpg',
    size: 540,
    photo: true,
    expect: '02502239036645',
    options: {},
  },
];

const server = await serve(0);
const base = `http://localhost:${server.address().port}`;
let failures = 0;

for (const s of scenarios) {
  const src = join(images, s.image);
  if (!existsSync(src)) {
    throw new Error(`missing fixture ${s.image}`);
  }
  const video = fakeCamera(s.name, src, s.size, { invert: s.invert, photo: s.photo });
  const browser = await chromium.launch({
    executablePath,
    args: [
      '--use-fake-ui-for-media-stream',
      '--use-fake-device-for-media-stream',
      `--use-file-for-fake-video-capture=${video}`,
      '--enable-unsafe-swiftshader',
    ],
  });
  try {
    const page = await browser.newPage();
    const logs = [];
    page.on('console', (m) => logs.push(`[${m.type()}] ${m.text()}`));
    page.on('pageerror', (e) => logs.push(`[pageerror] ${e.message}`));
    await page.goto(`${base}/test/e2e.html`);
    const out = await page.evaluate(async (options) => {
      const { BarcodeReader } = await import('/dist/index.js');
      const host = document.createElement('div');
      host.style.cssText = 'width:640px;height:360px';
      document.body.prepend(host);
      const reader = await BarcodeReader.create({ host, feedback: false, ...options });
      const t0 = performance.now();
      const detected = new Promise((ok) => reader.on('detected', ok));
      await reader.start();
      const codes = await Promise.race([detected, new Promise((r) => setTimeout(() => r(null), 8000))]);
      const elapsed = performance.now() - t0;
      // Throughput: the reader keeps scanning (duplicates are filtered), and
      // `stats.fps` is refreshed every second.
      await new Promise((r) => setTimeout(r, 2200));
      const result = {
        codes,
        elapsed,
        stats: reader.stats,
        video: [reader.video.videoWidth, reader.video.videoHeight],
      };
      await reader.destroy();
      return result;
    }, s.options);

    const code = out.codes?.[0];
    const ok = code?.text === s.expect;
    // Points must be in video space, around the center of the 1280×720 frame.
    const centered = code?.points.every(
      (p) => p.x > 640 - s.size && p.x < 640 + s.size && p.y > 0 && p.y < 720,
    );
    const st = out.stats;
    const info =
      `${st.backend}${st.worker ? `+${st.workers} workers` : ''} ` +
      `${st.frameWidth}×${st.frameHeight}, ` +
      `first read ${out.elapsed.toFixed(0)} ms, ${st.fps.toFixed(1)} fps, decode ${st.decodeMs.toFixed(1)} ms, ` +
      `grab ${st.grabMs.toFixed(1)} ms`;
    if (ok && centered) {
      console.log(`✓ ${s.name}: "${code.text}" (${code.format}) — ${info}`);
    } else {
      failures++;
      console.log(`✗ ${s.name}: expected "${s.expect}", got ${JSON.stringify(code)} — ${info}`);
      console.log(logs.join('\n'));
    }
  } catch (err) {
    failures++;
    console.log(`✗ ${s.name}: ${err.message}`);
  } finally {
    await browser.close();
  }
}

// scanImage() on a still image.
{
  const browser = await chromium.launch({ executablePath });
  try {
    const page = await browser.newPage();
    await page.goto(`${base}/test/e2e.html`);
    const png = readFileSync(join(images, 'qr/multiple.png')).toString('base64');
    const texts = await page.evaluate(async (b64) => {
      const { scanImage } = await import('/dist/index.js');
      const blob = await (await fetch(`data:image/png;base64,${b64}`)).blob();
      const codes = await scanImage(blob);
      return codes.map((c) => c.text).sort();
    }, png);
    if (JSON.stringify(texts) === JSON.stringify(['Code A', 'Code B'])) {
      console.log(`✓ scanImage multiple: ${texts.join(', ')}`);
    } else {
      failures++;
      console.log(`✗ scanImage multiple: got ${JSON.stringify(texts)}`);
    }
  } finally {
    await browser.close();
  }
}

// Reader lifecycle: concurrent start() calls must open the camera once, and a
// stop() during start() must release the camera it was opening.
{
  const video = fakeCamera('lifecycle', join(images, 'qr/clean_hd.png'), 400, {});
  const browser = await chromium.launch({
    executablePath,
    args: [
      '--use-fake-ui-for-media-stream',
      '--use-fake-device-for-media-stream',
      `--use-file-for-fake-video-capture=${video}`,
      '--enable-unsafe-swiftshader',
    ],
  });
  try {
    const page = await browser.newPage();
    await page.goto(`${base}/test/e2e.html`);
    const out = await page.evaluate(async () => {
      const streams = [];
      const gum = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
      navigator.mediaDevices.getUserMedia = async (c) => {
        const s = await gum(c);
        streams.push(s);
        return s;
      };
      const { BarcodeReader } = await import('/dist/index.js');
      const host = document.createElement('div');
      host.style.cssText = 'width:640px;height:360px';
      document.body.prepend(host);
      const reader = await BarcodeReader.create({ host, feedback: false });
      await Promise.all([reader.start(), reader.start()]);
      const opened = streams.length;
      await reader.stop();
      const start = reader.start();
      await reader.stop();
      await start;
      const live = streams.flatMap((s) => s.getTracks()).filter((t) => t.readyState === 'live').length;
      const running = reader.isRunning;
      await reader.destroy();
      return { opened, live, running };
    });
    const ok = out.opened === 1 && out.live === 0 && out.running === false;
    const line = `reader lifecycle: ${JSON.stringify(out)}`;
    if (ok) {
      console.log(`✓ ${line}`);
    } else {
      failures++;
      console.log(`✗ ${line} (expected one camera opened, none left live, not running)`);
    }

    // Hidden page: the camera is released, then reopened when visible again,
    // without `stopped` / `started` events and with decoding resuming.
    const bg = await page.evaluate(async () => {
      const streams = [];
      const gum = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
      navigator.mediaDevices.getUserMedia = async (c) => {
        const s = await gum(c);
        streams.push(s);
        return s;
      };
      let hidden = false;
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
      Object.defineProperty(document, 'visibilityState', {
        configurable: true,
        get: () => (hidden ? 'hidden' : 'visible'),
      });
      const setHidden = (h) => {
        hidden = h;
        document.dispatchEvent(new Event('visibilitychange'));
      };
      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
      const live = (s) => s.getTracks().some((t) => t.readyState === 'live');

      const { BarcodeReader } = await import('/dist/index.js');
      const host = document.createElement('div');
      host.style.cssText = 'width:640px;height:360px';
      document.body.prepend(host);
      const reader = await BarcodeReader.create({ host, feedback: false });
      const events = [];
      reader.on('started', () => events.push('started'));
      reader.on('stopped', () => events.push('stopped'));
      await reader.start();
      setHidden(true);
      await sleep(300);
      const whileHidden = { cameraLive: streams.some(live), running: reader.isRunning };
      setHidden(false);
      await sleep(2500);
      const afterVisible = {
        reopened: streams.length === 2 && live(streams[1]),
        running: reader.isRunning,
        fps: reader.stats.fps,
      };
      await reader.destroy();
      return { whileHidden, afterVisible, events };
    });
    const bgOk =
      !bg.whileHidden.cameraLive &&
      bg.whileHidden.running &&
      bg.afterVisible.reopened &&
      bg.afterVisible.running &&
      bg.afterVisible.fps > 0 &&
      JSON.stringify(bg.events) === JSON.stringify(['started', 'stopped']);
    const bgLine = `camera released while hidden: ${JSON.stringify(bg)}`;
    if (bgOk) {
      console.log(`✓ ${bgLine}`);
    } else {
      failures++;
      console.log(`✗ ${bgLine}`);
    }
  } finally {
    await browser.close();
  }
}

// Engine throughput: textured frames without a code (the common, most
// expensive case) sent straight to the engine, `workers` frames in flight.
// Decoding is CPU-bound, so the pool should scale with the available cores.
{
  const browser = await chromium.launch({ executablePath });
  try {
    const page = await browser.newPage();
    await page.goto(`${base}/test/e2e.html`);
    const runs = await page.evaluate(async () => {
      const { createEngine } = await import('/dist/index.js');
      const [w, h, frames] = [1024, 432, 36];
      const texture = new Uint8Array(w * h);
      for (let i = 0; i < texture.length; i++)
        texture[i] = 80 + ((i * 7919 + Math.floor(i / w) * 104729) % 97);
      const out = [];
      for (const workers of [1, 3]) {
        const engine = await createEngine({ workers });
        await engine.configure({});
        let sent = 0;
        const t0 = performance.now();
        const lane = async () => {
          while (sent < frames) {
            sent++;
            await engine.processFrame(texture.slice().buffer, w, h, 'luma');
          }
        };
        await Promise.all(Array.from({ length: engine.concurrency }, lane));
        out.push({ workers: engine.concurrency, fps: (frames * 1000) / (performance.now() - t0) });
        engine.destroy();
      }
      return out;
    });
    const [one, three] = runs;
    const ok = one.workers === 1 && three.workers === 3;
    const line =
      `engine throughput (${cpus().length} cores): 1 worker ${one.fps.toFixed(1)} fps, ` +
      `3 workers ${three.fps.toFixed(1)} fps (×${(three.fps / one.fps).toFixed(2)})`;
    if (ok) {
      console.log(`✓ ${line}`);
    } else {
      failures++;
      console.log(`✗ ${line}: ${JSON.stringify(runs)}`);
    }
  } finally {
    await browser.close();
  }
}

server.close();
rmSync(tmp, { recursive: true, force: true });
if (failures) {
  console.log(`\n${failures} scenario(s) failed`);
  process.exit(1);
}
console.log('\nAll e2e scenarios passed');
