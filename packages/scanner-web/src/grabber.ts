// Frame pipeline: turns a video frame into the small grayscale image the decoder needs.
//
// The WebGL2 path does all the per-pixel work on the GPU in a single pass:
//   upload video texture → crop to the region of interest → RGB→luma
//   → box-filtered downscale → pack 4 luma pixels per RGBA texel
// so the CPU readback is exactly `width × height` bytes (4× less than
// getImageData) and, thanks to a pixel-pack buffer + fence, it does not
// stall the main thread waiting for the GPU.

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FrameGrabber {
  readonly kind: 'webgl' | 'canvas';
  /**
   * Render `src` (cropped to `rect`, in source pixels) into `out`, a luma
   * buffer of `width × height` bytes. `width` must be a multiple of 4.
   */
  grab(source: TexImageSource, rect: Rect, width: number, height: number, out: Uint8Array): Promise<void>;
  destroy(): void;
}

type AnyCanvas = HTMLCanvasElement | OffscreenCanvas;

function makeCanvas(): AnyCanvas {
  return typeof OffscreenCanvas !== 'undefined'
    ? new OffscreenCanvas(4, 4)
    : document.createElement('canvas');
}

const VERT = `#version 300 es
in vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }`;

const FRAG = `#version 300 es
precision highp float;
uniform sampler2D u_src;
uniform vec4 u_rect;     // crop rectangle in texture coordinates (origin top-left)
uniform vec2 u_out;      // size of the luma image
uniform vec2 u_tap;      // sub-sample offset for the 2x2 box filter
out vec4 o_color;
const vec3 LUMA = vec3(0.299, 0.587, 0.114);

float luma(vec2 px) {
  vec2 uv = u_rect.xy + (px + 0.5) / u_out * u_rect.zw;
  vec3 c = texture(u_src, uv + vec2(-u_tap.x, -u_tap.y)).rgb
         + texture(u_src, uv + vec2( u_tap.x, -u_tap.y)).rgb
         + texture(u_src, uv + vec2(-u_tap.x,  u_tap.y)).rgb
         + texture(u_src, uv + vec2( u_tap.x,  u_tap.y)).rgb;
  return dot(c * 0.25, LUMA);
}

void main() {
  // Framebuffer row 0 is read back first, so row r samples image row r from the top.
  float y = floor(gl_FragCoord.y);
  float x = floor(gl_FragCoord.x) * 4.0;
  o_color = vec4(luma(vec2(x, y)), luma(vec2(x + 1.0, y)), luma(vec2(x + 2.0, y)), luma(vec2(x + 3.0, y)));
}`;

export class WebGLGrabber implements FrameGrabber {
  readonly kind = 'webgl';
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private srcTex: WebGLTexture;
  private dstTex: WebGLTexture;
  private fbo: WebGLFramebuffer;
  private pbo: WebGLBuffer;
  private uniforms: Record<'rect' | 'out' | 'tap', WebGLUniformLocation | null>;
  private size = { w: 0, h: 0 };
  private lost = false;

  /** Returns `null` when WebGL2 is not available. */
  static create(): WebGLGrabber | null {
    try {
      const canvas = makeCanvas();
      const gl = canvas.getContext('webgl2', {
        alpha: true,
        antialias: false,
        depth: false,
        stencil: false,
        premultipliedAlpha: false,
        preserveDrawingBuffer: false,
        powerPreference: 'low-power',
      }) as WebGL2RenderingContext | null;
      return gl ? new WebGLGrabber(canvas, gl) : null;
    } catch {
      return null;
    }
  }

  private constructor(canvas: AnyCanvas, gl: WebGL2RenderingContext) {
    this.gl = gl;
    (canvas as HTMLCanvasElement).addEventListener?.('webglcontextlost', (e) => {
      e.preventDefault();
      this.lost = true;
    });

    this.program = link(gl, VERT, FRAG);
    gl.useProgram(this.program);

    // Full-screen triangle.
    const vbo = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(this.program, 'a_pos');
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

    this.uniforms = {
      rect: gl.getUniformLocation(this.program, 'u_rect'),
      out: gl.getUniformLocation(this.program, 'u_out'),
      tap: gl.getUniformLocation(this.program, 'u_tap'),
    };
    gl.uniform1i(gl.getUniformLocation(this.program, 'u_src'), 0);

    this.srcTex = texture(gl);
    this.dstTex = texture(gl);
    this.fbo = must(gl.createFramebuffer(), 'framebuffer');
    this.pbo = must(gl.createBuffer(), 'pixel buffer');
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.pixelStorei(gl.PACK_ALIGNMENT, 1);
  }

  async grab(
    source: TexImageSource,
    rect: Rect,
    width: number,
    height: number,
    out: Uint8Array,
  ): Promise<void> {
    const gl = this.gl;
    if (this.lost || gl.isContextLost()) throw new Error('WebGL context lost');
    const [sw, sh] = sourceSize(source);
    const texW = width / 4;

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.srcTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);

    if (this.size.w !== texW || this.size.h !== height) {
      gl.bindTexture(gl.TEXTURE_2D, this.dstTex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, texW, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.dstTex, 0);
      gl.bindBuffer(gl.PIXEL_PACK_BUFFER, this.pbo);
      gl.bufferData(gl.PIXEL_PACK_BUFFER, width * height, gl.STREAM_READ);
      this.size = { w: texW, h: height };
      gl.bindTexture(gl.TEXTURE_2D, this.srcTex);
    }

    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
    gl.viewport(0, 0, texW, height);
    gl.uniform4f(this.uniforms.rect, rect.x / sw, rect.y / sh, rect.width / sw, rect.height / sh);
    gl.uniform2f(this.uniforms.out, width, height);
    // Box filter footprint: a quarter of an output pixel, only when downscaling.
    const fx = rect.width / width;
    const fy = rect.height / height;
    gl.uniform2f(this.uniforms.tap, fx > 1 ? (0.25 * fx) / sw : 0, fy > 1 ? (0.25 * fy) / sh : 0);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    // Asynchronous readback into the pixel-pack buffer.
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, this.pbo);
    gl.readPixels(0, 0, texW, height, gl.RGBA, gl.UNSIGNED_BYTE, 0);
    const sync = gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0);
    gl.flush();
    if (sync) {
      await waitSync(gl, sync);
      gl.deleteSync(sync);
    }
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, this.pbo);
    gl.getBufferSubData(gl.PIXEL_PACK_BUFFER, 0, out, 0, width * height);
    gl.bindBuffer(gl.PIXEL_PACK_BUFFER, null);
  }

  destroy() {
    const gl = this.gl;
    gl.deleteTexture(this.srcTex);
    gl.deleteTexture(this.dstTex);
    gl.deleteFramebuffer(this.fbo);
    gl.deleteBuffer(this.pbo);
    gl.deleteProgram(this.program);
    gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}

/** Fallback pipeline using Canvas 2D (`drawImage` does the crop + scale). */
export class CanvasGrabber implements FrameGrabber {
  readonly kind = 'canvas';
  private canvas = makeCanvas();
  private ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

  constructor() {
    const ctx = this.canvas.getContext('2d', { willReadFrequently: true, alpha: false });
    this.ctx = must(ctx as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null, '2D context');
  }

  async grab(
    source: TexImageSource,
    rect: Rect,
    width: number,
    height: number,
    out: Uint8Array,
  ): Promise<void> {
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    const ctx = this.ctx;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'medium';
    ctx.drawImage(source as CanvasImageSource, rect.x, rect.y, rect.width, rect.height, 0, 0, width, height);
    const rgba = ctx.getImageData(0, 0, width, height).data;
    rgbaToLuma(rgba, out);
  }

  destroy() {
    this.canvas.width = this.canvas.height = 0;
  }
}

export function createGrabber(backend: 'auto' | 'webgl' | 'canvas' = 'auto'): FrameGrabber {
  if (backend !== 'canvas') {
    const gl = WebGLGrabber.create();
    if (gl) return gl;
    if (backend === 'webgl') throw new Error('WebGL2 is not available');
  }
  return new CanvasGrabber();
}

/** BT.601 luma with integer arithmetic (same weights as the GPU path and the engine). */
export function rgbaToLuma(rgba: Uint8ClampedArray | Uint8Array, out: Uint8Array): void {
  const n = Math.min(out.length, rgba.length >> 2);
  for (let i = 0, j = 0; i < n; i++, j += 4) {
    // In bounds by construction (`n`): `?? 0` only satisfies the type checker.
    out[i] = (19595 * (rgba[j] ?? 0) + 38470 * (rgba[j + 1] ?? 0) + 7471 * (rgba[j + 2] ?? 0)) >> 16;
  }
}

export function sourceSize(source: TexImageSource): [number, number] {
  if (typeof HTMLVideoElement !== 'undefined' && source instanceof HTMLVideoElement) {
    return [source.videoWidth, source.videoHeight];
  }
  if (typeof HTMLImageElement !== 'undefined' && source instanceof HTMLImageElement) {
    return [source.naturalWidth, source.naturalHeight];
  }
  if (typeof VideoFrame !== 'undefined' && source instanceof VideoFrame) {
    return [source.displayWidth, source.displayHeight];
  }
  const s = source as { width: number; height: number };
  return [s.width, s.height];
}

// ── WebGL helpers ──────────────────────────────────────────────────────────

function must<T>(v: T | null, what: string): T {
  if (!v) throw new Error(`WebGL: failed to create ${what}`);
  return v;
}

function texture(gl: WebGL2RenderingContext): WebGLTexture {
  const t = must(gl.createTexture(), 'texture');
  gl.bindTexture(gl.TEXTURE_2D, t);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return t;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const compile = (type: number, src: string) => {
    const s = must(gl.createShader(type), 'shader');
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`Shader: ${gl.getShaderInfoLog(s)}`);
    return s;
  };
  const p = must(gl.createProgram(), 'program');
  gl.attachShader(p, compile(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, compile(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`Program: ${gl.getProgramInfoLog(p)}`);
  return p;
}

function waitSync(gl: WebGL2RenderingContext, sync: WebGLSync): Promise<void> {
  return new Promise((resolve) => {
    const started = performance.now();
    const poll = () => {
      const status = gl.clientWaitSync(sync, 0, 0);
      // Give up waiting after 50 ms: getBufferSubData will then block, but never hang.
      if (status !== gl.TIMEOUT_EXPIRED || performance.now() - started > 50) resolve();
      else setTimeout(poll, 1);
    };
    poll();
  });
}
