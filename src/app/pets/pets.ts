import { DecimalPipe, NgTemplateOutlet } from '@angular/common';
import { Component, DestroyRef, ElementRef, LOCALE_ID, afterNextRender, computed, effect, inject, input, linkedSignal, output, signal, untracked, viewChild } from '@angular/core';
import { Radar } from '../characters.service';
import { Box, FarmMonster, GameData, PetCollection, PetRow, MAX_ZOOM, clampBox, currentRegion, farmMap, fitBox, followBox, liveMonsters, pick, soulGain, span } from '../gamedata';
import { FarmRoute, farmRoute } from '../route';

/** `null` shows every pet; a number shows the pets at that level. */
type LevelFilter = number | null;

/** Which pets the farm map shows monsters for: all, those below the top level, the `closest` list, or one level. */
type FarmFilter = 'all' | 'open' | 'closest' | number;

type Tab = 'collection' | 'map';

/** How many pets "closest to the next level" lists. */
const CLOSEST = 10;

/** Markers (dots, discs, the player's ring) are this much bigger than the base dot at zoom 1 and, on screen, grow with `zoom ** MARKER_GROWTH`. */
const MARKER_BOOST = 1.2;
const MARKER_GROWTH = 0.3;

/** The overlay's first zoom: this many times closer than the region's spawn box. */
const OVERLAY_ZOOM = 4;

@Component({
  selector: 'app-pets',
  imports: [DecimalPipe, NgTemplateOutlet],
  templateUrl: './pets.html',
  styleUrl: './pets.scss',
  host: { '[class.overlay]': 'overlay()' },
})
export class Pets {
  private readonly lang = inject(LOCALE_ID).split('-')[0];
  readonly collection = input.required<PetCollection | null>();
  /** Epoch ms, ticked by the parent. */
  readonly now = input.required<number>();
  readonly gamedata = input<GameData | null>(null);
  /** What the game shows around the player; null while the game is not running. */
  readonly radar = input<Radar | null>(null);
  readonly back = output<void>();
  /** Shown in the overlay window: only the map, always filled (the rest of the page is hidden by CSS). */
  readonly overlay = input(false);
  /** The overlay's fallback while the game has not said where the player is: the region they were last seen in. */
  readonly lastRegion = input<string | null>(null);
  readonly tab = linkedSignal<Tab>(() => (this.overlay() ? 'map' : 'collection'));
  /** The map covers the whole app window (not the OS screen). */
  readonly filled = linkedSignal(() => this.overlay());
  readonly filter = signal<LevelFilter>(null);
  readonly levels = [0, 1, 2, 3];
  readonly maxZoom = MAX_ZOOM;
  /** The tallest the map grows, in px; its width follows the region's shape so the view box maps 1:1 to the element. */
  readonly mapHeight = 640;
  readonly farmFilter = signal<FarmFilter>('all');
  /** One pet picked from a list; it overrides the farm filter. */
  readonly picked = signal<number | null>(null);
  readonly regionKey = signal<string | null>(null);
  /** The map view the user moved to, with the region it belongs to; the region's own box otherwise. */
  private readonly moved = signal<(Box & { key: string }) | null>(null);
  /** The overlay map's zoom. */
  private readonly overlayZoom = signal(OVERLAY_ZOOM);
  /** Where the overlay map was dragged to (in this region); null follows the player. */
  private readonly overlayPan = signal<{ key: string; x: number; y: number } | null>(null);
  private readonly stageMap = viewChild<ElementRef<HTMLElement>>('stageMap');
  /** Width / height of the area the overlay's map fills; null until measured. */
  private readonly aspect = signal<number | null>(null);
  /** The pet levels' colour key over the map. */
  readonly legend = signal(false);
  /** The pet that last gained souls, kept until another does; the first collection seen is no gain. */
  private readonly gainId = linkedSignal<PetRow[], number | null>({
    source: () => this.collection()?.rows ?? [],
    computation: (rows, prev) => (prev ? soulGain(prev.source, rows) : null) ?? prev?.value ?? null,
  });
  readonly gain = computed(() => this.collection()?.rows.find(r => r.id === this.gainId()) ?? null);
  readonly hovered = signal<number | null>(null);
  readonly selected = signal<number | null>(null);
  readonly dragging = signal(false);
  /** Where the pointer is over the overlay map, in px inside `.stage-map`; the hover name sits there. */
  readonly tipAt = signal<{ x: number; y: number } | null>(null);
  private drag: { x: number; y: number; moved: boolean } | null = null;

  /** Pets with progress toward their next level, nearest first. Pets nobody has started are left out. */
  readonly closest = computed(() => (this.collection()?.rows ?? [])
    .filter(r => r.next !== null && (r.level > 0 || r.souls > 0))
    .sort((a, b) => a.toNext - b.toNext || a.id - b.id)
    .slice(0, CLOSEST));

  readonly listed = computed(() => (this.collection()?.rows ?? []).filter(r => this.filter() === null || r.level === this.filter()));

  private readonly farmRows = computed(() => {
    const rows = this.collection()?.rows ?? [];
    const [f, p] = [this.farmFilter(), this.picked()];
    if (p !== null) return rows.filter(r => r.id === p);
    if (f === 'all') return rows;
    if (f === 'open') return rows.filter(r => r.next !== null);
    if (f === 'closest') return this.closest();
    return rows.filter(r => r.level === f);
  });
  readonly farm = computed(() => {
    const gd = this.gamedata();
    return gd ? farmMap(gd, this.farmRows(), this.lang) : null;
  });
  /** The region the monsters in view are in; null when none is. */
  private readonly liveNow = computed(() => {
    const gd = this.gamedata();
    return gd ? currentRegion(gd, this.radar()) : null;
  });
  /** `liveNow`, kept while no pet monster is in view so the map does not jump back. */
  private readonly liveKey = linkedSignal<string | null, string | null>({
    source: this.liveNow,
    computation: (now, prev) => now ?? prev?.value ?? null,
  });
  /** The region picked with its chip, else the one the player is in, else the first. */
  readonly region = computed(() => {
    const regions = this.farm()?.regions ?? [];
    // the overlay is the minimap of where the player is: no other region, none outside them
    if (this.overlay()) return regions.find(r => r.key === (this.liveNow() ?? this.lastRegion())) ?? null;
    const key = this.regionKey() ?? this.liveKey();
    return regions.find(r => r.key === key) ?? regions[0] ?? null;
  });
  /** The pet monsters in view that the map lists. */
  readonly live = computed(() => liveMonsters(this.region(), this.radar()));
  /**
   * The player's last exact position, in map units, with its capture time. Shown only while pet
   * monsters in view put the player in this region: the wire does not name the map.
   */
  readonly me = computed(() => {
    const [r, p] = [this.region(), this.radar()?.player];
    return r && p && this.liveNow() === r.key ? { x: p.x / 100, y: p.y / 100, us: p.t_us } : null;
  });
  readonly view = computed(() => {
    const [r, m] = [this.region(), this.moved()];
    if (r && this.overlay()) {
      const [p, pan] = [this.me(), this.overlayPan()];
      const centre = pan?.key === r.key ? [pan.x, pan.y] as [number, number] : p && [p.x, p.y] as [number, number];
      return followBox(r.box, { x: r.image.x, y: r.image.y, w: r.image.w, h: r.image.h }, centre, this.overlayZoom(), this.aspect() ?? r.box.w / r.box.h);
    }
    return r && m?.key === r.key ? m : r?.box ?? null;
  });
  /** The farm route over the map: on in the overlay, a chip on the Map tab. */
  readonly showRoute = linkedSignal(() => this.overlay());
  private readonly openIds = computed(() => (this.collection()?.rows ?? []).filter(r => r.next !== null).map(r => r.id).join(','));
  private readonly routeKey = computed(() => this.region()?.key ?? null);
  /**
   * Covers every pet that is not maxed in this region. It reruns when a pet maxes or the region changes,
   * not on each soul or position fix; the player's position only picks where it starts.
   * ponytail: a fix that arrives after the first run does not reorder it; recompute on a region change only.
   */
  readonly route = computed((): FarmRoute | null => {
    const [gd, key] = [this.gamedata(), this.routeKey()];
    this.openIds();
    if (!gd || !key || !this.showRoute()) return null;
    const p = untracked(this.me);
    return farmRoute(gd, untracked(this.collection)?.rows ?? [], key, p && [p.x, p.y]);
  });
  /** Stops of `route` the player has been at; a full lap starts over, since the pets are not maxed yet. */
  readonly visited = linkedSignal<{ route: FarmRoute | null; me: { x: number; y: number } | null }, Set<number>>({
    source: () => ({ route: this.route(), me: this.me() }),
    computation: ({ route, me }, prev) => {
      let seen = prev && prev.source.route === route ? new Set(prev.value) : new Set<number>();
      if (route && me) route.stops.forEach((s, i) => { if (Math.hypot(s.x - me.x, s.y - me.y) <= route.radius * 1.5) seen.add(i); });
      if (route && seen.size === route.stops.length) seen = new Set();
      return seen;
    },
  });
  /** The first stop not visited yet, with the names the foot line shows. */
  readonly next = computed(() => {
    const [r, gd] = [this.route(), this.gamedata()];
    const i = r ? r.stops.findIndex((_, k) => !this.visited().has(k)) : -1;
    if (!r || !gd || i < 0) return null;
    const s = r.stops[i];
    return {
      x: s.x, y: s.y,
      monsters: s.monsters.map(m => pick(gd.pet_monsters[m]?.name, this.lang) ?? `#${m}`).join(', '),
    };
  });
  /** The overlay map was dragged away from the player. */
  readonly panned = computed(() => this.overlayPan()?.key === this.region()?.key && this.overlayPan() !== null);
  /** One marker unit in view-box units: the dot's size, growing with the zoom more slowly than the map does. */
  readonly unit = computed(() => (this.region()?.dot ?? 0) * MARKER_BOOST * this.zoom() ** (MARKER_GROWTH - 1));
  /** 1 at the region's own view, up to `MAX_ZOOM`. */
  readonly zoom = computed(() => (this.region()?.box.w ?? 1) / (this.view()?.w ?? 1));
  /** The monster the map highlights: the one under the pointer, else the selected one. */
  readonly active = computed((): FarmMonster | null => {
    const id = this.hovered() ?? this.selected();
    return this.region()?.monsters.find(m => m.id === id) ?? null;
  });
  readonly pickedName = computed(() => this.collection()?.rows.find(r => r.id === this.picked())?.name ?? '');

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  /** Ticks every second so the age of the player's fix reads in seconds (`now` only ticks per minute). */
  private readonly tick = signal(Date.now());

  constructor() {
    const host = this.host;
    afterNextRender(() => host.nativeElement.querySelector<HTMLElement>('.back')?.focus());
    const timer = setInterval(() => this.tick.set(Date.now()), 1000);
    // the overlay's map fills its area, so the view's shape follows the area's
    effect(onCleanup => {
      const el = this.stageMap()?.nativeElement;
      if (!el || !this.overlay() || typeof ResizeObserver === 'undefined') return;
      const ro = new ResizeObserver(([e]) => {
        const { width, height } = e.contentRect;
        if (width && height) this.aspect.set(width / height);
      });
      ro.observe(el);
      onCleanup(() => ro.disconnect());
    });
    // Esc leaves the fill first. Capture phase and stopImmediatePropagation keep the dashboard's own
    // Esc handler (back to the roster) from also firing.
    const onEsc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || !this.filled() || this.overlay()) return;
      e.stopImmediatePropagation();
      this.filled.set(false);
    };
    document.addEventListener('keydown', onEsc, true);
    // the page behind the fill must not scroll
    effect(() => document.documentElement.classList.toggle('lock-scroll', this.filled()));
    inject(DestroyRef).onDestroy(() => {
      clearInterval(timer);
      document.removeEventListener('keydown', onEsc, true);
      document.documentElement.classList.remove('lock-scroll');
    });
  }

  setTab(t: Tab) {
    this.tab.set(t);
    document.scrollingElement?.scrollTo?.({ top: 0 });
  }

  /** Age of a capture timestamp (µs since the epoch): seconds under a minute, else as `ago`. */
  fixAge(us: number): string {
    const ms = Math.max(0, this.tick() - us / 1000);
    if (ms >= 60_000) return span(ms, this.lang);
    return new Intl.NumberFormat(this.lang, { style: 'unit', unit: 'second', unitDisplay: 'narrow' }).format(Math.floor(ms / 1000));
  }

  /** Time since a capture timestamp (µs since the epoch). */
  ago(us: number): string {
    return span(this.now() - us / 1000, this.lang);
  }

  private setView(b: Box) {
    const r = this.region();
    if (r) this.moved.set({ ...clampBox(b, r.box), key: r.key });
  }

  /** Map units under a pointer event on the map. */
  private at(e: { clientX: number; clientY: number; currentTarget: EventTarget | null }, v: Box): [number, number] {
    const rect = (e.currentTarget as Element).getBoundingClientRect();
    return [
      v.x + (rect.width ? (e.clientX - rect.left) / rect.width : 0.5) * v.w,
      v.y + (rect.height ? (e.clientY - rect.top) / rect.height : 0.5) * v.h,
    ];
  }

  /** Zoom by `factor` keeping the map point `at` (default: the middle) where it is. */
  zoomBy(factor: number, at?: [number, number]) {
    if (this.overlay()) {
      this.overlayZoom.update(z => Math.min(MAX_ZOOM, Math.max(1, z * factor)));
      return;
    }
    const v = this.view();
    if (!v) return;
    const [cx, cy] = at ?? [v.x + v.w / 2, v.y + v.h / 2];
    const k = 1 / factor;
    this.setView({ x: cx - (cx - v.x) * k, y: cy - (cy - v.y) * k, w: v.w * k, h: v.h * k });
  }

  resetView() {
    this.moved.set(null);
  }

  /** Follow the player's region and frame their last position. */
  centerOnMe() {
    if (this.overlay()) {
      this.overlayPan.set(null);
      return;
    }
    const [r, p] = [this.region(), this.me()];
    if (!r || !p) return;
    this.regionKey.set(null);
    this.setView(fitBox({ x: p.x, y: p.y, w: 0, h: 0 }, r.box));
  }

  onWheel(e: WheelEvent) {
    const v = this.view();
    if (!v) return;
    e.preventDefault();
    this.zoomBy(e.deltaY < 0 ? 1.25 : 0.8, this.at(e, v));
  }

  onDown(e: PointerEvent) {
    this.drag = { x: e.clientX, y: e.clientY, moved: false };
  }

  onMove(e: PointerEvent) {
    const [d, v] = [this.drag, this.view()];
    if (!d || !v || !(e.buttons & 1)) return;
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) return;
    const el = e.currentTarget as Element;
    // capture only once it is a drag, so a plain click still reaches the dot under it
    if (!d.moved) el.setPointerCapture?.(e.pointerId);
    d.moved = true;
    this.dragging.set(true);
    const rect = el.getBoundingClientRect();
    if (rect.width) {
      const [x, y] = [v.x - (e.clientX - d.x) / rect.width * v.w, v.y - (e.clientY - d.y) / rect.height * v.h];
      const key = this.region()?.key;
      if (this.overlay() && key) this.overlayPan.set({ key, x: x + v.w / 2, y: y + v.h / 2 });
      else this.setView({ ...v, x, y });
    }
    d.x = e.clientX;
    d.y = e.clientY;
  }

  tipMove(e: PointerEvent) {
    const rect = this.stageMap()?.nativeElement.getBoundingClientRect();
    if (rect) this.tipAt.set({ x: e.clientX - rect.left, y: e.clientY - rect.top });
  }

  onUp() {
    this.drag = null;
    this.dragging.set(false);
  }

  /** Toggle a monster; `fit` also frames its spawns. */
  select(m: FarmMonster, fit: boolean) {
    const on = this.selected() !== m.id;
    this.selected.set(on ? m.id : null);
    const r = this.region();
    if (on && fit && r && m.d) this.setView(fitBox(m.box, r.box));
  }

  /** Show where this pet's souls drop. */
  pick(id: number) {
    this.picked.set(id);
    this.regionKey.set(null);
    this.selected.set(null);
    this.setTab('map');
  }

  pct(r: PetRow): number {
    return r.next === null ? 100 : Math.min(100, (r.souls / r.next) * 100);
  }
}
