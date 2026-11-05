// Camera preview DOM + canvas overlay (region of interest, detection highlights, buttons).
import type { OverlayOptions, Point, Region } from './types.js';

const TORCH_ICON =
  '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 2h12v5l-3 4v11H9V11L6 7z"/><path d="M12 14v2"/></svg>';
const SWITCH_ICON =
  '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h3l2-3h6l2 3h3v12H4z"/><path d="M9 13a3 3 0 0 1 5-2.2M15 13a3 3 0 0 1-5 2.2"/><path d="M14 9v2h-2M10 17v-2h2"/></svg>';

const HIGHLIGHT_MS = 400;

export class Preview {
  readonly root: HTMLDivElement;
  readonly video: HTMLVideoElement;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D | null;
  private controls: HTMLDivElement;
  private torchBtn: HTMLButtonElement;
  private switchBtn: HTMLButtonElement;
  private resizeObserver: ResizeObserver | null = null;
  private region: Region = { x: 0, y: 0, width: 1, height: 1 };
  private detections: { polygons: Point[][]; at: number } = { polygons: [], at: 0 };
  private dirty = true;
  private opts: Required<OverlayOptions> | null;
  private restoreHostPosition: string | null = null;

  constructor(
    private host: HTMLElement,
    options: OverlayOptions | false | undefined,
  ) {
    this.opts =
      options === false
        ? null
        : {
            showRegion: true,
            showDetections: true,
            showTorchButton: true,
            showCameraSwitch: true,
            color: '#00d4aa',
            ...options,
          };

    if (getComputedStyle(host).position === 'static') {
      this.restoreHostPosition = host.style.position;
      host.style.position = 'relative';
    }

    this.root = el('div', 'position:absolute;inset:0;overflow:hidden;background:#000;');
    this.root.className = 'scanner-preview';

    this.video = el('video', 'position:absolute;inset:0;width:100%;height:100%;object-fit:cover;');
    this.video.muted = true;
    this.video.playsInline = true;
    this.video.autoplay = true;
    this.video.setAttribute('playsinline', '');
    this.video.setAttribute('muted', '');

    this.canvas = el('canvas', 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none;');
    this.ctx = this.canvas.getContext('2d');

    this.controls = el('div', 'position:absolute;right:12px;bottom:12px;display:flex;gap:8px;');
    this.torchBtn = button(TORCH_ICON, 'Toggle flashlight');
    this.switchBtn = button(SWITCH_ICON, 'Switch camera');
    this.controls.append(this.switchBtn, this.torchBtn);

    this.root.append(this.video, this.canvas, this.controls);
    host.append(this.root);

    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObserver = new ResizeObserver(() => (this.dirty = true));
      this.resizeObserver.observe(this.root);
    }
  }

  get width(): number {
    return this.root.clientWidth;
  }

  get height(): number {
    return this.root.clientHeight;
  }

  setRegion(region: Region): void {
    this.region = region;
    this.dirty = true;
  }

  /** Polygons in element (CSS pixel) coordinates. */
  setDetections(polygons: Point[][]): void {
    if (!this.opts?.showDetections) return;
    if (polygons.length === 0 && this.detections.polygons.length === 0) return;
    if (polygons.length > 0) this.detections = { polygons, at: performance.now() };
    this.dirty = true;
  }

  setTorchButton(visible: boolean, on: boolean, onClick: () => void): void {
    const show = visible && !!this.opts?.showTorchButton;
    this.torchBtn.style.display = show ? 'flex' : 'none';
    this.torchBtn.style.background = on ? (this.opts?.color ?? '#00d4aa') : 'rgba(0,0,0,0.55)';
    this.torchBtn.onclick = onClick;
  }

  setSwitchButton(visible: boolean, onClick: () => void): void {
    this.switchBtn.style.display = visible && this.opts?.showCameraSwitch ? 'flex' : 'none';
    this.switchBtn.onclick = onClick;
  }

  /** Redraw if something changed. Cheap to call on every frame. */
  render(): void {
    const ctx = this.ctx;
    if (!ctx || !this.opts) return;
    const now = performance.now();
    const age = now - this.detections.at;
    const highlighting = this.detections.polygons.length > 0 && age < HIGHLIGHT_MS;
    if (!this.dirty && !highlighting) return;
    this.dirty = highlighting; // keep animating the fade-out

    const dpr = window.devicePixelRatio || 1;
    const w = this.width;
    const h = this.height;
    if (this.canvas.width !== Math.round(w * dpr) || this.canvas.height !== Math.round(h * dpr)) {
      this.canvas.width = Math.round(w * dpr);
      this.canvas.height = Math.round(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const color = this.opts.color;
    if (this.opts.showRegion) {
      const r = this.region;
      const rx = r.x * w;
      const ry = r.y * h;
      const rw = r.width * w;
      const rh = r.height * h;
      ctx.fillStyle = 'rgba(0,0,0,0.45)';
      ctx.beginPath();
      ctx.rect(0, 0, w, h);
      ctx.rect(rx, ry, rw, rh);
      ctx.fill('evenodd');

      const c = Math.min(24, rw / 4, rh / 4);
      ctx.strokeStyle = color;
      ctx.lineWidth = 3;
      ctx.lineCap = 'round';
      ctx.beginPath();
      const corners: [number, number, number, number][] = [
        [rx, ry, 1, 1],
        [rx + rw, ry, -1, 1],
        [rx, ry + rh, 1, -1],
        [rx + rw, ry + rh, -1, -1],
      ];
      for (const [x, y, dx, dy] of corners) {
        ctx.moveTo(x + dx * c, y);
        ctx.lineTo(x, y);
        ctx.lineTo(x, y + dy * c);
      }
      ctx.stroke();
    }

    if (highlighting) {
      ctx.globalAlpha = 1 - age / HIGHLIGHT_MS;
      ctx.strokeStyle = color;
      ctx.fillStyle = withAlpha(color, 0.25);
      ctx.lineWidth = 3;
      for (const poly of this.detections.polygons) {
        drawPolygon(ctx, poly);
      }
      ctx.globalAlpha = 1;
    }
  }

  destroy(): void {
    this.resizeObserver?.disconnect();
    this.video.srcObject = null;
    this.root.remove();
    if (this.restoreHostPosition !== null) this.host.style.position = this.restoreHostPosition;
  }
}

function drawPolygon(ctx: CanvasRenderingContext2D, pts: Point[]): void {
  const [first, second] = pts;
  if (!first) return;
  ctx.beginPath();
  if (pts.length === 2 && second) {
    // Fallback for a 1D code whose box could not be measured: the two ends
    // of the scan line, drawn as a thick bar.
    ctx.save();
    ctx.lineWidth = 10;
    ctx.globalAlpha *= 0.6;
    ctx.moveTo(first.x, first.y);
    ctx.lineTo(second.x, second.y);
    ctx.stroke();
    ctx.restore();
    return;
  }
  ctx.moveTo(first.x, first.y);
  for (const p of pts.slice(1)) ctx.lineTo(p.x, p.y);
  ctx.closePath();
  ctx.fill();
  ctx.stroke();
}

function withAlpha(color: string, alpha: number): string {
  const hex = /^#([0-9a-f]{6})$/i.exec(color)?.[1];
  if (!hex) return color;
  const n = parseInt(hex, 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${alpha})`;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, css: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.style.cssText = css;
  return e;
}

function button(icon: string, label: string): HTMLButtonElement {
  const b = el(
    'button',
    'display:none;align-items:center;justify-content:center;width:44px;height:44px;border-radius:50%;' +
      'border:none;color:#fff;background:rgba(0,0,0,0.55);cursor:pointer;padding:0;',
  );
  b.type = 'button';
  b.innerHTML = icon;
  b.title = label;
  b.setAttribute('aria-label', label);
  return b;
}
