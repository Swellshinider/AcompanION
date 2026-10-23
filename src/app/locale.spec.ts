import { clearTranslations } from '@angular/localize';
import { initLocale, parseXlf } from './locale';

const XLF = `<?xml version="1.0" encoding="UTF-8" ?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file><body>
<trans-unit id="a"><source>Open</source><target>Abrir</target></trans-unit>
<trans-unit id="b"><source>Level <x id="INTERPOLATION"/></source><target>Nível <x id="INTERPOLATION"/> &amp; mais</target></trans-unit>
<trans-unit id="d"><source>n</source><target>{VAR_PLURAL, plural, =1 {1 item} other {<x id="INTERPOLATION"/> itens}}</target></trans-unit>
<trans-unit id="c"><source>x</source></trans-unit>
</body></file></xliff>`;

describe('locale', () => {
  afterEach(() => {
    clearTranslations();
    vi.restoreAllMocks();
  });

  it('turns targets into $localize messages and skips untranslated units', () => {
    expect(parseXlf(XLF)).toEqual({ a: 'Abrir', b: 'Nível {$INTERPOLATION} & mais', d: '{VAR_PLURAL, plural, =1 {1 item} other {{INTERPOLATION} itens}}' });
  });

  it('stays English for other languages without fetching', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    expect(await initLocale('en-US')).toBe('en-US');
    expect(await initLocale('de')).toBe('en-US');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('loads Brazilian Portuguese for any pt language and translates $localize', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(XLF));
    expect(await initLocale('pt-PT')).toBe('pt-BR');
    expect($localize`:@@a:Open`).toBe('Abrir');
  });

  it('uses the language chosen in Settings over the system one', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(XLF));
    expect(await initLocale('pt-BR', 'en')).toBe('en-US');
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(await initLocale('en-US', 'pt-BR')).toBe('pt-BR');
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  it('reads the saved choice when none is passed', async () => {
    localStorage.setItem('acompanion.app', JSON.stringify({ language: 'en' }));
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    expect(await initLocale('pt-BR')).toBe('en-US');
    expect(fetchSpy).not.toHaveBeenCalled();
    localStorage.clear();
  });

  it('falls back to English when the file cannot be loaded', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 404 }));
    expect(await initLocale('pt-BR')).toBe('en-US');
  });
});
