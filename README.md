<!-- markdownlint-disable-file MD033 MD041 -->
<h1 align="center">AcompanION</h1>
<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
</p>

<p align="center">
  English | <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

A Windows companion for AION 2 that passively reads game traffic and tracks every character you play.

> **Status: early development.** The protocol decoder, the live engine and the desktop app (Tauri + Angular) work and ship as a Windows installer (unsigned, so SmartScreen warns on the first install). The curated game data (names, caps, resets) is partial, so some counters and dailies are not shown yet. See [`docs/agents/status.md`](docs/agents/status.md) for exactly where things stand.

## Quick Start

### Requirements

- Windows 11 with the [Npcap](https://npcap.com/) runtime (the installer’s default options are fine).
- Administrator rights for live capture; the app offers "Restart as admin".

### Installation

Download `AcompanION_<version>_x64-setup.exe` from the [latest release](https://github.com/Swellshinider/AcompanION/releases/latest) and run it. It installs for the current user (no administrator prompt). The installer checks for the [Npcap](https://npcap.com/) runtime and, if it is missing, offers to download and run Npcap's official installer (you click through its wizard) or to open its download page. Capture needs administrator rights (the app offers "Restart as admin").

On start the app checks the latest release; when a newer one exists, a banner offers "Update and restart".

### Uninstallation

Remove AcompanION in Windows Settings > Apps.

## Features

Working today:

- **Passive capture and own decoder.** Npcap capture of every AION 2 connection, TCP reassembly, frame and LZ4 decoding, no keys needed.
- **Per-character dashboard.** Every character seen, with tickets, energy, dailies and weeklies done against their cap, GS (item level) and combat power; updates live and survives restarts.
- **Pets.** The account's pet collection (souls, levels, closest to the next level), a farm map of the monsters that drop pet souls, a live radar of the monsters in view and your last known position, and a suggested farm route for the pets you have not maxed.
- **Overlay.** A movable, resizable, translucent window with the radar, the dailies or the DPS meter, each its own overlay that can be open at the same time. Opacity, always on top and click-through are settings of each.
- **DPS meter.** Fights ranked by damage with DPS, share, crits, healing and damage taken, a per-player detail (skills, targets) and a saved history.
- **Translatable UI.** Every user-facing string is extracted for translation. English and Brazilian Portuguese ship today, picked in Settings (or from the system language); a change to text is translated into every language in the same change (see [`docs/development.md`](docs/development.md), Translations).

## How it works

1. **Passive capture.** Packets are read through [Npcap](https://npcap.com/), loaded at runtime (no SDK needed). No memory reads, no injection, no input sent to the game.
2. **Own decoder.** Server to client traffic is length-framed and LZ4-compressed but not encrypted, so it decodes without keys (see [`docs/protocol.md`](docs/protocol.md)).
3. **Character state.** Decoded events update a per-character state (identity, combat power, checklist counters, pets) that is saved to `%APPDATA%\AcompanION\characters.json`. Fights are saved to `meter.json` next to it.

## Development

See the [development guide](docs/development.md) for prerequisites, building from source, tasks, capture analysis, translations and [releasing](docs/development.md#releasing), and the [contribution guide](docs/contributing.md) for the workflow.

## AI Coding Assistant

Read [AGENTS.md](AGENTS.md), the [project specification](docs/agents/specification.md) and [current status](docs/agents/status.md) before contributing.

## Documentation

See the [documentation index](docs/README.md) for human guides and agent context, including [architecture](docs/architecture.md), [protocol evidence](docs/protocol.md), [decisions](docs/decisions.md), [player patch notes](docs/patch-notes.md) ([pt-BR](docs/patch-notes.pt-BR.md)) and the [analysis helper](tools/re/README.md).

## Project policies

- [Contributing](CONTRIBUTING.md)
- [Security reporting](SECURITY.md)

### Safety and fair use

Capture is passive and read-only, the same approach as other community overlays, but it is still outside the game's official tooling. Use it on your own accounts at your own risk. AcompanION never sends anything to the game.

### Legal

AcompanION is an independent, unofficial project. It is not affiliated with, endorsed by or sponsored by NCSOFT. AION 2, its names, class icons and map images are property of NCSOFT and are used here only to identify game content. Use at your own risk.

## LICENSE

[MIT](LICENSE).
