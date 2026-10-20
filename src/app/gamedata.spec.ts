import { CounterDef, GameData, clampBox, currentRegion, farmMap, fitBox, followBox, fullAt, lastReset, leftNow, liveMonsters, liveRegion, nextReset, petCollection, pick, soulGain, span, summarize } from './gamedata';
import { Character, CounterValue, Radar } from './characters.service';
import shipped from '../../public/gamedata.json';

const H = 3_600_000;
const at = (d: number, h: number, m = 0) => Date.UTC(2026, 9, d, h, m);
const val = (id: number, value: number, updatedMs: number): CounterValue => ({ id, value, ms: false, updated_us: updatedMs * 1000 });

const daily: CounterDef = { name: { en: 'Nightmare' }, category: 'daily', cap: 2, reset: 'daily', refill: null };
const weekly: CounterDef = { name: { en: 'Raid' }, category: 'weekly', cap: 3, reset: 'weekly', refill: null };
const ticket: CounterDef = { name: { en: 'Ticket' }, category: 'ticket', cap: 7, reset: null, refill: { amount: 1, every_ms: 3 * H } };

describe('lastReset', () => {
  it('daily is today 07:00 UTC from 07:00 on, yesterday before', () => {
    expect(lastReset('daily', at(14, 7), 3)).toBe(at(14, 7));
    expect(lastReset('daily', at(14, 6, 59), 3)).toBe(at(13, 7));
  });
  it('weekly steps back to the reset weekday', () => {
    expect(lastReset('weekly', at(14, 8), 3)).toBe(at(14, 7));     // Wednesday after reset
    expect(lastReset('weekly', at(14, 6), 3)).toBe(at(7, 7));      // Wednesday before reset
    expect(lastReset('weekly', at(17, 12), 3)).toBe(at(14, 7));    // Saturday
  });
});

describe('nextReset', () => {
  it('is the next 07:00 UTC, and the next reset weekday', () => {
    expect(nextReset('daily', at(14, 6, 59), 3)).toBe(at(14, 7));
    expect(nextReset('daily', at(14, 7), 3)).toBe(at(15, 7));
    expect(nextReset('weekly', at(14, 8), 3)).toBe(at(21, 7));
    expect(nextReset('weekly', at(17, 12), 3)).toBe(at(21, 7));
  });
});

describe('span', () => {
  it('shows the two largest units, down to minutes, never negative', () => {
    expect(span(3 * 86_400_000 + 4 * H, 'en')).toBe('3d 4h');
    expect(span(4 * H + 12 * 60_000, 'en')).toBe('4h 12m');
    expect(span(35 * 60_000, 'en')).toBe('35m');
    expect(span(0, 'en')).toBe('0m');
    expect(span(-5000, 'en')).toBe('0m');
  });
});

// Refills land on 07:00 UTC + k * every_ms (measured against the game: 3 h energy, +N a day keys).
const banked: CounterDef = { name: { en: 'Banked' }, category: 'ticket', cap: 14, reset: null, refill: { amount: 2, every_ms: 24 * H } };

describe('leftNow', () => {
  it('returns to cap after a reset that passed since the update', () => {
    expect(leftNow(daily, val(1, 0, at(14, 6)), at(14, 8), 3)).toBe(2);
    expect(leftNow(daily, val(1, 0, at(14, 7, 30)), at(14, 8), 3)).toBe(0);
    expect(leftNow(weekly, val(2, 1, at(13, 9)), at(14, 8), 3)).toBe(3);
  });
  it('adds one amount per refill boundary crossed, up to cap, never lowering a value above cap', () => {
    // boundaries 01:00, 04:00 and 07:00 lie between 00:00 and 07:00
    expect(leftNow(ticket, val(3, 2, at(14, 0)), at(14, 7), 3)).toBe(5);
    expect(leftNow(ticket, val(3, 6, at(14, 0)), at(15, 0), 3)).toBe(7);
    expect(leftNow(ticket, val(3, 9, at(14, 0)), at(15, 0), 3)).toBe(9);
  });
  it('adds a daily-banked amount at 07:00 UTC, not 24 h after the update', () => {
    expect(leftNow(banked, val(4, 10, at(14, 8)), at(15, 6, 59), 3)).toBe(10);
    expect(leftNow(banked, val(4, 10, at(14, 8)), at(15, 7), 3)).toBe(12);
    expect(leftNow(banked, val(4, 10, at(14, 6)), at(14, 7), 3)).toBe(12); // update just before the boundary
  });
});

describe('fullAt', () => {
  it('is the boundary where the cap is reached, or null', () => {
    // left 5 at 07:00; boundaries 10:00 (6) and 13:00 (7)
    expect(fullAt(ticket, val(3, 2, at(14, 0)), at(14, 7), 3)).toBe(at(14, 13));
    expect(fullAt(ticket, val(3, 7, at(14, 0)), at(14, 7), 3)).toBeNull();
    expect(fullAt(daily, val(1, 0, at(14, 8)), at(14, 9), 3)).toBeNull();
  });
  it('follows an 8 h refill on the 07:00 UTC clock (07, 15, 23)', () => {
    const conquest: CounterDef = { name: { en: 'Conquest' }, category: 'ticket', cap: 21, reset: null, refill: { amount: 1, every_ms: 8 * H } };
    expect(leftNow(conquest, val(6, 3, at(14, 8)), at(14, 15), 3)).toBe(4);
    expect(leftNow(conquest, val(6, 3, at(14, 8)), at(14, 23), 3)).toBe(5);
    expect(fullAt(conquest, val(6, 19, at(14, 8)), at(14, 8), 3)).toBe(at(14, 23)); // 15:00 -> 20, 23:00 -> 21
  });
  it('lands on 07:00 UTC for daily-banked counters', () => {
    // 10 at 09:00, +2 on the 15th (12) and +2 on the 16th (14)
    expect(fullAt(banked, val(4, 10, at(14, 8)), at(14, 9), 3)).toBe(at(16, 7));
  });
});

describe('summarize', () => {
  const gd: GameData = { version: 1, weekly_reset_day: 3, classes: {}, counters: { 1: daily, 2: weekly, 3: ticket }, gains: {}, pets: {}, pet_souls: [5, 25, 75], pet_regions: {}, pet_monsters: {} };
  const ch = (counters: CounterValue[], gains: Character['gains'] = {}): Character =>
    ({ server: 7, name: 'Alpha', class_code: 3, level: 40, combat_power: 1, item_level: null, counters, last_seen_us: 0, gains });

  it('sums dailies and weeklies, lists refills, hides uncurated ids and scores left/cap', () => {
    const now = at(14, 8);
    const s = summarize(gd, ch([val(1, 1, at(14, 7, 30)), val(2, 3, at(14, 7, 30)), val(3, 7, at(14, 7, 30)), val(99, 5, at(14, 7, 30))]), now);
    expect(s.daily).toEqual({ left: 1, cap: 2 });
    expect(s.weekly).toEqual({ left: 3, cap: 3 });
    expect(s.items.find(i => i.id === 3)).toMatchObject({ left: 7, cap: 7, ms: false, fullAt: null });
    expect(s.score).toBeCloseTo(0.5 + 1 + 1);
  });

  it('keeps millisecond counters out of the count sums and lists them with the amounts', () => {
    const timed: CounterDef = { name: { en: 'Abyss time' }, category: 'weekly', cap: 25_200_000, reset: 'weekly', refill: null, ms: true };
    const withTimed: GameData = { ...gd, counters: { ...gd.counters, 5: timed } };
    const s = summarize(withTimed, ch([val(2, 3, at(14, 7, 30)), val(5, 6_000_000, at(14, 7, 30))]), at(14, 8));
    expect(s.weekly).toEqual({ left: 3, cap: 3 });
    expect(s.items.find(i => i.id === 5)).toMatchObject({ left: 6_000_000, cap: 25_200_000, ms: true, fullAt: null });
  });

  it('lists every curated counter as an item, in counter order, hiding uncurated ids', () => {
    const s = summarize(gd, ch([val(1, 1, at(14, 7, 30)), val(2, 3, at(14, 7, 30)), val(3, 7, at(14, 7, 30)), val(99, 5, at(14, 7, 30))]), at(14, 8));
    expect(s.items.map(i => i.id)).toEqual([1, 2, 3]);
    expect(s.items[2]).toEqual({ id: 3, name: ticket.name, category: 'ticket', left: 7, cap: 7, ms: false, fullAt: null, refill: ticket.refill });
  });

  it('lists millisecond counters as items too', () => {
    const timed: CounterDef = { name: { en: 'Abyss time' }, category: 'weekly', cap: 25_200_000, reset: 'weekly', refill: null, ms: true };
    const withTimed: GameData = { ...gd, counters: { ...gd.counters, 5: timed } };
    const s = summarize(withTimed, ch([val(5, 6_000_000, at(14, 7, 30))]), at(14, 8));
    expect(s.items).toEqual([
      { id: 5, name: timed.name, category: 'weekly', left: 6_000_000, cap: 25_200_000, ms: true, fullAt: null, refill: null },
    ]);
  });

  describe('gains', () => {
    const supply: CounterDef = { name: { en: 'Supply' }, category: 'daily', cap: 10, reset: 'daily', refill: null };
    const supplyWeek: CounterDef = { name: { en: 'Weekly supply' }, category: 'weekly', cap: 10, reset: 'weekly', refill: null };
    const kinah: CounterDef = { name: { en: 'Kinah' }, category: 'daily', cap: 1_000_000, reset: 'daily', refill: null, sum: false };
    const withGains: GameData = { ...gd, gains: { supply_daily: supply, supply_weekly: supplyWeek, kinah_today: kinah } };
    const gain = (value: number, updatedMs: number) => ({ value, updated_us: updatedMs * 1000 });
    const left = (s: ReturnType<typeof summarize>, id: string) => s.items.find(i => i.id === id)?.left;

    it('shows what is left of a gain, from the count done', () => {
      const s = summarize(withGains, ch([], { supply_daily: gain(3, at(14, 7, 30)), kinah_today: gain(250_000, at(14, 7, 30)) }), at(14, 8));
      expect(left(s, 'supply_daily')).toBe(7);
      expect(left(s, 'kinah_today')).toBe(750_000);
      expect(s.items[0]).toMatchObject({ id: 'supply_daily', category: 'daily', cap: 10, ms: false, fullAt: null, refill: null });
    });

    it('is back at cap once the daily or weekly reset passed since the update', () => {
      const g = { supply_daily: gain(4, at(14, 6)), supply_weekly: gain(4, at(13, 9)) };
      expect(left(summarize(withGains, ch([], g), at(14, 6, 30)), 'supply_daily')).toBe(6);
      expect(left(summarize(withGains, ch([], g), at(14, 8)), 'supply_daily')).toBe(10);
      expect(left(summarize(withGains, ch([], g), at(14, 6, 30)), 'supply_weekly')).toBe(6);
      expect(left(summarize(withGains, ch([], g), at(14, 8)), 'supply_weekly')).toBe(10);
    });

    it('clamps a value above the cap to zero left and ignores unknown keys', () => {
      const s = summarize(withGains, ch([], { supply_daily: gain(12, at(14, 7, 30)), mystery: gain(1, at(14, 7, 30)) }), at(14, 8));
      expect(s.items.map(i => i.id)).toEqual(['supply_daily']);
      expect(left(s, 'supply_daily')).toBe(0);
    });

    it('keeps kinah out of the daily sum but counts supply in the daily and weekly sums', () => {
      const s = summarize(withGains, ch([val(1, 1, at(14, 7, 30))], {
        supply_daily: gain(3, at(14, 7, 30)), supply_weekly: gain(5, at(14, 7, 30)), kinah_today: gain(250_000, at(14, 7, 30)),
      }), at(14, 8));
      expect(s.daily).toEqual({ left: 1 + 7, cap: 2 + 10 });
      expect(s.weekly).toEqual({ left: 5, cap: 10 });
      // supply is 0.7 and 0.5, the counter 0.5; kinah (0.75) is a limit and does not order the roster
      expect(s.score).toBeCloseTo(0.5 + 0.7 + 0.5);
    });

    it('turns the used amount of a pass counter into what is left, outside the weekly sum', () => {
      const pass: CounterDef = { name: { en: 'Pass' }, category: 'weekly', cap: 50_000, reset: 'weekly', refill: null, sum: false };
      const gdPass: GameData = { ...gd, gains: { '10044': pass } };
      const g = { '10044': gain(12_000, at(13, 9)) };
      const s = summarize(gdPass, ch([], g), at(13, 10));
      expect(s.items[0]).toMatchObject({ id: '10044', category: 'weekly', left: 38_000, cap: 50_000 });
      expect(s.weekly).toEqual({ left: 0, cap: 0 });
      expect(left(summarize(gdPass, ch([], g), at(14, 8)), '10044')).toBe(50_000);
    });

    it('treats a character without gains as having none', () => {
      const old = { ...ch([]), gains: undefined };
      expect(summarize(withGains, old, at(14, 8)).items).toEqual([]);
    });
  });

  it('gives a refilling ticket its full-at time', () => {
    const s = summarize(gd, ch([val(3, 2, at(14, 0))]), at(14, 7));
    expect(s.items[0].fullAt).toBe(at(14, 13));
    expect(s.items[0].refill).toEqual({ amount: 1, every_ms: 3 * H });
  });
});

describe('pick', () => {
  it('falls back to English, then undefined', () => {
    expect(pick({ en: 'A', de: 'B' }, 'de')).toBe('B');
    expect(pick({ en: 'A' }, 'fr')).toBe('A');
    expect(pick(undefined, 'en')).toBeUndefined();
  });
});

describe('petCollection', () => {
  const gd: GameData = {
    version: 1, weekly_reset_day: 3, classes: {}, counters: {}, gains: {}, pet_souls: [5, 25, 75], pet_regions: {}, pet_monsters: {},
    pets: {
      1: { name: { en: 'Alpha' }, genus: 'Fera' }, 2: { name: { en: 'Beta' }, genus: 'Cogni' },
      3: { name: { en: 'Gamma' }, genus: 'Natura' }, 4: { name: { en: 'Delta' }, genus: 'Varian' },
    },
  };
  const hero = (name: string, updated: number, active: number, list: { id: number; level: number; souls: number }[]): Character =>
    ({ server: 7, name, class_code: 3, level: 40, combat_power: null, item_level: null, counters: [], last_seen_us: 0, pets: { active, updated_us: updated, list } });
  const list = [{ id: 1, level: 1, souls: 24 }, { id: 2, level: 3, souls: 0 }, { id: 9, level: 2, souls: 31 }];

  it('is null until a character reported pets', () => {
    expect(petCollection(gd, [{ ...hero('A', 1, 1, []), pets: null }], 'en')).toBeNull();
    expect(petCollection(gd, [], 'en')).toBeNull();
  });

  it('measures each pet to its next level and to the top, and counts the levels', () => {
    const c = petCollection(gd, [hero('A', 5, 1, list)], 'en')!;
    const row = (id: number) => c.rows.find(r => r.id === id)!;
    expect(row(1)).toMatchObject({ name: 'Alpha', genus: 'Fera', next: 25, toNext: 1, toMax: 76 });
    expect(row(2)).toMatchObject({ level: 3, next: null, toNext: 0, toMax: 0 });
    expect(row(3)).toMatchObject({ level: 0, souls: 0, next: 5, toNext: 5, toMax: 105 });
    expect(row(9)).toMatchObject({ name: '#9', genus: null, toNext: 44, toMax: 44 }); // not curated: kept
    expect([c.total, c.obtained, c.byLevel]).toEqual([5, 3, [2, 1, 1, 1]]);
    expect(c.soulsToMax).toBe(76 + 0 + 105 + 105 + 44);
  });

  it('takes the collection from the freshest character and lists every active pet', () => {
    const c = petCollection(gd, [hero('Old', 1, 1, []), hero('New', 9, 2, list)], 'en')!;
    expect([c.updatedUs, c.obtained]).toEqual([9, 3]);
    expect(c.actives).toEqual([{ character: 'Old', pet: 'Alpha' }, { character: 'New', pet: 'Beta' }]);
  });
});

describe('soulGain', () => {
  const pet = (id: number, level: number, souls: number) => ({ id, name: `P${id}`, genus: null, level, souls, next: 25, toNext: 0, toMax: 0 });
  const before = [pet(1, 1, 10), pet(2, 0, 0)];

  it('finds the pet whose souls or level went up', () => {
    expect(soulGain(before, [pet(1, 1, 11), pet(2, 0, 0)])).toBe(1);
    expect(soulGain(before, [pet(1, 1, 10), pet(2, 1, 0)])).toBe(2);   // a level-up zeroes the souls
    expect(soulGain(before, [pet(1, 2, 0), pet(2, 0, 0)])).toBe(1);
  });

  it('is null when nothing went up, and ignores pets that were not there before', () => {
    expect(soulGain(before, before)).toBeNull();
    expect(soulGain(before, [pet(1, 1, 10), pet(2, 0, 0), pet(3, 2, 7)])).toBeNull();
    expect(soulGain([], before)).toBeNull();
  });
});

describe('public/gamedata.json', () => {
  const gd = shipped as unknown as GameData;

  it('matches the GameData shape', () => {
    expect(gd.version).toBe(1);
    expect(gd.weekly_reset_day).toBeGreaterThanOrEqual(0);
    expect(gd.weekly_reset_day).toBeLessThanOrEqual(6);
    for (const [id, c] of Object.entries(gd.counters)) {
      expect(Number.isInteger(Number(id)), id).toBe(true);
      expect(c.name['en'], id).toBeTruthy();
      expect(['daily', 'weekly', 'ticket', 'energy'], id).toContain(c.category);
      expect(c.cap, id).toBeGreaterThan(0);
      expect([null, 'daily', 'weekly'], id).toContain(c.reset);
      if (c.refill) expect(c.refill.amount > 0 && c.refill.every_ms > 0, id).toBe(true);
    }
    expect(Object.keys(gd.gains).sort()).toEqual(['10044', '10045', 'duty_daily', 'kinah_today', 'supply_daily', 'supply_weekly']);
    for (const [key, c] of Object.entries(gd.gains)) {
      expect(c.name['en'], key).toBeTruthy();
      expect(['daily', 'weekly'], key).toContain(c.category);
      expect(c.cap, key).toBeGreaterThan(0);
      expect(c.reset, key).toBe(c.category);
      expect(c.refill, key).toBeNull();
    }
    expect(gd.gains['kinah_today'].sum).toBe(false);
    expect(gd.gains['10044'].sum).toBe(false);
    expect(gd.gains['10045'].sum).toBe(false);
    expect(gd.pet_souls).toEqual([5, 25, 75]);
    expect(Object.keys(gd.pets).length).toBeGreaterThan(200);
    for (const [id, p] of Object.entries(gd.pets)) {
      expect(Number.isInteger(Number(id)), id).toBe(true);
      expect(p.name['en'], id).toBeTruthy();
      expect(['Cogni', 'Fera', 'Natura', 'Varian', 'Special'], id).toContain(p.genus);
    }
    for (const c of Object.values(gd.classes)) expect(c.name['en']).toBeTruthy();
    for (const c of Object.values(gd.classes)) if (c.icon) expect(c.icon).toMatch(/^[a-z]+$/);
  });

  it('only refills on periods that divide a day, so boundaries stay on the 07:00 UTC clock', () => {
    for (const [id, c] of Object.entries(gd.counters)) {
      if (c.refill) expect(86_400_000 % c.refill.every_ms, id).toBe(0);
    }
  });
});

describe('farmMap', () => {
  const gd: GameData = {
    version: 1, weekly_reset_day: 3, classes: {}, counters: {}, gains: {}, pets: {}, pet_souls: [5, 25, 75],
    pet_regions: {
      north: { name: { en: 'North' }, origin: [-200, -200], extent: [400, 400] },
      south: { name: { en: 'South' }, origin: [-200, -200], extent: [400, 400] },
    },
    pet_monsters: {
      10: { name: { en: 'Slime' }, level: 5, grade: 'normal', pet: 1, zones: ['Meadow'], spawns: { north: [[0, 0], [100, 50]] } },
      11: { name: { en: 'Wolf' }, level: 9, grade: 'elite', pet: 2, zones: ['Meadow'], spawns: { north: [[100, 100]], south: [[-20, 10]] } },
      12: { name: { en: 'Drake' }, level: 40, grade: 'hero', pet: 2, zones: [], dungeons: ['Lair'] },
      13: { name: { en: 'Orphan' }, level: 1, grade: 'normal', pet: 99, zones: [], spawns: { north: [[5, 5]] } },
    },
  };
  const row = (id: number, toNext: number) => ({ id, name: `Pet ${id}`, genus: null, level: 1, souls: 0, next: 25, toNext, toMax: 0 });
  const rows = [row(1, 20), row(2, 3)];

  it('groups monsters by region, nearest to a level first, with dots drawn north-up', () => {
    const { regions } = farmMap(gd, rows, 'en');
    expect(regions.map(r => r.name)).toEqual(['North', 'South']);
    expect(regions[0].monsters.map(m => m.name)).toEqual(['Wolf', 'Slime']);
    expect(regions[0].monsters[1].d).toBe('M0 0h0M100 50h0');
    expect(regions[0].image).toEqual({ href: 'maps/north.webp', x: -200, y: -200, w: 400, h: 400 });
    expect(regions[0].monsters[0]).toMatchObject({ petName: 'Pet 2', toNext: 3, level: 9 });
  });

  it('carries the level of the pet each monster feeds', () => {
    const { regions } = farmMap(gd, [{ ...row(1, 20), level: 0 }, { ...row(2, 3), level: 2 }], 'en');
    expect(regions[0].monsters.map(m => [m.name, m.petLevel])).toEqual([['Wolf', 2], ['Slime', 0]]);
  });

  it('lists monsters with no spawn separately and skips pets that are not in the rows', () => {
    const { regions, dungeons } = farmMap(gd, rows, 'en');
    expect(dungeons.map(m => m.name)).toEqual(['Drake']);
    expect(dungeons[0].dungeons).toEqual(['Lair']);
    expect(regions.flatMap(r => r.monsters).map(m => m.name)).not.toContain('Orphan');
  });

  it('keeps the view box and zone labels when the filter narrows the rows', () => {
    const all = farmMap(gd, rows, 'en').regions[0];
    const one = farmMap(gd, [rows[0]], 'en');
    expect(one.regions.map(r => r.name)).toEqual(['North']);
    expect(one.regions[0].box).toEqual(all.box);
    expect(one.regions[0].labels).toEqual(all.labels);
    expect(all.labels.map(l => l.zone)).toEqual(['Meadow']);
    expect(all.labels[0]).toEqual({ zone: 'Meadow', x: 200 / 3, y: 50 });   // centroid of (0,0), (100,50), (100,100)
    expect(farmMap(gd, [], 'en')).toEqual({ regions: [], dungeons: [] });
  });
});

describe('map view', () => {
  const base = { x: 0, y: 0, w: 1000, h: 500 };

  it('clampBox keeps the shape, 1x to 16x, and stays inside the base', () => {
    expect(clampBox({ x: -50, y: 900, w: 5000, h: 1 }, base)).toEqual(base);
    const deep = clampBox({ x: 10, y: 10, w: 1, h: 1 }, base);
    expect(deep.w).toBe(1000 / 16);
    expect(deep.h).toBe(500 / 16);
    expect(clampBox({ x: 990, y: 490, w: 200, h: 100 }, base)).toEqual({ x: 800, y: 400, w: 200, h: 100 });
  });

  it('fitBox frames a target with room around it, centered, at the base shape', () => {
    const v = fitBox({ x: 400, y: 200, w: 100, h: 50 }, base);
    expect(v.w / v.h).toBe(2);
    expect(v.w).toBeGreaterThan(100);
    expect(v.x + v.w / 2).toBeCloseTo(450);
    expect(v.y + v.h / 2).toBeCloseTo(225);
  });

  it('fitBox does not go deeper than 8x for a single point, nor outside the base', () => {
    expect(fitBox({ x: 5, y: 5, w: 0, h: 0 }, base)).toEqual({ x: 0, y: 0, w: 125, h: 62.5 });
  });
});

describe('followBox', () => {
  const image = { x: -100, y: -100, w: 200, h: 200 };
  const base = { x: -50, y: -50, w: 100, h: 100 };

  it('is the base width over the zoom, at the container shape, centred on the player', () => {
    expect(followBox(base, image, [10, 20], 4, 2)).toEqual({ x: -2.5, y: 13.75, w: 25, h: 12.5 });
    expect(followBox(base, image, null, 1, 1)).toEqual(base);
  });

  it('never leaves the image, so the container is always covered with map', () => {
    const v = followBox(base, image, [500, -500], 4, 1);
    expect([v.x + v.w, v.y]).toEqual([100, -100]);
    // a tall container at zoom 1 is as wide as the image allows for its height
    const tall = followBox({ ...base, w: 400 }, image, null, 1, 0.5);
    expect([tall.w, tall.h]).toEqual([100, 200]);
  });
});

describe('live radar', () => {
  const gd: GameData = {
    version: 1, weekly_reset_day: 3, classes: {}, counters: {}, gains: {}, pets: {}, pet_souls: [5, 25, 75],
    pet_regions: {
      north: { name: { en: 'North' }, origin: [-200, -200], extent: [400, 400], maps: [1010] },
      south: { name: { en: 'South' }, origin: [-200, -200], extent: [400, 400] },
    },
    pet_monsters: {
      10: { name: { en: 'Slime' }, level: 5, grade: 'normal', pet: 1, zones: [], spawns: { north: [[0, 0]] } },
      11: { name: { en: 'Wolf' }, level: 9, grade: 'elite', pet: 2, zones: [], spawns: { south: [[5, 5]] } },
      12: { name: { en: 'Bat' }, level: 9, grade: 'normal', pet: 2, zones: [], spawns: { north: [[9, 9]], south: [[9, 9]] } },
    },
  };
  const radar = (templates: number[], map: number | null = null): Radar => ({
    map,
    player: null,
    monsters: templates.map((template, i) => ({ entity: 100 + i, template, x: 1250, y: -340 })),
  });
  const row = (id: number) => ({ id, name: `Pet ${id}`, genus: null, level: 1, souls: 0, next: 25, toNext: 5, toMax: 0 });

  it('takes the region from the map id, and from the monsters in view when the map id is not listed', () => {
    expect(currentRegion(gd, radar([11, 11], 1010))).toBe('north'); // the map id wins over what is in view
    expect(currentRegion(gd, radar([11], 999))).toBe('south'); // an unlisted map: the monsters decide
    expect(currentRegion(gd, radar([], 999))).toBeNull();
    expect(currentRegion(gd, null)).toBeNull();
    expect(currentRegion(gd, radar([], 1010))).toBe('north');
  });

  it('names the region most of the pet monsters in view spawn in', () => {
    expect(liveRegion(gd, radar([10, 10, 11]))).toBe('north');
    expect(liveRegion(gd, radar([11, 11, 10]))).toBe('south');
  });

  it('splits the vote of a monster that spawns in several regions and ignores others', () => {
    expect(liveRegion(gd, radar([12, 11]))).toBe('south');   // north 0.5, south 1.5
    expect(liveRegion(gd, radar([999, 12]))).toBe('north');   // an unknown template does not vote; a tie goes to the first key
    expect(liveRegion(gd, radar([999]))).toBeNull();
    expect(liveRegion(gd, radar([]))).toBeNull();
    expect(liveRegion(gd, null)).toBeNull();
  });

  it('keeps the monsters the region lists, in map units', () => {
    const north = farmMap(gd, [row(1), row(2)], 'en').regions.find(r => r.key === 'north')!;
    const live = liveMonsters(north, radar([10, 11, 12, 999]));
    expect(live.map(m => m.id)).toEqual([10, 12]);   // Wolf spawns only in the south
    expect(live[0]).toMatchObject({ entity: 100, petLevel: 1, x: 12.5, y: -3.4 });
    expect(liveMonsters(north, null)).toEqual([]);
    expect(liveMonsters(null, radar([10]))).toEqual([]);
    // the farm filter reaches the radar: without pet 1 on the map the Slime is not listed
    const filtered = farmMap(gd, [row(2)], 'en').regions.find(r => r.key === 'north')!;
    expect(liveMonsters(filtered, radar([10, 12])).map(m => m.id)).toEqual([12]);
  });
});
