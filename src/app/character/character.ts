import { DecimalPipe, NgTemplateOutlet } from '@angular/common';
import { Component, ElementRef, LOCALE_ID, afterNextRender, computed, inject, input, output, signal } from '@angular/core';
import { Character } from '../characters.service';
import { Emblem } from '../emblem/emblem';
import { Item, Summary, pick, span } from '../gamedata';
import { Glyph } from '../glyph';

/** One character as the roster and the detail screen show it. */
export interface CharacterRow {
  key: string;
  c: Character;
  s: Summary;
  /** Class name in the UI language; undefined when the class code is not curated. */
  cls: string | undefined;
  /** Glyph file stem under `public/classes/`. */
  icon: string;
}

type Filter = 'all' | 'tickets' | 'weekly' | 'daily';

type Entry =Item & { wasting: boolean; soon: boolean; done: boolean };

const HOUR_MS = 3_600_000;
const FAR = Number.MAX_SAFE_INTEGER;
const byLeft = (a: Entry, b: Entry) => Number(a.done) - Number(b.done) || b.left / b.cap - a.left / a.cap;

@Component({
  selector: 'app-character',
  imports: [DecimalPipe, NgTemplateOutlet, Emblem, Glyph],
  templateUrl: './character.html',
  styleUrl: './character.scss',
  host: { '[class.compact]': 'compact()' },
})
export class CharacterDetail {
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  readonly row = input.required<CharacterRow>();
  /** Epoch ms, ticked by the parent. */
  readonly now = input.required<number>();
  readonly dailyReset = input.required<number>();
  readonly weeklyReset = input.required<number>();
  readonly back = output<void>();
  /** Shown in the overlay window: the sections only, without Back and the hero. */
  readonly compact = input(false);
  readonly hourMs = HOUR_MS;
  /** Which sections the detail shows; the filter chips set it. */
  readonly filter = signal<Filter>('all');

  private readonly entries = computed<Entry[]>(() =>
    this.row().s.items.map(i => ({
      ...i,
      wasting: i.left >= i.cap && i.refill !== null,
      soon: i.fullAt !== null && i.fullAt - this.now() < 24 * HOUR_MS,
      done: i.category !== 'ticket' && i.category !== 'energy' && i.left <= 0,
    })));

  readonly tickets = computed(() => this.entries()
    .filter(i => i.category === 'ticket' || i.category === 'energy')
    .sort((a, b) => Number(b.wasting) - Number(a.wasting) || (a.fullAt ?? FAR) - (b.fullAt ?? FAR)));
  readonly wasting = computed(() => this.tickets().filter(i => i.wasting));
  readonly weekly = computed(() => this.entries().filter(i => i.category === 'weekly').sort(byLeft));
  readonly daily = computed(() => this.entries().filter(i => i.category === 'daily').sort(byLeft));

  constructor() {
    const host = inject<ElementRef<HTMLElement>>(ElementRef);
    afterNextRender(() => host.nativeElement.querySelector<HTMLElement>('.back')?.focus());
  }

  name(i: Item): string {
    return pick(i.name, this.lang) ?? `#${i.id}`;
  }

  /** Time from now until `at` (epoch ms). */
  until(at: number): string {
    return span(at - this.now(), this.lang);
  }

  /** Time since a capture timestamp (µs since the epoch). */
  ago(us: number): string {
    return span(this.now() - us / 1000, this.lang);
  }

  /** Tickets and energy are stored resources, so they show what is left; counters show what was done. */
  shown(i: Item): number {
    return i.category === 'ticket' || i.category === 'energy' ? i.left : i.cap - i.left;
  }

  pct(i: Item): number {
    return Math.min(100, (this.shown(i) / i.cap) * 100);
  }
}
