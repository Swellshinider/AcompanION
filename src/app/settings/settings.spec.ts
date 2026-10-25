import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { CharactersService } from '../characters.service';
import { Settings } from './settings';

const stored = () => JSON.parse(localStorage.getItem('acompanion.app')!);
const tick = () => new Promise(r => setTimeout(r));
const calls = (invoke: ReturnType<typeof vi.fn>, cmd: string) => invoke.mock.calls.filter(c => c[0] === cmd);

function render(
  checkUpdate = vi.fn(async (): Promise<string | null> => null),
  invoke: ReturnType<typeof vi.fn> = vi.fn(async (..._: unknown[]) => undefined),
) {
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
  TestBed.configureTestingModule({ providers: [{ provide: CharactersService, useValue: { checkUpdate } }] });
  const f = TestBed.createComponent(Settings);
  f.detectChanges();
  const el = f.nativeElement as HTMLElement;
  const input = (name: string) => el.querySelector<HTMLInputElement>(`input[name="${name}"]`)!;
  const flip = async (name: string, checked: boolean) => {
    const box = input(name);
    box.checked = checked;
    box.dispatchEvent(new Event('change'));
    f.detectChanges(); // the browser re-renders right after the click, then again when the work settles
    await tick();
    f.detectChanges();
  };
  const check = async () => {
    el.querySelector<HTMLButtonElement>('button[name="checkUpdate"]')!.click();
    await tick();
    f.detectChanges();
  };
  return { f, el, invoke, input, flip, check, checkUpdate };
}

describe('Settings', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('shows a switch and settings for each overlay, below the app options', () => {
    const { el, input } = render();
    const panels = el.querySelectorAll('app-overlay-panel');
    expect(panels).toHaveLength(3);
    panels.forEach(p => expect(p.querySelector('input[role="switch"][name="open"]')).not.toBeNull());
    expect(el.querySelector('.sections')!.compareDocumentPosition(panels[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(input('closeToTray').checked).toBe(false);
    expect(input('reconnect').checked).toBe(true);
  });

  it('opens the patch notes page', () => {
    const { f, el } = render();
    const notes = vi.fn();
    f.componentInstance.notes.subscribe(notes);
    el.querySelector<HTMLButtonElement>('button[name="whatsNew"]')!.click();
    expect(notes).toHaveBeenCalled();
  });

  it('remembers reconnect and tells Rust', async () => {
    const { flip, invoke } = render();
    await flip('reconnect', false);
    expect(stored()).toEqual({ closeToTray: false, reconnect: false, language: 'auto' });
    expect(calls(invoke, 'app_settings').at(-1)![1]).toEqual({ closeToTray: false, reconnect: false });
  });

  it('closes to the tray only once the icon exists, and does not keep the option when it cannot be made', async () => {
    const invoke = vi.fn(async (cmd: unknown) => {
      if (String(cmd).startsWith('plugin:tray|new') || String(cmd).startsWith('plugin:menu|new')) throw new Error('no tray');
    });
    const { el, f, flip, input } = render(undefined, invoke);
    await flip('closeToTray', true);
    expect(calls(invoke, 'app_settings').at(-1)![1]).toEqual({ closeToTray: false, reconnect: true });
    expect(stored().closeToTray).toBe(false);
    expect(input('closeToTray').checked).toBe(false);
    f.detectChanges();
    expect(el.querySelector('[role="alert"]')).not.toBeNull();
  });

  it('remembers the language and asks for a restart only when it changed', async () => {
    const { f, el } = render();
    const pick = el.querySelector<HTMLSelectElement>('select[name="language"]')!;
    expect(pick.value).toBe('auto');
    expect(el.querySelector('[role="status"].note')).toBeNull();
    pick.value = 'pt-BR';
    pick.dispatchEvent(new Event('change'));
    f.detectChanges();
    expect(stored().language).toBe('pt-BR');
    expect(el.querySelector('[role="status"].note')!.textContent).toContain('Restart');
    pick.value = 'auto';
    pick.dispatchEvent(new Event('change'));
    f.detectChanges();
    expect(el.querySelector('[role="status"].note')).toBeNull();
  });

  describe('Shugo Festival alert', () => {
    const shugo = () => JSON.parse(localStorage.getItem('acompanion.shugo')!);
    const field = (el: HTMLElement, name: string) => el.querySelector<HTMLInputElement & HTMLSelectElement>(`[name="${name}"]`)!;

    it('starts with the defaults', () => {
      const { el, input } = render();
      expect(input('shugoEnabled').checked).toBe(true);
      expect(field(el, 'shugoCorner').value).toBe('bottom-right');
      expect(field(el, 'shugoSeconds').value).toBe('10');
      expect(input('shugoClickToClose').checked).toBe(true);
      expect(input('shugoSound').checked).toBe(true);
    });

    it('remembers each option', async () => {
      const { el, flip } = render();
      await flip('shugoSound', false);
      await flip('shugoClickToClose', false);
      const corner = field(el, 'shugoCorner');
      corner.value = 'top-left';
      corner.dispatchEvent(new Event('change'));
      expect(shugo()).toEqual({ enabled: true, corner: 'top-left', seconds: 10, clickToClose: false, sound: false });
    });

    it('keeps the time between 3 and 300 seconds and shows what was kept', () => {
      const { f, el } = render();
      const secs = field(el, 'shugoSeconds');
      for (const [typed, kept] of [['1', 3], ['9999', 300], ['45', 45]] as const) {
        secs.value = typed;
        secs.dispatchEvent(new Event('change'));
        f.detectChanges();
        expect(shugo().seconds).toBe(kept);
        expect(secs.value).toBe(String(kept));
      }
      secs.value = '';
      secs.dispatchEvent(new Event('change'));
      expect(shugo().seconds).toBe(45);
      expect(secs.value).toBe('45');
    });

    it('turns the other controls off with the alert', async () => {
      const { el, flip } = render();
      await flip('shugoEnabled', false);
      expect(shugo().enabled).toBe(false);
      for (const n of ['shugoCorner', 'shugoSeconds', 'shugoClickToClose', 'shugoSound', 'shugoTest']) expect(field(el, n).disabled).toBe(true);
    });

    it('shows a test alert in the chosen corner, with clicks passing through when click to close is off', async () => {
      const { el, invoke, flip } = render();
      el.querySelector<HTMLButtonElement>('button[name="shugoTest"]')!.click();
      await tick();
      expect(calls(invoke, 'shugo_alert').at(-1)![1]).toEqual({ corner: 'bottom-right', clickThrough: false });
      await flip('shugoClickToClose', false);
      el.querySelector<HTMLButtonElement>('button[name="shugoTest"]')!.click();
      await tick();
      expect(calls(invoke, 'shugo_alert').at(-1)![1]).toEqual({ corner: 'bottom-right', clickThrough: true });
    });
  });

  it('shows the current version', async () => {
    const { f, el } = render(undefined, vi.fn(async (cmd: unknown) => (cmd === 'plugin:app|version' ? '1.2.3' : undefined)));
    await tick();
    f.detectChanges();
    expect(el.querySelector('.current')!.textContent).toContain('Current version: 1.2.3');
  });

  it('says when the app is up to date', async () => {
    const { el, check, checkUpdate } = render();
    await check();
    expect(checkUpdate).toHaveBeenCalledTimes(1);
    expect(el.querySelector('.status')!.textContent).toContain('up to date');
  });

  it('hands a newer version to the dashboard', async () => {
    const { f, el, check } = render(vi.fn(async () => '0.2.0'));
    const found: string[] = [];
    f.componentInstance.update.subscribe((v: string) => found.push(v));
    await check();
    expect(found).toEqual(['0.2.0']);
    expect(el.querySelector('.status')!.textContent).toContain('0.2.0');
  });

  it('says when the check failed', async () => {
    const { el, check } = render(vi.fn(async () => { throw new Error('offline'); }));
    await check();
    expect(el.querySelector('.status .err')).not.toBeNull();
  });
});
