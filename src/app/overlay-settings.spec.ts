import { DEFAULTS, MIN_OPACITY, loadOverlaySettings, parse, saveOverlaySettings } from './overlay-settings';

describe('overlay settings', () => {
  beforeEach(() => localStorage.clear());

  it('uses the defaults when nothing is stored or the value is corrupt', () => {
    expect(loadOverlaySettings('radar')).toEqual(DEFAULTS);
    localStorage.setItem('acompanion.overlay.radar', '{not json');
    expect(loadOverlaySettings('radar')).toEqual(DEFAULTS);
    expect(parse(null)).toEqual(DEFAULTS);
    expect(parse('x')).toEqual(DEFAULTS);
  });

  it('round-trips what was saved', () => {
    const s = { open: true, opacity: 60, onTop: false, clickThrough: true, autoHide: false, bounds: [10, 20, 420, 300] as [number, number, number, number] };
    saveOverlaySettings('dailies', s);
    expect(loadOverlaySettings('dailies')).toEqual(s);
    expect(loadOverlaySettings('meter')).toEqual(DEFAULTS); // each overlay has its own
  });

  it('seeds each overlay from the single setting of older versions, opening only the one that was shown', () => {
    localStorage.setItem('acompanion.overlay', JSON.stringify({ open: true, view: 'meter', opacity: 60 }));
    expect(loadOverlaySettings('meter')).toEqual({ ...DEFAULTS, open: true, opacity: 60 });
    expect(loadOverlaySettings('radar')).toEqual({ ...DEFAULTS, open: false, opacity: 60 });
    saveOverlaySettings('radar', { ...DEFAULTS, opacity: 30 });
    expect(loadOverlaySettings('radar').opacity).toBe(30); // its own value wins from then on
  });

  it('keeps opacity in range and drops bad fields one by one', () => {
    expect(parse({ opacity: 5 }).opacity).toBe(MIN_OPACITY);
    expect(parse({ opacity: 500 }).opacity).toBe(100);
    expect(parse({ opacity: 'x' }).opacity).toBe(DEFAULTS.opacity);
    expect(parse({ onTop: 'yes', bounds: [1, 2, 0, 5] })).toEqual(DEFAULTS);
    expect(parse({ clickThrough: true }).clickThrough).toBe(true);
    expect(parse({ autoHide: 'no' }).autoHide).toBe(true); // on unless it says false
    expect(parse({ autoHide: false }).autoHide).toBe(false);
  });
});
