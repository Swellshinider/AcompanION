import { Component } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Dashboard } from './dashboard/dashboard';
import { Overlay } from './overlay/overlay';

/** The overlay is a second window on the same page; it is told apart by the window's label. */
function isOverlayWindow(): boolean {
  try {
    return getCurrentWindow().label === 'overlay';
  } catch {
    return false; // outside the Tauri window (plain `ng serve`)
  }
}

@Component({
  selector: 'app-root',
  imports: [Dashboard, Overlay],
  templateUrl: './app.html',
  styleUrl: './app.scss'
})
export class App {
  readonly overlay = isOverlayWindow();
}
