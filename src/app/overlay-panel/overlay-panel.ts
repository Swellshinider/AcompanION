import { Component, DestroyRef, inject, input, linkedSignal, signal } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { OVERLAY_CLOSED_EVENT, updateOverlay } from '../overlay-control';
import { MIN_OPACITY, OverlaySettings, OverlayView, loadOverlaySettings, saveOverlaySettings } from '../overlay-settings';

/** One overlay's controls, a card of the Settings screen. The overlay window itself may not take input. */
@Component({
  selector: 'app-overlay-panel',
  templateUrl: './overlay-panel.html',
  styleUrl: './overlay-panel.scss',
})
export class OverlayPanel {
  readonly view = input.required<OverlayView>();
  readonly s = linkedSignal(() => loadOverlaySettings(this.view()));
  readonly failed = signal(false);
  readonly minOpacity = MIN_OPACITY;

  constructor() {
    // this overlay was closed from its own X (or its window went away)
    let unlisten: (() => void) | undefined;
    let destroyed = false;
    listen<OverlayView>(OVERLAY_CLOSED_EVENT, e => e.payload === this.view() && void this.set({ open: false })).then(
      u => (destroyed ? u() : (unlisten = u)),
      () => undefined, // outside the Tauri window (plain `ng serve`) there is no backend
    );
    inject(DestroyRef).onDestroy(() => {
      destroyed = true;
      unlisten?.();
    });
  }

  async set(patch: Partial<OverlaySettings>): Promise<void> {
    this.failed.set(false);
    this.s.update(s => ({ ...s, ...patch }));
    try {
      this.s.set(await updateOverlay(this.view(), patch));
    } catch {
      // the window could not be opened or updated: do not remember it as open
      saveOverlaySettings(this.view(), { ...loadOverlaySettings(this.view()), open: false });
      this.s.set(loadOverlaySettings(this.view()));
      this.failed.set(true);
    }
  }

  toggle(key: 'open' | 'onTop' | 'clickThrough' | 'autoHide', e: Event): void {
    void this.set({ [key]: (e.target as HTMLInputElement).checked });
  }

  opacity(e: Event): void {
    void this.set({ opacity: Number((e.target as HTMLInputElement).value) });
  }
}
