import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { vi } from 'vitest';
import { Meter } from './meter';
import { CaptureStatus, CharactersService, Fight, Meter as MeterData, MeterPlayer, Tally } from '../characters.service';
import { GameData } from '../gamedata';

const gd = {
  version: 1, weekly_reset_day: 3, classes: { 30: { name: { en: 'Cleric' }, icon: 'cleric', color: '#d4a03c' }, 23: { name: { en: 'Spiritmaster' } } }, counters: {}, gains: {}, pets: {}, pet_souls: [], pet_regions: {},
  pet_monsters: {}, skills: { 14090000: { name: { en: 'Slash' } }, 15000000: { name: { en: 'Arrow' }, icon: 'icon_arrow' } }, monsters: { 5: { name: { en: 'Golem' } } },
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

function render(opts: { current?: Fight | null; fights?: Fight[]; recent?: Fight[]; status?: CaptureStatus; compact?: boolean } = {}) {
  const clearMeter = vi.fn(async () => undefined);
  const meter = signal<MeterData | null>({ me: 1, current: opts.current ?? null, fights: opts.fights?.length ?? 0 });
  const fights = signal(opts.fights ?? []);
  const recent = signal(opts.recent ?? []);
  TestBed.configureTestingModule({
    providers: [{ provide: CharactersService, useValue: { meter, fights, recent, status: signal(opts.status ?? { id: 'running', detail: null }), clearMeter } }],
  });
  const f = TestBed.createComponent(Meter);
  f.componentRef.setInput('gamedata', gd);
  f.componentRef.setInput('compact', opts.compact ?? false);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement, clearMeter, meter, fights, recent };
}
const names = (el: HTMLElement) => [...el.querySelectorAll('.board .line .n')].map(n => n.textContent?.trim());

describe('Meter', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('says so when there is no fight yet', () => {
    const { el } = render();
    expect(el.querySelector('.empty')).not.toBeNull();
    expect(el.querySelector('table')).toBeNull();
  });

  for (const compact of [false, true]) {
    it(`shows a known combat power after the name (${compact ? 'overlay list' : 'page'})`, () => {
      const { el } = render({ current: fight(0, [other, { ...mine, combat_power: 12345 }]), compact });
      expect([...el.querySelectorAll('.cp')].map(n => n.textContent?.trim())).toEqual(['12,345']);
    });
  }

  it('lists the players of the open fight by damage, the local one marked, with unknown names covered', () => {
    const { el } = render({ current: fight(0, [other, mine], { target: golem }) });
    expect(names(el)).toEqual(['Alpha', 'Unknown player']);
    expect(el.querySelector('.board li.me')?.textContent).toContain('Alpha');
    expect(el.querySelector('.tag')?.textContent).toContain('In a fight');
    expect(el.querySelector('.boss-line')?.textContent).toContain('Golem');
    expect(el.querySelector('.boss-line .hp')?.textContent).toContain('25%');
  });

  it('shows a target that has no HP, max HP or stagger yet (null on the wire)', () => {
    const bare = { entity: 9, template: 5, hp: null, max_hp: null, stagger: null };
    const { f, el, meter } = render({ current: fight(0, [mine], { target: bare }) });
    expect(names(el)).toEqual(['Alpha']);
    expect(el.querySelector('.boss-line')?.textContent).toContain('Golem');
    expect(el.querySelector('.boss-line')?.textContent).not.toContain('%');
    // HP known, no max (the spawn was missed) and no stagger gauge
    meter.set({ me: 1, current: fight(0, [mine], { target: { ...bare, hp: 50 } }), fights: 0 });
    f.detectChanges();
    expect(el.querySelector('.boss-line')?.textContent).not.toContain('%');
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

  it('switches fights from the list and back to the live card', () => {
    const { f, el } = render({ fights: [fight(0, [other]), fight(100, [mine])] });
    const cards = () => [...el.querySelectorAll<HTMLButtonElement>('.fights .entry')];
    expect(cards().length).toBe(3);
    expect(cards()[0].getAttribute('aria-current')).toBe('true');
    cards()[2].click(); // the oldest
    f.detectChanges();
    expect(names(el)).toEqual(['Unknown player']);
    expect(cards()[2].getAttribute('aria-current')).toBe('true');
    cards()[0].click();
    f.detectChanges();
    expect(names(el)).toEqual(['Alpha']);
  });

  it('describes each saved fight: boss, result, length, group rate and your place, grouped under the day', () => {
    const boss = { entity: 9, template: 5, hp: 0, max_hp: 200, stagger: { state: 0, value: 0, max: 0 } };
    const alive = { ...boss, hp: 50 };
    const t0 = Date.now() / 1000 - 3600; // an hour ago, in seconds
    const { el } = render({ fights: [fight(t0, [mine], { target: alive }), fight(t0 + 100, [other, mine], { target: alive }), fight(t0 + 200, [mine], { target: boss })] });
    const cards = [...el.querySelectorAll('.cards .entry')];
    expect(el.querySelectorAll('.cards .day').length).toBe(1);
    expect(cards.map(c => c.querySelector('.badge')?.textContent?.trim())).toEqual(['Victory', 'Attempt 2', 'Attempt 1']);
    expect(cards[0].textContent).toContain('Golem');
    expect(cards[0].querySelector('.sub')?.textContent?.replace(/\s+/g, ' ')).toMatch(/0:10 · 30\/s group · you 30\/s #1/);
    expect(cards[1].querySelector('.sub')?.textContent).toContain('#1');
  });

  it('shows the group rate big with the current one beside it while the fight is open, and only the average once it is over', () => {
    const base = fight(0, [{ ...mine, dealt: tally(100) }], { end_us: 5e6 });
    const now = fight(0, [{ ...mine, dealt: tally(600) }], { end_us: 10e6 });
    const { f, el, meter, fights } = render({ current: now, recent: [base, now] });
    expect(el.querySelector('.hero .rate > b')?.textContent).toBe('60'); // 600 over 10 s
    expect(el.querySelector('.nowrate')?.textContent).toContain('100/s'); // 500 over the last 5 s
    expect([...el.querySelectorAll('.cols span')].map(c => c.textContent?.trim())).toContain('Now');
    expect(el.querySelector('.board .line .now')?.textContent).toBe('100/s');
    fights.set([now]);
    meter.set({ me: 1, current: null, fights: 1 });
    f.detectChanges();
    expect(el.querySelector('.hero .rate > b')?.textContent).toBe('60');
    expect(el.querySelector('.nowrate')).toBeNull();
    expect(el.querySelector('.board .line .now')).toBeNull();
  });

  it('keeps the average only on a finished fight', () => {
    const { el } = render({ fights: [fight(0, [mine])] });
    expect(el.querySelector('.nowrate')).toBeNull();
    expect(el.querySelector('.board .line .now')).toBeNull();
    expect(el.querySelector('.hero .rate > b')?.textContent).toBe('30');
  });

  it('ranks by damage taken or healing from the tabs', () => {
    const healer = player(2, null, 10, { healing: { amount: 900, hits: 3 }, taken: { amount: 5, hits: 1 } });
    const { f, el } = render({ fights: [fight(0, [mine, healer])] });
    const tab = (i: number) => el.querySelectorAll<HTMLButtonElement>('.tabs button')[i];
    expect(names(el)).toEqual(['Alpha', 'Unknown player']);
    tab(2).click();
    f.detectChanges();
    expect(names(el)).toEqual(['Unknown player', 'Alpha']);
    expect(el.querySelector('.board .line .amt')?.textContent).toBe('900');
    expect(tab(2).getAttribute('aria-pressed')).toBe('true');
    tab(1).click();
    f.detectChanges();
    expect(names(el)).toEqual(['Alpha', 'Unknown player']);
    expect(el.querySelector('.board .line .amt')?.textContent).toBe('7');
  });

  it('shows the current rate in the overlay while the fight is open', () => {
    const base = fight(0, [{ ...mine, dealt: tally(100) }], { end_us: 5e6 });
    const now = fight(0, [{ ...mine, dealt: tally(600) }], { end_us: 10e6 });
    const { el } = render({ current: now, recent: [base, now], compact: true });
    expect(el.querySelector('.row .now')?.textContent).toBe('100/s');
    expect(el.querySelector('.row .v')?.textContent).toBe('60/s');
  });

  it('shows the boss health as numbers, a bar and a percentage in the overlay', () => {
    const { el } = render({ current: fight(0, [mine], { target: golem }), compact: true });
    expect(el.querySelector('.line3 .hpbar .hpnum')?.textContent?.trim()).toBe('50 / 200');
    expect(el.querySelector('.line2 .hpnum')).toBeNull();
    expect(el.querySelector<HTMLElement>('.line3 .hpbar .hp-fill')!.style.width).toBe('25%');
    expect(el.querySelector('.line3 .hppct')?.textContent?.trim()).toBe('25%');
  });

  for (const compact of [false, true]) {
    it(`keeps full HP numbers inside the bar at full health and death (${compact ? 'overlay' : 'page'})`, () => {
      const { f, el, meter } = render({ current: fight(0, [mine], { target: { ...golem, hp: 99_999_999, max_hp: 99_999_999 } }), compact });
      const bar = () => el.querySelector(compact ? '.hpbar' : '.boss-line .hp')!;
      expect(bar().textContent).toContain('99,999,999 / 99,999,999');
      expect(bar().getAttribute('aria-label')).toContain('100%');
      expect(bar().querySelector<HTMLElement>('.hp-fill')!.style.width).toBe('100%');
      meter.set({ me: 1, current: fight(0, [mine], { target: { ...golem, hp: 0 } }), fights: 0 });
      f.detectChanges();
      expect(bar().textContent).toContain('0 / 200');
      expect(bar().querySelector<HTMLElement>('.hp-fill')!.style.width).toBe('0%');
      if (!compact) expect(bar().textContent).toContain('Dead');
      meter.set({ me: 1, current: fight(0, [mine], { target: { ...golem, max_hp: null } }), fights: 0 });
      f.detectChanges();
      expect(el.querySelector(compact ? '.hpbar' : '.boss-line .hp')).toBeNull();
    });
  }

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

  it('draws each skill with its picture or an initial tile, a bar to the top skill and a slice of the damage mix', () => {
    const p = { ...mine, skills: [{ id: 15000130, ...tally(200) }, { id: 14090130, ...tally(100) }, { id: 99, ...tally(50) }] };
    const { f, el } = render({ current: fight(0, [p]) });
    el.querySelector<HTMLButtonElement>('.board .line')!.click();
    f.detectChanges();
    const rows = [...el.querySelectorAll<HTMLElement>('.detail .ranked tbody tr')].filter(r => r.querySelector('.sk'));
    expect(rows[0].querySelector('img.sk')?.getAttribute('src')).toBe('skills/icon_arrow.png'); // named with a picture
    expect(rows[1].querySelector('span.sk')?.getAttribute('data-l')).toBe('S'); // Slash, no picture
    expect(rows[2].querySelector('span.sk')?.getAttribute('data-l')).toBe('?'); // not in the game data
    expect(rows.map(r => r.style.getPropertyValue('--p'))).toEqual(['100', '50', '25']);
    expect(el.querySelectorAll('.detail .mix span').length).toBe(3);
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
