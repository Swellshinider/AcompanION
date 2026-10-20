# Development

AcompanION is a Rust crate (`src-tauri/`) with a reverse-engineering CLI, a headless live engine and a Tauri desktop app, plus an Angular frontend at the repository root (`src/`, `public/`). Run everything from the repository root with [Task](https://taskfile.dev/); `task` lists every shortcut.

## Requirements

- Windows 11 (live capture is Windows only; tests and replays run anywhere Rust does)
- [Rust](https://www.rust-lang.org/) 1.97+ (edition 2024)
- [Task](https://taskfile.dev/)
- [Npcap](https://npcap.com/) runtime at `C:\Windows\System32\Npcap\wpcap.dll` for live capture and the ignored tests
- An **elevated** terminal for `task record`, `task live` and for the app to capture (without it the app still opens, shows the saved characters and offers "Restart as admin")
- [Node](https://nodejs.org/) 24 and [pnpm](https://pnpm.io/) 11 for the frontend and the app (`pnpm install` once at the repository root); the Angular and Tauri CLIs are dev dependencies
- Optional: [uv](https://docs.astral.sh/uv/) for `tools/re/aion_re.py`

## Tasks

| Task | What it does |
| --- | --- |
| `task build` | Build the crate and the `re` tool |
| `task test` | Rust unit tests |
| `task test:ignored` | Tests that need the Npcap runtime |
| `task web:test` | Frontend unit tests once |
| `task lint` | `cargo clippy --all-targets -- -D warnings` |
| `task check` | `lint`, `test`, then `web:test`; run before every commit |
| `task app` | Desktop app in dev mode (Angular dev server + Tauri; elevated terminal to capture) |
| `task app:build` | Release app, no installer: `src-tauri/target/release/acompanion.exe` |
| `task i18n` | Extract translatable strings to `src/locale/messages.xlf` |
| `task re -- <args>` | Any `re` subcommand: `devices`, `record`, `dump`, `decode`, `live` |
| `task devices` | List Npcap adapters |
| `task record -- <prefix> [--seconds N]` | Record a capture to `<prefix>.pcap` |
| `task live -- [flags]` | Follow live traffic (or `--replay` a pcap) into the character state |
| `task analyze -- <args>` | `tools/re/aion_re.py` through `uv` |

Without Task, each one is a Cargo command with `--manifest-path src-tauri/Cargo.toml`; `task -n <name>` prints the exact command.

## Capture workflow

Captures contain account data. They live in `captures/` (gitignored); never commit, paste or share them, and never derive a test fixture from one.

1. Quit AION 2 completely.
2. Elevated terminal: `task record -- captures/<name> --seconds 480`.
3. Launch the game, log in, stand still about 30 s, do **one** deliberate action, stand still about 30 s.
4. `task re -- dump captures/<name>.pcap --summary` for totals, entropy and gaps.
5. `task re -- decode captures/<name>.pcap` for typed events (contains character names).
6. For exploration: `task analyze -- findint captures/<name>.pcap <value> --at <t_us> --within 2`. See [`tools/re/README.md`](../tools/re/README.md).

Start `task record` **before** launching the game when you can, so the handshake is recorded. A mid-stream capture still decodes: the decoder resyncs on the server ping.

## Live engine

```powershell
task live                                   # elevated; Enter stops it
task live -- --seconds 60 --state out.json  # time-boxed, custom state file
task live -- --replay captures/<name>.pcap --state out.json   # offline replay
```

The state file defaults to `%APPDATA%\AcompanION\characters.json`. A replay writes nothing unless `--state` is given, so it never touches the live file. `live::run` has no automated test (it needs Npcap and the game); `Engine` is covered by synthetic frames.

## Desktop app

```powershell
task app          # elevated terminal to capture; unelevated still opens with the saved characters
task app:build    # release exe, no installer
```

`task app` runs `pnpm tauri dev`, which starts the Angular dev server (`pnpm start`, port 4200) and the Tauri window. The app reads and writes the same state file as `re live` (`%APPDATA%\AcompanION\characters.json`), so it shows what an earlier `re live` run saved. Both write that file, so do not run them against live traffic at the same time.

The window's states, the commands and events it uses are described in [`architecture.md`](architecture.md) ("Desktop app"). `task app` and `task app:build` need the frontend dependencies and the Rust toolchain; the first build compiles Tauri and takes a while.

### Game data (`public/gamedata.json`)

Names, caps, reset kinds and refills are not on the wire; the dashboard reads them from this hand-curated file, and counter ids not in it are hidden. It holds game facts only: no character names, no values.

1. Find the counter ids the engine stores for a character (`%APPDATA%\AcompanION\characters.json`) and match them **with confidence** to the in-game activity they belong to, for example by doing one action in the game and watching which counter moves. A counter you cannot match stays out; list it in [`status.md`](status.md).
2. Add the entry: `name.en`, `category` (`daily`, `weekly`, `ticket`, `energy`), `cap`, `reset` (`daily`, `weekly` or `null`), `refill` (`amount`, `every_ms`) or `null`, and `"ms": true` when value and cap are milliseconds (store time caps in ms). A class is a `classes` entry keyed by `class_code`. Another language is another key beside `en`.
3. `task web:test` (the shipped file is checked against the `GameData` shape), then `task app` to look at it.

Reset facts are in `src/app/gamedata.ts` and `weekly_reset_day`: daily reset 07:00 UTC, weekly on Wednesday (`3`), refills on `07:00 UTC + k * every_ms`. Change them with a test if the game changes.

### Translations

Every user-facing string in a template has an `i18n` attribute with a custom `@@` id. After adding or changing one, run `task i18n`; it rewrites `src/locale/messages.xlf` and a second run must leave no diff. Display text never comes from Rust.

## Tests

- Unit tests are inline in `#[cfg(test)]` modules. Fixtures are synthetic; a finding confirmed on a real capture is reproduced with a small hand-built frame.
- Frontend tests are Vitest specs beside the code (`*.spec.ts`, run by `pnpm ng test`); fixtures are synthetic. The `acompanion` bin has `test = false` on purpose ([`decisions.md`](decisions.md)), so its logic lives in `app.rs`, which is tested.
- Clippy runs with `-D warnings`; keep lint and both test suites green (`task check`).
- `task test:ignored` needs the Npcap runtime and a working adapter.
- For decoder changes, replay the captures you have with `task re -- decode` and compare the events before and after.

## Conventions

- Smallest change that solves the problem; follow the existing structure.
- A known simplification gets a `ponytail:` comment naming its ceiling and upgrade path.
- When a protocol finding is confirmed, update [`protocol.md`](protocol.md) in the same change, with evidence and without account data.
- Record a decision in [`decisions.md`](decisions.md); update [`status.md`](status.md) at the end of a session.
- Branches: `feature/*`, `bugfix/*`, `docs/*`, `refactor/*`, `test/*`, `chore/*`; commits use conventional prefixes (`feat:`, `fix:`, `docs:`, `chore:`, `test:`). Do not open a PR or merge without the maintainer's approval.
