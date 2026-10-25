import { vi } from 'vitest';
import { restoreOverlay } from './overlay-control';

const stored = (view = 'radar') => JSON.parse(localStorage.getItem(`acompanion.overlay.${view}`)!);
const tick = () => new Promise(r => setTimeout(r));
const overlayCalls = (invoke: ReturnType<typeof vi.fn>) => invoke.mock.calls.filter(c => c[0] === 'overlay');

describe('restoreOverlay', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('reopens the overlay when it was open', async () => {
    localStorage.setItem('acompanion.overlay.radar', JSON.stringify({ open: true }));
    const invoke = vi.fn(async (..._: unknown[]) => undefined);
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
    restoreOverlay()();
    await tick();
    expect(overlayCalls(invoke)).toHaveLength(1);
    expect(stored().open).toBe(true);
  });

  it('reopens every overlay that was open, each on its own', async () => {
    localStorage.setItem('acompanion.overlay.radar', JSON.stringify({ open: true }));
    localStorage.setItem('acompanion.overlay.meter', JSON.stringify({ open: true }));
    const invoke = vi.fn(async (..._: unknown[]) => undefined);
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
    restoreOverlay()();
    await tick();
    expect(overlayCalls(invoke).map(c => (c[1] as { view: string }).view).sort()).toEqual(['meter', 'radar']);
  });

  it('leaves a closed overlay alone', async () => {
    const invoke = vi.fn(async (..._: unknown[]) => undefined);
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
    restoreOverlay()();
    await tick();
    expect(overlayCalls(invoke)).toHaveLength(0);
  });

  it('does not remember an overlay that failed to open', async () => {
    localStorage.setItem('acompanion.overlay.radar', JSON.stringify({ open: true }));
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke: vi.fn(async (cmd: unknown) => { if (cmd === 'overlay') throw new Error('no window'); }) });
    restoreOverlay()();
    await tick();
    expect(stored().open).toBe(false);
  });
});
