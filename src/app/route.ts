import { GameData, PetRow } from './gamedata';

/** One place to stop: the spawns of `monsters` within one grid cell, picked to cover `pets`. */
export interface RouteStop {
  x: number;
  y: number;
  monsters: number[];
  /** The pets this stop was picked for (the ones no earlier pick covered). */
  pets: number[];
  /** Souls those pets still need to reach the top. */
  souls: number;
}
export interface FarmRoute {
  stops: RouteStop[];
  /** The path through the stops as an SVG path in map units. */
  d: string;
  /** Pets the route covers. */
  pets: number;
  /** A grid cell, in map units: closer than this to a stop counts as having been there. */
  radius: number;
}

/** ponytail: fixed grid (1/GRID of the region's span) stands in for the aggro/loot range; tune it once measured. */
const GRID = 50;

interface Group { x: number; y: number; n: number; monsters: Set<number>; pets: Set<number> }

/**
 * A short route in `region` that passes a spawn group of every pet that is not maxed: spawn points
 * are bucketed on a grid, a greedy set cover picks the groups, nearest neighbour orders them from
 * `start` (else from the first pick) and 2-opt shortens the path. Null when no open pet drops there.
 * ponytail: greedy cover and 2-opt are not optimal, but n is a few dozen; swap in a real TSP solver
 * only if routes look visibly wasteful.
 */
export function farmRoute(gd: GameData, rows: PetRow[], region: string, start: [number, number] | null): FarmRoute | null {
  const open = new Map(rows.filter(r => r.next !== null).map(r => [r.id, r]));
  const here = Object.entries(gd.pet_monsters).filter(([, m]) => m.spawns?.[region]);
  const pts = here.flatMap(([, m]) => m.spawns![region]);
  if (!pts.length) return null;
  const [xs, ys] = [pts.map(p => p[0]), pts.map(p => p[1])];
  const cell = Math.max(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)) / GRID || 1;

  const cells = new Map<string, Group>();
  for (const [id, m] of here) {
    if (!open.has(m.pet)) continue;
    for (const [x, y] of m.spawns![region]) {
      const k = `${Math.floor(x / cell)},${Math.floor(y / cell)}`;
      const g = cells.get(k) ?? { x: 0, y: 0, n: 0, monsters: new Set(), pets: new Set() };
      g.x += x; g.y += y; g.n++;
      g.monsters.add(+id); g.pets.add(m.pet);
      cells.set(k, g);
    }
  }
  const groups = [...cells.values()];
  if (!groups.length) return null;

  const uncovered = new Set(groups.flatMap(g => [...g.pets]));
  const total = uncovered.size;
  const picked: RouteStop[] = [];
  while (uncovered.size) {
    let best = -1, bestPets = 0, bestN = 0;
    groups.forEach((g, i) => {
      const gain = [...g.pets].filter(p => uncovered.has(p)).length;
      if (gain > bestPets || (gain === bestPets && gain > 0 && g.n > bestN)) [best, bestPets, bestN] = [i, gain, g.n];
    });
    const g = groups[best];
    const pets = [...g.pets].filter(p => uncovered.has(p)).sort((a, b) => a - b);
    pets.forEach(p => uncovered.delete(p));
    picked.push({ x: g.x / g.n, y: g.y / g.n, monsters: [...g.monsters].sort((a, b) => a - b), pets, souls: pets.reduce((a, p) => a + open.get(p)!.toMax, 0) });
  }

  const stops = order(picked, start);
  return {
    stops,
    d: stops.map((s, i) => `${i ? 'L' : 'M'}${s.x} ${s.y}`).join(''),
    pets: total,
    radius: cell,
  };
}

/** Nearest neighbour from `start` (or the first stop), then 2-opt; `start` itself stays first and is not returned. */
function order(stops: RouteStop[], start: [number, number] | null): RouteStop[] {
  const dist = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);
  const left = [...stops];
  let at: { x: number; y: number } = start ? { x: start[0], y: start[1] } : left.shift()!;
  const path = [at];
  while (left.length) {
    let j = 0;
    left.forEach((s, i) => { if (dist(at, s) < dist(at, left[j])) j = i; });
    at = left.splice(j, 1)[0];
    path.push(at);
  }
  // 2-opt on the open path: with a start the first node is pinned
  const first = start ? 1 : 0;
  for (let improved = true; improved;) {
    improved = false;
    for (let i = first; i < path.length - 1; i++) {
      for (let j = i + 1; j < path.length; j++) {
        const before = i > 0 ? dist(path[i - 1], path[i]) : 0;
        const after = j < path.length - 1 ? dist(path[i], path[j + 1]) : 0;
        const now = before + (j < path.length - 1 ? dist(path[j], path[j + 1]) : 0);
        const flipped = (i > 0 ? dist(path[i - 1], path[j]) : 0) + after;
        if (flipped < now - 1e-9) {
          path.splice(i, j - i + 1, ...path.slice(i, j + 1).reverse());
          improved = true;
        }
      }
    }
  }
  return (start ? path.slice(1) : path) as RouteStop[];
}
