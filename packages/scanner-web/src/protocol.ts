// Messages exchanged between the SDK and its decoding Web Workers.
import type { FramePlan, FrameReport } from './core.js';
import type { PixelFormat } from './wasm/scanner_wasm.js';
import type { Detection, EngineOptions } from './types.js';

export type WorkerRequest =
  /** `module` is compiled once on the main thread and shared by every worker. */
  | { id: number; type: 'init'; module: WebAssembly.Module }
  | { id: number; type: 'configure'; options: EngineOptions }
  | {
      id: number;
      type: 'decode';
      buffer: ArrayBuffer;
      width: number;
      height: number;
      format: PixelFormat;
      plan: FramePlan;
    }
  | {
      id: number;
      type: 'scan';
      buffer: ArrayBuffer;
      width: number;
      height: number;
      format: PixelFormat;
      options: EngineOptions;
    };

export type WorkerResponse =
  | {
      id: number;
      ok: true;
      version?: string;
      report?: FrameReport;
      results?: Detection[];
      buffer?: ArrayBuffer;
    }
  | { id: number; ok: false; error: string; fatal?: boolean; buffer?: ArrayBuffer };
