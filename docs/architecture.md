# Architecture

AcompanION watches game traffic passively and turns it into per-character state. Today that is a Rust crate with a headless live engine, a reverse-engineering CLI (`re`) and a Tauri 2 desktop app (`acompanion`) with an Angular dashboard on top ([`status.md`](status.md) for what is verified).

## Pipeline

```text
Npcap adapter / .pcap file
        │  Ethernet frames
        ▼
tcp.rs      parse_ethernet -> Segment; Streams reassemble each direction per connection
        │  server -> client byte stream
        ▼
decode.rs   Framer (varint length, resync on the server ping) -> LZ4 containers -> frames
        │  frame bodies
        ▼
events.rs   Opcodes (opcodes.json) -> typed Event (identity, power, counters, pets, radar, combat)
        │  Decoded { ts_us, conn, event }
        ▼
state.rs    State: per-connection session -> per-character state, JSON persistence
radar.rs    transient view of the monsters around and the player's last fix
meter.rs    fights: tallies per player, skill and target, history in meter.json
        ▲
live.rs     Engine: drives the stages above, tracks SYN/RST, throttled save; run() = Npcap loop
```

Only server → client traffic is decoded. The client → server direction is encrypted after an RSA handshake and carries no state we need ([`decisions.md`](decisions.md)).

## Modules

All under `src-tauri/src/`, in one crate (`acompanion` library plus the `re` and `acompanion` binaries).

| Module | Responsibility |
| --- | --- |
| `npcap.rs` | Runtime-loaded `wpcap.dll` binding (`libloading`); the only place with `unsafe`/FFI. Windows only |
| `pcapfile.rs` | Classic `.pcap` reader and writer (Ethernet, microsecond timestamps) |
| `tcp.rs` | Ethernet/IPv4/TCP parsing and per-direction stream reassembly |
| `decode.rs` | Server → client framing, own LZ4 block decoder, resync, `Decoder` (one framer per connection) |
| `events.rs` | `opcodes.json` table, `Event` types and their layouts, JSON form (ids and raw values only) |
| `state.rs` | `State`/`Character`, session binding, JSON persistence at `%APPDATA%\AcompanION\characters.json` |
| `live.rs` | `Engine` (frames in; state, radar and meter out; throttled save) and `run` (Npcap capture loop) |
| `radar.rs` | Transient radar state: pet monsters in view, the player's last exact position, the map id |
| `meter.rs` | Combat meter: fights closed after 10 s without a counted hit or at a boss's death (one fight per boss), tallies per player, skill and target, the last 50 fights saved to `meter.json` next to the state file |
| `hexdump.rs` | Hexdump and Shannon entropy for `re dump` |
| `app.rs` | App glue testable without a window: `Status` (stable id plus raw error chain), `classify`, `Shared`, the UAC relaunch. Windows only |
| `main.rs` | The `acompanion` bin: Tauri builder, capture thread, the commands, join on exit |
| `bin/re.rs` | CLI: `devices`, `record`, `dump`, `decode`, `live` |

Outside the crate: `tools/re/aion_re.py` (stdlib + `lz4`) for ad-hoc analysis, `opcodes.json` for the opcode table, `captures/` for local recordings (gitignored), and the Angular frontend at the repository root (`src/`, `public/`).

## State model

- **Key:** a character is `(server, name)`; the same name can exist on two servers.
- **Sessions:** one per client TCP port. A character swap on a connection rebinds it; the old character keeps its state.
- **Identity trails the bundle:** on world entry the identity frame arrives *after* the combat power and counter snapshot, so those are held per connection and attributed to the identity that follows (details in [`protocol.md`](protocol.md), "Order at world entry"). A counter update on a bound connection applies immediately.
- **Connection lifecycle:** a first or changed SYN and an RST unbind a connection; a FIN keeps the binding so late server data still lands on the right character. Repeated SYNs (retransmissions) preserve it.
- **Persistence:** hand-built JSON (`{"version":1,"characters":[...]}`), written to `<path>.tmp`, synced, then renamed. Saved at most every 5 s while dirty; a kill loses at most that window and never leaves a half-written file. A corrupt file is an error and is left untouched.
- **Pets:** the snapshot (`00 90`) trails the counter snapshot like the boards, so it is held and attributed to the identity; `0d 90` (soul delta) and `14 90` (level-up) apply when bound. The collection is account-wide on the wire, but it is stored on each character that saw it; the frontend picks the freshest. `0e 90` (total) is ignored.

## Threading

`live::run` owns the Npcap `Capture` (not `Send`) in the thread that calls it. `on_start` runs once the adapter is open. The loop reads with a 100 ms timeout, pushes each frame into the `Engine`, calls `on_change` with the state and the changed keys, and ticks the throttled save. `Engine::tick` returns whether a write completed; `on_save` receives each attempted write's success or error, never a skipped tick. Failed writes remain dirty and retry after 5 s. A stop flag ends the loop, and the engine is flushed before returning.

## Contracts

- **Passive only.** No memory reads, injection or input to the game.
- **Rust emits ids and raw values, never display text.** Names, plurals and formatting belong to the frontend (i18n from day one, English first).
- **Account data stays in `captures/`.** Tests use synthetic fixtures; docs use placeholders.

## Desktop app

Built on the engine above without changing it. Choices and spec gaps are in [`decisions.md`](decisions.md).

```text
capture thread (main.rs)            Tauri main thread                 Angular window
  live::run(Engine) --on_start-->  Shared { status, characters }
                    --on_change--> emit characters://update  ------>  CharactersService (signals)
                    --on_save----> emit capture://status     ------>  Dashboard (save warning)
  failure -> classify()         --> emit capture://status    ------>  Dashboard (roster, banner)
                                   commands: characters, capture_status, restart_as_admin  <---
```

**Capture thread.** Spawned at setup if the state file loaded and its writer lock was acquired. It owns the `Engine` and calls `live::run` with `on_start` (publishes `running`) and `on_change` (copies `State::to_json()["characters"]` into `Shared` and emits the changed characters). A `live::run` error is classified once and published. On exit the stop flag is set and the thread is joined, so the engine flushes to disk. An unreadable state file publishes `state_unreadable` and never starts capture: the engine would overwrite it on its first save.

**One writer per state file.** The app and `re live` acquire an exclusive standard-library file lock on `<state path>.lock` before loading writable state and hold its handle through the final flush. The lock file stays in place; closing the handle releases ownership, including after a process exit. The app releases it before publishing a capture failure, so a UAC replacement can start. A second window shows a saved snapshot with `state_in_use` and never captures or saves; reopen it after the other writer closes. The CLI rejects contention, including replays with `--state`; different output files remain independent.

**Save failures.** A failed periodic write publishes `save_failed` and its raw error detail. Capture and the roster continue updating; the pill stays "Capturing" and a localized banner warns that changes are unsaved. Only a successful write restores `running` and clears the warning. Skipped ticks leave it visible, and identical status updates are not emitted again. The CLI keeps logging failed saves to stderr.

**Contract (Rust to frontend).** Rust sends ids and raw values only.

| Kind | Name | Payload |
| --- | --- | --- |
| command | `characters` | `[Character JSON]`, the `Character::to_json` shape |
| command | `capture_status` | `{"id", "detail"}`; `detail` is the raw error chain or `null` |
| command | `restart_as_admin` | `bool`: `true` when UAC was accepted (this instance then exits) |
| command | `game_detected` | `bool`: the game's port carried traffic in the last 5 s |
| command | `radar` | the radar snapshot (monsters in view, the player's last fix, the map id) |
| command | `meter` | `{me, current, fights}`, the meter without its history |
| command | `meter_history` | every finished fight, oldest first (the saved ones before capture runs) |
| command | `meter_clear` | empties the fight history; does nothing while capture is not running |
| command | `overlay` | `(open, onTop, clickThrough, bounds)`: opens, closes or updates the overlay window; `bounds` is `[x, y, w, h]` or `null` |
| event | `overlay://closed` | to the main window when the overlay window went away (not while the app quits) |
| event | `characters://update` | `[Character JSON]`, only the changed characters |
| event | `capture://status` | `{"id", "detail"}` |
| event | `capture://game` | `bool`, on each flip of game detection |
| event | `radar://update` | the radar snapshot, at most every 250 ms |
| event | `meter://update` | `{me, current, fights, history?}`, at most every 250 ms; `history` only when a fight closed |

Status ids: `starting`, `running`, `save_failed`, `not_admin`, `npcap_missing`, `no_adapter`, `state_unreadable`, `state_in_use`, `error`. `classify` finds the typed markers `npcap::NotInstalled` and `live::NoAdapter` with `downcast_ref`, asks `npcap::is_elevated` rather than parsing messages, and falls back to `not_admin` (not elevated) or `error`. "Restart as admin" only relaunches our own exe through `powershell.exe -Verb RunAs`.

**Overlay.** A second window (label `overlay`, transparent, no frame, off the taskbar) that Rust creates on demand (hidden: the overlay shows or hides itself, see the auto-hide option in its spec) and the same Angular page renders as `Overlay` instead of `Dashboard`, told apart by the window label. It reuses `Pets` (`overlay` input: the filled map), `CharacterDetail` (`compact` input) and the meter and has its own `CharactersService`. Its preferences (`overlay-settings.ts`: open, view, opacity, always on top, click-through, bounds) live in `localStorage`, shared by both windows; the controls are a popover in the main window (`overlay-panel/`), because a click-through overlay gets no input. Saved bounds are applied only if `app::on_screen` finds them on a connected monitor.

**Frontend.** Angular 21, standalone components, signals, zoneless, Vitest; `templateUrl` + `styleUrl` only.

| File | Responsibility |
| --- | --- |
| `src/app/characters.service.ts` | Wire types, `merge` by `(server, name)`, `CharactersService`: subscribes to both events first, then reads the snapshots, so later updates win |
| `src/app/gamedata.ts` | `GameData` types, `loadGamedata`, and the pure reset/refill math (`lastReset`, `nextReset`, `leftNow`, `fullAt`, `span`, `summarize` with `Summary.items`, `pick`) |
| `src/app/dashboard/` | The one page: header (status pill, reset countdowns), failure banner, roster of cards or the open character; keyboard-focusable cards |
| `src/app/character/` | Detail screen for one character (`CharacterRow`) |
| `src/app/emblem/` + `src/app/glyph.ts` | Diamond emblem and class icons: the game's own images (`public/classes/<class>.png`) and a mask-painted generic glyph (`unknown.svg`) |
| `src/app/pets/` + `src/app/route.ts` | Pets screen: collection, farm map, live radar layer and the farm route (`farmRoute`, pure) |
| `src/app/meter/` + `src/app/fight.ts` | Meter screen and overlay view; `fight.ts` is pure (skill folding, DPS window, player and skill rows) |
| `src/app/overlay/`, `overlay-panel/`, `overlay-settings.ts` | The overlay page, its control panel in the main window and the persisted preferences |
| `public/gamedata.json` | Hand-curated game facts: counter names, caps, reset kind, refills, classes, pets, farm regions and spawns, skill and monster names. Counter ids not in it are hidden |
| `src/locale/messages.xlf` | `ng extract-i18n` output; every user-facing string has an `@@` id |

**Time math.** A daily reset is 07:00 UTC and the weekly one is the `weekly_reset_day` at that hour. Refills land on `07:00 UTC + k * every_ms`, not on the time of the last update. Both are measured, with a `ponytail:` in `gamedata.ts` (fixed hour; use the game's next reset once decoded). Counters with `"ms": true` hold milliseconds in value and cap; the wire's own `ms` flag (mask bit0 of a `0b 61`/`0c 61` record) is not trusted for display; gamedata decides.

**Security surface.** `capabilities/default.json` grants `core:default` to the main window and nothing else. The CSP is `default-src 'self'` with `'unsafe-inline'` for styles only (Angular injects component styles at runtime).
