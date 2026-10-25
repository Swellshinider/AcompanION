import { Component } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Dashboard } from './dashboard/dashboard';
import { Overlay } from './overlay/overlay';
import { OverlayView, VIEWS } from './overlay-settings';
import { overlayLabel } from './overlay-control';

/** The overlays are more windows on the same page; the window's label tells which view one shows. */
function overlayView(): OverlayView | null {
  try {
    const label = getCurrentWindow().label;
    return VIEWS.find(v => overlayLabel(v) === label) ?? null;
  } catch {
    return null; // outside the Tauri window (plain `ng serve`)
  }
}

@Component({
  selector: 'app-root',
  imports: [Dashboard, Overlay],
  templateUrl: './app.html',
  styleUrl: './app.scss'
})
export class App {
  readonly overlay = overlayView();
}
