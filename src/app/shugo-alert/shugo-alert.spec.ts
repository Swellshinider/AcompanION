import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { ShugoAlert } from './shugo-alert';

const closes = (invoke: ReturnType<typeof vi.fn>) => invoke.mock.calls.filter(c => c[0] === 'shugo_alert' && (c[1] as { corner: unknown }).corner === null);

function render(settings: object = {}) {
  localStorage.setItem('acompanion.shugo', JSON.stringify(settings));
  const invoke = vi.fn(async (..._: unknown[]) => undefined);
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
  const f = TestBed.createComponent(ShugoAlert);
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement, invoke };
}

describe('ShugoAlert', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('closes itself after the saved time', () => {
    const { invoke, el } = render({ seconds: 5 });
    expect(el.querySelector('.bar')).not.toBeNull();
    vi.advanceTimersByTime(4_900);
    expect(closes(invoke)).toHaveLength(0);
    vi.advanceTimersByTime(200);
    expect(closes(invoke)).toHaveLength(1);
  });

  it('closes on a click when that is on, and ignores it when off', () => {
    const on = render({ clickToClose: true });
    on.el.querySelector<HTMLElement>('.shell')!.click();
    expect(closes(on.invoke)).toHaveLength(1);
    TestBed.resetTestingModule();
    const off = render({ clickToClose: false });
    off.el.querySelector<HTMLElement>('.shell')!.click();
    expect(closes(off.invoke)).toHaveLength(0);
  });

  it('plays the chime only when sound is on', () => {
    const ctx = vi.fn(function () {
      return {
        currentTime: 0, destination: {}, resume: async () => undefined, close: async () => undefined,
        createOscillator: () => ({ frequency: {}, connect: (n: unknown) => n, start() {}, stop() {} }),
        createGain: () => ({ gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect: (n: unknown) => n }),
      };
    });
    vi.stubGlobal('AudioContext', ctx);
    render({ sound: false });
    expect(ctx).not.toHaveBeenCalled();
    TestBed.resetTestingModule();
    render({ sound: true });
    expect(ctx).toHaveBeenCalledTimes(1);
  });

  it('survives a browser that has no audio', () => {
    vi.stubGlobal('AudioContext', undefined);
    expect(() => render({ sound: true })).not.toThrow();
  });
});
