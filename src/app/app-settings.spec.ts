import { DEFAULTS, loadAppSettings, parse, saveAppSettings } from './app-settings';

describe('app settings', () => {
  beforeEach(() => localStorage.clear());

  it('uses the defaults when nothing is stored or the value is corrupt', () => {
    expect(loadAppSettings()).toEqual({ closeToTray: false, reconnect: true, language: 'auto' });
    localStorage.setItem('acompanion.app', '{not json');
    expect(loadAppSettings()).toEqual(DEFAULTS);
    expect(parse(null)).toEqual(DEFAULTS);
    expect(parse('x')).toEqual(DEFAULTS);
  });

  it('round-trips what was saved', () => {
    saveAppSettings({ closeToTray: true, reconnect: false, language: 'pt-BR' });
    expect(loadAppSettings()).toEqual({ closeToTray: true, reconnect: false, language: 'pt-BR' });
  });

  it('keeps reconnect on unless it says false, and the tray off unless it says true', () => {
    expect(parse({ reconnect: 'no' }).reconnect).toBe(true);
    expect(parse({ reconnect: false }).reconnect).toBe(false);
    expect(parse({ closeToTray: 'yes' }).closeToTray).toBe(false);
    expect(parse({ closeToTray: true }).closeToTray).toBe(true);
  });

  it('keeps a known language and falls back to automatic for anything else', () => {
    expect(parse({ language: 'en' }).language).toBe('en');
    expect(parse({ language: 'pt-BR' }).language).toBe('pt-BR');
    expect(parse({ language: 'xx' }).language).toBe('auto');
    expect(parse({ language: 3 }).language).toBe('auto');
  });
});
