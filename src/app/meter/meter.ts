import { DecimalPipe, NgTemplateOutlet, PercentPipe } from '@angular/common';
import { Component, LOCALE_ID, computed, inject, input, output, signal } from '@angular/core';
import { CharactersService, Fight, MeterPlayer } from '../characters.service';
import { Metric, clock, compact, monsterName, myPlace, outcome, playerRows, recentRates, seconds, skillHue, skillIcon, skillName, skillRows, total } from '../fight';
import { GameData, pick } from '../gamedata';
import { Glyph } from '../glyph';

/** A saved fight as a card of the list: when, against whom, how it went. */
export interface Card {
  f: Fight;
  name: string | null;
  template: number;
  result: { won: boolean; attempt: number } | null;
  time: string;
  length: string;
  dps: number;
  mine: { dps: number; rank: number } | null;
}

/** The combat meter: the main window's screen, or (`compact`) the overlay's list of the latest fight. */
@Component({
  selector: 'app-meter',
  imports: [DecimalPipe, PercentPipe, NgTemplateOutlet, Glyph],
  templateUrl: './meter.html',
  styleUrl: './meter.scss',
  host: { '[class.compact]': 'compact()' },
})
export class Meter {
  private readonly chars = inject(CharactersService);
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  readonly gamedata = input.required<GameData | null>();
  readonly compact = input(false);
  readonly back = output<void>();

  /** `start_us` of the fight picked in the selector; null follows the latest one. */
  readonly pickedStart = signal<number | null>(null);
  /** The entity whose skills and targets are open. */
  readonly pickedPlayer = signal<number | null>(null);
  readonly confirming = signal(false);
  /** What the ranking counts; the overlay always shows damage. */
  readonly metric = signal<Metric>('dealt');
  readonly metrics: readonly Metric[] = ['dealt', 'taken', 'healing'];
  private readonly shown = computed<Metric>(() => (this.compact() ? 'dealt' : this.metric()));

  /** Finished fights, the newest first. */
  readonly history = computed(() => [...this.chars.fights()].reverse());
  /** The open fight, else the last finished one. */
  private readonly latest = computed(() => this.chars.meter()?.current ?? this.history()[0] ?? null);
  readonly fight = computed<Fight | null>(() => {
    const start = this.compact() ? null : this.pickedStart();
    return (start === null ? null : this.chars.fights().find(f => f.start_us === start)) ?? this.latest();
  });
  /** The fight is still going (it is the open one), not a finished fight. */
  readonly live = computed(() => this.fight() !== null && this.fight() === this.chars.meter()?.current);
  /** Per-second figures of the last few seconds; null unless the fight is open. */
  private readonly rates = computed(() => {
    const f = this.fight();
    return f && this.live() ? recentRates(f, this.chars.recent(), this.shown()) : null;
  });
  readonly rows = computed(() => {
    const f = this.fight();
    return f ? playerRows(f, this.shown(), this.rates() ?? undefined) : [];
  });
  readonly damage = computed(() => {
    const f = this.fight();
    return f ? total(f, this.shown()) : 0;
  });
  /** The group's rate over the last few seconds; null unless the fight is open. */
  readonly currentRate = computed(() => {
    const r = this.rates();
    return r ? [...r.values()].reduce((a, b) => a + b, 0) : null;
  });
  /** How the fight shown went, for a boss. */
  readonly result = computed(() => {
    const f = this.fight();
    return f ? outcome(this.chars.fights(), f) : null;
  });
  readonly length = computed(() => {
    const f = this.fight();
    return f ? clock(seconds(f)) : '';
  });
  readonly target = computed(() => {
    const t = this.fight()?.target;
    if (!t) return null;
    const gd = this.gamedata();
    return {
      template: t.template,
      name: monsterName(gd, t.template, this.lang),
      cur: t.hp,
      max: t.max_hp,
      hp: t.hp !== null && t.max_hp ? Math.min(1, t.hp / t.max_hp) : null,
      stagger: t.stagger && t.stagger.max > 0 ? Math.min(1, t.stagger.value / t.stagger.max) : null,
      dead: t.hp === 0,
    };
  });

  /** The player whose detail is open, if still in the fight shown. */
  readonly detail = computed(() => this.rows().find(r => r.p.entity === this.pickedPlayer()) ?? null);
  readonly skills = computed(() => {
    const d = this.detail();
    const rows = d ? skillRows(d.p) : [];
    const top = rows[0]?.amount || 1;
    return rows.map(s => {
      const name = skillName(this.gamedata(), s.id, this.lang);
      return { ...s, name, icon: skillIcon(this.gamedata(), s.id), hue: skillHue(s.id), letter: (name ?? '?').charAt(0), top: s.amount / top };
    });
  });
  readonly targets = computed(() => {
    const d = this.detail();
    return d
      ? [...d.p.targets].sort((a, b) => b.amount - a.amount).map((t, _, all) => ({ ...t, name: monsterName(this.gamedata(), t.template, this.lang), top: t.amount / (all[0].amount || 1) }))
      : [];
  });

  /** Clearing needs the capture thread, which exists only while capture runs. */
  readonly canClear = computed(() => {
    const s = this.chars.status().id;
    return (s === 'running' || s === 'save_failed') && this.chars.fights().length > 0;
  });

  /** The saved fights as cards, the newest first, split by day. */
  readonly groups = computed(() => {
    const gd = this.gamedata();
    const hour = new Intl.DateTimeFormat(this.lang, { hour: '2-digit', minute: '2-digit' });
    const today = new Date().setHours(0, 0, 0, 0);
    const days = new Map<number, Card[]>();
    for (const f of this.history()) {
      const when = new Date(f.start_us / 1000);
      const t = f.target?.template ?? 0;
      const card: Card = {
        f, template: t, name: monsterName(gd, t, this.lang), result: outcome(this.chars.fights(), f), time: hour.format(when),
        length: clock(seconds(f)), dps: total(f) / seconds(f), mine: myPlace(f),
      };
      const day = new Date(when).setHours(0, 0, 0, 0);
      days.set(day, [...(days.get(day) ?? []), card]);
    }
    return [...days].map(([day, cards]) => ({ day, label: this.dayLabel(day, today), cards }));
  });

  /** `today`, `yesterday` or `Oct 20`, in the UI language. */
  private dayLabel(day: number, today: number): string {
    const ago = Math.round((today - day) / 86_400_000);
    return ago <= 1
      ? new Intl.RelativeTimeFormat(this.lang, { numeric: 'auto' }).format(ago === 0 ? 0 : -1, 'day')
      : new Intl.DateTimeFormat(this.lang, { day: 'numeric', month: 'short' }).format(day);
  }

  /** The class name in the UI language; undefined when the code is not curated. */
  cls(p: MeterPlayer): string | undefined {
    return p.class_code === null ? undefined : pick(this.gamedata()?.classes[p.class_code]?.name, this.lang);
  }

  /** The whole fight's damage per second. */
  readonly damageRate = computed(() => {
    const f = this.fight();
    return f ? this.damage() / seconds(f) : 0;
  });

  /** `5.71M`, `20.9k`: a number short enough for a ranking line. */
  short(n: number): string {
    return compact(n, this.lang);
  }

  /** The class icon's file stem; the generic one when the class is unknown. */
  icon(p: MeterPlayer): string {
    return (p.class_code === null ? undefined : this.gamedata()?.classes[p.class_code]?.icon) ?? 'unknown';
  }

  /** The class colour of a player's bar; null (the accent) when the class is unknown or has none. */
  tint(p: MeterPlayer): string | null {
    return p.class_code === null ? null : (this.gamedata()?.classes[p.class_code]?.color ?? null);
  }

  /** Show the fight starting at `start`, or (null) the open one, else the latest. */
  choose(start: number | null): void {
    this.pickedStart.set(start);
    this.pickedPlayer.set(null);
  }

  /** The player's damage per second over the whole fight, for the detail (the ranking may count something else). */
  dealtRate(p: MeterPlayer): number {
    const f = this.fight();
    return f ? p.dealt.amount / seconds(f) : 0;
  }

  toggle(entity: number): void {
    this.pickedPlayer.update(cur => (cur === entity ? null : entity));
  }

  async clear(): Promise<void> {
    this.confirming.set(false);
    this.pickedStart.set(null);
    this.pickedPlayer.set(null);
    await this.chars.clearMeter();
  }
}
