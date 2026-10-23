import type { Fight, MeterPlayer, SkillTally, Tally } from './characters.service';
import { type GameData, pick } from './gamedata';

/** The skill without its variant. ponytail: a hypothesis (`id / 10000 * 10000` groups the variants of one skill in the recordings); ids below 10,000,000 are kept. */
export function foldSkill(id: number): number {
  return id >= 10_000_000 ? Math.floor(id / 10_000) * 10_000 : id;
}

/** ponytail: the fight lasts from its first to its last counted hit (the payload has no "now"), at least one second. */
export function seconds(f: Fight): number {
  return Math.max(1, (f.end_us - f.start_us) / 1e6);
}

/** `5.71M`, `20.9k`, `745k`, `830`: three significant digits. */
export function compact(n: number, lang: string): string {
  // `20.9k` like the other meters, not the `20.9K` of English
  return new Intl.NumberFormat(lang, { notation: 'compact', maximumSignificantDigits: 3 }).format(n).replace(/(\d)K$/, '$1k');
}

/** `m:ss`. */
export function clock(secs: number): string {
  const s = Math.round(secs);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** What the ranking counts. */
export type Metric = 'dealt' | 'taken' | 'healing';

export interface PlayerRow {
  p: MeterPlayer;
  /** The metric's total for this player. */
  amount: number;
  /** The metric per second over the whole fight (the average). */
  dps: number;
  /** The metric per second over the last few seconds; null when the fight is not live. */
  now: number | null;
  /** Of everything the fight's players counted, 0 to 1. */
  share: number;
  /** Of the best player's amount, 0 to 1: the bar width. */
  top: number;
  /** Crits over direct hits, 0 to 1 (damage dealt only). */
  crit: number;
  me: boolean;
}

export function total(f: Fight, metric: Metric = 'dealt'): number {
  return f.players.reduce((sum, p) => sum + p[metric].amount, 0);
}

/** The "current DPS" looks this far back. */
export const WINDOW_US = 5_000_000;

/**
 * `recent` plus `cur`, for the current rate: the open fight's snapshots, the newest one at or before the
 * window kept as its base. Another fight, or none open, starts over.
 */
export function pushRecent(recent: Fight[], cur: Fight | null, windowUs = WINDOW_US): Fight[] {
  if (!cur) return [];
  const next = recent.filter(f => f.start_us === cur.start_us && f.end_us < cur.end_us);
  next.push(cur); // a snapshot with the same last hit as `cur` is replaced
  const cut = cur.end_us - windowUs;
  let base = -1;
  for (let i = 0; i < next.length; i++) if (next[i].end_us <= cut) base = i;
  return base > 0 ? next.slice(base) : next;
}

/**
 * Per second over the last `windowUs` of the open fight, per player; the fight's start is the base while
 * it is younger than the window, so the first seconds show the average.
 * ponytail: the payload has no "now" (the fight's end is its last hit), so with nobody hitting the figure
 * stays where it was until the fight closes; give the payload a clock if that looks wrong.
 */
export function recentRates(now: Fight, recent: Fight[], metric: Metric = 'dealt', windowUs = WINDOW_US): Map<number, number> {
  const cut = now.end_us - windowUs;
  let base: Fight | undefined;
  for (const f of recent) if (f.start_us === now.start_us && f.end_us <= cut) base = f;
  const secs = Math.max(1, (now.end_us - (base?.end_us ?? now.start_us)) / 1e6);
  const before = new Map(base?.players.map(p => [p.entity, p[metric].amount]));
  return new Map(now.players.map(p => [p.entity, Math.max(0, p[metric].amount - (before.get(p.entity) ?? 0)) / secs]));
}

/**
 * Players by the metric, the largest first; players who counted nothing (a healer, someone only hit) stay
 * at the end. `rates` (from `recentRates`) fills `now`; without it the fight is not live.
 */
export function playerRows(f: Fight, metric: Metric = 'dealt', rates?: Map<number, number>): PlayerRow[] {
  const all = total(f, metric);
  const best = Math.max(0, ...f.players.map(p => p[metric].amount));
  const secs = seconds(f);
  return [...f.players]
    .sort((a, b) => b[metric].amount - a[metric].amount)
    .map(p => ({
      p,
      amount: p[metric].amount,
      dps: p[metric].amount / secs,
      now: rates ? (rates.get(p.entity) ?? 0) : null,
      share: all > 0 ? p[metric].amount / all : 0,
      top: best > 0 ? p[metric].amount / best : 0,
      crit: p.dealt.direct > 0 ? p.dealt.crits / p.dealt.direct : 0,
      me: p.entity === f.me,
    }));
}

/** A boss fight's result: a win, or the n-th try in a row on the same boss. Null for a fight without a boss (no stagger gauge). */
export function outcome(history: Fight[], f: Fight): { won: boolean; attempt: number } | null {
  if (!f.target?.stagger) return null;
  if (f.target.hp === 0) return { won: true, attempt: 0 };
  let attempt = 1;
  const before = history.filter(h => h.start_us < f.start_us).sort((a, b) => b.start_us - a.start_us);
  for (const h of before) {
    if (h.target?.template !== f.target.template || h.target.hp === 0 || !h.target.stagger) break;
    attempt++;
  }
  return { won: false, attempt };
}

/** Your damage rate and place (1-based) in a fight; null when you are not in it. */
export function myPlace(f: Fight): { dps: number; rank: number } | null {
  const rows = playerRows(f);
  const i = rows.findIndex(r => r.me);
  return i < 0 ? null : { dps: rows[i].dps, rank: i + 1 };
}

export interface SkillRow extends Tally {
  /** The folded skill id. */
  id: number;
  share: number;
  crit: number;
}

/** A player's skills with the variants of one skill added together, the largest first. */
export function skillRows(p: MeterPlayer): SkillRow[] {
  const byId = new Map<number, SkillTally>();
  for (const s of p.skills) {
    const id = foldSkill(s.id);
    const t = byId.get(id) ?? { id, amount: 0, hits: 0, direct: 0, crits: 0 };
    t.amount += s.amount;
    t.hits += s.hits;
    t.direct += s.direct;
    t.crits += s.crits;
    byId.set(id, t);
  }
  return [...byId.values()]
    .sort((a, b) => b.amount - a.amount)
    .map(t => ({ ...t, share: p.dealt.amount > 0 ? t.amount / p.dealt.amount : 0, crit: t.direct > 0 ? t.crits / t.direct : 0 }));
}

/** The skill's name, or null when the game data does not list it (the template shows `Skill N`). */
export function skillName(gd: GameData | null, id: number, lang: string): string | null {
  return pick(gd?.skills?.[id]?.name, lang) ?? null;
}

/** The skill's icon file stem in `public/skills/`, or null when the game data has none. */
export function skillIcon(gd: GameData | null, id: number): string | null {
  return gd?.skills?.[id]?.icon ?? null;
}

/** A stable hue (0 to 359) per skill, for its tile and its slice of the damage mix. */
export function skillHue(id: number): number {
  return ((id >= 10_000_000 ? Math.floor(id / 10_000) : id) * 47) % 360;
}

/** The monster's name, or null (the template shows `Monster N`); a template of 0 is a monster whose spawn was not seen. */
export function monsterName(gd: GameData | null, template: number, lang: string): string | null {
  return pick(gd?.monsters?.[template]?.name ?? gd?.pet_monsters[template]?.name, lang) ?? null;
}
