import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Glyph } from './glyph';

@Component({ imports: [Glyph], template: '<span [appGlyph]="icon"></span>' })
class Host {
  icon = 'ranger';
}

function render(icon: string) {
  const f = TestBed.createComponent(Host);
  f.componentInstance.icon = icon;
  f.detectChanges();
  return f.nativeElement.querySelector('span') as HTMLElement;
}

describe('Glyph', () => {
  it('shows the game icon of a class as an image', () => {
    const el = render('ranger');
    expect(el.classList).toContain('game');
    expect(el.style.backgroundImage).toContain('classes/ranger.png');
    expect(el.style.maskImage).toBeFalsy();
  });

  it('paints the generic glyph as a mask in the text colour', () => {
    const el = render('unknown');
    expect(el.classList).not.toContain('game');
    expect(el.style.maskImage).toContain('classes/unknown.svg');
    expect(el.style.backgroundImage).toBeFalsy();
  });
});
