import type { Fight, MeterPlayer, Tally } from './characters.service';
import type { GameData } from './gamedata';
import { foldSkill, monsterName, myPlace, outcome, playerRows, pushRecent, recentRates, seconds, skillHue, skillIcon, skillName, skillRows, total } from './fight';

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

  it('ranks by taken or healing too, and sums that metric', () => {
    const f = fight([player(1, 100, { taken: { amount: 5, hits: 1 } }), player(2, 10, { taken: { amount: 50, hits: 2 }, healing: { amount: 9, hits: 1 } })]);
    expect(playerRows(f, 'taken').map(r => r.p.entity)).toEqual([2, 1]);
    expect(playerRows(f, 'taken')[0]).toMatchObject({ amount: 50, dps: 5 });
    expect(playerRows(f, 'healing')[0].p.entity).toBe(2);
    expect(total(f, 'taken')).toBe(55);
  });

  describe('current rate', () => {
    const at = (end: number, amounts: number[], start = 0) =>
      fight(amounts.map((a, i) => player(i + 1, a)), { start_us: start * 1e6, end_us: end * 1e6 });

    it('is the last few seconds, from the newest snapshot at or before the window', () => {
      const recent = [at(2, [20, 0]), at(5, [50, 0]), at(8, [200, 30])];
      const now = at(10, [300, 30]);
      // window 5 s: the base is the snapshot at 5 s; 250 more over 5 s for P1, 30 over 5 s for P2
      const rates = recentRates(now, recent);
      expect(rates.get(1)).toBe(50);
      expect(rates.get(2)).toBe(6);
    });

    it('is the average while the fight is younger than the window, and 0 for a player not in the base', () => {
      const now = at(4, [400, 80]);
      expect(recentRates(now, [at(2, [100])]).get(1)).toBe(100);
      expect(recentRates(now, [at(2, [100])]).get(2)).toBe(20);
    });

    it('keeps one snapshot before the window and forgets another fight', () => {
      const r1 = pushRecent([], at(1, [1]));
      const r2 = pushRecent(pushRecent(r1, at(4, [4])), at(9, [9]));
      expect(r2.map(f => f.end_us / 1e6)).toEqual([4, 9]);
      expect(pushRecent(r2, at(9, [10])).map(f => f.end_us / 1e6)).toEqual([4, 9]); // same last hit: replaced
      expect(pushRecent(r2, at(9, [10], 5))).toHaveLength(1); // another fight
      expect(pushRecent(r2, null)).toEqual([]);
    });
  });

  describe('outcome', () => {
    const boss = (start: number, hp: number | null, template = 5, stagger = true) =>
      fight([player(1, 10)], { start_us: start, target: { entity: 9, template, hp, max_hp: 100, stagger: stagger ? { state: 0, value: 0, max: 1 } : null } });

    it('is a win at 0 HP, else the n-th try in a row on that boss', () => {
      const history = [boss(1, 50), boss(2, 50, 6), boss(3, 40), boss(4, 30), boss(5, 0)];
      expect(outcome(history, history[4])).toEqual({ won: true, attempt: 0 });
      expect(outcome(history, history[3])).toEqual({ won: false, attempt: 2 });
      expect(outcome(history, history[2])).toEqual({ won: false, attempt: 1 }); // another boss in between
    });

    it('has none without a boss (no stagger gauge) or a target', () => {
      expect(outcome([], boss(1, 0, 5, false))).toBeNull();
      expect(outcome([], fight([player(1, 1)]))).toBeNull();
    });

    it('counts a win as the end of the tries', () => {
      const history = [boss(1, 0), boss(2, 50)];
      expect(outcome(history, history[1])).toEqual({ won: false, attempt: 1 });
    });
  });

  it('finds your rate and place, or none when you are not in the fight', () => {
    expect(myPlace(fight([player(1, 500), player(2, 100)]))).toEqual({ dps: 10, rank: 2 });
    expect(myPlace(fight([player(1, 500)]))).toBeNull();
  });

  it('gives each skill a stable hue, the same for its variants, and its icon only when listed', () => {
    expect(skillHue(14090130)).toBe(skillHue(14090000));
    expect(skillHue(14090000)).not.toBe(skillHue(14100000));
    expect(skillHue(99)).toBe(99 * 47 % 360);
    const gd = { skills: { '14090000': { name: { en: 'Slash' }, icon: 'slash' }, '1': { name: { en: 'X' } } } } as unknown as GameData;
    expect(skillIcon(gd, 14090000)).toBe('slash');
    expect(skillIcon(gd, 1)).toBeNull();
    expect(skillIcon(null, 1)).toBeNull();
  });
});
