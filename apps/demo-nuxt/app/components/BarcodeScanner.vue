<script setup lang="ts">
// Reusable wrapper around the SDK's BarcodeReader: copy it into your own app.
import type { BarcodeFormat, BarcodeReader, Detection, ReaderStats } from '@gueshbzh-perso/scanner';

const props = defineProps<{ formats?: BarcodeFormat[] }>();
const emit = defineEmits<{
  detected: [codes: Detection[]];
  error: [error: Error];
  stats: [stats: ReaderStats];
}>();

const host = ref<HTMLElement | null>(null);
const running = ref(false);
let reader: BarcodeReader | null = null;
let creating: Promise<BarcodeReader> | null = null;
let statsTimer: ReturnType<typeof setInterval> | undefined;

/** One reader per component, even if `start` is clicked twice while it loads. */
function ensureReader(): Promise<BarcodeReader> {
  if (reader) return Promise.resolve(reader);
  creating ??= createReader().finally(() => (creating = null));
  return creating;
}

async function createReader(): Promise<BarcodeReader> {
  if (!host.value) throw new Error('BarcodeScanner is not mounted');
  // Imported lazily: the SDK only runs in the browser.
  const { BarcodeReader } = await import('@gueshbzh-perso/scanner');
  const created = await BarcodeReader.create({ host: host.value, formats: props.formats });
  created.on('detected', (codes) => emit('detected', codes));
  created.on('error', (err) => emit('error', err));
  reader = created;
  return created;
}

/** Must be called from a user gesture (button click) for audio feedback on iOS. */
async function start() {
  try {
    const r = await ensureReader();
    await r.start();
    running.value = true;
    clearInterval(statsTimer);
    statsTimer = setInterval(() => emit('stats', r.stats), 500);
  } catch (err) {
    emit('error', err instanceof Error ? err : new Error(String(err)));
  }
}

async function stop() {
  clearInterval(statsTimer);
  await reader?.stop();
  running.value = false;
}

watch(
  () => props.formats,
  (formats) => reader?.configure({ formats: formats ?? [] }),
);

onBeforeUnmount(() => {
  clearInterval(statsTimer);
  void reader?.destroy(); // releases the camera, the worker and WASM memory
  reader = null;
});

defineExpose({ start, stop, running });
</script>

<template>
  <div ref="host" class="barcode-scanner" />
</template>

<style scoped>
.barcode-scanner {
  width: 100%;
  aspect-ratio: 3 / 4;
  max-height: 70vh;
  border-radius: 12px;
  overflow: hidden;
  background: #000;
}
@media (min-width: 700px) {
  .barcode-scanner {
    aspect-ratio: 4 / 3;
  }
}
</style>
