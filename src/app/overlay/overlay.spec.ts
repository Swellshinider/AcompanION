import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { Overlay } from './overlay';
import { Character, CharactersService, Fight, Meter as MeterData, Radar } from '../characters.service';
import { GameData } from '../gamedata';
import { OverlaySettings, OverlayView } from '../overlay-settings';

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

async function render(view: OverlayView, settings: Partial<OverlaySettings>, chars: Character[] = [], game: { on: boolean; radar: Radar | null } = { on: false, radar: null }) {
  localStorage.setItem(`acompanion.overlay.${view}`, JSON.stringify(settings));
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => gd })));
  const service = {
    characters: signal(chars), game: signal(game.on), radar: signal(game.radar), meter: signal<MeterData | null>(null),
    fights: signal<Fight[]>([]), recent: signal<Fight[]>([]), status: signal({ id: 'running', detail: null }),
  };
  TestBed.configureTestingModule({
    providers: [{ provide: CharactersService, useValue: service }],
  });
  const f = TestBed.createComponent(Overlay);
  f.componentRef.setInput('view', view);
  await new Promise(r => setTimeout(r)); // let loadGamedata resolve
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement, service };
}

describe('Overlay', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    vi.useRealTimers();
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('shows the dailies of the character seen last, without the hero', async () => {
    const { el } = await render('dailies', {}, [ch('Alpha', nowUs - 5_000_000), ch('Beta', nowUs)]);
    expect(el.querySelector('.who')?.textContent).toBe('Beta');
    expect(el.querySelector('app-character')?.classList).toContain('compact');
    expect(el.querySelector('[data-sec="daily"]')).not.toBeNull();
    expect(el.querySelector('header .title')?.textContent).toContain('Dailies');
  });

  it('says so when no character was seen yet', async () => {
    const { el } = await render('dailies', {});
    expect(el.querySelector('.empty')).not.toBeNull();
    expect(el.querySelector('app-character')).toBeNull();
  });

  it('shows the pets map as the radar view', async () => {
    const { el } = await render('radar', {});
    expect(el.querySelector('app-pets')?.classList).toContain('overlay');
    expect(el.querySelector('header .title')?.textContent).toContain('Radar');
  });

  it('shows the meter as a compact list', async () => {
    const { el } = await render('meter', {});
    expect(el.querySelector('app-meter')?.classList).toContain('compact');
    expect(el.querySelector('header .title')?.textContent).toContain('Meter');
    expect(el.querySelector('app-meter .empty')).not.toBeNull();
  });

  it('applies the opacity and keeps the title bar', async () => {
    const { el } = await render('radar', { opacity: 40 });
    expect(el.querySelector<HTMLElement>('.shell')!.style.opacity).toBe('0.4');
    expect(el.querySelector('header')).not.toBeNull();
  });

  it('has a slider over the map that changes and remembers the opacity', async () => {
    const { f, el } = await render('radar', { opacity: 40 });
    const slider = el.querySelector<HTMLInputElement>('input.fade')!;
    expect(slider.value).toBe('40');
    slider.value = '70';
    slider.dispatchEvent(new Event('input'));
    f.detectChanges();
    expect(el.querySelector<HTMLElement>('.shell')!.style.opacity).toBe('0.7');
    expect(JSON.parse(localStorage.getItem('acompanion.overlay.radar')!).opacity).toBe(70);
  });

  it('has no slider in the dailies view or while clicks pass through', async () => {
    expect((await render('dailies', {})).el.querySelector('input.fade')).toBeNull();
    TestBed.resetTestingModule();
    expect((await render('radar', { clickThrough: true })).el.querySelector('input.fade')).toBeNull();
  });

  it('fades only the bars of the meter, not the whole window', async () => {
    const { el } = await render('meter', { opacity: 40 });
    const shell = el.querySelector<HTMLElement>('.shell')!;
    expect(shell.classList).toContain('bare');
    expect(shell.style.opacity).toBe('');
    expect(shell.style.getPropertyValue('--bar-a')).toBe('0.4');
  });

  it('hides the title bar while clicks pass through', async () => {
    const { el } = await render('radar', { clickThrough: true });
    expect(el.querySelector('header')).toBeNull();
  });

  it('marks the page as the overlay while it lives', async () => {
    const { f } = await render('radar', {});
    expect(document.documentElement.classList).toContain('overlay');
    f.destroy();
    expect(document.documentElement.classList).not.toContain('overlay');
  });

  describe('auto-hide', () => {
    const here = (map: number | null): Radar => ({ map, player: null, monsters: [] });
    /** Which of `show`/`hide` the window was asked for last. */
    async function windowCalls(settings: Partial<OverlaySettings>, game: { on: boolean; radar: Radar | null }, view: OverlayView = 'radar') {
      const invoke = vi.fn(async (..._: unknown[]) => undefined);
      vi.stubGlobal('__TAURI_INTERNALS__', { invoke, metadata: { currentWindow: { label: `overlay-${view}` } }, transformCallback: () => 1 });
      vi.stubGlobal('__TAURI_EVENT_PLUGIN_INTERNALS__', { unregisterListener: () => undefined });
      const { f } = await render(view, settings, [], game);
      await f.whenStable();
      f.destroy();
      return invoke.mock.calls.map(c => c[0]).filter(c => c === 'plugin:window|show' || c === 'plugin:window|hide').at(-1);
    }

    it('listens for settings sent to its own window only', async () => {
      const invoke = vi.fn(async (..._: unknown[]) => undefined);
      vi.stubGlobal('__TAURI_INTERNALS__', { invoke, metadata: { currentWindow: { label: 'overlay-meter' } }, transformCallback: () => 1 });
      vi.stubGlobal('__TAURI_EVENT_PLUGIN_INTERNALS__', { unregisterListener: () => undefined });
      const { f } = await render('meter', {});
      await f.whenStable();
      f.destroy();
      const call = invoke.mock.calls.find(c => c[0] === 'plugin:event|listen' && (c[1] as { event: string }).event === 'overlay://settings');
      expect((call![1] as { target: unknown }).target).toEqual({ kind: 'AnyLabel', label: 'overlay-meter' });
    });

    it('shows the radar while the game runs in a known map', async () => {
      expect(await windowCalls({}, { on: true, radar: here(1010) })).toBe('plugin:window|show');
    });

    it('hides it when the game is not running', async () => {
      expect(await windowCalls({}, { on: false, radar: here(1010) })).toBe('plugin:window|hide');
    });

    it('shows the radar while the game runs, in a map that is not a region and none was ever seen', async () => {
      expect(await windowCalls({}, { on: true, radar: here(999) })).toBe('plugin:window|show');
    });

    it('shows the radar while the game runs, before any map is known', async () => {
      expect(await windowCalls({}, { on: true, radar: null })).toBe('plugin:window|show');
    });

    it('remembers the region it saw', async () => {
      await windowCalls({}, { on: true, radar: here(1010) });
      expect(localStorage.getItem('acompanion.overlay.region')).toBe('verteron');
    });

    it('shows the dailies while the game runs, in a map that is not a region', async () => {
      expect(await windowCalls({}, { on: true, radar: here(999) }, 'dailies')).toBe('plugin:window|show');
    });

    it('hides the dailies when the game is not running', async () => {
      expect(await windowCalls({}, { on: false, radar: here(1010) }, 'dailies')).toBe('plugin:window|hide');
    });

    it('shows the meter while the game runs, in a map that is not a region', async () => {
      expect(await windowCalls({}, { on: true, radar: here(999) }, 'meter')).toBe('plugin:window|show');
    });

    it('shows the meter while the game runs, before any radar', async () => {
      expect(await windowCalls({}, { on: true, radar: null }, 'meter')).toBe('plugin:window|show');
    });

    it('hides the meter when the game is not running', async () => {
      expect(await windowCalls({}, { on: false, radar: here(1010) }, 'meter')).toBe('plugin:window|hide');
    });

    it('stays shown everywhere when auto-hide is off', async () => {
      expect(await windowCalls({ autoHide: false }, { on: false, radar: null })).toBe('plugin:window|show');
    });
  });

  describe('hide outside combat', () => {
    const fight = (end = Date.now()): Fight => ({ start_us: (end - 1000) * 1000, end_us: end * 1000, me: 1, players: [], target: null });
    async function setup() {
      const invoke = vi.fn(async (..._: unknown[]) => undefined);
      vi.stubGlobal('__TAURI_INTERNALS__', { invoke, metadata: { currentWindow: { label: 'overlay-meter' } }, transformCallback: () => 1 });
      vi.stubGlobal('__TAURI_EVENT_PLUGIN_INTERNALS__', { unregisterListener: () => undefined });
      const result = await render('meter', { open: true, hideOutOfCombat: true }, [], { on: true, radar: null });
      vi.useFakeTimers();
      const lastWindowCall = () => invoke.mock.calls.map(c => c[0]).filter(c => c === 'plugin:window|show' || c === 'plugin:window|hide').at(-1);
      const hit = (current = fight()) => {
        result.service.meter.set({ me: 1, current, fights: result.service.fights().length });
        result.f.detectChanges();
        return current;
      };
      const advance = (ms: number) => { vi.advanceTimersByTime(ms); result.f.detectChanges(); };
      return { ...result, lastWindowCall, hit, advance };
    }

    it('hides without a fight, shows on damage, hides at 10 seconds and shows on the next hit', async () => {
      const { f, hit, advance, lastWindowCall } = await setup();
      expect(lastWindowCall()).toBe('plugin:window|hide');
      hit();
      expect(lastWindowCall()).toBe('plugin:window|show');
      advance(9999);
      expect(f.componentInstance.shown()).toBe(true);
      advance(1);
      expect(lastWindowCall()).toBe('plugin:window|hide');
      hit();
      expect(lastWindowCall()).toBe('plugin:window|show');
      expect(JSON.parse(localStorage.getItem('acompanion.overlay.meter')!).open).toBe(true);
    });

    it('restarts the timeout only when the last hit changes', async () => {
      const { hit, advance, lastWindowCall, service, f } = await setup();
      const first = hit();
      advance(5000);
      hit({ ...first }); // duplicate snapshot
      const second = hit();
      advance(5000); // the first hit's deadline
      expect(lastWindowCall()).toBe('plugin:window|show');
      service.meter.update(m => ({ ...m!, current: { ...second, target: { entity: 9, template: 5, hp: 50, max_hp: 200, stagger: null } } }));
      f.detectChanges();
      advance(4999);
      expect(lastWindowCall()).toBe('plugin:window|show');
      advance(1);
      expect(lastWindowCall()).toBe('plugin:window|hide');
    });

    it('keeps the last result until 10 seconds after its hit even when the boss died', async () => {
      const { hit, advance, lastWindowCall, service, f } = await setup();
      const dead = hit({ ...fight(), target: { entity: 9, template: 5, hp: 0, max_hp: 200, stagger: null } });
      advance(1100);
      service.fights.set([dead]);
      service.meter.set({ me: 1, current: null, fights: 1 });
      f.detectChanges();
      advance(8899);
      expect(lastWindowCall()).toBe('plugin:window|show');
      advance(1);
      expect(lastWindowCall()).toBe('plugin:window|hide');
    });

    it('uses the remaining time of a saved fight and leaves old history hidden', async () => {
      const { service, f, advance, lastWindowCall } = await setup();
      service.fights.set([fight(Date.now() - 20_000)]);
      f.detectChanges();
      expect(lastWindowCall()).toBe('plugin:window|hide');
      service.fights.set([fight(Date.now() - 7000)]);
      f.detectChanges();
      advance(2999);
      expect(lastWindowCall()).toBe('plugin:window|show');
      advance(1);
      expect(lastWindowCall()).toBe('plugin:window|hide');
    });

    it('applies settings immediately and combines combat hiding with game hiding', async () => {
      const { service, f, hit, advance, lastWindowCall } = await setup();
      hit();
      service.game.set(false);
      f.detectChanges();
      expect(lastWindowCall()).toBe('plugin:window|hide');
      f.componentInstance.settings.update(s => ({ ...s, autoHide: false }));
      f.detectChanges();
      expect(lastWindowCall()).toBe('plugin:window|show');
      advance(10_000);
      expect(lastWindowCall()).toBe('plugin:window|hide');
      f.componentInstance.settings.update(s => ({ ...s, hideOutOfCombat: false }));
      f.detectChanges();
      expect(lastWindowCall()).toBe('plugin:window|show');
      f.componentInstance.settings.update(s => ({ ...s, hideOutOfCombat: true }));
      f.detectChanges();
      expect(lastWindowCall()).toBe('plugin:window|hide');
    });

    it('cancels its combat timeout when destroyed', async () => {
      const { f, hit } = await setup();
      hit();
      expect(vi.getTimerCount()).toBe(1);
      f.destroy();
      expect(vi.getTimerCount()).toBe(0);
    });

    for (const view of ['radar', 'dailies'] as const) {
      it(`ignores the combat preference for ${view}`, async () => {
        const { f } = await render(view, { hideOutOfCombat: true }, [], { on: true, radar: null });
        expect(f.componentInstance.shown()).toBe(true);
      });
    }
  });

  it('closes through the overlay command and remembers it as closed', async () => {
    const invoke = vi.fn(async () => undefined);
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
    const { el } = await render('radar', { open: true });
    el.querySelector<HTMLButtonElement>('.x')!.click();
    await new Promise(r => setTimeout(r));
    expect(invoke).toHaveBeenCalledWith('overlay', expect.objectContaining({ open: false }), undefined);
    expect(JSON.parse(localStorage.getItem('acompanion.overlay.radar')!).open).toBe(false);
  });
});
