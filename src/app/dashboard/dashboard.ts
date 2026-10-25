import { DecimalPipe } from '@angular/common';
import { Component, DestroyRef, ElementRef, LOCALE_ID, computed, inject, signal } from '@angular/core';
import { CharactersService } from '../characters.service';
import { CharacterDetail, CharacterRow } from '../character/character';
import { Emblem } from '../emblem/emblem';
import { DAILY_RESET_HOUR_UTC, GameData, loadGamedata, nextReset, petCollection, pick, span, summarize } from '../gamedata';
import { Glyph } from '../glyph';
import { Meter } from '../meter/meter';
import { applyAppSettings, loadAppSettings } from '../app-settings';
import { restoreOverlay } from '../overlay-control';
import { Pets } from '../pets/pets';
import { Settings } from '../settings/settings';
import { WhatsNew } from '../whats-new/whats-new';

@Component({
  selector: 'app-dashboard',
  imports: [DecimalPipe, CharacterDetail, Emblem, Glyph, Meter, Pets, Settings, WhatsNew],
  templateUrl: './dashboard.html',
  styleUrl: './dashboard.scss',
  host: { '(document:keydown.escape)': 'back()' },
})
export class Dashboard {
  private readonly chars = inject(CharactersService);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  /** Ticks once a minute so resets, refills and countdowns are re-evaluated. */
  readonly now = signal(Date.now());
  readonly status = this.chars.status;
  readonly game = this.chars.game;
  /** What the game shows around the player; null while the game is not running. */
  readonly radar = computed(() => (this.chars.game() ? this.chars.radar() : null));
  readonly gamedata = signal<GameData | null>(null);
  readonly gamedataFailed = signal(false);
  readonly restarting = signal(false);
  readonly takingOver = signal(false);
  readonly takeOverFailed = signal(false);
  /** Starts true so nothing shows without a backend. */
  readonly elevated = signal(true);
  /** The newer version on GitHub Releases; null while unknown, up to date or offline. */
  readonly update = signal<string | null>(null);
  readonly installing = signal(false);
  readonly updateFailed = signal(false);
  /** `server:name` of the character whose detail is open. */
  readonly selected = signal<string | null>(null);
  /** `server:name` of the roster card asking "Remove?". */
  readonly confirming = signal<string | null>(null);
  /** The screen: the roster (and a character's detail), the Pets screen, the combat Meter, Settings or its What's new page. */
  readonly screen = signal<'roster' | 'pets' | 'meter' | 'settings' | 'whatsnew'>('roster');
  readonly resetHour = String(DAILY_RESET_HOUR_UTC).padStart(2, '0');

  readonly rows = computed<CharacterRow[]>(() => {
    const gd = this.gamedata();
    if (!gd) return [];
    const now = this.now();
    return this.chars.characters()
      .map(c => ({
        key: `${c.server}:${c.name}`,
        c,
        s: summarize(gd, c, now),
        cls: pick(gd.classes[c.class_code]?.name, this.lang),
        icon: gd.classes[c.class_code]?.icon ?? 'unknown',
      }))
      .sort((a, b) => b.s.score - a.s.score);
  });

  /** The open character; null (back to the roster) when none is selected or it left the list. */
  readonly open = computed(() => this.rows().find(r => r.key === this.selected()) ?? null);

  /** Removing needs the capture thread, which exists only while capture runs. */
  readonly canForget = computed(() => {
    const s = this.chars.status().id;
    return s === 'running' || s === 'save_failed';
  });

  /** The account's pets; null until a character reported them. */
  readonly pets = computed(() => {
    const gd = this.gamedata();
    return gd ? petCollection(gd, this.chars.characters(), this.lang) : null;
  });

  readonly resets = computed(() => {
    const gd = this.gamedata();
    if (!gd) return null;
    const now = this.now();
    return { daily: nextReset('daily', now, gd.weekly_reset_day), weekly: nextReset('weekly', now, gd.weekly_reset_day) };
  });

  readonly weekday = computed(() => {
    const gd = this.gamedata();
    // 2023-01-01 is a Sunday
    return gd ? new Intl.DateTimeFormat(this.lang, { weekday: 'long', timeZone: 'UTC' }).format(Date.UTC(2023, 0, 1 + gd.weekly_reset_day)) : '';
  });

  constructor() {
    loadGamedata().then(gd => this.gamedata.set(gd), () => this.gamedataFailed.set(true));
    this.chars.isElevated().then(e => this.elevated.set(e), () => {});
    // offline or no backend: no banner
    this.chars.checkUpdate().then(v => this.update.set(v), () => {});
    // send Rust the saved preferences (and bring the tray icon back); no backend: nothing to do
    applyAppSettings(loadAppSettings()).catch(() => {});
    const stopOverlay = restoreOverlay();
    const timer = setInterval(() => this.now.set(Date.now()), 60_000);
    inject(DestroyRef).onDestroy(() => {
      clearInterval(timer);
      stopOverlay();
    });
  }

  /** Time from now until `at` (epoch ms). */
  until(at: number): string {
    return span(at - this.now(), this.lang);
  }

  /** Time since a capture timestamp (µs since the epoch). */
  ago(us: number): string {
    return span(this.now() - us / 1000, this.lang);
  }

  back(): void {
    if (this.confirming() !== null) {
      this.confirming.set(null);
      return;
    }
    if (this.screen() !== 'roster') {
      this.screen.set(this.screen() === 'whatsnew' ? 'settings' : 'roster');
      return;
    }
    const key = this.selected();
    if (key === null) return;
    this.selected.set(null);
    // the roster re-creates its cards: give focus back to the one that was open
    setTimeout(() => this.host.nativeElement.querySelectorAll<HTMLElement>('.card').forEach(el => {
      if (el.dataset['key'] === key) el.focus();
    }));
  }

  /** The character selection, from wherever the user is. */
  roster(): void {
    this.screen.set('roster');
    this.selected.set(null);
  }

  /** The roster drops the character when the capture thread confirms the removal. */
  forget(r: CharacterRow): void {
    this.confirming.set(null);
    this.chars.forget(r.c).catch(() => {});
  }

  async restart(): Promise<void> {
    this.restarting.set(true);
    try {
      await this.chars.restartAsAdmin(); // on success this window closes
    } finally {
      this.restarting.set(false);
    }
  }

  async takeOver(): Promise<void> {
    this.takingOver.set(true);
    this.takeOverFailed.set(false);
    try {
      await this.chars.takeOver(); // on success the status changes and the banner goes away
    } catch {
      this.takeOverFailed.set(true);
    } finally {
      this.takingOver.set(false);
    }
  }

  async install(): Promise<void> {
    this.installing.set(true);
    this.updateFailed.set(false);
    try {
      await this.chars.installUpdate(); // on success the app restarts
    } catch {
      this.updateFailed.set(true);
      this.installing.set(false);
    }
  }
}
