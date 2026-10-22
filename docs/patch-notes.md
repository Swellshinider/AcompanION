# Patch notes

What changed for players, in plain language. Every change a player can notice adds a line in the same commit, under the section of the next version (the top one); the notes of a release are taken from here (see [README](../README.md), Releasing).

How to write a line:

- Say what the player sees, not how it was done: no module names, opcodes, commit hashes or jargon.
- One short sentence per change, filed under **New**, **Improved** or **Fixed**.
- Changes nobody would notice (tests, refactors, docs) get no line.

The top section is the next version, named by the patch bump of the last release (`v0.1.1` is out, so `v0.1.2`; a bigger change may rename it to the minor or major it needs). When the release is cut, put its date on the heading and open a section for the version after it. Released versions stay below, newest first.

## v0.1.3 (next)

### Improved

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
