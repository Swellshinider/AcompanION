import { vi } from 'vitest';
import { DEFAULTS, loadShugoSettings, nextFestival, parse, startShugoAlert } from './shugo';

const at = (h: number, m: number, s = 0) => Date.UTC(2026, 9, 25, h, m, s);

describe('nextFestival', () => {
  it('is the next top of the hour', () => {
    expect(nextFestival(at(10, 59, 59))).toBe(at(11, 0));
    expect(nextFestival(at(10, 0, 1))).toBe(at(11, 0));
    expect(nextFestival(at(23, 30))).toBe(Date.UTC(2026, 9, 26, 0, 0));
  });

  it('is strictly after now: a start that just fired points at the next one', () => {
    expect(nextFestival(at(11, 0))).toBe(at(12, 0));
  });

  it('follows the minutes it is given', () => {
    expect(nextFestival(at(10, 20), [15, 45])).toBe(at(10, 45));
    expect(nextFestival(at(10, 50), [15, 45])).toBe(at(11, 15));
  });
});

describe('parse', () => {
  it('falls back to the defaults for anything missing or corrupt', () => {
    expect(parse(null)).toEqual(DEFAULTS);
    expect(parse({ enabled: 'yes', corner: 'middle', seconds: 'x', sound: 1 })).toEqual(DEFAULTS);
  });

  it('keeps valid values and clamps the time to 3..300 seconds', () => {
    expect(parse({ enabled: false, corner: 'top-left', seconds: 42.4, clickToClose: false, sound: false })).toEqual({
      enabled: false, corner: 'top-left', seconds: 42, clickToClose: false, sound: false,
    });
    expect(parse({ seconds: 1 }).seconds).toBe(3);
    expect(parse({ seconds: 9999 }).seconds).toBe(300);
    expect(parse({ seconds: Infinity }).seconds).toBe(DEFAULTS.seconds);
  });
});

describe('startShugoAlert', () => {
  const invoke = vi.fn(async (..._: unknown[]) => true);
  const shown = () => invoke.mock.calls.filter(c => c[0] === 'shugo_alert' && (c[1] as { corner: unknown }).corner !== null);

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(at(10, 58, 50));
    vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    localStorage.clear();
    invoke.mockClear();
  });

  it('shows the alert a minute before the hour while the game runs, then waits for the next one', async () => {
    const stop = startShugoAlert(() => true);
    await vi.advanceTimersByTimeAsync(9_000);
    expect(shown()).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(shown()).toHaveLength(1);
    expect(shown()[0][1]).toEqual({ corner: 'bottom-right', clickThrough: false });
    await vi.advanceTimersByTimeAsync(3_600_000);
    expect(shown()).toHaveLength(2);
    stop();
  });

  it('stays quiet when the game is not running or the alert is off', async () => {
    let game = false;
    const stop = startShugoAlert(() => game);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(shown()).toHaveLength(0);
    game = true;
    localStorage.setItem('acompanion.shugo', JSON.stringify({ enabled: false }));
    await vi.advanceTimersByTimeAsync(3_600_000);
    expect(shown()).toHaveLength(0);
    expect(loadShugoSettings().enabled).toBe(false);
    stop();
  });

  it('skips a start the computer slept through', async () => {
    const stop = startShugoAlert(() => true);
    vi.setSystemTime(at(11, 4)); // woke up five minutes late
    await vi.advanceTimersByTimeAsync(10_000);
    expect(shown()).toHaveLength(0);
    stop();
  });

  it('stops when told to', async () => {
    const stop = startShugoAlert(() => true);
    stop();
    await vi.advanceTimersByTimeAsync(3_600_000);
    expect(shown()).toHaveLength(0);
  });
});
