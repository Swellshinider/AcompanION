import source from '../locale/messages.xlf';
import pt from '../locale/messages.pt-BR.xlf';
import gamedata from '../../public/gamedata.json';
import untranslated from './untranslated-names.json';

/**
 * Guards for the rule "every user-facing text exists in every language" (AGENTS.md, Conventions).
 * To add a language: add its file to `LOCALES` and its key to `LANGS`.
 */
const LOCALES = { 'pt-BR': pt };
const LANGS = ['en', 'pt'];

/** Per message id: the placeholder ids of its source and, when there is one, of its target. */
function units(xml: string): Map<string, { source: string[]; target: string[] | null }> {
  const doc = new DOMParser().parseFromString(xml, 'application/xml');
  const ph = (e: Element | undefined) => (e ? Array.from(e.getElementsByTagName('x'), x => x.getAttribute('id')!).sort() : null);
  return new Map(Array.from(doc.getElementsByTagName('trans-unit'), u => [
    u.getAttribute('id')!,
    { source: ph(u.getElementsByTagName('source')[0])!, target: ph(u.getElementsByTagName('target')[0]) },
  ]));
}

describe('interface translations', () => {
  const base = units(source);

  for (const [locale, xml] of Object.entries(LOCALES)) {
    it(`${locale} has a target for every message in messages.xlf, with the same placeholders (run task i18n, then translate)`, () => {
      const t = units(xml);
      expect([...base.keys()].filter(id => !t.get(id)?.target)).toEqual([]);
      expect([...t.keys()].filter(id => !base.has(id))).toEqual([]);
      expect([...base].filter(([id, u]) => t.get(id) && JSON.stringify(t.get(id)!.target) !== JSON.stringify(u.source)).map(([id]) => id)).toEqual([]);
    });
  }
});

describe('game data names', () => {
  type Entry = { name: Record<string, string> };
  const sections = Object.entries(gamedata as unknown as Record<string, unknown>).flatMap(([sec, v]) =>
    typeof v === 'object' && v && Object.values(v).every(e => typeof (e as Entry)?.name === 'object')
      ? [[sec, v as Record<string, Entry>] as const] : []);
  const allow = untranslated as Record<string, string[]>;
  const names = sections.flatMap(([sec, v]) => Object.entries(v).map(([id, e]) => ({ sec, id, name: e.name })));

  it('finds the name maps', () => {
    expect(sections.map(([s]) => s)).toEqual(expect.arrayContaining(['classes', 'counters', 'pets', 'skills', 'monsters']));
  });

  it('has every language on every name, or lists it in untranslated-names.json', () => {
    const missing = names.filter(n => LANGS.some(l => !n.name[l]) && !allow[n.sec]?.includes(n.id));
    expect(missing.map(n => `${n.sec}/${n.id} ${n.name['en']}`)).toEqual([]);
  });

  it('lists only names that are still untranslated (remove an entry once it has every language)', () => {
    const done = names.filter(n => LANGS.every(l => n.name[l]) && allow[n.sec]?.includes(n.id));
    expect(done.map(n => `${n.sec}/${n.id} ${n.name['en']}`)).toEqual([]);
    const known = new Set(names.map(n => `${n.sec}/${n.id}`));
    expect(Object.entries(allow).flatMap(([s, ids]) => ids.map(id => `${s}/${id}`)).filter(k => !known.has(k))).toEqual([]);
  });
});
