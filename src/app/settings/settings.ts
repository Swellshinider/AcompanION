import { Component, computed, inject, output, signal } from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { AppSettings, LANGUAGES, applyAppSettings, loadAppSettings, saveAppSettings } from '../app-settings';
import { CharactersService } from '../characters.service';
import { OverlayPanel } from '../overlay-panel/overlay-panel';

/** The Settings screen: the overlay's controls, the app's own preferences and the update check. */
@Component({
  selector: 'app-settings',
  imports: [OverlayPanel],
  templateUrl: './settings.html',
  styleUrl: './settings.scss',
})
export class Settings {
  private readonly chars = inject(CharactersService);
  readonly back = output<void>();
  /** A newer version was found: the dashboard shows its "Update and restart" banner. */
  readonly update = output<string>();
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

  private save(s: AppSettings): void {
    this.s.set(s);
    saveAppSettings(s);
  }
}
