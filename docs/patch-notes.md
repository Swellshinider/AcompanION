# Patch notes

What changed for players, in plain language. Every change a player can notice adds a line in the same commit, under the section of the next version (the top one); the notes of a release are taken from here (see [README](../README.md), Releasing).

How to write a line:

- Say what the player sees, not how it was done: no module names, opcodes, commit hashes or jargon.
- One short sentence per change, filed under **New**, **Improved** or **Fixed**.
- Changes nobody would notice (tests, refactors, docs) get no line.

The top section is the next version, named by the patch bump of the last release (`v0.1.1` is out, so `v0.1.2`; a bigger change may rename it to the minor or major it needs). When the release is cut, put its date on the heading and open a section for the version after it. Released versions stay below, newest first.

## v0.1.3 (next)

### Improved

- Meter: while a fight is on, every player shows two rates, the current one (last 5 seconds) and the average; when it ends only the average stays. The group's total works the same way, and so does the overlay.
- Meter: new look. A big group DPS at the top, the boss with its health bar and a Victory or Attempt tag, and a cleaner ranking with a column header.
- Meter: the fights are now a list on the left, grouped by day, with the boss, result, duration, group DPS and your own DPS and place.
- Meter: opening a player is more colourful: each figure has its own colour, a bar shows how the damage splits between skills, and each skill and target row has a bar. Skills show their in-game icon.
- Meter: many more skills have their real name instead of "Skill 123456".
- Meter: you can switch the ranking between damage dealt, damage taken and healing.
- Overlay minimap: hovering a monster shows its name next to the mouse.
- Overlay minimap: the route line at the bottom now just shows the next monster to farm.

## v0.1.2 (2026-10-22)

### New

- The combat meter knows many more boss and monster names (for example Ferocious Horn Nuakum, Necromancer Duanka and Watchdog Kwapo) instead of showing "Monster 1234".

### Fixed

- Combat meter: the damage of a Spiritmaster's spirits now counts for the Spiritmaster, so their total, DPS and place in the ranking are right.
- Combat meter: every boss now gets its own fight with its own time and DPS. Two bosses killed back to back used to show up as one long fight.

## v0.1.1 (2026-10-22)

- A small update to check that automatic updates work. Nothing changes in how the app looks or behaves.
