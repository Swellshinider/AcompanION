<!-- markdownlint-disable-file MD033 MD041 -->
<h1 align="center">AcompanION</h1>
<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
</p>

<p align="center">
  A Windows companion for AION 2 that passively reads game traffic and tracks every character you play.
</p>

> **Status: early development.** The protocol decoder, the live engine and the desktop app (Tauri + Angular) work and ship as a Windows installer (unsigned, so SmartScreen warns on the first install). The curated game data (names, caps, resets) is partial, so some counters and dailies are not shown yet. See [`docs/status.md`](docs/status.md) for exactly where things stand.

## Features

Working today (from source):

- **Passive capture and own decoder.** Npcap capture of every AION 2 connection, TCP reassembly, frame and LZ4 decoding, no keys needed.
- **Per-character dashboard.** Every character seen, with tickets, energy, dailies and weeklies done against their cap, GS (item level) and combat power; updates live and survives restarts.
- **Pets.** The account's pet collection (souls, levels, closest to the next level), a farm map of the monsters that drop pet souls, a live radar of the monsters in view and your last known position, and a suggested farm route for the pets you have not maxed.
- **Overlay.** A movable, resizable, translucent window with the radar, the dailies or the DPS meter. Opacity, always on top and click-through are settings.
- **DPS meter.** Fights ranked by damage with DPS, share, crits, healing and damage taken, a per-player detail (skills, targets) and a saved history.
- **Translatable UI.** Every user-facing string is extracted for translation (English first).

## How it works

1. **Passive capture.** Packets are read through [Npcap](https://npcap.com/), loaded at runtime (no SDK needed). No memory reads, no injection, no input sent to the game.
2. **Own decoder.** Server to client traffic is length-framed and LZ4-compressed but not encrypted, so it decodes without keys (see [`docs/protocol.md`](docs/protocol.md)).
3. **Character state.** Decoded events update a per-character state (identity, combat power, checklist counters, pets) that is saved to `%APPDATA%\AcompanION\characters.json`. Fights are saved to `meter.json` next to it.

## Install

Download `AcompanION_<version>_x64-setup.exe` from the [latest release](https://github.com/Swellshinider/AcompanION/releases/latest) and run it. It installs for the current user (no administrator prompt) and you remove it in Settings > Apps. You still need the [Npcap](https://npcap.com/) runtime, and capture needs administrator rights (the app offers "Restart as admin").

On start the app checks the latest release; when a newer one exists, a banner offers "Update and restart".

## Quick start

### Requirements

- Windows 11 with the [Npcap](https://npcap.com/) runtime (the installer's default options are fine);
- [Rust](https://www.rust-lang.org/) 1.97 or newer (edition 2024);
- [Task](https://taskfile.dev/) for the repository shortcuts (optional, see below);
- [Node](https://nodejs.org/) 24 and [pnpm](https://pnpm.io/) 11, for the desktop app (`pnpm install` once);
- An **elevated** (Administrator) terminal for live capture;
- Optional, for the analysis scripts: [uv](https://docs.astral.sh/uv/).

### Build and run

```powershell
git clone https://github.com/Swellshinider/AcompanION.git
cd AcompanION
task build
task devices      # list the Npcap adapters
task live         # elevated terminal; press Enter to stop
```

`task live` follows every game connection, whether you start it before or after the game, and a mid-stream start is decoded too. Each changed character is printed as one JSON line and written to the state file. The output contains character names, so do not paste it anywhere.

Useful variants:

```powershell
task live -- --seconds 60                          # stop after a minute
task live -- --replay captures/x.pcap --state out.json   # replay a capture, no adapter needed
task live -- --device "<adapter name>"             # pick the adapter from `task devices`
```

### Desktop app

```powershell
pnpm install
task app          # elevated terminal to capture; unelevated shows saved characters and offers "Restart as admin"
task app:build    # release exe, no installer (the installer is built by the release workflow)
```

The dashboard lists every character seen with what is left to collect and updates live. It reads and writes the same state file as `task live`.

### Without Task

Every task is a plain command; run it from the repository root. For example, `task live` is:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin re -- live
```

and `task app` is `pnpm tauri dev`. `task -n <name>` prints the exact command of any task.

## Development

The same tools as above are enough; the Angular and Tauri CLIs are dev dependencies installed by `pnpm install`.

| What | Command |
| --- | --- |
| Lint (clippy, warnings as errors) | `task lint` |
| Rust unit tests | `task test` |
| Frontend unit tests | `task web:test` |
| Lint + Rust + frontend tests (run before every commit) | `task check` |
| Tests that need the Npcap runtime | `task test:ignored` |
| Build the crate and the `re` tool | `task build` |
| Extract translatable strings | `task i18n` |

### Releasing

The version lives in `src-tauri/Cargo.toml` only. One-time setup: `pnpm tauri signer generate -w <path>`, put the public key in `plugins.updater.pubkey` of `src-tauri/tauri.conf.json`, and add the private key and its password as the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never commit the private key: without it nobody can ship updates, and anyone who has it can.

1. Bump `version` in `src-tauri/Cargo.toml` (and `Cargo.lock`) through a pull request and merge it.
2. On `main`: `git tag vX.Y.Z && git push origin vX.Y.Z`. The `Release` workflow builds the installer and creates a **draft** release.
3. Write the release notes into the draft (`gh release edit vX.Y.Z -R Swellshinider/AcompanION --notes-file <file>`), starting from the "Unreleased" section of `docs/patch-notes.md` (then rename it to this version). They are for players, not developers: plain language, what is new or fixed in terms of what the user sees, no module names, commit hashes or jargon. Keep the install steps and the SmartScreen note. Do not paste the commit list.
4. Check the draft and publish it. Only then do installed apps see the update.

Hotfix for an old version: `git switch -c release/X.Y vX.Y.Z`, fix, bump the patch version, tag `vX.Y.(Z+1)` on that branch.

### Reverse-engineering workflow

```powershell
# quit the game first, start this, THEN launch the game and log in
task record -- captures/my-login --seconds 600

task re -- dump   captures/my-login.pcap --summary    # reassembled streams
task re -- decode captures/my-login.pcap              # typed events as JSON lines
task analyze -- chain captures/my-login.pcap          # ad-hoc Python analysis
```

`task record` writes `<prefix>.pcap`. It contains account data, so it lives in `captures/`, which is gitignored. Never commit or share captures.

See [`docs/development.md`](docs/development.md) for the full guide and [`docs/architecture.md`](docs/architecture.md) for how the pieces fit.

## AI coding assistants

If you are an LLM or AI-powered coding assistant, read [`AGENTS.md`](AGENTS.md) and the relevant docs under [`docs/`](docs) before contributing.

## Documentation

- [`docs/status.md`](docs/status.md) - where things stand, what is not verified, what is next
- [`docs/development.md`](docs/development.md) - setup, tasks, capture workflow, game data, translations
- [`docs/architecture.md`](docs/architecture.md) - pipeline, modules, threading, state model and the desktop app
- [`docs/protocol.md`](docs/protocol.md) - everything known about the wire protocol, with evidence
- [`docs/decisions.md`](docs/decisions.md) - why things are the way they are
- [`tools/re/README.md`](tools/re/README.md) - the Python analysis helper

## Safety and fair use

Capture is passive and read-only, the same approach as other community overlays, but it is still outside the game's official tooling. Use it on your own accounts at your own risk. AcompanION never sends anything to the game.

## Legal

AcompanION is an independent, unofficial project. It is not affiliated with, endorsed by or sponsored by NCSOFT. AION 2, its names, class icons and map images are property of NCSOFT and are used here only to identify game content. Use at your own risk.

## License

[MIT](LICENSE).
