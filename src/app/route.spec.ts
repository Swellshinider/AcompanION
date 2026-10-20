import { GameData, PetRow } from './gamedata';
import { farmRoute } from './route';

const row = (id: number, next: number | null, toMax = 10): PetRow => ({
  id, name: `P${id}`, genus: 'Fera', level: next === null ? 3 : 1, souls: 0, next, toNext: next ?? 0, toMax,
});
const mon = (pet: number, spawns: Record<string, [number, number][]>) =>
  ({ name: { en: `M${pet}` }, level: 1, grade: 'normal' as const, pet, zones: [], spawns });
const gd = (monsters: GameData['pet_monsters']): GameData => ({
  version: 1, weekly_reset_day: 3, classes: {}, counters: {}, gains: {}, pets: {}, pet_souls: [5, 25, 75],
  pet_regions: { north: { name: { en: 'North' }, origin: [0, 0], extent: [100, 100] } }, pet_monsters: monsters,
});
const length = (pts: { x: number; y: number }[]) => pts.slice(1).reduce((a, p, i) => a + Math.hypot(p.x - pts[i].x, p.y - pts[i].y), 0);

describe('farmRoute', () => {
  it('covers every open pet in the region and nothing else', () => {
    const r = farmRoute(gd({
      1: mon(1, { north: [[0, 0]] }), 2: mon(2, { north: [[50, 50]] }),
      3: mon(3, { north: [[100, 100]] }),   // maxed
      4: mon(4, { south: [[10, 10]] }),     // other region
    }), [row(1, 5), row(2, 5), row(3, null), row(4, 5)], 'north', null)!;
    expect(r.stops.flatMap(s => s.pets).sort()).toEqual([1, 2]);
    expect(r.pets).toBe(2);
  });

  it('prefers one stop that covers two pets over two single stops', () => {
    const r = farmRoute(gd({
      1: mon(1, { north: [[0, 0], [90, 90]] }), 2: mon(2, { north: [[0, 0], [10, 90]] }),
    }), [row(1, 5), row(2, 5)], 'north', null)!;
    expect(r.stops).toHaveLength(1);
    expect(r.stops[0].pets).toEqual([1, 2]);
    expect(r.stops[0].souls).toBe(20);
  });

  it('starts at the stop nearest the player', () => {
    const g = gd({ 1: mon(1, { north: [[0, 0]] }), 2: mon(2, { north: [[100, 0]] }), 3: mon(3, { north: [[50, 0]] }) });
    const rows = [row(1, 5), row(2, 5), row(3, 5)];
    expect(farmRoute(g, rows, 'north', [95, 0])!.stops.map(s => s.pets[0])).toEqual([2, 3, 1]);
    expect(farmRoute(g, rows, 'north', [5, 0])!.stops.map(s => s.pets[0])).toEqual([1, 3, 2]);
  });

  it('shortens a path that nearest neighbour leaves crossed', () => {
    // nearest neighbour from the first pick (10) goes to 0 then back out to 21: 31; sweeping 0, 10, 21 is 21
    const g = gd({ 1: mon(1, { north: [[10, 0]] }), 2: mon(2, { north: [[0, 0]] }), 3: mon(3, { north: [[21, 0]] }) });
    const r = farmRoute(g, [row(1, 5), row(2, 5), row(3, 5)], 'north', null)!;
    expect(length(r.stops)).toBeCloseTo(21);
  });

  it('is deterministic', () => {
    const g = gd({ 1: mon(1, { north: [[3, 4], [30, 40]] }), 2: mon(2, { north: [[30, 40], [60, 80]] }), 3: mon(3, { north: [[90, 5]] }) });
    const rows = [row(1, 5), row(2, 5), row(3, 5)];
    expect(farmRoute(g, rows, 'north', [1, 1])).toEqual(farmRoute(g, rows, 'north', [1, 1]));
  });

  it('is null when every pet there is maxed or the region has no spawns', () => {
    const g = gd({ 1: mon(1, { north: [[0, 0]] }) });
    expect(farmRoute(g, [row(1, null)], 'north', null)).toBeNull();
    expect(farmRoute(g, [row(1, 5)], 'south', null)).toBeNull();
  });
});
