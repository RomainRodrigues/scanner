<script setup lang="ts">
import type { BarcodeFormat, Detection, ReaderStats } from '@gueshbzh-perso/scanner';

const presets: { label: string; formats?: BarcodeFormat[] }[] = [
  { label: 'All formats' },
  { label: 'QR only', formats: ['qr_code'] },
  { label: 'Retail (EAN / UPC)', formats: ['ean_13', 'ean_8', 'upc_a', 'upc_e'] },
  { label: 'Logistics', formats: ['code_128', 'code_39', 'itf', 'data_matrix'] },
];

const { sdkVersion, commit, repository } = useRuntimeConfig().public;
const shortCommit = commit.slice(0, 7);

const preset = ref(0);
const formats = computed(() => presets[preset.value]?.formats);
const scanner = ref<{ start: () => Promise<void>; stop: () => Promise<void>; running: boolean } | null>(null);
const results = ref<(Detection & { source: string })[]>([]);
const error = ref('');
const stats = ref<ReaderStats | null>(null);
const busy = ref(false);

const statsLine = computed(() => {
  const s = stats.value;
  if (!s) return '';
  return (
    `${s.fps.toFixed(0)} fps · decode ${s.decodeMs.toFixed(1)} ms · grab ${s.grabMs.toFixed(1)} ms · ` +
    `${s.frameWidth}×${s.frameHeight} · ${s.backend}${s.simd ? ' · simd' : ''}${s.worker ? ` · ${s.workers} workers` : ''}`
  );
});

function onDetected(codes: Detection[], source = 'camera') {
  results.value.unshift(...codes.map((c) => ({ ...c, source })));
}

function onError(err: Error) {
  error.value = err.message;
}

async function toggle() {
  if (!scanner.value) return;
  error.value = '';
  busy.value = true;
  if (scanner.value.running) await scanner.value.stop();
  else await scanner.value.start();
  busy.value = false;
}

async function onFile(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0];
  if (!file) return;
  error.value = '';
  try {
    const { scanImage } = await import('@gueshbzh-perso/scanner');
    const codes = await scanImage(file, { formats: formats.value });
    if (codes.length) onDetected(codes, file.name);
    else error.value = `No code found in ${file.name}`;
  } catch (err) {
    onError(err as Error);
  }
}

async function copy(text: string) {
  await navigator.clipboard?.writeText(text).catch(() => undefined);
}
</script>

<template>
  <main>
    <h1>Scanner<span>.web</span> <small>Nuxt 4 demo</small></h1>

    <section class="card">
      <BarcodeScanner
        ref="scanner"
        :formats="formats"
        @detected="onDetected"
        @error="onError"
        @stats="stats = $event"
      />
      <div class="row">
        <button class="primary" :disabled="busy" @click="toggle">
          {{ scanner?.running ? 'Stop' : 'Start camera' }}
        </button>
        <select v-model.number="preset" aria-label="Formats">
          <option v-for="(p, i) in presets" :key="p.label" :value="i">{{ p.label }}</option>
        </select>
        <label class="file">
          Scan a photo
          <input type="file" accept="image/*" hidden @change="onFile" />
        </label>
      </div>
      <p v-if="statsLine" class="stats">{{ statsLine }}</p>
      <p v-if="error" class="error">{{ error }}</p>
    </section>

    <section class="card">
      <p v-if="!results.length" class="dim">Nothing scanned yet.</p>
      <ul>
        <li v-for="(r, i) in results" :key="i" @click="copy(r.text)">
          <b>{{ r.text }}</b>
          <small>{{ r.format }} · {{ r.source }} · {{ new Date(r.timestamp).toLocaleTimeString() }}</small>
        </li>
      </ul>
    </section>
    <footer>
      <span>SDK v{{ sdkVersion }}</span>
      <template v-if="shortCommit">
        ·
        <a :href="`https://github.com/${repository}/commit/${commit}`" target="_blank" rel="noopener">{{
          shortCommit
        }}</a>
      </template>
    </footer>
  </main>
</template>

<style>
:root {
  --bg: #0f0f14;
  --surface: #1a1a24;
  --border: #2a2a3a;
  --accent: #00d4aa;
  --text: #e8e8f0;
  --dim: #8a8a9a;
}
* {
  box-sizing: border-box;
}
body {
  margin: 0;
  font-family: system-ui, sans-serif;
  background: var(--bg);
  color: var(--text);
}
main {
  max-width: 760px;
  margin: 0 auto;
  padding: 16px;
  display: grid;
  gap: 16px;
}
h1 {
  font-size: 1.2rem;
  margin: 0;
}
h1 span {
  color: var(--accent);
}
h1 small {
  color: var(--dim);
  font-weight: 400;
  font-size: 0.8rem;
}
.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 12px;
}
.row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 12px;
}
button,
select,
.file {
  font: inherit;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--border);
  color: var(--text);
  cursor: pointer;
}
button.primary {
  background: var(--accent);
  color: #002b22;
  border-color: var(--accent);
  font-weight: 600;
}
button:disabled {
  opacity: 0.5;
}
.stats {
  font:
    12px ui-monospace,
    monospace;
  color: var(--dim);
  margin: 10px 0 0;
}
.error {
  color: #ff6b6b;
  margin: 10px 0 0;
}
.dim {
  color: var(--dim);
  margin: 0;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 8px;
}
li {
  border-left: 3px solid var(--accent);
  background: var(--bg);
  padding: 8px 12px;
  border-radius: 6px;
  cursor: copy;
}
li b {
  word-break: break-all;
}
li small {
  color: var(--dim);
  display: block;
}
footer {
  text-align: center;
  font:
    12px ui-monospace,
    monospace;
  color: var(--dim);
}
footer a {
  color: inherit;
}
</style>
