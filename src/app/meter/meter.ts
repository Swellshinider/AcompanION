import { DecimalPipe, NgTemplateOutlet, PercentPipe } from '@angular/common';
import { Component, LOCALE_ID, computed, inject, input, output, signal } from '@angular/core';
import { CharactersService, Fight, MeterPlayer } from '../characters.service';
import { clock, compact, monsterName, playerRows, seconds, skillName, skillRows, total } from '../fight';
import { GameData, pick } from '../gamedata';
import { Glyph } from '../glyph';

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
  readonly rows = computed(() => {
    const f = this.fight();
    return f ? playerRows(f) : [];
  });
  readonly damage = computed(() => {
    const f = this.fight();
    return f ? total(f) : 0;
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
      hp: t.hp !== null && t.max_hp ? Math.min(1, t.hp / t.max_hp) : null,
      stagger: t.stagger && t.stagger.max > 0 ? Math.min(1, t.stagger.value / t.stagger.max) : null,
    };
  });

  /** The player whose detail is open, if still in the fight shown. */
  readonly detail = computed(() => this.rows().find(r => r.p.entity === this.pickedPlayer()) ?? null);
  readonly skills = computed(() => {
    const d = this.detail();
    return d ? skillRows(d.p).map(s => ({ ...s, name: skillName(this.gamedata(), s.id, this.lang) })) : [];
  });
  readonly targets = computed(() => {
    const d = this.detail();
    return d
      ? [...d.p.targets].sort((a, b) => b.amount - a.amount).map(t => ({ ...t, name: monsterName(this.gamedata(), t.template, this.lang) }))
      : [];
  });

  /** Clearing needs the capture thread, which exists only while capture runs. */
  readonly canClear = computed(() => {
    const s = this.chars.status().id;
    return (s === 'running' || s === 'save_failed') && this.chars.fights().length > 0;
  });

  /** A fight in the selector: when it began, how long it lasted, what it dealt and to whom. */
  label(f: Fight): string {
    const at = new Intl.DateTimeFormat(this.lang, { hour: '2-digit', minute: '2-digit' }).format(f.start_us / 1000);
    const t = f.target ? (monsterName(this.gamedata(), f.target.template, this.lang) ?? `#${f.target.template}`) : '';
    return [at, clock(seconds(f)), new Intl.NumberFormat(this.lang).format(total(f)), t].filter(Boolean).join(' · ');
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

  choose(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    this.pickedStart.set(v === '' ? null : Number(v));
    this.pickedPlayer.set(null);
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
