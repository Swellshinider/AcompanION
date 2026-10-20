import { Directive, computed, input } from '@angular/core';

/** The class icon: the game's own `public/classes/<name>.png`, or for `unknown` the generic glyph (`unknown.svg`) painted as a mask in the element's `color`. Styles: `.glyph` in `styles.scss`. */
@Directive({
  selector: '[appGlyph]',
  host: {
    class: 'glyph',
    '[class.game]': '!generic()',
    '[attr.data-icon]': 'appGlyph()',
    '[style.mask-image]': 'mask()',
    '[style.background-image]': 'image()',
  },
})
export class Glyph {
  readonly appGlyph = input.required<string>();
  readonly generic = computed(() => this.appGlyph() === 'unknown');
  readonly mask = computed(() => (this.generic() ? 'url(classes/unknown.svg)' : null));
  readonly image = computed(() => (this.generic() ? null : `url(classes/${this.appGlyph()}.png)`));
}
