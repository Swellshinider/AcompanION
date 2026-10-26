# Patch notes

What changed for players, in plain language. Every change a player can notice adds a line in the same commit, under the section of the next version (the top one); the notes of a release are taken from here (see [README](../README.md), Releasing).

How to write a line:

- Say what the player sees, not how it was done: no module names, opcodes, commit hashes or jargon.
- One short sentence per change, filed under **New**, **Improved** or **Fixed**.
- Changes nobody would notice (tests, refactors, docs) get no line.
- Every line also goes into [patch-notes.pt-BR.md](patch-notes.pt-BR.md), translated, in the same place: the app shows this file in Settings › See what's in your version.

The top section is the next version, named by the patch bump of the last release (`v0.1.3` is out, so `v0.1.4`; a bigger change may rename it to the minor or major it needs). When the release is cut, put its date on the heading and open a section for the version after it. Released versions stay below, newest first.

## v0.2.4 (next)

### Improved

- The combat meter now recognizes global field bosses and Hero/Legend bosses across the game.

## v0.2.3 (2026-10-26)

### New

- Meter overlay: an optional setting hides it after 10 seconds without combat and shows it again when combat resumes.

### Improved

- The boss's current and total health now appear inside its health bar, both in the meter and its overlay.
- Overlay content now moves below the drag bar only while the bar is visible.

## v0.2.2 (2026-10-25)

### New

- Meter: four more monster names (Draconute Handler, Executor of the Magical Sword, Abyss Rusty Rotan, Dimensional Core) and the Krao Cave dungeon are now recognised.
- Meter: other players' combat power now shows next to their name, in the meter and in its overlay.
- When another AcompanION window is already open, the banner now has a **Close it and capture here** button that closes the other one and takes over.
- **Shugo Festival alert**: a pop-up in the corner of your choice tells you when the festival starts (every hour, on the hour) while the game is running. In Settings you can turn it on or off, pick the corner, set how many seconds it stays (3 to 300), choose whether a click closes it, and mute its sound. A button shows a test alert.

### Fixed

- Meter: a boss fight keeps the boss as its name and health bar after the boss dies, instead of showing one of its minions.

## v0.2.1 (2026-10-25)

### New

- A banner tells you when AcompanION is not running as administrator and offers **Restart as admin**.
- Settings: a **See what's in your version** button lists the changes of every version.
- Meter: your combat power now shows next to your name, in the meter and in its overlay. Other players do not show one yet.

### Improved

- Meter overlay: the boss now has a dark background behind it, so it is easier to read over the game.
- Meter overlay: the title bar that appears under the mouse no longer covers the boss's information.

## v0.2.0 (2026-10-25)

### Improved

- Settings: reorganized. Radar, Dailies and Meter are now separate overlay switches, each with its own opacity, always-on-top, click-through and auto-hide, and all three can be open at the same time.
- Meter overlay: a new look with rounded bars per player, a health bar for the boss and the full damage per second. It has no background, and the opacity setting now fades only the bars.
- Overlay: the border and title bar only show when the mouse is over it, and the thin white window border is gone.
- Meter: only boss fights are counted now. Normal monsters no longer start a fight, so a boss's numbers stay on screen after it dies, until the next boss.
- Meter: it knows more boss names (for example Neikel of the East, Executor Kaira and Zikel's Apparition) instead of showing "Monster 1234".

### Fixed

- Overlays: they reopen at the size and position you left them, instead of shrinking a little each time on a second monitor with a different display scaling.
- Pets overlay: "Next" now names a single monster instead of listing every monster at that spot.
- Pets overlay: a soul you collect now shows its pet and its souls (or Max) even when AcompanION was opened after you logged in.

## v0.1.3 (2026-10-23)

### New

- The Overlay button is now **Settings**: a page with the overlay options and the new ones below.
- Settings: an option to keep AcompanION running in the system tray when you close the window. Click the tray icon to bring it back, or choose Quit there.
- Settings: a **Check for updates now** button that tells you whether you are up to date.
- Settings: capture reopens by itself when the network connection drops and comes back. You can turn this off.
- You can remove a character from the roster: hover its card and click the red trash can (for example after deleting it in the game). It comes back only if you log in with it again. Needs capture running.
- AcompanION now speaks Brazilian Portuguese: the whole app, with the class and counter names, shows in Portuguese. Pet, monster and skill names are still in English for now.
- Settings: a **Language** option: Automatic (follows Windows, the default), English or Português (Brasil). A new choice applies after you restart AcompanION.
- The installer checks for Npcap, which AcompanION needs to read game traffic. If it is missing, you can let the installer download and run the official Npcap installer, or open its download page.

### Improved

- Meter: while a fight is on, every player shows two rates, the current one (last 5 seconds) and the average; when it ends only the average stays. The group's total works the same way, and so does the overlay.
- Meter: new look. A big group DPS at the top, the boss with its health bar and a Victory or Attempt tag, and a cleaner ranking with a column header.
- Meter: the fights are now a list on the left, grouped by day, with the boss, result, duration, group DPS and your own DPS and place.
- Meter: opening a player is more colourful: each figure has its own colour, a bar shows how the damage splits between skills, and each skill and target row has a bar. Skills show their in-game icon.
- Meter: many more skills have their real name instead of "Skill 123456".
- Meter: you can switch the ranking between damage dealt, damage taken and healing.
- Overlay minimap: hovering a monster shows its name next to the mouse.
- Overlay minimap: the route line at the bottom now just shows the next monster to farm.
- Overlay minimap: with "Hide when the game is closed" on, the minimap now stays up whenever the game is running, like the dailies and the meter, even in a map the radar does not know.
- The AcompanION title now fades the "compan" part, so the name reads as AION with a companion inside.

## v0.1.2 (2026-10-22)

### New

- The combat meter knows many more boss and monster names (for example Ferocious Horn Nuakum, Necromancer Duanka and Watchdog Kwapo) instead of showing "Monster 1234".

### Fixed

- Combat meter: the damage of a Spiritmaster's spirits now counts for the Spiritmaster, so their total, DPS and place in the ranking are right.
- Combat meter: every boss now gets its own fight with its own time and DPS. Two bosses killed back to back used to show up as one long fight.

## v0.1.1 (2026-10-22)

- A small update to check that automatic updates work. Nothing changes in how the app looks or behaves.
