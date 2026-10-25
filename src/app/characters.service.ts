import { Injectable, signal } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { pushRecent } from './fight';

/** Wire shapes from `src-tauri` (`Character::to_json`, `app::Status::to_json`). Times are capture µs since the epoch. */
export interface CounterValue { id: number; value: number; ms: boolean; updated_us: number }
/** A per-character gain: a count of completed requests (supply) or kinah gained today. */
export interface Gain { value: number; updated_us: number; group?: number }
/** A character's view of the pet collection (account-wide on the wire; only `active` differs). */
export interface PetsState { active: number; updated_us: number; list: { id: number; level: number; souls: number }[] }
export interface Character {
  server: number; name: string; class_code: number; level: number;
  combat_power: number | null; item_level: number | null; counters: CounterValue[]; last_seen_us: number;
  /** Absent on characters saved before gains were decoded. */
  gains?: Record<string, Gain>;
  /** Absent or null before a world entry was seen with pets decoded. */
  pets?: PetsState | null;
}
/** What the game shows around the player now (`radar::Radar::to_json`). Positions are hundredths of a map unit; `t_us` is capture µs. */
export interface Radar {
  /** The game's map id the player is in; null before one was seen. */
  map: number | null;
  player: { x: number; y: number; t_us: number } | null;
  monsters: { entity: number; template: number; x: number; y: number }[];
}
/** `meter::Meter::to_json` and `Fight`. Timestamps are capture µs since the epoch; ids and raw values only. */
export interface Tally { amount: number; hits: number; direct: number; crits: number }
export interface Small { amount: number; hits: number }
export interface SkillTally extends Tally { id: number }
export interface TargetTally extends Tally { template: number }
export interface MeterPlayer {
  entity: number; name: string | null; class_code: number | null;
  dealt: Tally; taken: Small; healing: Small; overheal: number; healing_received: Small;
  skills: SkillTally[]; targets: TargetTally[];
}
/** `hp`, `max_hp` and `stagger` are null until the wire gave them (the stagger gauge is a boss's: most foes never have one). */
export interface MeterTarget {
  entity: number; template: number; hp: number | null; max_hp: number | null;
  stagger: { state: number; value: number; max: number } | null;
}
export interface Fight { start_us: number; end_us: number; me: number | null; players: MeterPlayer[]; target: MeterTarget | null }
/** What `meter://update` carries without `history`. */
export interface Meter { me: number | null; current: Fight | null; fights: number }
export type StatusId = 'starting' | 'running' | 'save_failed' | 'not_admin' | 'npcap_missing' | 'no_adapter' | 'state_unreadable' | 'state_in_use' | 'reconnecting' | 'error';
export interface CaptureStatus { id: StatusId; detail: string | null }

const key = (c: Pick<Character, 'server' | 'name'>) => `${c.server}\u0000${c.name}`;

/** `base` with every update replacing the character with the same (server, name). */
export function merge(base: Character[], updates: Character[]): Character[] {
  const byKey = new Map(base.map(c => [key(c), c]));
  for (const u of updates) byKey.set(key(u), u);
  return [...byKey.values()];
}

/** `base` without the characters with these (server, name). */
export function without(base: Character[], removed: Pick<Character, 'server' | 'name'>[]): Character[] {
  const gone = new Set(removed.map(key));
  return base.filter(c => !gone.has(key(c)));
}

@Injectable({ providedIn: 'root' })
export class CharactersService {
  readonly characters = signal<Character[]>([]);
  readonly status = signal<CaptureStatus>({ id: 'starting', detail: null });
  /** The game's port is carrying traffic (the game is running and connected). */
  readonly game = signal(false);
  /** Null until the game sent anything the radar shows. */
  readonly radar = signal<Radar | null>(null);
  /** Null until the first meter update. */
  readonly meter = signal<Meter | null>(null);
  /** Every finished fight, oldest first (`meter://update` sends it only when a fight closed). */
  readonly fights = signal<Fight[]>([]);
  /** The open fight's last few seconds of snapshots, for the current DPS (`recentRates`). */
  readonly recent = signal<Fight[]>([]);
  private statusSeen = false;
  private gameSeen = false;
  private radarSeen = false;
  private meterSeen = false;
  private fightsSeen = false;

  constructor() {
    void this.connect();
  }

  /** Listen first, then read the snapshot: whatever arrives in between is newer and wins. */
  private async connect(): Promise<void> {
    try {
      await listen<Character[]>('characters://update', e => this.characters.update(cs => merge(cs, e.payload)));
      await listen<Pick<Character, 'server' | 'name'>[]>('characters://removed', e => this.characters.update(cs => without(cs, e.payload)));
      await listen<CaptureStatus>('capture://status', e => {
        this.statusSeen = true;
        this.status.set(e.payload);
      });
      await listen<boolean>('capture://game', e => {
        this.gameSeen = true;
        this.game.set(e.payload);
      });
      await listen<Radar>('radar://update', e => {
        this.radarSeen = true;
        this.radar.set(e.payload);
      });
      await listen<Meter & { history?: Fight[] }>('meter://update', e => {
        const { history, ...meter } = e.payload;
        this.meterSeen = true;
        this.meter.set(meter);
        this.recent.update(r => pushRecent(r, meter.current));
        if (history) {
          this.fightsSeen = true;
          this.fights.set(history);
        }
      });
      const [snapshot, status, game, radar, meter, fights] = await Promise.all([
        invoke<Character[]>('characters'),
        invoke<CaptureStatus>('capture_status'),
        invoke<boolean>('game_detected'),
        invoke<Radar | null>('radar'),
        invoke<Meter | null>('meter'),
        invoke<Fight[]>('meter_history'),
      ]);
      this.characters.update(received => merge(snapshot, received));
      if (!this.statusSeen) this.status.set(status);
      if (!this.gameSeen) this.game.set(game);
      if (!this.radarSeen) this.radar.set(radar);
      if (!this.meterSeen) this.meter.set(meter);
      if (!this.fightsSeen) this.fights.set(Array.isArray(fights) ? fights : []);
    } catch (e) {
      // outside the Tauri window (plain `ng serve`) there is no backend
      this.status.set({ id: 'error', detail: String(e) });
    }
  }

  /** Empties the saved fights; the capture thread does it, and the next update sends the empty list. */
  clearMeter(): Promise<void> {
    return invoke<void>('meter_clear');
  }

  /** Removes a saved character; the capture thread does it and the next update drops it from the list. */
  forget(c: Pick<Character, 'server' | 'name'>): Promise<void> {
    return invoke<void>('forget_character', { server: c.server, name: c.name });
  }

  restartAsAdmin(): Promise<boolean> {
    return invoke<boolean>('restart_as_admin');
  }

  /** The newer version on GitHub Releases, or null when up to date. */
  checkUpdate(): Promise<string | null> {
    return invoke<string | null>('check_update');
  }

  /** Downloads and installs the update found by `checkUpdate`; the app restarts. */
  installUpdate(): Promise<void> {
    return invoke<void>('install_update');
  }
}
