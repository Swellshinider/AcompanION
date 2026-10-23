import { TestBed } from '@angular/core/testing';
import { Pets } from './pets';
import { Radar } from '../characters.service';
import { GameData, PetCollection, PetRow } from '../gamedata';

const pet = (id: number, name: string, level: number, souls: number, next: number | null): PetRow => ({
  id, name, genus: 'Fera', level, souls, next, toNext: next === null ? 0 : next - souls, toMax: 0,
});
const collection: PetCollection = {
  rows: [pet(1, 'Alpha', 1, 20, 25), pet(2, 'Beta', 2, 74, 75), pet(3, 'Gamma', 3, 0, null), pet(4, 'Delta', 0, 0, 5), pet(5, 'Eps', 0, 3, 5)],
  obtained: 3, total: 5, byLevel: [2, 1, 1, 1], soulsToMax: 1234, updatedUs: 0,
  actives: [{ character: 'Hero', pet: 'Alpha' }],
};

function render() {
  const f = TestBed.createComponent(Pets);
  f.componentRef.setInput('collection', collection);
  f.componentRef.setInput('now', 60_000);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement };
}
const names = (el: HTMLElement, sec: string) =>
  [...el.querySelectorAll(`[data-sec="${sec}"] .pcard .n`)].map(n => n.textContent?.trim());

describe('Pets', () => {
  it('summarizes the collection and lists the active pet per character', () => {
    const { el } = render();
    expect(el.querySelector('.summary')?.textContent).toContain('3/5');
    expect(el.querySelector('.summary')?.textContent).toContain('1,234');
    expect(el.querySelector('[data-sec="active"]')?.textContent).toContain('Hero');
  });

  it('lists the pets nearest to a level first and leaves out maxed and untouched ones', () => {
    const { el } = render();
    expect(names(el, 'closest')).toEqual(['Beta', 'Eps', 'Alpha']);
  });

  it('filters the full list by level', () => {
    const { f, el } = render();
    expect(names(el, 'all')).toEqual(['Alpha', 'Beta', 'Gamma', 'Delta', 'Eps']);
    [...el.querySelectorAll<HTMLButtonElement>('.chip')].find(b => b.textContent?.includes('Level 2'))!.click();
    f.detectChanges();
    expect(names(el, 'all')).toEqual(['Beta']);
    [...el.querySelectorAll<HTMLButtonElement>('.chip')].find(b => b.textContent?.includes('Locked'))!.click();
    f.detectChanges();
    expect(names(el, 'all')).toEqual(['Delta', 'Eps']);
  });

  it('emits back', () => {
    const { f, el } = render();
    let emitted = 0;
    f.componentInstance.back.subscribe(() => emitted++);
    el.querySelector<HTMLButtonElement>('.back')!.click();
    expect(emitted).toBe(1);
  });

  it('has no map tab and inert cards without game data', () => {
    const { el } = render();
    expect(el.querySelector('#tab-map')).toBeNull();
    expect(el.querySelector<HTMLButtonElement>('.pcard')!.disabled).toBe(true);
    expect(el.querySelector('.pcard .go')).toBeNull();
  });
});

describe('Pets farm map', () => {
  const gd: GameData = {
    version: 1, weekly_reset_day: 3, classes: {}, counters: {}, gains: {}, pets: {}, pet_souls: [5, 25, 75],
    pet_regions: {
      north: { name: { en: 'North' }, origin: [-10, -10], extent: [20, 20], maps: [1010] },
      south: { name: { en: 'South' }, origin: [-10, -10], extent: [20, 20] },
    },
    pet_monsters: {
      10: { name: { en: 'Slime' }, level: 5, grade: 'normal', pet: 1, zones: ['Meadow'], spawns: { north: [[0, 0]] } },
      11: { name: { en: 'Wolf' }, level: 9, grade: 'elite', pet: 3, zones: ['Dunes'], spawns: { south: [[5, 5]] } },
      12: { name: { en: 'Bat' }, level: 9, grade: 'normal', pet: 4, zones: ['Cave'], spawns: { north: [[9, 9]] } },
    },
  };
  const monsters = (el: HTMLElement) => [...el.querySelectorAll('[data-sec="farm"] .mon .n')].map(n => n.textContent?.replace(/\s+/g, ' ').trim());
  const chip = (el: HTMLElement, text: string) =>
    [...el.querySelectorAll<HTMLButtonElement>('[data-sec="farm"] .chip')].find(b => b.textContent?.trim().startsWith(text))!;
  /** The pets screen with game data, on the Map tab unless `map` is false. */
  function renderFarm(map = true) {
    const f = TestBed.createComponent(Pets);
    f.componentRef.setInput('collection', collection);
    f.componentRef.setInput('gamedata', gd);
    f.componentRef.setInput('now', 60_000);
    f.detectChanges();
    if (map) openMap(f);
    return { f, el: f.nativeElement as HTMLElement };
  }
  function openMap(f: { nativeElement: HTMLElement; detectChanges(): void }) {
    f.nativeElement.querySelector<HTMLButtonElement>('#tab-map')!.click();
    f.detectChanges();
  }

  it('is absent without game data', () => {
    expect(render().el.querySelector('[data-sec="farm"]')).toBeNull();
  });

  it('keeps the map behind its own tab', () => {
    const { f, el } = renderFarm(false);
    expect(el.querySelector('[data-sec="farm"]')).toBeNull();
    expect(el.querySelector('#tab-collection')?.getAttribute('aria-selected')).toBe('true');
    openMap(f);
    expect(el.querySelector('[data-sec="farm"]')).not.toBeNull();
    expect(el.querySelector('[data-sec="all"]')).toBeNull();
    expect(el.querySelector('#tab-map')?.getAttribute('aria-selected')).toBe('true');
  });

  it('shows the monsters of every pet by region and narrows by filter', () => {
    const { f, el } = renderFarm();
    expect(monsters(el)).toEqual(['Slime Lv 5', 'Bat Lv 9']);   // North first, both 5 souls to go, lower level first
    chip(el, 'South').click();
    f.detectChanges();
    expect(monsters(el)).toEqual(['Wolf Lv 9']);
    chip(el, 'Not maxed').click();   // Wolf drops for the maxed Gamma, so South disappears
    f.detectChanges();
    expect(monsters(el)).toEqual(['Slime Lv 5', 'Bat Lv 9']);
    chip(el, 'Level 2').click();   // Beta has no monster
    f.detectChanges();
    expect(el.querySelector('[data-sec="farm"] .note')).not.toBeNull();
  });

  const viewBox = (el: HTMLElement) => el.querySelector('[data-sec="farm"] svg.map')!.getAttribute('viewBox');
  const rowOf = (el: HTMLElement, name: string) =>
    [...el.querySelectorAll<HTMLElement>('[data-sec="farm"] .mon')].find(li => li.textContent?.includes(name))!;

  it('zooms with the buttons and resets', () => {
    const { f, el } = renderFarm();
    const base = viewBox(el);
    const button = (label: string) => el.querySelector<HTMLButtonElement>(`[data-sec="farm"] .tools button[aria-label="${label}"]`)!;
    expect(button('Zoom out').disabled).toBe(true);
    button('Zoom in').click();
    f.detectChanges();
    const zoomed = viewBox(el)!.split(' ').map(Number);
    expect(zoomed[2]).toBeLessThan(base!.split(' ').map(Number)[2]);
    expect(button('Zoom out').disabled).toBe(false);
    [...el.querySelectorAll<HTMLButtonElement>('[data-sec="farm"] .tools .chip')].find(b => b.textContent?.includes('Reset'))!.click();
    f.detectChanges();
    expect(viewBox(el)).toBe(base);
  });

  it('highlights a monster on hover and frames its spawns when its row is clicked', () => {
    const { f, el } = renderFarm();
    const base = viewBox(el);
    expect(el.querySelector('.cap')?.textContent?.trim()).toBe('');
    rowOf(el, 'Bat').dispatchEvent(new Event('pointerenter'));
    f.detectChanges();
    expect(el.querySelector('.cap')?.textContent).toContain('Bat');
    expect(el.querySelector('.halo')).not.toBeNull();
    expect(el.querySelector('path.lit')).not.toBeNull();   // `.lit` has pointer-events: none, or the hover flickers
    expect(rowOf(el, 'Bat').classList).toContain('on');
    rowOf(el, 'Bat').dispatchEvent(new Event('pointerleave'));
    f.detectChanges();
    expect(el.querySelector('.halo')).toBeNull();
    rowOf(el, 'Bat').querySelector<HTMLButtonElement>('.n')!.click();
    f.detectChanges();
    expect(viewBox(el)).not.toBe(base);
    expect(el.querySelector('.cap')?.textContent).toContain('Bat');   // selected, pointer gone
    rowOf(el, 'Bat').querySelector<HTMLButtonElement>('.n')!.click();
    f.detectChanges();
    expect(el.querySelector('.cap')?.textContent?.trim()).toBe('');
  });

  it('selects a monster by clicking its dot and goes back to the full view on another region', () => {
    const { f, el } = renderFarm();
    el.querySelector<SVGPathElement>('[data-sec="farm"] svg.map path.lv1')!.dispatchEvent(new Event('click'));
    f.detectChanges();
    expect(el.querySelector('.cap')?.textContent).toContain('Slime');
    chip(el, 'South').click();
    f.detectChanges();
    expect(el.querySelector('.cap')?.textContent?.trim()).toBe('');
  });

  it('picks one pet from a card, which opens the map, and clears it again', () => {
    const { f, el } = renderFarm(false);
    el.querySelector<HTMLButtonElement>('[data-sec="closest"] .pcard')!.click();   // Beta, no monsters
    f.detectChanges();
    expect(el.querySelector('#tab-map')?.getAttribute('aria-selected')).toBe('true');
    expect(el.querySelector('[data-sec="farm"] .picked')?.textContent).toContain('Beta');
    expect(el.querySelector('[data-sec="farm"] .note')).not.toBeNull();
    el.querySelector<HTMLButtonElement>('[data-sec="farm"] .picked')!.click();
    f.detectChanges();
    expect(el.querySelector('[data-sec="farm"] .picked')).toBeNull();
    expect(monsters(el).length).toBe(2);
  });

  describe('route', () => {
    const stops = (el: HTMLElement) => el.querySelectorAll('svg.map circle.stop').length;
    const note = (el: HTMLElement) => el.querySelector('[data-route]')?.textContent?.replace(/\s+/g, ' ').trim();

    it('is off on the Map tab until the chip turns it on, and covers the pets not maxed in the region', () => {
      const { f, el } = renderFarm();
      expect(el.querySelector('svg.map path.route')).toBeNull();
      expect(note(el)).toBeUndefined();
      chip(el, 'Route').click();
      f.detectChanges();
      expect(el.querySelector('svg.map path.route')).not.toBeNull();
      expect(stops(el)).toBe(2);   // Alpha (Slime) and Delta (Bat); Gamma's Wolf is in the other region
      expect(note(el)).toBe('Next: Slime');
      expect(chip(el, 'Route').getAttribute('aria-pressed')).toBe('true');
    });

    it('names one monster for the next stop even when several share its spot', () => {
      const shared: GameData = { ...gd, pet_monsters: { ...gd.pet_monsters, 13: { name: { en: 'Rat' }, level: 5, grade: 'normal', pet: 5, zones: [], spawns: { north: [[0.05, 0.05]] } } } };
      const f = TestBed.createComponent(Pets);
      f.componentRef.setInput('collection', collection);
      f.componentRef.setInput('gamedata', shared);
      f.componentRef.setInput('now', 60_000);
      f.componentRef.setInput('overlay', true);
      f.componentRef.setInput('lastRegion', 'north');
      f.detectChanges();
      expect(note(f.nativeElement)).toBe('Next: Slime');
    });

    it('drops a pet from the route when it reaches the top, and says so when none is left', () => {
      const { f, el } = renderFarm();
      chip(el, 'Route').click();
      f.detectChanges();
      const maxed = (ids: number[]) => ({ ...collection, rows: collection.rows.map(r => (ids.includes(r.id) ? pet(r.id, r.name, 3, 0, null) : r)) });
      f.componentRef.setInput('collection', maxed([4]));
      f.detectChanges();
      expect(stops(el)).toBe(1);
      f.componentRef.setInput('collection', maxed([1, 2, 4, 5]));
      f.detectChanges();
      expect(stops(el)).toBe(0);
      expect(note(el)).toContain('No open pets');
    });

    it('is on in the overlay, and moves to the next stop once the player has been at one', () => {
      const f = TestBed.createComponent(Pets);
      const at = (x: number, y: number): Radar => ({ map: 1010, player: { x, y, t_us: 1 }, monsters: [] });
      f.componentRef.setInput('collection', collection);
      f.componentRef.setInput('gamedata', gd);
      f.componentRef.setInput('now', 60_000);
      f.componentRef.setInput('radar', at(100, 100));
      f.componentRef.setInput('overlay', true);
      f.detectChanges();
      const el = f.nativeElement as HTMLElement;
      expect(note(el)).toBe('Next: Slime');
      expect(el.querySelector('line.route-next')).not.toBeNull();
      f.componentRef.setInput('radar', at(0, 0));
      f.detectChanges();
      expect(note(el)).toBe('Next: Bat');
      expect(el.querySelectorAll('circle.stop.done').length).toBe(1);
    });
  });

  describe('fill', () => {
    const fillButton = (el: HTMLElement) => el.querySelector<HTMLButtonElement>('[data-sec="farm"] .tools .fill')!;
    const esc = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));

    it('covers the window, locks the page scroll and leaves on the button', () => {
      const { f, el } = renderFarm();
      expect(el.querySelector('.stage.filled')).toBeNull();
      fillButton(el).click();
      f.detectChanges();
      expect(el.querySelector('.stage.filled')).not.toBeNull();
      expect(fillButton(el).getAttribute('aria-pressed')).toBe('true');
      expect(fillButton(el).textContent).toContain('Exit fill');
      expect(el.querySelector('svg.map')!.getAttribute('style')).not.toContain('max-width');
      expect(document.documentElement.classList).toContain('lock-scroll');
      fillButton(el).click();
      f.detectChanges();
      expect(el.querySelector('.stage.filled')).toBeNull();
      expect(document.documentElement.classList).not.toContain('lock-scroll');
    });

    it('takes the first Escape to leave the fill and only the next one to go back', () => {
      const { f, el } = renderFarm();
      let backs = 0;
      f.componentInstance.back.subscribe(() => backs++);
      // what the dashboard does on Escape
      const toRoster = (e: KeyboardEvent) => e.key === 'Escape' && f.componentInstance.back.emit();
      document.addEventListener('keydown', toRoster);
      try {
        fillButton(el).click();
        f.detectChanges();
        esc();
        f.detectChanges();
        expect(el.querySelector('.stage.filled')).toBeNull();
        expect(backs).toBe(0);
        esc();
        expect(backs).toBe(1);
      } finally {
        document.removeEventListener('keydown', toRoster);
      }
    });

    it('releases the scroll lock when the screen goes away', () => {
      const { f, el } = renderFarm();
      fillButton(el).click();
      f.detectChanges();
      f.destroy();
      expect(document.documentElement.classList).not.toContain('lock-scroll');
    });
  });

  describe('overlay', () => {
    function renderOverlay(radar: Radar | null) {
      const f = TestBed.createComponent(Pets);
      f.componentRef.setInput('collection', collection);
      f.componentRef.setInput('gamedata', gd);
      f.componentRef.setInput('now', 60_000);
      f.componentRef.setInput('radar', radar);
      f.componentRef.setInput('overlay', true);
      f.detectChanges();
      return { f, el: f.nativeElement as HTMLElement };
    }
    const here = (map: number | null): Radar => ({ map, player: { x: 100, y: 100, t_us: 1 }, monsters: [] });
    const viewWidth = (el: HTMLElement) => Number(el.querySelector('svg.map')!.getAttribute('viewBox')!.split(' ')[2]);

    it('is the filled minimap of the region the player is in, with no region chips', () => {
      const { el } = renderOverlay(here(1010));
      expect(el.classList).toContain('overlay');
      expect(el.querySelector('.stage.filled')).not.toBeNull();
      expect(el.querySelector('.cap')).not.toBeNull();
      expect(el.querySelector('svg.map')).not.toBeNull();
      expect(el.querySelector('svg.map')!.getAttribute('style') ?? '').not.toContain('aspect-ratio');
    });

    it('names the monster under the pointer, only while hovering', () => {
      const { f, el } = renderOverlay(here(1010));
      const dot = el.querySelector('svg.map path.lv1')!;
      expect(el.querySelector('.tip')).toBeNull();
      dot.dispatchEvent(new Event('pointerenter'));
      dot.dispatchEvent(new Event('pointermove'));
      f.detectChanges();
      expect(el.querySelector('.tip')?.textContent).toContain('Slime');
      dot.dispatchEvent(new Event('pointerleave'));
      f.detectChanges();
      expect(el.querySelector('.tip')).toBeNull();
    });

    it('draws no map outside the regions', () => {
      expect(renderOverlay(here(999)).el.querySelector('svg.map')).toBeNull();
      expect(renderOverlay(null).el.querySelector('svg.map')).toBeNull();
    });

    it('falls back to the region the player was last seen in while the game has not said where they are', () => {
      const f = TestBed.createComponent(Pets);
      for (const [k, v] of Object.entries({ collection, gamedata: gd, now: 60_000, radar: null, overlay: true, lastRegion: 'north' })) f.componentRef.setInput(k, v);
      f.detectChanges();
      expect((f.nativeElement as HTMLElement).querySelector('svg.map')).not.toBeNull();
    });

    it('zooms with the buttons', () => {
      const { f, el } = renderOverlay(here(1010));
      const w0 = viewWidth(el);
      const zoom = (label: string) => el.querySelector<HTMLButtonElement>(`.tools button[aria-label="${label}"]`)!.click();
      zoom('Zoom in');
      f.detectChanges();
      expect(viewWidth(el)).toBeCloseTo(w0 / 1.5);
      zoom('Zoom out');
      zoom('Zoom out');
      f.detectChanges();
      expect(viewWidth(el)).toBeGreaterThan(w0);
    });

    it('shows the colour key on request', () => {
      const { f, el } = renderOverlay(here(1010));
      expect(el.querySelector('.legend')).toBeNull();
      el.querySelector<HTMLButtonElement>('.legend-btn')!.click();
      f.detectChanges();
      expect([...el.querySelectorAll('.legend li')].map(li => li.textContent?.trim())).toEqual(['Locked', 'Level 1', 'Level 2', 'Max']);
      el.querySelector<HTMLButtonElement>('.legend-btn')!.click();
      f.detectChanges();
      expect(el.querySelector('.legend')).toBeNull();
    });

    it('shows the pet that last gained souls, not the first collection it sees', () => {
      const { f, el } = renderOverlay(here(1010));
      const gain = () => el.querySelector('[data-gain]')?.textContent?.replace(/\s+/g, ' ').trim();
      expect(gain()).toBeUndefined();
      const grew = (rows: PetRow[]) => { f.componentRef.setInput('collection', { ...collection, rows }); f.detectChanges(); };
      const alpha = (souls: number) => collection.rows.map(r => (r.id === 1 ? pet(1, 'Alpha', 1, souls, 25) : r));
      grew(alpha(21));
      expect(gain()).toBe('Alpha · Level 1 · 21/25');
      grew(alpha(21));   // same numbers again: the gain stays
      expect(gain()).toBe('Alpha · Level 1 · 21/25');
      grew(collection.rows.map(r => (r.id === 2 ? pet(2, 'Beta', 3, 0, null) : r.id === 1 ? pet(1, 'Alpha', 1, 21, 25) : r)));
      expect(gain()).toBe('Beta · Max');   // a level-up to the top
    });

    it('can be dragged away from the player and brought back with Centre on me', () => {
      const { f, el } = renderOverlay(here(1010));
      const svg = el.querySelector('svg.map')!;
      svg.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 200, right: 200, bottom: 200, x: 0, y: 0, toJSON() {} });
      const origin = () => svg.getAttribute('viewBox')!.split(' ').slice(0, 2).map(Number);
      const [x0, y0] = origin();
      expect(el.querySelector('.me-btn')).toBeNull(); // following the player: nothing to go back to
      svg.dispatchEvent(new PointerEvent('pointerdown', { clientX: 100, clientY: 100 }));
      svg.dispatchEvent(new PointerEvent('pointermove', { clientX: 140, clientY: 100, buttons: 1 }));
      svg.dispatchEvent(new PointerEvent('pointerup'));
      f.detectChanges();
      const [x1, y1] = origin();
      expect(x1).toBeLessThan(x0); // dragged right: the map moves right, the view left
      expect(y1).toBeCloseTo(y0);
      el.querySelector<HTMLButtonElement>('.me-btn')!.click();
      f.detectChanges();
      expect(origin()[0]).toBeCloseTo(x0);
      expect(el.querySelector('.me-btn')).toBeNull();
    });
  });

  describe('live radar', () => {
    // Slime (north) is pet 1's monster, Wolf (south) pet 3's; positions are hundredths of a map unit
    const radar = (templates: number[], player: Radar['player'] = null, map: number | null = null): Radar => ({
      map,
      player,
      monsters: templates.map((template, i) => ({ entity: 100 + i, template, x: 100 * (i + 1), y: 200 })),
    });
    function renderLive(r: Radar | null) {
      const f = TestBed.createComponent(Pets);
      f.componentRef.setInput('collection', collection);
      f.componentRef.setInput('gamedata', gd);
      f.componentRef.setInput('now', 60_000);
      f.componentRef.setInput('radar', r);
      f.detectChanges();
      openMap(f);
      return { f, el: f.nativeElement as HTMLElement };
    }
    const circles = (el: HTMLElement, cls: string) => el.querySelectorAll(`[data-sec="farm"] svg.map circle.${cls}`);
    const note = (el: HTMLElement) => el.querySelector('[data-sec="farm"] [data-live]')?.textContent?.replace(/\s+/g, ' ').trim();

    it('draws nothing while the game is not running', () => {
      const { el } = renderLive(null);
      expect(circles(el, 'live').length + circles(el, 'me').length).toBe(0);
      expect(note(el)).toBeUndefined();
    });

    it('follows the region of the monsters in view and marks them, with the player and the age of the fix', () => {
      const { f, el } = renderLive(radar([11], { x: 500, y: 600, t_us: Date.now() * 1000 - 12_000_000 }));
      expect(chip(el, 'South').classList).toContain('on');   // the first region is North
      const live = circles(el, 'live');
      expect(live.length).toBe(1);
      expect(live[0].classList).toContain('lv3');   // the Wolf feeds Gamma, at the top level
      expect([live[0].getAttribute('cx'), live[0].getAttribute('cy')]).toEqual(['1', '2']);
      const me = circles(el, 'me');
      expect([me[0].getAttribute('cx'), me[0].getAttribute('cy')]).toEqual(['5', '6']);
      expect(note(el)).toMatch(/^In view: 1 · Your last position: \d+s ago$/);
      f.componentRef.setInput('radar', radar([10], null));   // the Slime is in the north
      f.detectChanges();
      expect(chip(el, 'North').classList).toContain('on');
      expect(circles(el, 'me').length).toBe(0);
    });

    it('keeps the region while nothing is in view and lets a region chip win', () => {
      const { f, el } = renderLive(radar([11]));
      f.componentRef.setInput('radar', radar([]));
      f.detectChanges();
      expect(chip(el, 'South').classList).toContain('on');
      chip(el, 'North').click();
      f.detectChanges();
      f.componentRef.setInput('radar', radar([11]));   // still the south, but the pick holds
      f.detectChanges();
      expect(chip(el, 'North').classList).toContain('on');
      expect(circles(el, 'live').length).toBe(0);   // the Wolf is not on the North map
    });

    it('only marks monsters the farm filter lists, and frames the player on request', () => {
      const { f, el } = renderLive(radar([10, 11], { x: 0, y: 0, t_us: 0 }));
      expect(circles(el, 'live').length).toBe(1);   // a tie between North and South goes to North
      chip(el, 'Level 2').click();   // Beta has no monster: the map is empty
      f.detectChanges();
      expect(circles(el, 'live').length).toBe(0);
      chip(el, 'All').click();
      f.detectChanges();
      const base = viewBox(el);
      chip(el, 'Centre on me').click();
      f.detectChanges();
      expect(viewBox(el)).not.toBe(base);
    });
  });
});
