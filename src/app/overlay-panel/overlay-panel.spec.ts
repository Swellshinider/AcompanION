import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { OverlayPanel } from './overlay-panel';
import { OverlayView } from '../overlay-settings';

const stored = (view: OverlayView = 'radar') => JSON.parse(localStorage.getItem(`acompanion.overlay.${view}`)!);
const tick = () => new Promise(r => setTimeout(r));

function render(invoke = vi.fn(async (..._: unknown[]) => undefined), view: OverlayView = 'radar') {
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
  const f = TestBed.createComponent(OverlayPanel);
  f.componentRef.setInput('view', view);
  f.detectChanges();
  const el = f.nativeElement as HTMLElement;
  const input = (name: string) => el.querySelector<HTMLInputElement>(`input[name="${name}"]`)!;
  return { f, el, invoke, input };
}
const calls = (invoke: ReturnType<typeof vi.fn>, cmd: string) => invoke.mock.calls.filter(c => c[0] === cmd);

describe('OverlayPanel', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('opens the overlay through Rust, tells the window, and remembers it', async () => {
    const { f, input, invoke } = render();
    const open = input('open');
    open.checked = true;
    open.dispatchEvent(new Event('change'));
    await tick();
    f.detectChanges();
    expect(calls(invoke, 'overlay')[0][1]).toEqual({ view: 'radar', open: true, onTop: true, clickThrough: false, bounds: null });
    expect(calls(invoke, 'plugin:event|emit_to')).toHaveLength(1);
    expect(stored().open).toBe(true);
  });

  it('passes click-through and always-on-top to Rust', async () => {
    localStorage.setItem('acompanion.overlay.radar', JSON.stringify({ open: true }));
    const { input, invoke } = render();
    await tick();
    const through = input('clickThrough');
    through.checked = true;
    through.dispatchEvent(new Event('change'));
    const top = input('onTop');
    top.checked = false;
    top.dispatchEvent(new Event('change'));
    await tick();
    const last = calls(invoke, 'overlay').at(-1)![1];
    expect(last).toEqual(expect.objectContaining({ open: true, onTop: false, clickThrough: true }));
  });

  it('lets the user switch auto-hide off', async () => {
    const { input } = render();
    expect(input('autoHide').checked).toBe(true);
    const box = input('autoHide');
    box.checked = false;
    box.dispatchEvent(new Event('change'));
    await tick();
    expect(stored().autoHide).toBe(false);
  });

  it('saves and sends the combat setting to an open meter overlay', async () => {
    localStorage.setItem('acompanion.overlay.meter', JSON.stringify({ open: true }));
    const { input, invoke } = render(undefined, 'meter');
    expect(input('hideOutOfCombat').checked).toBe(false);
    input('hideOutOfCombat').checked = true;
    input('hideOutOfCombat').dispatchEvent(new Event('change'));
    await tick();
    expect(stored('meter').hideOutOfCombat).toBe(true);
    expect(calls(invoke, 'plugin:event|emit_to').at(-1)?.[1]).toEqual(expect.objectContaining({
      event: 'overlay://settings', payload: expect.objectContaining({ hideOutOfCombat: true }),
    }));
  });

  for (const view of ['radar', 'dailies'] as const) {
    it(`does not offer the combat setting for ${view}`, () => {
      expect(render(undefined, view).el.querySelector('input[name="hideOutOfCombat"]')).toBeNull();
    });
  }

  it('keeps the position the overlay saved while the panel was open', async () => {
    const { input } = render();
    localStorage.setItem('acompanion.overlay.radar', JSON.stringify({ ...stored(), bounds: [5, 6, 420, 300] }));
    const range = input('opacity');
    range.value = '50';
    range.dispatchEvent(new Event('input'));
    await tick();
    expect(stored()).toEqual(expect.objectContaining({ opacity: 50, bounds: [5, 6, 420, 300] }));
  });

  it('keeps each overlay to itself: its own storage and its own window', async () => {
    const { input, invoke } = render(undefined, 'meter');
    const open = input('open');
    open.checked = true;
    open.dispatchEvent(new Event('change'));
    await tick();
    expect(calls(invoke, 'overlay')[0][1]).toEqual(expect.objectContaining({ view: 'meter', open: true }));
    expect(stored('meter').open).toBe(true);
    expect(localStorage.getItem('acompanion.overlay.radar')).toBeNull();
  });

  it('does not remember an overlay that failed to open', async () => {
    const { f, el, input, invoke } = render(vi.fn(async () => { throw new Error('no window'); }));
    const open = input('open');
    open.checked = true;
    open.dispatchEvent(new Event('change'));
    f.detectChanges(); // the browser re-renders right after the click, then again when the work settles
    await tick();
    f.detectChanges();
    expect(calls(invoke, 'overlay')).toHaveLength(1);
    expect(stored().open).toBe(false);
    expect(el.querySelector('.err')).not.toBeNull();
    expect(el.querySelector<HTMLInputElement>('input[name="open"]')!.checked).toBe(false);
  });
});
