import { Component, DestroyRef, LOCALE_ID, computed, effect, inject, signal, untracked } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Character, CharactersService } from '../characters.service';
import { CharacterDetail, CharacterRow } from '../character/character';
import { GameData, currentRegion, loadGamedata, nextReset, petCollection, pick, summarize } from '../gamedata';
import { OVERLAY_SETTINGS_EVENT, updateOverlay } from '../overlay-control';
import { MIN_OPACITY, loadLastRegion, loadOverlaySettings, parse, saveLastRegion, saveOverlaySettings } from '../overlay-settings';
import { Meter } from '../meter/meter';
import { Pets } from '../pets/pets';

/** The overlay window: the pets radar, the combat meter or the current character's dailies, over the game or on another monitor. */
@Component({
  selector: 'app-overlay',
  imports: [CharacterDetail, Meter, Pets],
  templateUrl: './overlay.html',
  styleUrl: './overlay.scss',
})
export class Overlay {
  private readonly chars = inject(CharactersService);
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  readonly settings = signal(loadOverlaySettings());
  readonly minOpacity = MIN_OPACITY;
  readonly now = signal(Date.now());
  readonly gamedata = signal<GameData | null>(null);
  /** What the game shows around the player; null while the game is not running. */
  readonly radar = computed(() => (this.chars.game() ? this.chars.radar() : null));

  /** Whether the window is shown: always, or (auto-hide) only while the game runs (and, for the radar, the player is in or was last seen in a known region). */
  readonly shown = computed(() => {
    const s = this.settings();
    if (!s.autoHide) return true;
    // only the radar needs a map: the dailies and the meter just need the game being up
    if (s.view !== 'radar') return this.chars.game();
    const gd = this.gamedata();
    // an app started with the game already running has not seen a map yet: the last known region stands in
    return gd !== null && (currentRegion(gd, this.radar()) ?? this.lastKnown(gd)) !== null;
  });

  /** The region the player was last seen in (kept across runs), while it is still a region. */
  readonly lastRegion = signal(loadLastRegion());
  private lastKnown(gd: GameData): string | null {
    const key = this.lastRegion();
    return key !== null && key in gd.pet_regions ? key : null;
  }

  readonly pets = computed(() => {
    const gd = this.gamedata();
    return gd ? petCollection(gd, this.chars.characters(), this.lang) : null;
  });

  /** The character seen last: the one being played. */
  readonly row = computed<CharacterRow | null>(() => {
    const gd = this.gamedata();
    const c = this.chars.characters().reduce<Character | null>((best, x) => (best === null || x.last_seen_us > best.last_seen_us ? x : best), null);
    return gd && c ? {
      key: `${c.server}:${c.name}`, c, s: summarize(gd, c, this.now()),
      cls: pick(gd.classes[c.class_code]?.name, this.lang), icon: gd.classes[c.class_code]?.icon ?? 'unknown',
    } : null;
  });

  readonly resets = computed(() => {
    const gd = this.gamedata();
    const now = this.now();
    return gd ? { daily: nextReset('daily', now, gd.weekly_reset_day), weekly: nextReset('weekly', now, gd.weekly_reset_day) } : null;
  });

  constructor() {
    const html = document.documentElement;
    html.classList.add('overlay');
    loadGamedata().then(gd => this.gamedata.set(gd), () => undefined);
    const timer = setInterval(() => this.now.set(Date.now()), 60_000);
    let saveTimer: ReturnType<typeof setTimeout> | undefined;
    const unlisten: (() => void)[] = [];
    inject(DestroyRef).onDestroy(() => {
      clearInterval(timer);
      clearTimeout(saveTimer);
      unlisten.forEach(u => u());
      html.classList.remove('overlay');
    });
    effect(() => {
      const gd = this.gamedata();
      const now = gd ? currentRegion(gd, this.radar()) : null;
      if (now !== null && now !== untracked(this.lastRegion)) {
        this.lastRegion.set(now);
        saveLastRegion(now);
      }
    });
    // Rust creates the window hidden: this decides when it is seen
    effect(() => {
      const show = this.shown();
      try {
        const win = getCurrentWindow();
        void (show ? win.show() : win.hide()).catch(() => undefined);
      } catch {
        // no Tauri window (plain `ng serve`)
      }
    });
    // outside the Tauri window (plain `ng serve`, tests) there is no window to follow
    void (async () => {
      try {
        const win = getCurrentWindow();
        unlisten.push(await listen(OVERLAY_SETTINGS_EVENT, e => this.settings.set(parse(e.payload))));
        const remember = () => {
          clearTimeout(saveTimer);
          saveTimer = setTimeout(async () => {
            const [p, s] = [await win.outerPosition(), await win.outerSize()];
            saveOverlaySettings({ ...loadOverlaySettings(), bounds: [p.x, p.y, s.width, s.height] });
          }, 500);
        };
        unlisten.push(await win.onMoved(remember), await win.onResized(remember));
      } catch {
        // see above
      }
    })();
  }

  /** The slider over the map: applies now and is remembered; the main window's panel shows it the next time it opens. */
  fade(e: Event): void {
    const opacity = Number((e.target as HTMLInputElement).value);
    this.settings.update(s => ({ ...s, opacity }));
    saveOverlaySettings({ ...loadOverlaySettings(), opacity });
  }

  close(): void {
    void updateOverlay({ open: false });
  }
}
