import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { Meter } from './meter';
import { CaptureStatus, CharactersService, Fight, Meter as MeterData, MeterPlayer, Tally } from '../characters.service';
import { GameData } from '../gamedata';

const gd = {
  version: 1, weekly_reset_day: 3, classes: { 30: { name: { en: 'Cleric' }, icon: 'cleric', color: '#d4a03c' }, 23: { name: { en: 'Spiritmaster' } } }, counters: {}, gains: {}, pets: {}, pet_souls: [], pet_regions: {},
  pet_monsters: {}, skills: { 14090000: { name: { en: 'Slash' } } }, monsters: { 5: { name: { en: 'Golem' } } },
} as unknown as GameData;

const tally = (amount: number, hits = 2, crits = 1): Tally => ({ amount, hits, direct: hits, crits });
const player = (entity: number, name: string | null, amount: number, extra: Partial<MeterPlayer> = {}): MeterPlayer => ({
  entity, name, class_code: null, dealt: tally(amount), taken: { amount: 7, hits: 1 }, healing: { amount: 0, hits: 0 }, overheal: 0,
  healing_received: { amount: 0, hits: 0 }, skills: [], targets: [], ...extra,
});
const fight = (start: number, players: MeterPlayer[], extra: Partial<Fight> = {}): Fight =>
  ({ start_us: start * 1e6, end_us: (start + 10) * 1e6, me: 1, players, target: null, ...extra });

const mine = player(1, 'Alpha', 300, {
  class_code: 30,
  skills: [{ id: 14090130, ...tally(200) }, { id: 99, ...tally(100) }],
  targets: [{ template: 5, ...tally(250) }, { template: 0, ...tally(50) }],
});
const other = player(2, null, 100);
const golem = { entity: 9, template: 5, hp: 50, max_hp: 200, stagger: { state: 0, value: 0, max: 0 } };

function render(opts: { current?: Fight | null; fights?: Fight[]; status?: CaptureStatus; compact?: boolean } = {}) {
  const clearMeter = vi.fn(async () => undefined);
  const meter = signal<MeterData | null>({ me: 1, current: opts.current ?? null, fights: opts.fights?.length ?? 0 });
  const fights = signal(opts.fights ?? []);
  TestBed.configureTestingModule({
    providers: [{ provide: CharactersService, useValue: { meter, fights, status: signal(opts.status ?? { id: 'running', detail: null }), clearMeter } }],
  });
  const f = TestBed.createComponent(Meter);
  f.componentRef.setInput('gamedata', gd);
  f.componentRef.setInput('compact', opts.compact ?? false);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement, clearMeter, meter, fights };
}
const names = (el: HTMLElement) => [...el.querySelectorAll('.board .line .n')].map(n => n.textContent?.trim());

describe('Meter', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('says so when there is no fight yet', () => {
    const { el } = render();
    expect(el.querySelector('.empty')).not.toBeNull();
    expect(el.querySelector('table')).toBeNull();
  });

  it('lists the players of the open fight by damage, the local one marked, with unknown names covered', () => {
    const { el } = render({ current: fight(0, [other, mine], { target: golem }) });
    expect(names(el)).toEqual(['Alpha', 'Unknown player']);
    expect(el.querySelector('.board li.me')?.textContent).toContain('Alpha');
    expect(el.querySelector('.tag')?.textContent).toContain('In a fight');
    expect(el.querySelector('.foe')?.textContent).toContain('Golem');
    expect(el.querySelector('.foe')?.textContent).toContain('25%');
  });

  it('shows a target that has no HP, max HP or stagger yet (null on the wire)', () => {
    const bare = { entity: 9, template: 5, hp: null, max_hp: null, stagger: null };
    const { f, el, meter } = render({ current: fight(0, [mine], { target: bare }) });
    expect(names(el)).toEqual(['Alpha']);
    expect(el.querySelector('.foe')?.textContent).toContain('Golem');
    expect(el.querySelector('.foe')?.textContent).not.toContain('%');
    // HP known, no max (the spawn was missed) and no stagger gauge
    meter.set({ me: 1, current: fight(0, [mine], { target: { ...bare, hp: 50 } }), fights: 0 });
    f.detectChanges();
    expect(el.querySelector('.foe')?.textContent).not.toContain('%');
  });

  it('follows the open fight as updates arrive', () => {
    const { f, el, meter } = render({ current: fight(0, [mine]) });
    expect(el.querySelector('.board li.me .amt')?.textContent).toContain('300');
    meter.set({ me: 1, current: fight(0, [{ ...mine, dealt: tally(1_500_000) }]), fights: 0 });
    f.detectChanges();
    expect(el.querySelector('.board li.me .amt')?.textContent).toContain('1.5M');
  });

  it('colours each bar by the class, and leaves the accent to a class without a colour', () => {
    const { el } = render({ current: fight(0, [mine, player(2, 'Bravo', 50, { class_code: 23 }), other]) });
    const colours = [...el.querySelectorAll<HTMLElement>('.board .line')].map(b => b.style.getPropertyValue('--c'));
    expect(colours).toEqual(['#d4a03c', '', '']);
  });

  it('shows the class icon of each player, the generic one when the class has none or is unknown', () => {
    const { el } = render({ current: fight(0, [mine, player(2, 'Bravo', 50, { class_code: 23 }), other]) });
    const icons = [...el.querySelectorAll('.board .line .ico')].map(i => i.getAttribute('data-icon'));
    expect(icons).toEqual(['cleric', 'unknown', 'unknown']);
  });

  it('colours the compact bars too', () => {
    const { el } = render({ current: fight(0, [mine]), compact: true });
    expect(el.querySelector<HTMLElement>('.row')!.style.getPropertyValue('--c')).toBe('#d4a03c');
  });

  it('follows the last finished fight when none is open', () => {
    const { el } = render({ fights: [fight(0, [other]), fight(100, [mine])] });
    expect(names(el)).toEqual(['Alpha']);
    expect(el.querySelector('.tag')?.textContent).toContain('Last fight');
  });

  it('switches fights from the selector and back to the latest', () => {
    const { f, el } = render({ fights: [fight(0, [other]), fight(100, [mine])] });
    const select = el.querySelector('select')!;
    expect([...select.options].length).toBe(3);
    select.value = String(0);
    select.dispatchEvent(new Event('change'));
    f.detectChanges();
    expect(names(el)).toEqual(['Unknown player']);
    select.value = '';
    select.dispatchEvent(new Event('change'));
    f.detectChanges();
    expect(names(el)).toEqual(['Alpha']);
  });

  it('ranks the players on one short line each, with the extra figures kept for the detail', () => {
    const { el } = render({ current: fight(0, [other, { ...mine, dealt: tally(5_710_000, 40, 16), taken: { amount: 4200, hits: 3 } }]) });
    const line = el.querySelector('.board li')!;
    expect(line.querySelector('.rank')?.textContent).toBe('1');
    expect(line.querySelector('.amt')?.textContent).toBe('5.71M');
    expect(line.querySelector('.rate')?.textContent).toMatch(/k\/s$/);
    expect(line.querySelector('.pct')?.textContent).toMatch(/%$/);
    expect(line.textContent).not.toContain('4,200');
    expect(el.querySelector('.detail')).toBeNull();
    expect(el.querySelector('.board li:nth-child(2) .rank')?.textContent).toBe('2');
  });

  it('opens a player to show the extra figures, skills (folded, named, or numbered) and targets', () => {
    const { f, el } = render({ current: fight(0, [mine]) });
    expect(el.querySelector('.detail')).toBeNull();
    el.querySelector<HTMLButtonElement>('.board .line')!.click();
    f.detectChanges();
    const tiles = Object.fromEntries([...el.querySelectorAll('.stats div')].map(d => [d.querySelector('dt')?.textContent?.trim(), d.querySelector('dd')?.textContent?.trim()]));
    expect(tiles).toMatchObject({ Class: 'Cleric', Taken: '7', Crit: '50%', Damage: '300' });
    const rows = [...el.querySelectorAll('.detail section')].map(s => [...s.querySelectorAll('tbody th')].map(t => t.textContent?.trim()));
    expect(rows).toEqual([['Slash', 'Skill 99'], ['Golem', 'Unknown monster']]);
    el.querySelector<HTMLButtonElement>('.board .line')!.click();
    f.detectChanges();
    expect(el.querySelector('.detail')).toBeNull();
  });

  it('shows the latest fight as a compact list without the table or the controls', () => {
    const { el } = render({ current: fight(0, [other, mine]), compact: true });
    expect(el.classList).toContain('compact');
    expect([...el.querySelectorAll('.row .n')].map(n => n.textContent?.trim())).toEqual(['Alpha', 'Unknown player']);
    expect(el.querySelector('.row.me')?.textContent).toContain('Alpha');
    expect(el.querySelector('table, select, .btn')).toBeNull();
  });

  it('clears the history after a confirmation', async () => {
    const { f, el, clearMeter } = render({ fights: [fight(0, [mine])] });
    const clear = () => el.querySelector<HTMLButtonElement>('.bar .btn')!;
    clear().click();
    f.detectChanges();
    expect(clearMeter).not.toHaveBeenCalled();
    expect(el.querySelector('.ask')).not.toBeNull();
    el.querySelector<HTMLButtonElement>('.btn.danger')!.click();
    await f.whenStable();
    f.detectChanges();
    expect(clearMeter).toHaveBeenCalledOnce();
    expect(el.querySelector('.ask')).toBeNull();
  });

  it('cannot clear without saved fights or while capture is off', () => {
    expect(render().el.querySelector<HTMLButtonElement>('.bar .btn')!.disabled).toBe(true);
    TestBed.resetTestingModule();
    const { el } = render({ fights: [fight(0, [mine])], status: { id: 'not_admin', detail: null } });
    expect(el.querySelector<HTMLButtonElement>('.bar .btn')!.disabled).toBe(true);
  });
});
