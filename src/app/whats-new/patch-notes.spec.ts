import en from '../../../docs/patch-notes.md';
import pt from '../../../docs/patch-notes.pt-BR.md';
import { parseNotes, parts, splitTopic } from './patch-notes';

const SAMPLE = `# Patch notes

Intro.

## v0.3.0 (next)

### New

- Not released yet.

## v0.2.0 (2026-10-25)

### Improved

- Meter: **bold** and plain.
- Second.

### Fixed

- Third.

## v0.1.1 (2026-10-22)

- A bare line.
`;

describe('parseNotes', () => {
  it('skips the next version, keeps the order and files a bare line under an untitled section', () => {
    const r = parseNotes(SAMPLE);
    expect(r.map(x => x.version)).toEqual(['0.2.0', '0.1.1']);
    expect(r[0].date).toBe('2026-10-25');
    expect(r[0].sections).toEqual([
      { title: 'Improved', items: ['Meter: **bold** and plain.', 'Second.'] },
      { title: 'Fixed', items: ['Third.'] },
    ]);
    expect(r[1].sections).toEqual([{ title: '', items: ['A bare line.'] }]);
  });

  it('keeps the English and Portuguese notes in step, version by version', () => {
    const shape = (md: string) => parseNotes(md).map(r => [r.version, r.date, r.sections.map(s => s.items.length)]);
    expect(parseNotes(en).length).toBeGreaterThan(0);
    expect(shape(pt)).toEqual(shape(en));
  });
});

describe('note lines', () => {
  it('splits bold marks without keeping them', () => {
    expect(parts('a **b** c')).toEqual([{ text: 'a ', bold: false }, { text: 'b', bold: true }, { text: ' c', bold: false }]);
  });

  it('splits a short topic prefix and leaves a sentence with a late colon alone', () => {
    expect(splitTopic('Meter: new look.')).toEqual({ topic: 'Meter', rest: 'new look.' });
    expect(splitTopic('You can remove a character from the roster: hover its card.').topic).toBe('');
    expect(splitTopic('The button is now **Settings**: a page.').topic).toBe('');
  });
});
