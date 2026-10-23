/** The overlay's preferences, shared by the main window (controls) and the overlay window (view, opacity, position). */
export interface OverlaySettings {
  open: boolean;
  view: 'radar' | 'dailies' | 'meter';
  /** Percent, `MIN_OPACITY` to 100. */
  opacity: number;
  onTop: boolean;
  clickThrough: boolean;
  /** Hide the window while the game is closed. */
  autoHide: boolean;
  /** Physical pixels `[x, y, width, height]` of the window when it was last moved or resized. */
  bounds: [number, number, number, number] | null;
}

export const MIN_OPACITY = 20;
const KEY = 'acompanion.overlay';

export const DEFAULTS: OverlaySettings = { open: false, view: 'radar', opacity: 85, onTop: true, clickThrough: false, autoHide: true, bounds: null };

/** Whatever is stored, with every field checked: a missing, old or corrupt value falls back to its default. */
export function parse(raw: unknown): OverlaySettings {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const opacity = typeof o['opacity'] === 'number' && Number.isFinite(o['opacity']) ? o['opacity'] : DEFAULTS.opacity;
  const b = o['bounds'];
  const bounds = Array.isArray(b) && b.length === 4 && b.every(n => Number.isInteger(n)) && b[2] > 0 && b[3] > 0
    ? (b as OverlaySettings['bounds']) : null;
  return {
    open: o['open'] === true,
    view: o['view'] === 'dailies' || o['view'] === 'meter' ? o['view'] : 'radar',
    opacity: Math.min(100, Math.max(MIN_OPACITY, Math.round(opacity))),
    onTop: typeof o['onTop'] === 'boolean' ? o['onTop'] : DEFAULTS.onTop,
    clickThrough: o['clickThrough'] === true,
    autoHide: typeof o['autoHide'] === 'boolean' ? o['autoHide'] : DEFAULTS.autoHide,
    bounds,
  };
}

export function loadOverlaySettings(): OverlaySettings {
  try {
    return parse(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return { ...DEFAULTS };
  }
}

const REGION_KEY = 'acompanion.overlay.region';

/** The last pet region the player was seen in, so the radar can open before the game says where they are. */
export function loadLastRegion(): string | null {
  try {
    return localStorage.getItem(REGION_KEY);
  } catch {
    return null;
  }
}

export function saveLastRegion(key: string): void {
  try {
    localStorage.setItem(REGION_KEY, key);
  } catch {
    // see saveOverlaySettings
  }
}

export function saveOverlaySettings(s: OverlaySettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // storage blocked: the overlay still works, it just forgets its preferences
  }
}
