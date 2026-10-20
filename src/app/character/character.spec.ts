import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { Character } from '../characters.service';
import { Item } from '../gamedata';
import { CharacterDetail, CharacterRow } from './character';

const H = 3_600_000;
const NOW = Date.UTC(2026, 9, 14, 8);
const refill = { amount: 1, every_ms: 12 * H };
const item = (o: Partial<Item> & Pick<Item, 'id' | 'category'>): Item =>
  ({ name: { en: `Counter ${o.id}` }, left: 0, cap: 10, ms: false, fullAt: null, refill: null, ...o });
const row = (items: Item[], c: Partial<Character> = {}): CharacterRow => ({
  key: '7:Alpha', icon: 'ranger', cls: 'Ranger',
  c: { server: 7, name: 'Alpha', class_code: 14, level: 60, combat_power: 18296, item_level: 1500, counters: [], last_seen_us: (NOW - 2 * H) * 1000, ...c },
  s: { daily: { left: 0, cap: 0 }, weekly: { left: 0, cap: 0 }, items, score: 0 },
});

function render(r: CharacterRow) {
  const f = TestBed.createComponent(CharacterDetail);
  f.componentRef.setInput('row', r);
  f.componentRef.setInput('now', NOW);
  f.componentRef.setInput('dailyReset', NOW + 23 * H);
  f.componentRef.setInput('weeklyReset', NOW + 6 * 24 * H);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement };
}
const names = (el: HTMLElement, sec: string) =>
  [...el.querySelectorAll(`[data-sec="${sec}"] .row .n`)].map(e => e.textContent?.trim());

describe('CharacterDetail', () => {
  it('puts full tickets first, then the soonest to fill, and lists the full ones under Spend first', () => {
    const { el } = render(row([
      item({ id: 1, category: 'ticket', left: 5, refill, fullAt: NOW + 10 * H }),
      item({ id: 2, category: 'ticket', left: 10, refill }),
      item({ id: 3, category: 'energy', left: 8, refill, fullAt: NOW + 3 * H }),
    ]));
    expect(names(el, 'tickets')).toEqual(['Counter 2', 'Counter 3', 'Counter 1']);
    expect([...el.querySelectorAll('.attn li')].map(li => li.textContent)).toEqual([expect.stringContaining('Counter 2')]);
    expect(el.querySelector('.tag')?.textContent).toContain('Full, wasting refills');
    expect(el.querySelector('[data-sec="tickets"] .meta')?.textContent).toContain('1 full');
  });

  it('marks itself compact for the overlay, which hides Back and the hero by CSS', () => {
    const { f, el } = render(row([]));
    expect(el.classList).not.toContain('compact');
    f.componentRef.setInput('compact', true);
    f.detectChanges();
    expect(el.classList).toContain('compact');
  });

  it('shows the refill rhythm and a countdown to full', () => {
    const { el } = render(row([item({ id: 1, category: 'ticket', left: 5, refill, fullAt: NOW + 3 * H })]));
    const r = el.querySelector('.row')!;
    expect(r.querySelector('.sub')?.textContent).toContain('+1 every 12h');
    expect(r.querySelector('.st')?.textContent).toContain('full in 3h 0m');
    expect(r.classList).toContain('is-soon');
  });

  it('sorts weekly by most left and leaves done rows last', () => {
    const { el } = render(row([
      item({ id: 4, category: 'weekly', left: 0, cap: 5 }),
      item({ id: 5, category: 'weekly', left: 2 }),
      item({ id: 6, category: 'weekly', left: 9 }),
    ]));
    expect(names(el, 'weekly')).toEqual(['Counter 6', 'Counter 5', 'Counter 4']);
    expect(el.querySelectorAll('[data-sec="weekly"] .row')[2].classList).toContain('is-done');
    expect(el.querySelector('[data-sec="weekly"] .meta')?.textContent).toContain('6d 0h');
  });

  it('shows time counters in hours', () => {
    const { el } = render(row([item({ id: 7, category: 'weekly', left: 16_200_000, cap: 25_200_000, ms: true })]));
    const val = el.querySelector('[data-sec="weekly"] .val')?.textContent ?? '';
    expect(val).toContain('2.5h');
    expect(val).toContain('7h');
  });

  it('shows what was done for counters and what is left for tickets', () => {
    const { el } = render(row([
      item({ id: 1, category: 'ticket', left: 3, refill, fullAt: NOW + 3 * H }),
      item({ id: 2, category: 'daily', left: 8 }),
    ]));
    expect(el.querySelector('[data-sec="tickets"] .val')?.textContent).toContain('3/10');
    expect(el.querySelector('[data-sec="daily"] .val')?.textContent).toContain('2/10');
    expect(el.querySelector('[data-sec="daily"] .bar')?.getAttribute('aria-valuenow')).toBe('2');
  });

  it('shows the daily empty state and the daily countdown', () => {
    const { el } = render(row([]));
    expect(el.querySelector('[data-sec="daily"] .daily-empty')?.textContent).toContain('not decoded yet');
    expect(el.querySelector('[data-sec="daily"] .meta')?.textContent).toContain('23h 0m');
    expect(el.querySelector('.attn')?.textContent).toContain('Nothing is wasting refills');
    expect(el.querySelector('.hero')?.textContent).toContain('Alpha');
  });

  it('shows GS, or a dash while it is unknown', () => {
    const gs = (el: HTMLElement) => el.querySelectorAll('.stat .v')[1]?.textContent?.trim();
    expect(gs(render(row([])).el)).toBe('1,500');
    expect(gs(render(row([], { item_level: null })).el)).toBe('–');
  });

  it('lists daily counters when the game data has them', () => {
    const { el } = render(row([item({ id: 8, category: 'daily', left: 1, cap: 2 })]));
    expect(names(el, 'daily')).toEqual(['Counter 8']);
    expect(el.querySelector('.daily-empty')).toBeNull();
  });

  it('lists supply and kinah gains in the Daily section, supply weekly in Weekly, with separators', () => {
    const { el } = render(row([
      item({ id: 'supply_daily', category: 'daily', left: 7 }),
      item({ id: 'kinah_today', category: 'daily', left: 750_000, cap: 1_000_000 }),
      item({ id: 'supply_weekly', category: 'weekly', left: 9 }),
    ]));
    expect(names(el, 'daily')).toEqual(['Counter kinah_today', 'Counter supply_daily']);
    expect(names(el, 'weekly')).toEqual(['Counter supply_weekly']);
    expect(el.querySelector('[data-sec="daily"] .val')?.textContent).toContain('250,000/1,000,000');
    expect(el.querySelector('.daily-empty')).toBeNull();
  });

  it('emits back from the button', () => {
    const { f, el } = render(row([]));
    const spy = vi.fn();
    f.componentInstance.back.subscribe(spy);
    el.querySelector<HTMLButtonElement>('.back')!.click();
    expect(spy).toHaveBeenCalledOnce();
  });
});
