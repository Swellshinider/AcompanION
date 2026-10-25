import en from '../../../docs/patch-notes.md';
import pt from '../../../docs/patch-notes.pt-BR.md';

export interface Release {
  version: string;
  /** `YYYY-MM-DD`. */
  date: string;
  /** A section title is empty for notes filed without a `###` heading. */
  sections: { title: string; items: string[] }[];
}

/** A piece of a note line: plain text or `**bold**`. */
export interface Part {
  text: string;
  bold: boolean;
}

/** The released versions of a patch-notes file, newest first as written; the `(next)` section is not released and is skipped. */
export function parseNotes(md: string): Release[] {
  const releases: Release[] = [];
  let release: Release | null = null;
  for (const line of md.split(/\r?\n/)) {
    if (line.startsWith('## ')) {
      const m = /^## v(\d+\.\d+\.\d+) \((\d{4}-\d{2}-\d{2})\)\s*$/.exec(line);
      release = m ? { version: m[1], date: m[2], sections: [] } : null;
      if (release) releases.push(release);
    } else if (release && line.startsWith('### ')) {
      release.sections.push({ title: line.slice(4).trim(), items: [] });
    } else if (release && line.startsWith('- ')) {
      if (release.sections.length === 0) release.sections.push({ title: '', items: [] });
      release.sections[release.sections.length - 1].items.push(line.slice(2).trim());
    }
  }
  return releases;
}

/** The notes in the app language (`en` or `pt`, as `LOCALE_ID` gives it before the dash). */
export function notesFor(lang: string): Release[] {
  return parseNotes(lang === 'pt' ? pt : en);
}

/** Splits a line on its `**bold**` marks, so the template needs no innerHTML. */
export function parts(text: string): Part[] {
  return text.split('**').map((t, i) => ({ text: t, bold: i % 2 === 1 })).filter(p => p.text !== '');
}

/** A short `Topic: ` prefix (`Meter: ...`) is split off so it can be styled apart. */
export function splitTopic(text: string): { topic: string; rest: string } {
  const m = /^([^:*]{1,28}): (.*)$/.exec(text);
  return m ? { topic: m[1], rest: m[2] } : { topic: '', rest: text };
}
