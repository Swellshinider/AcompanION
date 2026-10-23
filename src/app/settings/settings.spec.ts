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

  it('shows the overlay controls with the app options', () => {
    const { el, input } = render();
    expect(el.querySelector('app-overlay-panel input[name="open"]')).not.toBeNull();
    expect(input('closeToTray').checked).toBe(false);
    expect(input('reconnect').checked).toBe(true);
  });

  it('remembers reconnect and tells Rust', async () => {
    const { flip, invoke } = render();
    await flip('reconnect', false);
    expect(stored()).toEqual({ closeToTray: false, reconnect: false });
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
