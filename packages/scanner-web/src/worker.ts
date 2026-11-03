// Web Worker entry point: decodes frames off the main thread.
import { EngineCore } from './core.js';
import type { WorkerRequest, WorkerResponse } from './protocol.js';

interface WorkerScope {
  onmessage: ((ev: MessageEvent<WorkerRequest>) => void) | null;
  postMessage(message: WorkerResponse, transfer?: Transferable[]): void;
}

const scope = self as unknown as WorkerScope;
let core: EngineCore | null = null;

function engine(): EngineCore {
  if (!core) throw new Error('Engine not initialised');
  return core;
}

scope.onmessage = async (ev) => {
  const msg = ev.data;
  const buffer = 'buffer' in msg ? msg.buffer : undefined;
  try {
    switch (msg.type) {
      case 'init':
        core = await EngineCore.create(msg.wasmUrl);
        scope.postMessage({ id: msg.id, ok: true, version: core.version });
        break;
      case 'configure':
        engine().configure(msg.options);
        scope.postMessage({ id: msg.id, ok: true });
        break;
      case 'frame': {
        const out = engine().process(new Uint8Array(msg.buffer), msg.width, msg.height, msg.format);
        // Hand the pixel buffer back so the main thread can reuse it.
        scope.postMessage({ id: msg.id, ok: true, ...out, buffer: msg.buffer }, [msg.buffer]);
        break;
      }
      case 'scan': {
        const results = engine().scan(
          new Uint8Array(msg.buffer),
          msg.width,
          msg.height,
          msg.format,
          msg.options,
        );
        scope.postMessage({ id: msg.id, ok: true, results, buffer: msg.buffer }, [msg.buffer]);
        break;
      }
      case 'reset':
        engine().reset();
        scope.postMessage({ id: msg.id, ok: true });
        break;
      case 'clearCooldown':
        engine().clearCooldown(msg.text);
        scope.postMessage({ id: msg.id, ok: true });
        break;
    }
  } catch (err) {
    const error = err instanceof Error ? err.message : String(err);
    // A WebAssembly trap (Rust panic) leaves the module in an undefined state:
    // report it as fatal so the SDK spawns a fresh worker.
    const fatal = err instanceof WebAssembly.RuntimeError;
    scope.postMessage({ id: msg.id, ok: false, error, fatal, buffer }, buffer ? [buffer] : []);
    if (fatal) core = null;
  }
};
