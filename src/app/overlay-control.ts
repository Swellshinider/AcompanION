import { invoke } from '@tauri-apps/api/core';
import { emitTo, listen } from '@tauri-apps/api/event';
import { OverlaySettings, OverlayView, VIEWS, loadOverlaySettings, saveOverlaySettings } from './overlay-settings';

/** Sent to the overlay window with the settings it should show. */
export const OVERLAY_SETTINGS_EVENT = 'overlay://settings';
/** Sent by Rust to the main window when an overlay window went away; the payload is its view. */
export const OVERLAY_CLOSED_EVENT = 'overlay://closed';

/** The label of the window that shows `view`. */
export const overlayLabel = (view: OverlayView) => `overlay-${view}`;

/**
 * At app start: reopen the overlays where they were, and remember one as off when its own X closes it.
 * Runs with the dashboard, since the Settings panel that shows these controls is not always on screen.
 * Returns the function that stops listening.
 */
export function restoreOverlay(): () => void {
  const off = (view: OverlayView) => saveOverlaySettings(view, { ...loadOverlaySettings(view), open: false });
  let unlisten: (() => void) | undefined;
  let stopped = false;
  listen<OverlayView>(OVERLAY_CLOSED_EVENT, e => off(e.payload)).then(
    u => (stopped ? u() : (unlisten = u)),
    () => undefined, // outside the Tauri window (plain `ng serve`) there is no backend
  );
  for (const view of VIEWS) {
    if (loadOverlaySettings(view).open) updateOverlay(view, {}).catch(() => off(view)); // do not remember an overlay that failed to open
  }
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/**
 * Merge `patch` into `view`'s saved settings (so a position the overlay saved meanwhile is kept),
 * then make Rust (window, always on top, click-through) and the overlay window (opacity) follow.
 */
export async function updateOverlay(view: OverlayView, patch: Partial<OverlaySettings>): Promise<OverlaySettings> {
  const s = { ...loadOverlaySettings(view), ...patch };
  saveOverlaySettings(view, s);
  await invoke('overlay', { view, open: s.open, onTop: s.onTop, clickThrough: s.clickThrough, bounds: s.bounds });
  if (s.open) await emitTo(overlayLabel(view), OVERLAY_SETTINGS_EVENT, s);
  return s;
}
