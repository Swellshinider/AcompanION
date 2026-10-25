import { Component, computed, inject, output, signal } from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { AppSettings, LANGUAGES, applyAppSettings, loadAppSettings, saveAppSettings } from '../app-settings';
import { CharactersService } from '../characters.service';
import { OverlayPanel } from '../overlay-panel/overlay-panel';
import { VIEWS } from '../overlay-settings';
import { MAX_SECONDS, MIN_SECONDS, ShugoSettings, loadShugoSettings, parse as parseShugo, saveShugoSettings, showShugoAlert } from '../shugo';

/** The Settings screen: the app's own preferences, the update check and each overlay's controls. */
@Component({
  selector: 'app-settings',
  imports: [OverlayPanel],
  templateUrl: './settings.html',
  styleUrl: './settings.scss',
})
export class Settings {
  private readonly chars = inject(CharactersService);
  readonly back = output<void>();
  readonly views = VIEWS;
  readonly minSeconds = MIN_SECONDS;
  readonly maxSeconds = MAX_SECONDS;
  /** A newer version was found: the dashboard shows its "Update and restart" banner. */
  readonly update = output<string>();
  /** Open the What's new screen. */
  readonly notes = output<void>();
  readonly s = signal(loadAppSettings());
  private readonly startLanguage = this.s().language;
  /** The language differs from the one this run started with: it applies after a restart. */
  readonly languageChanged = computed(() => this.s().language !== this.startLanguage);
  /** The tray icon could not be created, so closing the window still quits. */
  readonly trayFailed = signal(false);
  readonly check = signal<'idle' | 'checking' | 'current' | 'found' | 'failed'>('idle');
  readonly found = signal<string | null>(null);
  /** Null outside the Tauri window. */
  readonly version = signal<string | null>(null);

  constructor() {
    getVersion().then(v => this.version.set(v), () => undefined);
  }

  async toggle(key: keyof AppSettings, e: Event): Promise<void> {
    this.trayFailed.set(false);
    this.save({ ...this.s(), [key]: (e.target as HTMLInputElement).checked });
    try {
      await applyAppSettings(this.s());
    } catch {
      // Rust was told "no tray"; do not remember the option as on. Without a backend (`ng serve`) nothing else to do.
      if (key === 'closeToTray') {
        this.save({ ...this.s(), closeToTray: false });
        this.trayFailed.set(true);
      }
    }
  }

  /** The Shugo Festival alert's preferences: checked, kept, and read by the alert window from storage. */
  readonly shugo = signal(loadShugoSettings());

  toggleShugo(key: 'enabled' | 'clickToClose' | 'sound', e: Event): void {
    this.saveShugo({ ...this.shugo(), [key]: (e.target as HTMLInputElement).checked });
  }

  setCorner(e: Event): void {
    this.saveShugo({ ...this.shugo(), corner: (e.target as HTMLSelectElement).value as ShugoSettings['corner'] });
  }

  setSeconds(e: Event): void {
    const box = e.target as HTMLInputElement;
    if (!Number.isNaN(box.valueAsNumber)) this.saveShugo({ ...this.shugo(), seconds: box.valueAsNumber });
    box.value = String(this.shugo().seconds); // show what was kept (a value out of range is clamped)
  }

  /** Without a backend (plain `ng serve`) there is no window to show. */
  testShugo(): void {
    showShugoAlert(this.shugo()).catch(() => undefined);
  }

  setLanguage(e: Event): void {
    const language = LANGUAGES.find(l => l === (e.target as HTMLSelectElement).value);
    if (language) this.save({ ...this.s(), language });
  }

  async checkNow(): Promise<void> {
    this.check.set('checking');
    try {
      const v = await this.chars.checkUpdate();
      this.found.set(v);
      this.check.set(v ? 'found' : 'current');
      if (v) this.update.emit(v);
    } catch {
      this.check.set('failed');
    }
  }

  private saveShugo(s: ShugoSettings): void {
    const checked = parseShugo(s);
    this.shugo.set(checked);
    saveShugoSettings(checked);
  }

  private save(s: AppSettings): void {
    this.s.set(s);
    saveAppSettings(s);
  }
}
