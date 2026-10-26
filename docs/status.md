# Status

Where things stand and what comes next. Update it at the end of every working session. Run commands through `task` (see [development.md](development.md)); wire-level evidence is in [protocol.md](protocol.md) and the reasons behind choices in [decisions.md](decisions.md).

## Where things stand

- **Decoder:** done. Length framing, own LZ4, ping resync (a capture that starts mid-stream decodes), typed events through `opcodes.json`: identity, combat power, GS (item level), checklist counters, Daeva Pass, Supply Emergency, kinah gained today, Duty Missions, pets, map id, radar and combat frames. `re decode` prints them.
- **Live engine:** done. `state.rs` keeps one session per client connection and a state per character, saved as JSON at most every 5 s under a single-writer lock; `live.rs` drives capture or replay; `re live` runs it headless.
- **Desktop app:** done. Roster of character cards, detail screen per character (tickets and energy with refill rhythm, weeklies and dailies as done/cap, GS, combat power), header badges for capture and game detection, reset countdowns, banners for each capture failure, "Restart as admin".
- **Pets:** the account's collection (obtained, per level, souls to max, closest pets); a farm map of the monsters that drop pet souls with zoom, pan and filters; a live radar of the monsters in view and the player's last exact position; a farm route over the remaining pets for the current region (`src/app/route.ts`).
- **Settings:** a screen (replaces the Overlay popover) with the overlay controls, close to the tray (tray icon with Open and Quit), automatic reconnection of capture when the adapter drops, and a "Check for updates now" button.
- **Shugo Festival alert:** a pop-up in a chosen corner (top/bottom, left/right) when a round starts, with a configurable time (3 to 300 s), click to close, a chime and an on/off switch, all in Settings, shown only while the game is running. The schedule is curated (every hour on the hour: confirmed for the global version, Season 1), not read from the wire; if the game changes it, edit `SHUGO_MINUTES_UTC` in `src/app/shugo.ts`. It uses the primary monitor.
- **Overlay:** three transparent, frameless windows, one each for the radar (minimap of the current region), the dailies and the meter, each switched on in Settings and open at the same time if wanted; each has its own opacity, always on top, click-through, saved position, auto-hide when the game is closed (the same for the radar, the dailies and the meter; the radar shows the last region the player was seen in, kept in `localStorage`, until the game says where they are). Its border and title bar show only on hover, with the content moving below the title bar while it is visible; the meter view has no background (opacity fades only its bars).
- **Combat meter:** damage hits and ticks, crits, summons credited to their owner, players in view, heals, overheal, damage taken and the fight's target; a fight opens only on a hit on a boss (a foe with a stagger gauge; normal monsters and PvP are not metered, and its adds count once it is open), closes 10 s after the last counted hit, each boss gets its own fight, a Spiritmaster's spirits count for their owner, and the last 50 are kept in `meter.json`. Combat power next to each name (the player's own from `56 36`, the others' from their spawn frame `45 36`). UI: a Meter screen (ranking, player detail with skills and targets, Clear history) and the overlay view.

## Not verified live yet

The meter overlay now has a default-off setting to hide 10 s after its last counted hit, including after boss death; both boss bars show current/max HP inside. The hover title bar now reserves space only while visible across the three overlays. All 258 frontend tests and the production web build pass (with existing CSS size budget warnings); the native window still needs the manual pass below.

These have unit tests or browser checks, but no one has watched them in a real elevated `task app` session:

- [ ] Settings in a real session: close to tray (X hides, tray icon Open and Quit, overlay stays up, state saved on Quit), reconnection (disable and enable the adapter during capture: banner appears and clears, the meter and characters carry on), and "Check for updates now" against a published release.
- [ ] Overlay: the real transparent window, content moving below the title bar only on hover in all three views, dragging and edge resizing, opacity, click-through reaching the game, always on top, reopening in place, centring after a monitor is unplugged, hiding and showing itself with the game, the app exiting with the overlay open, and the new meter look (bars without background, the boss health bar).
- [ ] Meter overlay: enable hiding outside combat, check it hides 10 s after the last counted hit even after boss death and reappears on the next fight; read current/max HP in both bars at narrow widths and low opacity.
- [ ] Overlay radar following a moving player, and a map change hiding it; the radar discs and the player ring in Verteron against a live game; the 250 ms update rate in a crowded zone.
- [ ] Footer "last soul gain" after a real kill; a live soul gain or level-up moving a Pets row. Soul and level frames on a connection with no identity (the app started after the login) go to the character with the freshest pet collection (`State::pet_owner`); confirm in game.
- [ ] Farm route on the real maps: whether the grid cell matches the real loot range, and visited tracking while moving.
- [ ] Meter in a real fight: the overlay view updating, a fight landing in the history 10 s after the last hit, names showing up, Clear history emptying the list and `meter.json`, `meter://update` at the real rate in a crowd.
- [ ] Boss fights against a reference meter (Ferocious Horn Den): one fight per boss with the same duration and totals, and no small fight left after a boss's death by hits on other foes (the old merged fight had some 23 s after its boss died, on a `2920591` entity with 1-damage hits). Checked on one boss against the reference meter's log (`spirit-1`, a Spiritmaster with the Fire and Water spirits: same total, hits, duration and skills); not yet with Earth and Wind spirits or a party.
- [ ] Others' combat power in the meter: the values match the party window on one recording (`cp-1`, 4 players); the tail rule has the one-capture limit noted in `docs/protocol.md`. Check it in a party fight in the real app.
- [ ] A live Daeva Pass change, a live equipment change updating GS, a live Supply Emergency delivery or sale, a live Duty Mission completion.
- [ ] The "Game detected" badge flipping with the game, and the new rows' look.

## Known limits

- **Meter DPS window:** DPS runs from the first to the last counted hit (the payload has no "now"), so a fight still going shows its DPS up to the last hit.
- **Mid-session identity gap:** an app started while the game is already connected has no identity until the next login or character swap, so no meter and no player ring on the radar until then.
- **`meter.json` holds other players' names** (and entity ids) from the fights. Clear history empties it. Healing and damage taken count only inside a fight, and other players' hits made before the local player's first hit on a target are dropped.
- **Quitting mid-fight loses that fight** (`Engine::flush` does not close it).
- **Skill variants** are folded by `id / 10000 * 10000` in the frontend; a reference meter's records list 231 skills, all with ids already at the folded form (and the 50 names we had agree with theirs), which supports it but is not proof. Skill names cover those 231 skills (`public/gamedata.json`) and 198 of them have an icon (`public/skills/`); a skill without an icon shows a tile with its initial, and one outside the 231 shows `Skill N` and a coloured tile. Monster names are partial (`Monster N` otherwise).
- **Premium caps assumed:** caps assume the premium membership (Shugo Festival keys 21, Odyle Energy 840). The expiry is not on the wire, so the dashboard cannot switch.
- **Class codes** are ranges per class (all 39 codes of nine classes are named in `public/gamedata.json`); what the later codes of a class mean (specialisations?) is not decoded.
- **Partial curated game data:** `public/gamedata.json` covers the main tickets, energy, weeklies and a few gains. Left out: per-dungeon Conquest and Exploration reward counts, Spacetime Rift time, a second Shugo Festival key id, the Abyss corridors, Odyle crafts. Counter ids not in the file are hidden. `dungeons` (map id, mode, bosses seen) lists only five dungeons, the open-world map names (20, 1110, 200003, 500038) are not recorded, and nothing in the app reads `dungeons` yet.
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
