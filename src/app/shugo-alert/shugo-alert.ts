import { Component, DestroyRef, inject, signal } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { SHUGO_SHOW_EVENT, closeShugoAlert, loadShugoSettings } from '../shugo';

/** Two short rising notes. Silence if the browser blocks audio: the alert is still seen. */
export function chime(): void {
  try {
    const ctx = new AudioContext();
    void ctx.resume();
    [660, 880].forEach((hz, i) => {
      const at = ctx.currentTime + i * 0.18;
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = hz;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.15, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.4);
      osc.connect(gain).connect(ctx.destination);
      osc.start(at);
      osc.stop(at + 0.45);
    });
    setTimeout(() => void ctx.close(), 1000);
  } catch {
    // no audio
  }
}

/** The Shugo Festival pop-up window: it closes itself after the saved time, or on a click when that is on. */
@Component({
  selector: 'app-shugo-alert',
  templateUrl: './shugo-alert.html',
  styleUrl: './shugo-alert.scss',
})
export class ShugoAlert {
  readonly s = signal(loadShugoSettings());
  /** Changes on each start so the bar's animation begins again. */
  readonly round = signal(0);
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    const html = document.documentElement;
    html.classList.add('overlay');
    this.start();
    let unlisten: (() => void) | undefined;
    let stopped = false;
    // a second start while it is up (the test button) begins again; outside the Tauri window there is no backend
    listen(SHUGO_SHOW_EVENT, () => this.start()).then(
      u => (stopped ? u() : (unlisten = u)),
      () => undefined,
    );
    inject(DestroyRef).onDestroy(() => {
      stopped = true;
      unlisten?.();
      clearTimeout(this.timer);
      html.classList.remove('overlay');
    });
  }

  private start(): void {
    this.s.set(loadShugoSettings());
    this.round.update(n => n + 1);
    if (this.s().sound) chime();
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.close(), this.s().seconds * 1000);
  }

  click(): void {
    if (this.s().clickToClose) this.close();
  }

  private close(): void {
    closeShugoAlert().catch(() => undefined);
  }
}
