import type { Fight, MeterPlayer, Tally } from './characters.service';
import type { GameData } from './gamedata';
import { foldSkill, monsterName, playerRows, seconds, skillName, skillRows } from './fight';

const tally = (amount: number, hits = 1, direct = hits, crits = 0): Tally => ({ amount, hits, direct, crits });

function player(entity: number, amount: number, extra: Partial<MeterPlayer> = {}): MeterPlayer {
  return {
    entity, name: `P${entity}`, class_code: null, dealt: tally(amount, 4, 4, 1), taken: { amount: 0, hits: 0 },
    healing: { amount: 0, hits: 0 }, overheal: 0, healing_received: { amount: 0, hits: 0 }, skills: [], targets: [], ...extra,
  };
}

const fight = (players: MeterPlayer[], extra: Partial<Fight> = {}): Fight =>
  ({ start_us: 0, end_us: 10_000_000, me: 2, players, target: null, ...extra });

describe('fight', () => {
  it('folds the variant of a skill and keeps small ids', () => {
    expect(foldSkill(14090130)).toBe(14090000);
    expect(foldSkill(14090000)).toBe(14090000);
    expect(foldSkill(9_999_999)).toBe(9_999_999);
  });

  it('lasts at least one second', () => {
    expect(seconds(fight([], { end_us: 500_000 }))).toBe(1);
    expect(seconds(fight([]))).toBe(10);
  });

  it('sorts players by damage with dps, share, bar width and crit rate', () => {
    const rows = playerRows(fight([player(1, 300), player(2, 100), player(3, 0, { dealt: tally(0, 0, 0, 0) })]));
    expect(rows.map(r => r.p.entity)).toEqual([1, 2, 3]);
    expect(rows[0].dps).toBe(30);
    expect(rows[0].share).toBeCloseTo(0.75);
    expect(rows[1].top).toBeCloseTo(1 / 3);
    expect(rows[0].crit).toBe(0.25);
    expect(rows[2].crit).toBe(0);
    expect(rows.map(r => r.me)).toEqual([false, true, false]);
  });

  it('has no NaN when nobody dealt damage', () => {
    const rows = playerRows(fight([player(1, 0, { dealt: tally(0, 0, 0, 0) })]));
    expect(rows[0]).toMatchObject({ share: 0, top: 0, dps: 0, crit: 0 });
  });

  it('adds the variants of a skill together', () => {
    const p = player(1, 100, {
      skills: [
        { id: 14090130, ...tally(60, 3, 3, 1) },
        { id: 14090000, ...tally(20, 1, 1, 1) },
        { id: 777, ...tally(20, 2, 1, 0) },
      ],
    });
    const rows = skillRows(p);
    expect(rows.map(r => r.id)).toEqual([14090000, 777]);
    expect(rows[0]).toMatchObject({ amount: 80, hits: 4, direct: 4, crits: 2, share: 0.8, crit: 0.5 });
    expect(rows[1].crit).toBe(0);
  });

  it('names skills and monsters, with the pet monsters as a fallback, or null', () => {
    const gd = {
      skills: { '14090000': { name: { en: 'Slash' } } },
      monsters: { '5': { name: { en: 'Boss' } } },
      pet_monsters: { '7': { name: { en: 'Rafflesia' } } },
    } as unknown as GameData;
    expect(skillName(gd, 14090000, 'en')).toBe('Slash');
    expect(skillName(gd, 1, 'en')).toBeNull();
    expect(skillName(null, 1, 'en')).toBeNull();
    expect(monsterName(gd, 5, 'en')).toBe('Boss');
    expect(monsterName(gd, 7, 'en')).toBe('Rafflesia');
    expect(monsterName(gd, 0, 'en')).toBeNull();
  });
});
