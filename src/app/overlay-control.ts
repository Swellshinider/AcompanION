import { invoke } from '@tauri-apps/api/core';
import { emitTo, listen } from '@tauri-apps/api/event';
import { OverlaySettings, loadOverlaySettings, saveOverlaySettings } from './overlay-settings';

/** Sent to the overlay window with the settings it should show. */
export const OVERLAY_SETTINGS_EVENT = 'overlay://settings';
/** Sent by Rust to the main window when the overlay window went away. */
export const OVERLAY_CLOSED_EVENT = 'overlay://closed';

/**
 * At app start: reopen the overlay where it was, and remember it as off when its own X closes it.
 * Runs with the dashboard, since the Settings panel that shows these controls is not always on screen.
 * Returns the function that stops listening.
 */
export function restoreOverlay(): () => void {
  const off = () => saveOverlaySettings({ ...loadOverlaySettings(), open: false });
  let unlisten: (() => void) | undefined;
  let stopped = false;
  listen(OVERLAY_CLOSED_EVENT, off).then(
    u => (stopped ? u() : (unlisten = u)),
    () => undefined, // outside the Tauri window (plain `ng serve`) there is no backend
  );
  if (loadOverlaySettings().open) updateOverlay({}).catch(off); // do not remember an overlay that failed to open
  return () => {
    stopped = true;
    unlisten?.();
  };
}

/**
 * Merge `patch` into the saved settings (so a position the overlay saved meanwhile is kept),
 * then make Rust (window, always on top, click-through) and the overlay window (view, opacity) follow.
 */
export async function updateOverlay(patch: Partial<OverlaySettings>): Promise<OverlaySettings> {
  const s = { ...loadOverlaySettings(), ...patch };
  saveOverlaySettings(s);
  await invoke('overlay', { open: s.open, onTop: s.onTop, clickThrough: s.clickThrough, bounds: s.bounds });
  if (s.open) await emitTo('overlay', OVERLAY_SETTINGS_EVENT, s);
  return s;
}
