// One-shot scanning of still images (file uploads, photos, canvases…).
import { pickEngineOptions } from './core.js';
import { sharedEngine } from './engine.js';
import type { AssetOptions, Detection, EngineOptions } from './types.js';

export type ImageInput =
  | Blob
  | ImageData
  | ImageBitmap
  | HTMLImageElement
  | HTMLCanvasElement
  | HTMLVideoElement
  | OffscreenCanvas;

export interface ScanImageOptions extends EngineOptions, AssetOptions {
  /** Images larger than this (longest side, px) are downscaled first. Default: `2048`. */
  maxDimension?: number;
}

/**
 * Scan a still image. Runs every decoding strategy (rotations, inversion,
 * contrast enhancement…) and returns all codes found, with points in the
 * pixel coordinates of the source image.
 *
 * ```ts
 * const [code] = await scanImage(fileInput.files[0], { formats: ['qr_code'] });
 * ```
 */
export async function scanImage(input: ImageInput, options: ScanImageOptions = {}): Promise<Detection[]> {
  const engine = await sharedEngine(options);
  const { data, width, height, scale } = await rasterize(input, options.maxDimension ?? 2048);
  const engineOptions: EngineOptions = { ...pickEngineOptions(options), maxResults: options.maxResults ?? 8 };
  const results = await engine.scanImage(data.buffer as ArrayBuffer, width, height, 'rgba', engineOptions);
  return scale === 1
    ? results
    : results.map((r) => ({ ...r, points: r.points.map((p) => ({ x: p.x / scale, y: p.y / scale })) }));
}

async function rasterize(input: ImageInput, maxDim: number) {
  if (typeof ImageData !== 'undefined' && input instanceof ImageData && Math.max(input.width, input.height) <= maxDim) {
    return { data: new Uint8Array(input.data.slice().buffer), width: input.width, height: input.height, scale: 1 };
  }

  let source: CanvasImageSource | ImageBitmap;
  let owned: ImageBitmap | null = null;
  if (input instanceof Blob) {
    owned = await createImageBitmap(input);
    source = owned;
  } else if (typeof ImageData !== 'undefined' && input instanceof ImageData) {
    owned = await createImageBitmap(input);
    source = owned;
  } else {
    if (typeof HTMLImageElement !== 'undefined' && input instanceof HTMLImageElement && !input.complete) {
      await input.decode();
    }
    source = input as CanvasImageSource;
  }

  const [sw, sh] = size(source);
  if (!sw || !sh) throw new Error('scanImage: image has no pixels');
  const scale = Math.min(1, maxDim / Math.max(sw, sh));
  const width = Math.max(1, Math.round(sw * scale));
  const height = Math.max(1, Math.round(sh * scale));

  const canvas =
    typeof OffscreenCanvas !== 'undefined'
      ? new OffscreenCanvas(width, height)
      : Object.assign(document.createElement('canvas'), { width, height });
  const ctx = canvas.getContext('2d', { willReadFrequently: true }) as
    | CanvasRenderingContext2D
    | OffscreenCanvasRenderingContext2D
    | null;
  if (!ctx) throw new Error('scanImage: 2D canvas is not available');
  ctx.drawImage(source, 0, 0, width, height);
  owned?.close();
  const pixels = ctx.getImageData(0, 0, width, height).data;
  return { data: new Uint8Array(pixels.buffer), width, height, scale };
}

function size(s: CanvasImageSource | ImageBitmap): [number, number] {
  if (typeof HTMLImageElement !== 'undefined' && s instanceof HTMLImageElement) return [s.naturalWidth, s.naturalHeight];
  if (typeof HTMLVideoElement !== 'undefined' && s instanceof HTMLVideoElement) return [s.videoWidth, s.videoHeight];
  const o = s as { width: number; height: number };
  return [o.width, o.height];
}
