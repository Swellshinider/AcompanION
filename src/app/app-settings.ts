import { invoke } from '@tauri-apps/api/core';
import { defaultWindowIcon } from '@tauri-apps/api/app';
import { Menu } from '@tauri-apps/api/menu';
import { TrayIcon } from '@tauri-apps/api/tray';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** The app's own preferences (the overlay has its own, `OverlaySettings`). Stored here; Rust gets them through `app_settings`. */
export interface AppSettings {
  /** The window's close button hides it to a tray icon instead of quitting. */
  closeToTray: boolean;
  /** Reopen capture when it stops because the network adapter went away. */
  reconnect: boolean;
}

const KEY = 'acompanion.app';
const TRAY = 'main-tray';

export const DEFAULTS: AppSettings = { closeToTray: false, reconnect: true };

/** Whatever is stored, with every field checked: a missing, old or corrupt value falls back to its default. */
export function parse(raw: unknown): AppSettings {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    closeToTray: o['closeToTray'] === true,
    reconnect: typeof o['reconnect'] === 'boolean' ? o['reconnect'] : DEFAULTS.reconnect,
  };
}

export function loadAppSettings(): AppSettings {
  try {
    return parse(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveAppSettings(s: AppSettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // storage blocked: the settings apply for this run only
  }
}

async function showMain(): Promise<void> {
  const w = getCurrentWindow();
  await w.show();
  await w.unminimize();
  await w.setFocus();
}

/** The tray icon that brings the hidden window back; its labels are display text, so they live here and not in Rust. */
async function addTray(): Promise<void> {
  if (await TrayIcon.getById(TRAY)) return;
  const menu = await Menu.new({
    items: [
      { id: 'open', text: $localize`:@@trayOpen:Open`, action: () => void showMain() },
      { id: 'quit', text: $localize`:@@trayQuit:Quit`, action: () => void invoke('quit') },
    ],
  });
  await TrayIcon.new({
    id: TRAY,
    icon: (await defaultWindowIcon()) ?? undefined,
    tooltip: 'AcompanION',
    menu,
    showMenuOnLeftClick: false,
    action: e => {
      if (e.type === 'Click' && e.button === 'Left' && e.buttonState === 'Up') void showMain();
    },
  });
}

async function removeTray(): Promise<void> {
  await (await TrayIcon.getById(TRAY))?.close();
}

/**
 * Make Rust and the tray follow `s`. The icon comes first when closing to the tray and goes last when not,
 * so the window can never be hidden without a way back. If the icon cannot be created this rejects and Rust is told `false`.
 */
export async function applyAppSettings(s: AppSettings): Promise<void> {
  let tray = false;
  try {
    if (s.closeToTray) {
      await addTray();
      tray = true;
    }
  } finally {
    await invoke('app_settings', { closeToTray: tray, reconnect: s.reconnect });
  }
  if (!s.closeToTray) await removeTray();
}
