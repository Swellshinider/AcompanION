import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { Overlay } from './overlay';
import { Character, CharactersService, Radar } from '../characters.service';
import { GameData } from '../gamedata';
import { OverlaySettings } from '../overlay-settings';

const gd: GameData = {
  version: 1, weekly_reset_day: 3,
  classes: { 3: { name: { en: 'Templar' } } },
  counters: { 1: { name: { en: 'Nightmare' }, category: 'daily', cap: 2, reset: 'daily', refill: null } },
  gains: {}, pets: { 1: { name: { en: 'Pet One' }, genus: 'Fera' } }, pet_souls: [5, 25, 75],
  pet_regions: { verteron: { name: { en: 'Verteron' }, origin: [-10, -10], extent: [20, 20], maps: [1010] } }, pet_monsters: {},
};
const nowUs = Date.now() * 1000;
const ch = (name: string, seenUs: number): Character => ({
  server: 7, name, class_code: 3, level: 40, combat_power: 1234, item_level: 1500,
  counters: [{ id: 1, value: 1, ms: false, updated_us: nowUs }], last_seen_us: seenUs,
});

async function render(settings: Partial<OverlaySettings>, chars: Character[] = [], game: { on: boolean; radar: Radar | null } = { on: false, radar: null }) {
  localStorage.setItem('acompanion.overlay', JSON.stringify(settings));
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => gd })));
  TestBed.configureTestingModule({
    providers: [{ provide: CharactersService, useValue: { characters: signal(chars), game: signal(game.on), radar: signal(game.radar), meter: signal(null), fights: signal([]), status: signal({ id: 'running', detail: null }) } }],
  });
  const f = TestBed.createComponent(Overlay);
  await new Promise(r => setTimeout(r)); // let loadGamedata resolve
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement };
}

describe('Overlay', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('shows the dailies of the character seen last, without the hero', async () => {
    const { el } = await render({ view: 'dailies' }, [ch('Alpha', nowUs - 5_000_000), ch('Beta', nowUs)]);
    expect(el.querySelector('.who')?.textContent).toBe('Beta');
    expect(el.querySelector('app-character')?.classList).toContain('compact');
    expect(el.querySelector('[data-sec="daily"]')).not.toBeNull();
    expect(el.querySelector('header .title')?.textContent).toContain('Dailies');
  });

  it('says so when no character was seen yet', async () => {
    const { el } = await render({ view: 'dailies' });
    expect(el.querySelector('.empty')).not.toBeNull();
    expect(el.querySelector('app-character')).toBeNull();
  });

  it('shows the pets map as the radar view', async () => {
    const { el } = await render({ view: 'radar' });
    expect(el.querySelector('app-pets')?.classList).toContain('overlay');
    expect(el.querySelector('header .title')?.textContent).toContain('Radar');
  });

  it('shows the meter as a compact list', async () => {
    const { el } = await render({ view: 'meter' });
    expect(el.querySelector('app-meter')?.classList).toContain('compact');
    expect(el.querySelector('header .title')?.textContent).toContain('Meter');
    expect(el.querySelector('app-meter .empty')).not.toBeNull();
  });

  it('applies the opacity and keeps the title bar', async () => {
    const { el } = await render({ opacity: 40 });
    expect(el.querySelector<HTMLElement>('.shell')!.style.opacity).toBe('0.4');
    expect(el.querySelector('header')).not.toBeNull();
  });

  it('has a slider over the map that changes and remembers the opacity', async () => {
    const { f, el } = await render({ view: 'radar', opacity: 40 });
    const slider = el.querySelector<HTMLInputElement>('input.fade')!;
    expect(slider.value).toBe('40');
    slider.value = '70';
    slider.dispatchEvent(new Event('input'));
    f.detectChanges();
    expect(el.querySelector<HTMLElement>('.shell')!.style.opacity).toBe('0.7');
    expect(JSON.parse(localStorage.getItem('acompanion.overlay')!).opacity).toBe(70);
  });

  it('has no slider in the dailies view or while clicks pass through', async () => {
    expect((await render({ view: 'dailies' })).el.querySelector('input.fade')).toBeNull();
    TestBed.resetTestingModule();
    expect((await render({ view: 'radar', clickThrough: true })).el.querySelector('input.fade')).toBeNull();
  });

  it('hides the title bar while clicks pass through', async () => {
    const { el } = await render({ clickThrough: true });
    expect(el.querySelector('header')).toBeNull();
  });

  it('marks the page as the overlay while it lives', async () => {
    const { f } = await render({});
    expect(document.documentElement.classList).toContain('overlay');
    f.destroy();
    expect(document.documentElement.classList).not.toContain('overlay');
  });

  describe('auto-hide', () => {
    const here = (map: number | null): Radar => ({ map, player: null, monsters: [] });
    /** Which of `show`/`hide` the window was asked for last. */
    async function windowCalls(settings: Partial<OverlaySettings>, game: { on: boolean; radar: Radar | null }) {
      const invoke = vi.fn(async (..._: unknown[]) => undefined);
      vi.stubGlobal('__TAURI_INTERNALS__', { invoke, metadata: { currentWindow: { label: 'overlay' } }, transformCallback: () => 1 });
      vi.stubGlobal('__TAURI_EVENT_PLUGIN_INTERNALS__', { unregisterListener: () => undefined });
      const { f } = await render(settings, [], game);
      await f.whenStable();
      f.destroy();
      return invoke.mock.calls.map(c => c[0]).filter(c => c === 'plugin:window|show' || c === 'plugin:window|hide').at(-1);
    }

    it('shows the window while the game runs in a known map', async () => {
      expect(await windowCalls({}, { on: true, radar: here(1010) })).toBe('plugin:window|show');
    });

    it('hides it when the game is not running', async () => {
      expect(await windowCalls({}, { on: false, radar: here(1010) })).toBe('plugin:window|hide');
    });

    it('hides it in a map that is not a region when no region was ever seen', async () => {
      expect(await windowCalls({}, { on: true, radar: here(999) })).toBe('plugin:window|hide');
    });

    it('shows it, before any map is known, in the region the player was last seen in', async () => {
      localStorage.setItem('acompanion.overlay.region', 'verteron');
      expect(await windowCalls({}, { on: true, radar: null })).toBe('plugin:window|show');
    });

    it('remembers the region it saw', async () => {
      await windowCalls({}, { on: true, radar: here(1010) });
      expect(localStorage.getItem('acompanion.overlay.region')).toBe('verteron');
    });

    it('ignores a remembered region that no longer exists', async () => {
      localStorage.setItem('acompanion.overlay.region', 'gone');
      expect(await windowCalls({}, { on: true, radar: null })).toBe('plugin:window|hide');
    });

    it('shows the dailies while the game runs, in a map that is not a region', async () => {
      expect(await windowCalls({ view: 'dailies' }, { on: true, radar: here(999) })).toBe('plugin:window|show');
    });

    it('hides the dailies when the game is not running', async () => {
      expect(await windowCalls({ view: 'dailies' }, { on: false, radar: here(1010) })).toBe('plugin:window|hide');
    });

    it('shows the meter while the game runs, in a map that is not a region', async () => {
      expect(await windowCalls({ view: 'meter' }, { on: true, radar: here(999) })).toBe('plugin:window|show');
    });

    it('shows the meter while the game runs, before any radar', async () => {
      expect(await windowCalls({ view: 'meter' }, { on: true, radar: null })).toBe('plugin:window|show');
    });

    it('hides the meter when the game is not running', async () => {
      expect(await windowCalls({ view: 'meter' }, { on: false, radar: here(1010) })).toBe('plugin:window|hide');
    });

    it('stays shown everywhere when auto-hide is off', async () => {
      expect(await windowCalls({ autoHide: false }, { on: false, radar: null })).toBe('plugin:window|show');
    });
  });

  it('closes through the overlay command and remembers it as closed', async () => {
    const invoke = vi.fn(async () => undefined);
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
    const { el } = await render({ open: true });
    el.querySelector<HTMLButtonElement>('.x')!.click();
    await new Promise(r => setTimeout(r));
    expect(invoke).toHaveBeenCalledWith('overlay', expect.objectContaining({ open: false }), undefined);
    expect(JSON.parse(localStorage.getItem('acompanion.overlay')!).open).toBe(false);
  });
});
