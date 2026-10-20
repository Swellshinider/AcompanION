import { DEFAULTS, MIN_OPACITY, loadOverlaySettings, parse, saveOverlaySettings } from './overlay-settings';

describe('overlay settings', () => {
  beforeEach(() => localStorage.clear());

  it('uses the defaults when nothing is stored or the value is corrupt', () => {
    expect(loadOverlaySettings()).toEqual(DEFAULTS);
    localStorage.setItem('acompanion.overlay', '{not json');
    expect(loadOverlaySettings()).toEqual(DEFAULTS);
    expect(parse(null)).toEqual(DEFAULTS);
    expect(parse('x')).toEqual(DEFAULTS);
  });

  it('round-trips what was saved', () => {
    const s = { open: true, view: 'dailies' as const, opacity: 60, onTop: false, clickThrough: true, autoHide: false, bounds: [10, 20, 420, 300] as [number, number, number, number] };
    saveOverlaySettings(s);
    expect(loadOverlaySettings()).toEqual(s);
  });

  it('keeps opacity in range and drops bad fields one by one', () => {
    expect(parse({ opacity: 5 }).opacity).toBe(MIN_OPACITY);
    expect(parse({ opacity: 500 }).opacity).toBe(100);
    expect(parse({ opacity: 'x' }).opacity).toBe(DEFAULTS.opacity);
    expect(parse({ view: 'map', onTop: 'yes', bounds: [1, 2, 0, 5] })).toEqual(DEFAULTS);
    expect(parse({ view: 'meter' }).view).toBe('meter');
    expect(parse({ clickThrough: true }).clickThrough).toBe(true);
    expect(parse({ autoHide: 'no' }).autoHide).toBe(true); // on unless it says false
    expect(parse({ autoHide: false }).autoHide).toBe(false);
  });
});
