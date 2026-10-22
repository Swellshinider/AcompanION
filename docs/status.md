# Status

Where things stand and what comes next. Update it at the end of every working session. Run commands through `task` (see [development.md](development.md)); wire-level evidence is in [protocol.md](protocol.md) and the reasons behind choices in [decisions.md](decisions.md).

## Where things stand

- **Decoder:** done. Length framing, own LZ4, ping resync (a capture that starts mid-stream decodes), typed events through `opcodes.json`: identity, combat power, GS (item level), checklist counters, Daeva Pass, Supply Emergency, kinah gained today, Duty Missions, pets, map id, radar and combat frames. `re decode` prints them.
- **Live engine:** done. `state.rs` keeps one session per client connection and a state per character, saved as JSON at most every 5 s under a single-writer lock; `live.rs` drives capture or replay; `re live` runs it headless.
- **Desktop app:** done. Roster of character cards, detail screen per character (tickets and energy with refill rhythm, weeklies and dailies as done/cap, GS, combat power), header badges for capture and game detection, reset countdowns, banners for each capture failure, "Restart as admin".
- **Pets:** the account's collection (obtained, per level, souls to max, closest pets); a farm map of the monsters that drop pet souls with zoom, pan and filters; a live radar of the monsters in view and the player's last exact position; a farm route over the remaining pets for the current region (`src/app/route.ts`).
- **Overlay:** a transparent, frameless second window with the radar (minimap of the current region), the dailies or the meter; opacity, always on top, click-through, saved position, auto-hide when the game is closed (the radar also when no region is known; it falls back to the last region the player was seen in, kept in `localStorage`, so an app started with the game already running still opens the Verteron map).
- **Combat meter:** damage hits and ticks, crits, summons credited to their owner, players in view, heals, overheal, damage taken and the fight's target; fights close after 10 s without a counted hit, each boss gets its own fight, a Spiritmaster's spirits count for their owner, and the last 50 are kept in `meter.json`. UI: a Meter screen (ranking, player detail with skills and targets, Clear history) and the overlay view.

## Not verified live yet

These have unit tests or browser checks, but no one has watched them in a real elevated `task app` session:

- [ ] Overlay: the real transparent window, dragging and edge resizing, opacity, click-through reaching the game, always on top, reopening in place, centring after a monitor is unplugged, hiding and showing itself with the game, the app exiting with the overlay open.
- [ ] Overlay radar following a moving player, and a map change hiding it; the radar discs and the player ring in Verteron against a live game; the 250 ms update rate in a crowded zone.
- [ ] Footer "last soul gain" after a real kill; a live soul gain or level-up moving a Pets row.
- [ ] Farm route on the real maps: whether the grid cell matches the real loot range, and visited tracking while moving.
- [ ] Meter in a real fight: the overlay view updating, a fight landing in the history 10 s after the last hit, names showing up, Clear history emptying the list and `meter.json`, `meter://update` at the real rate in a crowd.
- [ ] Boss fights against a reference meter (Ferocious Horn Den): one fight per boss with the same duration and totals, and no small fight left after a boss's death by hits on other foes (the old merged fight had some 23 s after its boss died, on a `2920591` entity with 1-damage hits). Checked on one boss against the reference meter's log (`spirit-1`, a Spiritmaster with the Fire and Water spirits: same total, hits, duration and skills); not yet with Earth and Wind spirits or a party.
- [ ] A live Daeva Pass change, a live equipment change updating GS, a live Supply Emergency delivery or sale, a live Duty Mission completion.
- [ ] The "Game detected" badge flipping with the game, and the new rows' look.

## Known limits

- **Meter DPS window:** DPS runs from the first to the last counted hit (the payload has no "now"), so a fight still going shows its DPS up to the last hit.
- **Mid-session identity gap:** an app started while the game is already connected has no identity until the next login or character swap, so no meter and no player ring on the radar until then.
- **`meter.json` holds other players' names** (and entity ids) from the fights. Clear history empties it. Healing and damage taken count only inside a fight, and other players' hits made before the local player's first hit on a target are dropped.
- **Quitting mid-fight loses that fight** (`Engine::flush` does not close it).
- **Skill variants** are folded by `id / 10000 * 10000` in the frontend; this is a hypothesis. Skill and monster names are partial (`Skill N`, `Monster N` otherwise).
- **Premium caps assumed:** caps assume the premium membership (Shugo Festival keys 21, Odyle Energy 840). The expiry is not on the wire, so the dashboard cannot switch.
- **Class codes** are ranges per class (all 39 codes of nine classes are named in `public/gamedata.json`); what the later codes of a class mean (specialisations?) is not decoded.
- **Partial curated game data:** `public/gamedata.json` covers the main tickets, energy, weeklies and a few gains. Left out: per-dungeon Conquest and Exploration reward counts, Spacetime Rift time, a second Shugo Festival key id, the Abyss corridors, Odyle crafts. Counter ids not in the file are hidden.
- **Radar:** a dead monster stays up to about 4 s until its despawn; one radar for the whole capture (a second client on the machine would mix); the player's position is the last exact fix, as client to server traffic is encrypted.
- Exclusive fullscreen hides the overlay; turning click-through off from inside the game needs Alt+Tab to the main window.

## Open protocol unknowns

Each needs one deliberate in-game experiment, recorded with `task record -- captures/<name>` from before the first login:

| Unknown | Experiment |
| --- | --- |
| Daily attendance (the claim coincides only with generic item-gain frames `1b 56` / `8a 56`) | After the daily reset, record before the first login, claim only the daily attendance, then relog on the same character and diff the world-entry frames |
| Deaths | Die once in a dungeon or in the field |
| Absorbed damage | Take damage while an absorb shield is active |
| Spiritstones | Use a spiritstone consumable |
| Altgard and Chaotic Lower Reshanta map ids | Enter each map while recording, read the `map_enter` ids with `task re -- decode`, list them in `public/gamedata.json` `pet_regions.<key>.maps` (until then those regions work only through the monsters in view) |
| Membership expiry | A recording that includes a purchase or renewal; scans of earlier recordings found it in no encoding |
| `0c 61` leading and trailer bytes, `24 e3` fields (`a`, `ms`, `x`, sub-lists, tag byte), the `charged` varint of `0b 61` | One recording per counter changing alone, then diff the field against the action |

Layouts and evidence for everything decoded are in [protocol.md](protocol.md).

## Next steps

1. Manual pass of the overlay, radar and meter in a real session (the checklist above), fixing what it finds.
2. Real dailies: curate more of `public/gamedata.json` and decode attendance once its recording exists.
3. Name more skills, monsters and class specialisations as recordings allow.
4. Later, each with its own spec: build and macro suggestions, and opt-in input automation (separate, heavy ban-risk warnings).

## Deferred issues

- `tcp.rs`: truncated or large-send-offload frames may need `LaxSlicedPacket` if `dump` reports gaps on a clean capture.
- `re record` chooses the default adapter from the route to 8.8.8.8, not to the game server; pass `--device` to override.
- Installer and updater: verified end to end on 2026-10-22 (install, uninstall, and the in-app update from 0.1.0 to 0.1.1 through the banner). The installer is unsigned (no Authenticode certificate), so SmartScreen warns on first install.
- Opcodes are remapped per game build; there is no build detection or per-patch update flow yet.
