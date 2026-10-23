import { Component, DestroyRef, inject, signal } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { OVERLAY_CLOSED_EVENT, updateOverlay } from '../overlay-control';
import { MIN_OPACITY, OverlaySettings, loadOverlaySettings, saveOverlaySettings } from '../overlay-settings';

/** The overlay's controls, a section of the Settings screen. The overlay window itself may not take input. */
@Component({
  selector: 'app-overlay-panel',
  templateUrl: './overlay-panel.html',
  styleUrl: './overlay-panel.scss',
})
export class OverlayPanel {
  readonly s = signal(loadOverlaySettings());
  readonly failed = signal(false);
  readonly minOpacity = MIN_OPACITY;

  constructor() {
    // the overlay was closed from its own X (or the window went away)
    let unlisten: (() => void) | undefined;
    let destroyed = false;
    listen(OVERLAY_CLOSED_EVENT, () => void this.set({ open: false })).then(
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
      this.s.set(await updateOverlay(patch));
    } catch {
      // the window could not be opened or updated: do not remember it as open
      saveOverlaySettings({ ...loadOverlaySettings(), open: false });
      this.s.set(loadOverlaySettings());
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
