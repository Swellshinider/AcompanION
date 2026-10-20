import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { Dashboard } from './dashboard';
import { CaptureStatus, Character, CharactersService } from '../characters.service';
import { GameData } from '../gamedata';

const gd: GameData = {
  version: 1, weekly_reset_day: 3,
  classes: { 3: { name: { en: 'Templar' } }, 4: { name: { en: 'Ranger' }, icon: 'ranger' } },
  counters: { 1: { name: { en: 'Nightmare' }, category: 'daily', cap: 2, reset: 'daily', refill: null } },
  gains: {}, pets: { 1: { name: { en: 'Pet One' }, genus: 'Fera' } }, pet_souls: [5, 25, 75], pet_regions: {}, pet_monsters: {},
};
const nowUs = Date.now() * 1000;
const ch = (name: string, left: number, classCode = 3): Character => ({
  server: 7, name, class_code: classCode, level: 40, combat_power: 1234, item_level: 1500,
  counters: [{ id: 1, value: left, ms: false, updated_us: nowUs }], last_seen_us: nowUs,
});

async function render(status: CaptureStatus, chars: Character[], game = false) {
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => gd })));
  const restartAsAdmin = vi.fn(async () => false);
  TestBed.configureTestingModule({
    providers: [{ provide: CharactersService, useValue: { characters: signal(chars), status: signal(status), game: signal(game), radar: signal(null), meter: signal(null), fights: signal([]), restartAsAdmin } }],
  });
  const f = TestBed.createComponent(Dashboard);
  await new Promise(r => setTimeout(r)); // let loadGamedata resolve
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement, restartAsAdmin };
}
const cardNames = (el: HTMLElement) => [...el.querySelectorAll('.card .name')].map(n => n.textContent?.trim());

describe('Dashboard', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('shows combat power and GS on the card', async () => {
    const { el } = await render({ id: 'running', detail: null }, [ch('Alpha', 0), { ...ch('Beta', 0), item_level: null }]);
    const stats = [...el.querySelectorAll('.card')].map(c => [...c.querySelectorAll('.stat .v')].map(v => v.textContent?.trim()));
    expect(stats).toEqual([['1,234', '1,500'], ['1,234', '–']]);
  });

  it('sorts cards by what is left, names the class and counts the characters', async () => {
    const { el } = await render({ id: 'running', detail: null }, [ch('Alpha', 0), ch('Beta', 2)]);
    expect(cardNames(el)).toEqual(['Beta', 'Alpha']);
    expect(el.textContent).toContain('Templar');
    expect(el.querySelector('.meta')?.textContent).toContain('2 characters');
    expect(el.querySelector('.resets')?.textContent).toContain('Daily reset in');
    expect(el.querySelector('.resets')?.textContent).toContain('Weekly reset in');
    expect(el.querySelector('.banner')).toBeNull();
  });

  it('draws the class glyph, or the generic one with the class code when the class is unknown', async () => {
    const { el } = await render({ id: 'running', detail: null }, [ch('Archer', 1, 4), ch('Odd', 1, 99)]);
    const icon = (name: string) => [...el.querySelectorAll('.card')]
      .find(c => c.textContent?.includes(name))?.querySelector('.mark')?.getAttribute('data-icon');
    expect(icon('Archer')).toBe('ranger');
    expect(icon('Odd')).toBe('unknown');
    expect(el.textContent).toContain('Class 99');
  });

  it('opens a character from its card and returns on Escape', async () => {
    const { f, el } = await render({ id: 'running', detail: null }, [ch('Alpha', 1), ch('Beta', 2)]);
    [...el.querySelectorAll<HTMLButtonElement>('.card')].find(b => b.textContent?.includes('Alpha'))!.click();
    f.detectChanges();
    expect(el.querySelector('app-character h2')?.textContent).toContain('Alpha');
    expect(el.querySelectorAll('.card').length).toBe(0);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    f.detectChanges();
    expect(el.querySelector('app-character')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(2);
  });

  it('opens the pets screen from the header when a character reported pets, and returns on Escape', async () => {
    const withPets: Character = { ...ch('Alpha', 1), pets: { active: 1, updated_us: nowUs, list: [{ id: 1, level: 1, souls: 3 }] } };
    const { f, el } = await render({ id: 'running', detail: null }, [withPets]);
    el.querySelector<HTMLButtonElement>('.pets-btn')!.click();
    f.detectChanges();
    expect(el.querySelector('app-pets h2')?.textContent).toContain('Pets');
    expect(el.querySelector('app-pets')?.textContent).toContain('Pet One');
    expect(el.querySelectorAll('.card').length).toBe(0);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    f.detectChanges();
    expect(el.querySelector('app-pets')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(1);
  });

  it('opens the meter screen from the header, marks it, and returns on Escape', async () => {
    const { f, el } = await render({ id: 'running', detail: null }, [ch('Alpha', 1)]);
    el.querySelector<HTMLButtonElement>('.meter-btn')!.click();
    f.detectChanges();
    expect(el.querySelector('app-meter h2')?.textContent).toContain('Meter');
    expect(el.querySelector('.meter-btn')!.getAttribute('aria-current')).toBe('page');
    expect(el.querySelector('.roster-btn')!.hasAttribute('aria-current')).toBe(false);
    expect(el.querySelectorAll('.card').length).toBe(0);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    f.detectChanges();
    expect(el.querySelector('app-meter')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(1);
  });

  it('always offers the pets screen and explains when no pets were seen yet', async () => {
    const { f, el } = await render({ id: 'running', detail: null }, [ch('Alpha', 1)]);
    el.querySelector<HTMLButtonElement>('.pets-btn')!.click();
    f.detectChanges();
    expect(el.querySelector('app-pets .empty')?.textContent).toContain('No pets yet');
  });

  it('goes back to the character selection from a character or from pets, and marks where the user is', async () => {
    const { f, el } = await render({ id: 'running', detail: null }, [ch('Alpha', 1), ch('Beta', 2)]);
    const roster = () => el.querySelector<HTMLButtonElement>('.roster-btn')!;
    const pets = () => el.querySelector<HTMLButtonElement>('.pets-btn')!;
    expect(roster().getAttribute('aria-current')).toBe('page');
    expect(pets().hasAttribute('aria-current')).toBe(false);

    [...el.querySelectorAll<HTMLButtonElement>('.card')].find(b => b.textContent?.includes('Alpha'))!.click();
    f.detectChanges();
    expect(el.querySelector('app-character')).not.toBeNull();
    roster().click();
    f.detectChanges();
    expect(el.querySelector('app-character')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(2);

    pets().click();
    f.detectChanges();
    expect(pets().getAttribute('aria-current')).toBe('page');
    expect(roster().hasAttribute('aria-current')).toBe(false);
    roster().click();
    f.detectChanges();
    expect(el.querySelector('app-pets')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(2);

    // a character opened, then pets on top of it: Characters still lands on the roster, not on the character
    el.querySelector<HTMLButtonElement>('.card')!.click();
    f.detectChanges();
    pets().click();
    f.detectChanges();
    roster().click();
    f.detectChanges();
    expect(el.querySelector('app-character')).toBeNull();
    expect(el.querySelectorAll('.card').length).toBe(2);
  });

  it('offers a restart when capture needs admin rights', async () => {
    const { el, restartAsAdmin } = await render({ id: 'not_admin', detail: null }, [ch('Alpha', 1)]);
    const button = el.querySelector<HTMLButtonElement>('.banner button')!;
    button.click();
    expect(restartAsAdmin).toHaveBeenCalledOnce();
    expect(cardNames(el)).toEqual(['Alpha']); // saved characters still show
  });

  it('shows an empty state without characters', async () => {
    const { el } = await render({ id: 'running', detail: null }, []);
    expect(el.querySelector('.empty')).not.toBeNull();
  });

  it('shows saved characters without offering capture when the state is in use', async () => {
    const { el } = await render({ id: 'state_in_use', detail: null }, [ch('Alpha', 1)], true);
    expect(el.querySelector('.banner')?.textContent).toContain('Another instance is saving');
    expect(el.querySelector('.pill')?.textContent).toContain('Capture off');
    expect(el.querySelectorAll('.pill').length).toBe(1); // no game badge without capture
    expect(el.querySelector('.banner button')).toBeNull();
    expect(cardNames(el)).toEqual(['Alpha']);
  });

  it('keeps capturing and shows cards until a save failure recovers', async () => {
    const { f, el } = await render({ id: 'save_failed', detail: 'disk full' }, [ch('Alpha', 1)]);
    expect(el.querySelector('.pill')?.textContent).toContain('Network connected');
    expect(el.querySelector('.banner')?.textContent).toContain('changes could not be saved');
    expect(el.querySelector('.detail')?.textContent).toBe('disk full');
    expect(el.querySelector('.banner button')).toBeNull();
    expect(cardNames(el)).toEqual(['Alpha']);
    TestBed.inject(CharactersService).status.set({ id: 'running', detail: null });
    f.detectChanges();
    expect(el.querySelector('.banner')).toBeNull();
    expect(el.querySelector('.pill')?.textContent).toContain('Network connected');
    expect(cardNames(el)).toEqual(['Alpha']);
  });

  it('shows whether the game is detected next to the network pill, only while capturing', async () => {
    const pills = (el: HTMLElement) => [...el.querySelectorAll('.pill')].map(p => p.textContent?.trim());
    const { f, el } = await render({ id: 'running', detail: null }, [ch('Alpha', 1)]);
    expect(pills(el)).toEqual(['Network connected', 'Game not detected']);
    TestBed.inject(CharactersService).game.set(true);
    f.detectChanges();
    expect(pills(el)).toEqual(['Network connected', 'Game detected']);
    expect(el.querySelector('.pill[data-game]')?.getAttribute('data-game')).toBe('true');
  });
});
