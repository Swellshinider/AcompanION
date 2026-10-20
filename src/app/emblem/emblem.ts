import { Component, input } from '@angular/core';
import { Glyph } from '../glyph';

/** The HUD-style diamond: class glyph inside, level plate on the bottom vertex. `size` is the box side in px. */
@Component({
  selector: 'app-emblem',
  imports: [Glyph],
  templateUrl: './emblem.html',
  styleUrl: './emblem.scss',
})
export class Emblem {
  readonly icon = input.required<string>();
  readonly level = input<number | null>(null);
  readonly size = input(84);
  readonly ghost = input(false);
}
