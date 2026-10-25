import { invoke } from '@tauri-apps/api/core';
import { emitTo } from '@tauri-apps/api/event';

/** The Shugo Festival alert: a pop-up window in a screen corner when a round starts. Its preferences live here, apart from the overlays'. */
export type Corner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
export const CORNERS: Corner[] = ['top-left', 'top-right', 'bottom-left', 'bottom-right'];

export interface ShugoSettings {
  enabled: boolean;
  corner: Corner;
  /** How long it stays up, `MIN_SECONDS` to `MAX_SECONDS`. */
  seconds: number;
  /** A click closes it early; off, clicks pass through to the game. */
  clickToClose: boolean;
  sound: boolean;
}

export const MIN_SECONDS = 3;
export const MAX_SECONDS = 300;
const KEY = 'acompanion.shugo';

/** The bottom-right corner keeps clear of the game's minimap. */
export const DEFAULTS: ShugoSettings = { enabled: true, corner: 'bottom-right', seconds: 10, clickToClose: true, sound: true };

/** The alert window's label (an `overlay-` window, so quitting closes it) and the event that restarts an alert already on screen. */
export const SHUGO_LABEL = 'overlay-shugo';
export const SHUGO_SHOW_EVENT = 'shugo://show';

/** Whatever is stored, with every field checked: a missing, old or corrupt value falls back to its default. */
export function parse(raw: unknown): ShugoSettings {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const bool = (k: 'enabled' | 'clickToClose' | 'sound') => (typeof o[k] === 'boolean' ? (o[k] as boolean) : DEFAULTS[k]);
  const seconds = typeof o['seconds'] === 'number' && Number.isFinite(o['seconds']) ? o['seconds'] : DEFAULTS.seconds;
  return {
    enabled: bool('enabled'),
    corner: CORNERS.find(c => c === o['corner']) ?? DEFAULTS.corner,
    seconds: Math.min(MAX_SECONDS, Math.max(MIN_SECONDS, Math.round(seconds))),
    clickToClose: bool('clickToClose'),
    sound: bool('sound'),
  };
}

export function loadShugoSettings(): ShugoSettings {
  try {
    return parse(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveShugoSettings(s: ShugoSettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // storage blocked: the settings apply for this run only
  }
}

const MIN_MS = 60_000;
const HOUR_MS = 60 * MIN_MS;
/**
 * Minutes past the hour (UTC) at which a round starts: every hour on the hour in the global version (Season 1,
 * confirmed by a player), which is minute 0 in any whole-hour time zone. The Korean version used :15 and :45 until
 * its Season 3. Curated, not on the wire.
 */
export const SHUGO_MINUTES_UTC = [0];

/** The first start strictly after `nowMs` (epoch ms). */
export function nextFestival(nowMs: number, minutes: number[] = SHUGO_MINUTES_UTC): number {
  const hour = Math.floor(nowMs / HOUR_MS) * HOUR_MS;
  return Math.min(...[hour, hour + HOUR_MS].flatMap(h => minutes.map(m => h + m * MIN_MS)).filter(t => t > nowMs));
}

/** Show the alert now, in `s.corner`. An alert already up starts over (new timer, new chime). */
export async function showShugoAlert(s: ShugoSettings = loadShugoSettings()): Promise<void> {
  const created = await invoke<boolean>('shugo_alert', { corner: s.corner, clickThrough: !s.clickToClose });
  if (!created) await emitTo(SHUGO_LABEL, SHUGO_SHOW_EVENT);
}

export async function closeShugoAlert(): Promise<void> {
  await invoke('shugo_alert', { corner: null, clickThrough: false });
}

/** A start this late (the computer slept through it) is stale: no alert. */
const LATE_MS = 60_000;

/**
 * Show the alert at every start while it is on and the game is running. Runs in the main window, which stays loaded
 * when hidden to the tray. Returns the function that stops it.
 */
export function startShugoAlert(game: () => boolean): () => void {
  let timer: ReturnType<typeof setTimeout>;
  const arm = (after: number) => {
    const at = nextFestival(after);
    timer = setTimeout(() => {
      const s = loadShugoSettings();
      if (s.enabled && game() && Date.now() - at < LATE_MS) void showShugoAlert(s).catch(() => undefined);
      arm(Math.max(Date.now(), at)); // a timer that fires a few ms early must not fire the same start twice
    }, Math.max(0, at - Date.now()));
  };
  arm(Date.now());
  return () => clearTimeout(timer);
}
