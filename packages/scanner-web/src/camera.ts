// Camera access helpers (getUserMedia, device listing, torch, zoom, focus).
import type { CameraOptions, CameraResolution } from './types.js';

const RESOLUTIONS: Record<CameraResolution, [number, number]> = {
  sd: [640, 480],
  hd: [1280, 720],
  fhd: [1920, 1080],
};

const STORAGE_KEY = 'scanner.cameraId';

export function rememberedCamera(): string | undefined {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

export function rememberCamera(id: string | undefined): void {
  try {
    if (id) localStorage.setItem(STORAGE_KEY, id);
  } catch {
    /* storage unavailable: ignore */
  }
}

export async function openCamera(options: CameraOptions = {}): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error('Camera access is not available (a secure https:// context is required)');
  }
  const [width, height] = RESOLUTIONS[options.resolution ?? 'hd'];
  const deviceId = options.deviceId ?? (options.rememberDevice !== false ? rememberedCamera() : undefined);

  const video: MediaTrackConstraints = {
    width: { ideal: width },
    height: { ideal: height },
    frameRate: { ideal: 30 },
  };
  if (deviceId) video.deviceId = { exact: deviceId };
  else video.facingMode = { ideal: options.facingMode ?? 'environment' };

  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
  } catch (err) {
    // A remembered camera may have been unplugged: retry with the facing mode.
    if (!deviceId || options.deviceId) throw err;
    delete video.deviceId;
    video.facingMode = { ideal: options.facingMode ?? 'environment' };
    stream = await navigator.mediaDevices.getUserMedia({ video, audio: false });
  }

  const track = stream.getVideoTracks()[0];
  if (track) await applyAdvanced(track, { focusMode: 'continuous' });
  return stream;
}

/** Video input devices. Labels are only populated once camera permission was granted. */
export async function listCameras(): Promise<MediaDeviceInfo[]> {
  if (!navigator.mediaDevices?.enumerateDevices) return [];
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((d) => d.kind === 'videoinput');
}

interface ExtendedCapabilities extends MediaTrackCapabilities {
  torch?: boolean;
  zoom?: { min: number; max: number; step?: number };
  focusMode?: string[];
}

export function capabilities(track: MediaStreamTrack | undefined): ExtendedCapabilities {
  try {
    return (track?.getCapabilities?.() ?? {}) as ExtendedCapabilities;
  } catch {
    return {};
  }
}

export async function setTorch(track: MediaStreamTrack, on: boolean): Promise<void> {
  if (!capabilities(track).torch) throw new Error('This camera has no controllable flashlight');
  await track.applyConstraints({ advanced: [{ torch: on } as MediaTrackConstraintSet] });
}

export async function setZoom(track: MediaStreamTrack, zoom: number): Promise<void> {
  const range = capabilities(track).zoom;
  if (!range) throw new Error('This camera does not support zoom');
  const value = Math.min(range.max, Math.max(range.min, zoom));
  await track.applyConstraints({ advanced: [{ zoom: value } as MediaTrackConstraintSet] });
}

async function applyAdvanced(track: MediaStreamTrack, set: Record<string, unknown>): Promise<void> {
  const caps = capabilities(track) as Record<string, unknown>;
  const supported = Object.keys(set).filter((k) => {
    const c = caps[k];
    return Array.isArray(c) ? c.includes(set[k]) : c !== undefined;
  });
  if (supported.length === 0) return;
  try {
    await track.applyConstraints({ advanced: [Object.fromEntries(supported.map((k) => [k, set[k]]))] });
  } catch {
    /* best effort */
  }
}
