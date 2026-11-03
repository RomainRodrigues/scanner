// Coordinate mapping between the host element (what the user sees), the video
// frame, and the downscaled image handed to the decoder.
import type { Rect } from './grabber.js';
import type { Point, Region } from './types.js';

/** How a video of `vw×vh` is laid out in an element of `ew×eh` with `object-fit: cover`. */
export interface CoverLayout {
  scale: number;
  offsetX: number;
  offsetY: number;
}

export function coverLayout(vw: number, vh: number, ew: number, eh: number): CoverLayout {
  const scale = Math.max(ew / vw, eh / vh);
  return { scale, offsetX: (ew - vw * scale) / 2, offsetY: (eh - vh * scale) / 2 };
}

export function videoToElement(p: Point, l: CoverLayout): Point {
  return { x: p.x * l.scale + l.offsetX, y: p.y * l.scale + l.offsetY };
}

/** Region of interest (fractions of the element) → rectangle in video pixels, clamped to the frame. */
export function regionToVideoRect(r: Region, vw: number, vh: number, ew: number, eh: number): Rect {
  const l = coverLayout(vw, vh, ew, eh);
  const x0 = clamp((r.x * ew - l.offsetX) / l.scale, 0, vw);
  const y0 = clamp((r.y * eh - l.offsetY) / l.scale, 0, vh);
  const x1 = clamp(((r.x + r.width) * ew - l.offsetX) / l.scale, 0, vw);
  const y1 = clamp(((r.y + r.height) * eh - l.offsetY) / l.scale, 0, vh);
  return { x: x0, y: y0, width: Math.max(1, x1 - x0), height: Math.max(1, y1 - y0) };
}

/** Size of the decoder image for a crop: downscaled to `maxDim`, width rounded to a multiple of 4. */
export function outputSize(rect: Rect, maxDim: number): { width: number; height: number } {
  const scale = Math.min(1, maxDim / Math.max(rect.width, rect.height));
  const width = Math.max(4, Math.floor((rect.width * scale) / 4) * 4);
  const height = Math.max(1, Math.round(rect.height * scale));
  return { width, height };
}

/** Map points from decoder-image space back to video pixels. */
export function decoderToVideo(points: Point[], rect: Rect, width: number, height: number): Point[] {
  const sx = rect.width / width;
  const sy = rect.height / height;
  return points.map((p) => ({ x: rect.x + (p.x + 0.5) * sx, y: rect.y + (p.y + 0.5) * sy }));
}

export function clampRegion(r: Region): Region {
  const x = clamp(r.x, 0, 1);
  const y = clamp(r.y, 0, 1);
  return { x, y, width: clamp(r.width, 0.01, 1 - x), height: clamp(r.height, 0.01, 1 - y) };
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}
