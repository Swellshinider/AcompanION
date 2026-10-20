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

export interface PlayerRow {
  p: MeterPlayer;
  dps: number;
  /** Of everything the fight's players dealt, 0 to 1. */
  share: number;
  /** Of the best player's damage, 0 to 1: the bar width. */
  top: number;
  /** Crits over direct hits, 0 to 1. */
  crit: number;
  me: boolean;
}

export function total(f: Fight): number {
  return f.players.reduce((sum, p) => sum + p.dealt.amount, 0);
}

/** Players by damage dealt, the largest first; players who dealt nothing (a healer, someone only hit) stay at the end. */
export function playerRows(f: Fight): PlayerRow[] {
  const all = total(f);
  const best = Math.max(0, ...f.players.map(p => p.dealt.amount));
  const secs = seconds(f);
  return [...f.players]
    .sort((a, b) => b.dealt.amount - a.dealt.amount)
    .map(p => ({
      p,
      dps: p.dealt.amount / secs,
      share: all > 0 ? p.dealt.amount / all : 0,
      top: best > 0 ? p.dealt.amount / best : 0,
      crit: p.dealt.direct > 0 ? p.dealt.crits / p.dealt.direct : 0,
      me: p.entity === f.me,
    }));
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

/** The monster's name, or null (the template shows `Monster N`); a template of 0 is a monster whose spawn was not seen. */
export function monsterName(gd: GameData | null, template: number, lang: string): string | null {
  return pick(gd?.monsters?.[template]?.name ?? gd?.pet_monsters[template]?.name, lang) ?? null;
}
