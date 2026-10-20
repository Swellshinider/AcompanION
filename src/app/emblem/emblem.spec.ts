import { TestBed } from '@angular/core/testing';
import { Emblem } from './emblem';

function render(inputs: Record<string, unknown>) {
  const f = TestBed.createComponent(Emblem);
  for (const [k, v] of Object.entries(inputs)) f.componentRef.setInput(k, v);
  f.detectChanges();
  return f.nativeElement as HTMLElement;
}

describe('Emblem', () => {
  it('draws the class glyph and the level plate', () => {
    const el = render({ icon: 'ranger', level: 60 });
    expect(el.querySelector('.mark')?.getAttribute('data-icon')).toBe('ranger');
    expect(el.querySelector('.lv')?.textContent).toContain('60');
  });

  it('omits the plate without a level', () => {
    const el = render({ icon: 'unknown', ghost: true });
    expect(el.querySelector('.lv')).toBeNull();
    expect(el.querySelector('.emblem')?.classList).toContain('ghost');
  });
});
