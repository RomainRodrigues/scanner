// Messages exchanged between the SDK and its Web Worker.
import type { PixelFormat } from './wasm/scanner_wasm.js';
import type { Detection, EngineOptions } from './types.js';

export type WorkerRequest =
  | { id: number; type: 'init'; wasmUrl: string }
  | { id: number; type: 'configure'; options: EngineOptions }
  | { id: number; type: 'frame'; buffer: ArrayBuffer; width: number; height: number; format: PixelFormat }
  | { id: number; type: 'scan'; buffer: ArrayBuffer; width: number; height: number; format: PixelFormat; options: EngineOptions }
  | { id: number; type: 'reset' }
  | { id: number; type: 'clearCooldown'; text: string };

export type WorkerResponse =
  | { id: number; ok: true; version?: string; results?: Detection[] | null; live?: Detection[]; decodeMs?: number; buffer?: ArrayBuffer }
  | { id: number; ok: false; error: string; fatal?: boolean; buffer?: ArrayBuffer };
