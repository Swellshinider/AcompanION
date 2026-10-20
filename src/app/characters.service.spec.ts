import { Character, merge } from './characters.service';

const ch = (server: number, name: string, level: number): Character =>
  ({ server, name, class_code: 3, level, combat_power: null, item_level: null, counters: [], last_seen_us: 0 });

describe('merge', () => {
  it('replaces by (server, name), keeps the rest and appends new characters', () => {
    const out = merge([ch(7, 'Alpha', 40), ch(7, 'Beta', 20)], [ch(7, 'Alpha', 41), ch(8, 'Alpha', 1)]);
    expect(out.map(c => [c.server, c.name, c.level])).toEqual([[7, 'Alpha', 41], [7, 'Beta', 20], [8, 'Alpha', 1]]);
  });
});
