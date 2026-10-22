# Patch notes

What changed for players, in plain language. Every change a player can notice adds a line under **Unreleased** in the same commit; the notes of a release are taken from here (see [README](../README.md), Releasing).

How to write a line:

- Say what the player sees, not how it was done: no module names, opcodes, commit hashes or jargon.
- One short sentence per change, filed under **New**, **Improved** or **Fixed**.
- Changes nobody would notice (tests, refactors, docs) get no line.

When a release is cut, rename **Unreleased** to `vX.Y.Z (date)` and add a new empty **Unreleased** above it.

## Unreleased

### New

- The combat meter knows many more boss and monster names (for example Ferocious Horn Nuakum, Necromancer Duanka and Watchdog Kwapo) instead of showing "Monster 1234".

### Fixed

- Combat meter: the damage of a Spiritmaster's spirits now counts for the Spiritmaster, so their total, DPS and place in the ranking are right.
- Combat meter: every boss now gets its own fight with its own time and DPS. Two bosses killed back to back used to show up as one long fight.
