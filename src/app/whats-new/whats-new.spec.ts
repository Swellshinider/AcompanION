import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { WhatsNew } from './whats-new';

const tick = () => new Promise(r => setTimeout(r));

async function render(version: string) {
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke: vi.fn(async () => version) });
  const f = TestBed.createComponent(WhatsNew);
  f.detectChanges();
  await tick();
  f.detectChanges();
  return { f, el: f.nativeElement as HTMLElement };
}

describe('WhatsNew', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('lists every released version, newest first, without the next one', async () => {
    const { el } = await render('0.2.6');
    const titles = [...el.querySelectorAll('.panel h3')].map(h => h.textContent);
    expect(titles[0]).toBe('v0.2.6');
    expect(titles.at(-1)).toBe('v0.1.1');
    expect(titles).not.toContain('v0.2.7');
  });

  it('marks only the installed version', async () => {
    const { el } = await render('0.1.3');
    const mine = el.querySelectorAll('.panel.mine');
    expect(mine).toHaveLength(1);
    expect(mine[0].querySelector('h3')!.textContent).toBe('v0.1.3');
    expect(mine[0].querySelector('.badge')).not.toBeNull();
  });

  it('shows bold text as bold and a topic apart, without the ** marks', async () => {
    const { el } = await render('0.2.0');
    expect(el.textContent).not.toContain('**');
    expect(el.querySelector('.bold')).not.toBeNull();
    expect([...el.querySelectorAll('.topic')].map(t => t.textContent)).toContain('Settings');
  });

  it('emits back', async () => {
    const { f, el } = await render('0.2.0');
    const back = vi.fn();
    f.componentInstance.back.subscribe(back);
    el.querySelector<HTMLButtonElement>('.back')!.click();
    expect(back).toHaveBeenCalled();
  });
});
