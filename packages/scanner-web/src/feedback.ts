// Audio / haptic feedback on detection.
import type { FeedbackOptions } from './types.js';

export class Feedback {
  private audio: AudioContext | null = null;

  constructor(private options: FeedbackOptions | false) {}

  /** Must be called from a user gesture on iOS for audio to be allowed. */
  unlock(): void {
    if (!this.options || this.options.beep === false || this.audio) return;
    const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    try {
      this.audio = new Ctx();
      void this.audio.resume();
    } catch {
      this.audio = null;
    }
  }

  play(): void {
    if (!this.options) return;
    if (this.options.vibrate !== false) navigator.vibrate?.(60);
    if (this.options.beep !== false) this.beep();
  }

  private beep(): void {
    const ctx = this.audio;
    if (!ctx || ctx.state !== 'running') return;
    const t = ctx.currentTime;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = 'sine';
    osc.frequency.value = 2400;
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(0.25, t + 0.005);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.09);
    osc.connect(gain).connect(ctx.destination);
    osc.start(t);
    osc.stop(t + 0.1);
  }

  destroy(): void {
    void this.audio?.close();
    this.audio = null;
  }
}
