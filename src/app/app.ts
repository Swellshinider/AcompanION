import { Component } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Dashboard } from './dashboard/dashboard';
import { Overlay } from './overlay/overlay';
import { OverlayView, VIEWS } from './overlay-settings';
import { overlayLabel } from './overlay-control';
import { SHUGO_LABEL } from './shugo';
import { ShugoAlert } from './shugo-alert/shugo-alert';

/** The window's label, which tells the overlays (and the alert) from the main window; null outside the Tauri window (plain `ng serve`). */
function windowLabel(): string | null {
  try {
    return getCurrentWindow().label;
  } catch {
    return null;
  }
}

@Component({
  selector: 'app-root',
  imports: [Dashboard, Overlay, ShugoAlert],
  templateUrl: './app.html',
  styleUrl: './app.scss'
})
export class App {
  private readonly label = windowLabel();
  /** The overlays are more windows on the same page; the label tells which view one shows. */
  readonly overlay: OverlayView | null = VIEWS.find(v => overlayLabel(v) === this.label) ?? null;
  /** The Shugo Festival pop-up is one more window on the same page. */
  readonly alert = this.label === SHUGO_LABEL;
}
