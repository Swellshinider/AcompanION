import { Component, LOCALE_ID, inject, output, signal } from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { notesFor, parts, splitTopic } from './patch-notes';

/** Section titles (English and Portuguese, as the notes files write them) and the colour each one gets. */
const KINDS: Record<string, 'new' | 'improved' | 'fixed'> = {
  new: 'new', novo: 'new',
  improved: 'improved', melhorado: 'improved',
  fixed: 'fixed', corrigido: 'fixed',
};

/** What's new: the patch notes of every released version, newest first, with the installed one marked. */
@Component({
  selector: 'app-whats-new',
  templateUrl: './whats-new.html',
  styleUrl: './whats-new.scss',
})
export class WhatsNew {
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  readonly back = output<void>();
  readonly releases = notesFor(this.lang);
  readonly parts = parts;
  readonly splitTopic = splitTopic;
  /** Null outside the Tauri window. */
  readonly version = signal<string | null>(null);
  private readonly dates = new Intl.DateTimeFormat(this.lang, { dateStyle: 'long', timeZone: 'UTC' });

  constructor() {
    getVersion().then(v => this.version.set(v), () => undefined);
  }

  date(iso: string): string {
    return this.dates.format(new Date(iso));
  }

  kind(title: string): string {
    return KINDS[title.toLowerCase()] ?? '';
  }
}
