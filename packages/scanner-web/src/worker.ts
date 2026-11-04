// Web Worker entry point: decodes frames off the main thread. Stateless: the
// scan session (scheduling, confirmations, de-duplication) lives on the main
// thread, so any number of these workers can serve one camera stream.
import { DecoderCore, loadWasm } from './core.js';
import type { WorkerRequest, WorkerResponse } from './protocol.js';

interface WorkerScope {
  onmessage: ((ev: MessageEvent<WorkerRequest>) => void) | null;
  postMessage(message: WorkerResponse, transfer?: Transferable[]): void;
}

const scope = self as unknown as WorkerScope;
let core: DecoderCore | null = null;

function engine(): DecoderCore {
  if (!core) throw new Error('Engine not initialised');
  return core;
}

scope.onmessage = async (ev) => {
  const msg = ev.data;
  const buffer = 'buffer' in msg ? msg.buffer : undefined;
  try {
    switch (msg.type) {
      case 'init': {
        const version = await loadWasm(msg.module);
        core = new DecoderCore();
        scope.postMessage({ id: msg.id, ok: true, version });
        break;
      }
      case 'configure':
        engine().configure(msg.options);
        scope.postMessage({ id: msg.id, ok: true });
        break;
      case 'decode': {
        const pixels = new Uint8Array(msg.buffer);
        const report = engine().decode(pixels, msg.width, msg.height, msg.format, msg.plan);
        // Hand the pixel buffer back so the main thread can reuse it.
        scope.postMessage({ id: msg.id, ok: true, report, buffer: msg.buffer }, [msg.buffer]);
        break;
      }
      case 'scan': {
        const pixels = new Uint8Array(msg.buffer);
        const results = engine().scan(pixels, msg.width, msg.height, msg.format, msg.options);
        scope.postMessage({ id: msg.id, ok: true, results, buffer: msg.buffer }, [msg.buffer]);
        break;
      }
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
