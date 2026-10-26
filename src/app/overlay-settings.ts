/** The overlays: each is its own window with its own `OverlaySettings`. */
export type OverlayView = 'radar' | 'dailies' | 'meter';
export const VIEWS: OverlayView[] = ['radar', 'dailies', 'meter'];

/** One overlay's preferences, shared by the main window (controls) and the overlay window (opacity, position). */
export interface OverlaySettings {
  open: boolean;
  /** Percent, `MIN_OPACITY` to 100. */
  opacity: number;
  onTop: boolean;
  clickThrough: boolean;
  /** Hide the window while the game is closed. */
  autoHide: boolean;
  /** Meter only: hide after 10 seconds without a counted hit. */
  hideOutOfCombat: boolean;
  /** Physical pixels `[x, y, width, height]` of the window when it was last moved or resized. */
  bounds: [number, number, number, number] | null;
}

export const MIN_OPACITY = 20;
const KEY = 'acompanion.overlay';
const keyOf = (view: OverlayView) => `${KEY}.${view}`;

export const DEFAULTS: OverlaySettings = { open: false, opacity: 85, onTop: true, clickThrough: false, autoHide: true, hideOutOfCombat: false, bounds: null };

/** Whatever is stored, with every field checked: a missing, old or corrupt value falls back to its default. */
export function parse(raw: unknown): OverlaySettings {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  const opacity = typeof o['opacity'] === 'number' && Number.isFinite(o['opacity']) ? o['opacity'] : DEFAULTS.opacity;
  const b = o['bounds'];
  const bounds = Array.isArray(b) && b.length === 4 && b.every(n => Number.isInteger(n)) && b[2] > 0 && b[3] > 0
    ? (b as OverlaySettings['bounds']) : null;
  return {
    open: o['open'] === true,
    opacity: Math.min(100, Math.max(MIN_OPACITY, Math.round(opacity))),
    onTop: typeof o['onTop'] === 'boolean' ? o['onTop'] : DEFAULTS.onTop,
    clickThrough: o['clickThrough'] === true,
    autoHide: typeof o['autoHide'] === 'boolean' ? o['autoHide'] : DEFAULTS.autoHide,
    hideOutOfCombat: o['hideOutOfCombat'] === true,
    bounds,
  };
}

/** The settings of one overlay. Before there was one per view, a single value held them all: it seeds each view that has none yet. */
export function loadOverlaySettings(view: OverlayView): OverlaySettings {
  try {
    const own = localStorage.getItem(keyOf(view));
    if (own !== null) return parse(JSON.parse(own));
    const legacy = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    const was = legacy?.view === 'dailies' || legacy?.view === 'meter' ? legacy.view : 'radar';
    return parse(legacy && typeof legacy === 'object' ? { ...legacy, open: legacy.open === true && was === view } : null);
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

export function saveOverlaySettings(view: OverlayView, s: OverlaySettings): void {
  try {
    localStorage.setItem(keyOf(view), JSON.stringify(s));
  } catch {
    // storage blocked: the overlay still works, it just forgets its preferences
  }
}
