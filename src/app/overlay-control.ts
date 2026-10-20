import { invoke } from '@tauri-apps/api/core';
import { emitTo } from '@tauri-apps/api/event';
import { OverlaySettings, loadOverlaySettings, saveOverlaySettings } from './overlay-settings';

/** Sent to the overlay window with the settings it should show. */
export const OVERLAY_SETTINGS_EVENT = 'overlay://settings';
/** Sent by Rust to the main window when the overlay window went away. */
export const OVERLAY_CLOSED_EVENT = 'overlay://closed';

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
