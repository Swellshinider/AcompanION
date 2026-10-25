import { Character, CounterValue, Radar } from './characters.service';

/** Game facts keyed by wire id, from `public/gamedata.json`. Everything below is pure. */
export type Names = Record<string, string>;
export interface CounterDef {
  name: Names;
  category: 'daily' | 'weekly' | 'ticket' | 'energy';
  cap: number;
  reset: 'daily' | 'weekly' | null;
  refill: { amount: number; every_ms: number } | null;
  /** Value and cap are milliseconds (the wire's own flag is lost on `0c 61` updates). */
  ms?: boolean;
  /** `false`: shown, but kept out of the daily/weekly `left/cap` sums (its cap is not a count of tasks). */
  sum?: false;
}
export interface ClassDef {
  name: Names;
  /** File stem under `public/classes/`; absent means the generic glyph. */
  icon?: string;
  /** CSS colour of the class (the meter's bars); absent means the accent. */
  color?: string;
}
export type Genus = 'Cogni' | 'Fera' | 'Natura' | 'Varian' | 'Special';
export interface PetDef {
  name: Names;
  genus: Genus;
}
/** A monster that drops a pet's souls. `spawns` are map points per region key; absent for dungeon-only ones. */
export interface PetMonsterDef {
  name: Names;
  level: number;
  grade: 'normal' | 'elite' | 'hero';
  pet: number;
  zones: string[];
  spawns?: Record<string, [number, number][]>;
  dungeons?: string[];
}
export interface GameData {
  version: 1;
  /** UTC weekday of the weekly reset (0 = Sunday), at the daily reset hour. */
  weekly_reset_day: number;
  classes: Record<string, ClassDef>;
  counters: Record<string, CounterDef>;
  /** Per-character gains by key (`supply_daily`, `supply_weekly`, `duty_daily`, `kinah_today`). */
  gains: Record<string, CounterDef>;
  /** Souls needed to leave each level: `[5, 25, 75]` takes a pet from 0 to 1, 1 to 2, 2 to 3. */
  pet_souls: number[];
  pets: Record<string, PetDef>;
  /** Regions that have spawn points, by key. */
  /** `maps`: the game's map ids that are this region (`radar.map`); only those seen on the wire are listed. */
  pet_regions: Record<string, { name: Names; origin: [number, number]; extent: [number, number]; maps?: number[] }>;
  pet_monsters: Record<string, PetMonsterDef>;
  /** Skill names by the folded skill id and monster names by template (partial; absent ones fall back to `Skill N` / `Monster N`). A skill's `icon` is a file stem in `public/skills/` (absent: a coloured tile with its initial). */
  skills?: Record<string, { name: Names; icon?: string }>;
  monsters?: Record<string, { name: Names }>;
  /** Dungeons by the game's map id (the id a fight or `radar.map` carries inside one), with the template ids of the bosses seen in each (partial). */
  dungeons?: Record<string, { name: Names; mode: 'conquest' | 'transcendence'; bosses: number[] }>;
}
/** One curated counter as the dashboard shows it. `left` is what remains now. */
export interface Item {
  /** Counter id, or the gain key. */
  id: number | string;
  name: Names;
  category: CounterDef['category'];
  left: number;
  cap: number;
  ms: boolean;
  fullAt: number | null;
  refill: CounterDef['refill'];
}
export interface Summary {
  daily: { left: number; cap: number };
  weekly: { left: number; cap: number };
  items: Item[];
  score: number;
}

/** One pet as the Pets screen shows it. `next` is null at the top level. */
export interface PetRow {
  id: number;
  /** Curated name; `#id` when the pet is not in `gamedata.json`. */
  name: string;
  genus: Genus | null;
  level: number;
  souls: number;
  next: number | null;
  toNext: number;
  toMax: number;
}
export interface PetCollection {
  rows: PetRow[];
  obtained: number;
  total: number;
  /** Pets at level 0..3. */
  byLevel: number[];
  soulsToMax: number;
  updatedUs: number;
  /** The pet each character has out. */
  actives: { character: string; pet: string }[];
}

/** A rectangle in map units. */
export interface Box { x: number; y: number; w: number; h: number }

/** One monster on the farm map. `d` is its spawn points as one SVG path of zero-length segments (round caps draw the dots). */
export interface FarmMonster {
  id: number;
  name: string;
  level: number;
  grade: PetMonsterDef['grade'];
  pet: number;
  petName: string;
  /** The level of the pet its souls go to: 0 locked, up to 3. */
  petLevel: number;
  toNext: number;
  zones: string[];
  dungeons: string[];
  d: string;
  /** Bounding box of its spawns in this region; zero-size for a dungeon-only monster. */
  box: Box;
}
export interface FarmRegion {
  key: string;
  name: string;
  /** The region image in map units: `public/maps/<key>.webp`. */
  image: { href: string; x: number; y: number; w: number; h: number };
  /** The region's whole spawn bounding box, whatever the filter, so the map holds still. */
  box: Box;
  /** Dot diameter in view-box units. */
  dot: number;
  labels: { zone: string; x: number; y: number }[];
  monsters: FarmMonster[];
}

/** ponytail: fixed global reset (parent spec §6); use the game's next_reset once it is decoded. */
export const DAILY_RESET_HOUR_UTC = 7;
const HOUR_MS = 3_600_000;
const DAY_MS = 24 * HOUR_MS;

export async function loadGamedata(): Promise<GameData> {
  const r = await fetch('gamedata.json');
  if (!r.ok) throw new Error(`gamedata.json: HTTP ${r.status}`);
  return r.json();
}

/** The name in `lang`, else English. */
export function pick(names: Names | undefined, lang: string): string | undefined {
  return names?.[lang] ?? names?.['en'];
}

/** The most recent reset of `kind` at or before `nowMs`. */
export function lastReset(kind: 'daily' | 'weekly', nowMs: number, weeklyDay: number): number {
  const d = new Date(nowMs);
  let t = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), DAILY_RESET_HOUR_UTC);
  if (t > nowMs) t -= DAY_MS;
  if (kind === 'weekly') t -= ((new Date(t).getUTCDay() - weeklyDay + 7) % 7) * DAY_MS;
  return t;
}

/** The first reset of `kind` after `nowMs`. */
export function nextReset(kind: 'daily' | 'weekly', nowMs: number, weeklyDay: number): number {
  return lastReset(kind, nowMs, weeklyDay) + (kind === 'weekly' ? 7 : 1) * DAY_MS;
}

/** A countdown in the two largest of days, hours, minutes: `3d 4h`, `4h 12m`, `35m`. */
export function span(ms: number, lang: string): string {
  const min = Math.max(0, Math.floor(ms / 60_000));
  const parts = [
    [Math.floor(min / 1440), 'day'],
    [Math.floor(min / 60) % 24, 'hour'],
    [min % 60, 'minute'],
  ] as const;
  const first = parts.findIndex(([n]) => n > 0);
  const start = first < 0 ? 2 : first;
  return parts.slice(start, start + 2)
    .map(([n, unit]) => new Intl.NumberFormat(lang, { style: 'unit', unit, unitDisplay: 'narrow' }).format(n))
    .join(' ');
}

/**
 * Refills land on `DAILY_RESET_HOUR_UTC` + k * every_ms, whatever the period (measured: 3 h energy
 * and +N-a-day keys both follow the 07:00 UTC clock, not the time of the last update).
 */
const boundary = (ms: number, everyMs: number) =>
  Math.floor((ms - DAILY_RESET_HOUR_UTC * HOUR_MS) / everyMs);

/** Refill boundaries in `(updatedMs, nowMs]`. */
const ticks = (def: CounterDef, updatedMs: number, nowMs: number) =>
  def.refill ? Math.max(0, boundary(nowMs, def.refill.every_ms) - boundary(updatedMs, def.refill.every_ms)) : 0;

/** What is left now: `cap` after a reset that passed since the update, plus refills since then. */
export function leftNow(def: CounterDef, c: CounterValue, nowMs: number, weeklyDay: number): number {
  const updated = c.updated_us / 1000;
  if (def.reset && updated < lastReset(def.reset, nowMs, weeklyDay)) return def.cap;
  if (!def.refill || c.value >= def.cap) return c.value;
  return Math.min(def.cap, c.value + ticks(def, updated, nowMs) * def.refill.amount);
}

/** When a refilling counter reaches `cap`; null when it does not refill or is full. */
export function fullAt(def: CounterDef, c: CounterValue, nowMs: number, weeklyDay: number): number | null {
  if (!def.refill) return null;
  const left = leftNow(def, c, nowMs, weeklyDay);
  if (left >= def.cap) return null;
  const { amount, every_ms } = def.refill;
  const missing = Math.ceil((def.cap - left) / amount);
  const next = (boundary(nowMs, every_ms) + 1) * every_ms + DAILY_RESET_HOUR_UTC * HOUR_MS;
  return next + (missing - 1) * every_ms;
}

/**
 * The pet collection from the character that reported it last (the wire sends the same collection
 * to every character of an account), with each pet's distance to its next level and to the top.
 * ponytail: two accounts on one PC would show only the fresher one's collection.
 */
export function petCollection(gd: GameData, chars: Character[], lang: string): PetCollection | null {
  const withPets = chars.filter(c => c.pets);
  const fresh = withPets.reduce<Character | null>((a, c) => (!a || c.pets!.updated_us > a.pets!.updated_us ? c : a), null);
  if (!fresh) return null;
  const seen = new Map(fresh.pets!.list.map(p => [p.id, p]));
  const ids = new Set([...Object.keys(gd.pets).map(Number), ...seen.keys()]);
  const name = (id: number) => pick(gd.pets[id]?.name, lang) ?? `#${id}`;
  const rows = [...ids].sort((a, b) => a - b).map((id): PetRow => {
    const { level = 0, souls = 0 } = seen.get(id) ?? {};
    const next = gd.pet_souls[level] ?? null;
    const toMax = gd.pet_souls.slice(level).reduce((a, n) => a + n, 0) - souls;
    return {
      id, name: name(id), genus: gd.pets[id]?.genus ?? null, level, souls, next,
      toNext: next === null ? 0 : Math.max(0, next - souls), toMax: Math.max(0, toMax),
    };
  });
  const byLevel = gd.pet_souls.map((_, l) => rows.filter(r => r.level === l).length).concat(rows.filter(r => r.level >= gd.pet_souls.length).length);
  return {
    rows,
    obtained: rows.filter(r => r.level >= 1).length,
    total: rows.length,
    byLevel,
    soulsToMax: rows.reduce((a, r) => a + r.toMax, 0),
    updatedUs: fresh.pets!.updated_us,
    actives: withPets.filter(c => c.pets!.active !== 0).map(c => ({ character: c.name, pet: name(c.pets!.active) })),
  };
}

/** One dashboard row's numbers. Counters missing from `gd` are hidden. */
export function summarize(gd: GameData, ch: Character, nowMs: number): Summary {
  const s: Summary = { daily: { left: 0, cap: 0 }, weekly: { left: 0, cap: 0 }, items: [], score: 0 };
  const add = (id: number | string, def: CounterDef, left: number, at: number | null) => {
    // kinah is a limit, not a task: it would reorder the roster by how much was spent
    if (def.sum !== false) s.score += Math.min(left, def.cap) / def.cap;
    // a time counter is milliseconds and kinah a million: summing them with counts would drown the counts
    if (!def.ms && def.sum !== false && (def.category === 'daily' || def.category === 'weekly')) {
      s[def.category].left += left;
      s[def.category].cap += def.cap;
    }
    s.items.push({ id, name: def.name, category: def.category, left, cap: def.cap, ms: def.ms === true, fullAt: at, refill: def.refill });
  };
  for (const c of ch.counters) {
    const def = gd.counters[c.id];
    if (!def) continue;
    add(c.id, def, leftNow(def, c, nowMs, gd.weekly_reset_day), fullAt(def, c, nowMs, gd.weekly_reset_day));
  }
  // a gain's value is what was done; the row shows what is left, and a reset since the update refills it
  for (const [key, g] of Object.entries(ch.gains ?? {})) {
    const def = gd.gains[key];
    if (!def) continue;
    const left = leftNow(def, { id: 0, value: Math.max(0, def.cap - g.value), ms: false, updated_us: g.updated_us }, nowMs, gd.weekly_reset_day);
    add(key, def, left, null);
  }
  return s;
}

/**
 * Where the souls of `rows` drop: per region the monsters (nearest to a level first) and zone labels,
 * and the monsters with no map spawn (dungeons). Points are in the region image's own frame
 * (pixel = (point - origin) / extent * size, y down; checked against the zone anchors).
 */
export function farmMap(gd: GameData, rows: PetRow[], lang: string): { regions: FarmRegion[]; dungeons: FarmMonster[] } {
  const byPet = new Map(rows.map(r => [r.id, r]));
  const box = new Map<string, { x0: number; x1: number; y0: number; y1: number }>();
  const zones = new Map<string, { x: number; y: number; n: number }>();
  const monsters = new Map<string, FarmMonster[]>();
  const dungeons: FarmMonster[] = [];
  for (const [id, m] of Object.entries(gd.pet_monsters)) {
    const row = byPet.get(m.pet);
    for (const [region, pts] of Object.entries(m.spawns ?? {})) {
      let b = box.get(region);
      if (!b) box.set(region, b = { x0: Infinity, x1: -Infinity, y0: Infinity, y1: -Infinity });
      for (const [x, y] of pts) {
        b.x0 = Math.min(b.x0, x); b.x1 = Math.max(b.x1, x); b.y0 = Math.min(b.y0, y); b.y1 = Math.max(b.y1, y);
        if (m.zones.length === 1) {
          const z = zones.get(`${region}|${m.zones[0]}`) ?? { x: 0, y: 0, n: 0 };
          z.x += x; z.y += y; z.n++;
          zones.set(`${region}|${m.zones[0]}`, z);
        }
      }
    }
    if (!row) continue;
    const fm = (d: string, box: Box): FarmMonster => ({
      id: +id, name: pick(m.name, lang) ?? `#${id}`, level: m.level, grade: m.grade, pet: m.pet,
      petName: row.name, petLevel: row.level, toNext: row.toNext, zones: m.zones, dungeons: m.dungeons ?? [], d, box,
    });
    const spawns = Object.entries(m.spawns ?? {});
    if (!spawns.length) dungeons.push(fm('', { x: 0, y: 0, w: 0, h: 0 }));
    for (const [region, pts] of spawns) {
      const list = monsters.get(region) ?? [];
      const [xs, ys] = [pts.map(p => p[0]), pts.map(p => p[1])];
      const [x, y] = [Math.min(...xs), Math.min(...ys)];
      list.push(fm(pts.map(([px, py]) => `M${px} ${py}h0`).join(''), { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y }));
      monsters.set(region, list);
    }
  }
  const order = (a: FarmMonster, b: FarmMonster) => a.toNext - b.toNext || a.level - b.level || a.id - b.id;
  const regions = [...monsters.entries()].map(([key, list]): FarmRegion => {
    const b = box.get(key)!;
    const def = gd.pet_regions[key];
    const pad = Math.max(b.x1 - b.x0, b.y1 - b.y0) / 40;
    return {
      key,
      name: pick(def.name, lang) ?? key,
      image: { href: `maps/${key}.webp`, x: def.origin[0], y: def.origin[1], w: def.extent[0], h: def.extent[1] },
      box: { x: b.x0 - pad, y: b.y0 - pad, w: b.x1 - b.x0 + 2 * pad, h: b.y1 - b.y0 + 2 * pad },
      dot: Math.max(b.x1 - b.x0, b.y1 - b.y0) / 150,
      labels: [...zones.entries()].filter(([k]) => k.startsWith(`${key}|`))
        .map(([k, z]) => ({ zone: k.slice(key.length + 1), x: z.x / z.n, y: z.y / z.n })),
      monsters: list.sort(order),
    };
  }).sort((a, b) => a.name.localeCompare(b.name));
  return { regions, dungeons: dungeons.sort(order) };
}

/** The pet whose level or souls went up from `prev` to `rows` (a kill); null when none did. A pet missing from `prev` does not count. */
export function soulGain(prev: PetRow[], rows: PetRow[]): number | null {
  const before = new Map(prev.map(r => [r.id, r]));
  return rows.find(r => {
    const p = before.get(r.id);
    return p && (r.level > p.level || (r.level === p.level && r.souls > p.souls));
  })?.id ?? null;
}

/** A pet monster in view now, in map units. */
export interface LiveMonster { entity: number; id: number; petLevel: number; x: number; y: number }

/**
 * The region the monsters in view are in: the one most of them spawn in (a monster with spawns in
 * several regions splits its vote). `null` when no pet monster is in view.
 */
export function liveRegion(gd: GameData, radar: Radar | null): string | null {
  const votes = new Map<string, number>();
  for (const m of radar?.monsters ?? []) {
    const keys = Object.keys(gd.pet_monsters[m.template]?.spawns ?? {});
    for (const k of keys) votes.set(k, (votes.get(k) ?? 0) + 1 / keys.length);
  }
  let best: string | null = null;
  for (const [k, v] of [...votes].sort((a, b) => a[0].localeCompare(b[0]))) if (best === null || v > votes.get(best)!) best = k;
  return best;
}

/**
 * The region the player is in: the one the current map id belongs to, else (a map id not listed yet,
 * or none was seen) the one the monsters in view point to. `null` outside every region.
 */
export function currentRegion(gd: GameData, radar: Radar | null): string | null {
  const byMap = Object.entries(gd.pet_regions).find(([, r]) => radar?.map != null && r.maps?.includes(radar.map))?.[0];
  return byMap ?? liveRegion(gd, radar);
}

/** The radar's monsters that `region` lists (so they follow the farm filter), at their map-unit position. */
export function liveMonsters(region: FarmRegion | null, radar: Radar | null): LiveMonster[] {
  const byId = new Map((region?.monsters ?? []).map(m => [m.id, m]));
  return (radar?.monsters ?? []).flatMap(m => {
    const fm = byId.get(m.template);
    return fm ? [{ entity: m.entity, id: fm.id, petLevel: fm.petLevel, x: m.x / 100, y: m.y / 100 }] : [];
  });
}

/** Deepest map zoom: the view is never narrower than `base.w / MAX_ZOOM`. */
export const MAX_ZOOM = 16;

/**
 * The minimap view: `base.w / zoom` wide at the container's `aspect` (width / height), centred on
 * `centre` (the base's middle when null) and kept inside `image`, so it covers the container with
 * map and never shows the margin around it.
 */
export function followBox(base: Box, image: Box, centre: [number, number] | null, zoom: number, aspect: number): Box {
  const w = Math.min(image.w, image.h * aspect, base.w / zoom);
  const h = w / aspect;
  const [cx, cy] = centre ?? [base.x + base.w / 2, base.y + base.h / 2];
  return {
    x: Math.min(image.x + image.w - w, Math.max(image.x, cx - w / 2)),
    y: Math.min(image.y + image.h - h, Math.max(image.y, cy - h / 2)),
    w, h,
  };
}

/** `box` kept at `base`'s shape, between 1x and `MAX_ZOOM`x, and inside `base`. */
export function clampBox(box: Box, base: Box): Box {
  const w = Math.min(base.w, Math.max(base.w / MAX_ZOOM, box.w));
  const h = w * base.h / base.w;
  return {
    x: Math.min(base.x + base.w - w, Math.max(base.x, box.x)),
    y: Math.min(base.y + base.h - h, Math.max(base.y, box.y)),
    w, h,
  };
}

/** The view that frames `target` with room around it, at `base`'s shape. */
export function fitBox(target: Box, base: Box): Box {
  const aspect = base.h / base.w;
  const w = Math.max(target.w * 1.8, target.h * 1.8 / aspect, base.w / 8);
  const [cx, cy] = [target.x + target.w / 2, target.y + target.h / 2];
  return clampBox({ x: cx - w / 2, y: cy - w * aspect / 2, w, h: w * aspect }, base);
}
