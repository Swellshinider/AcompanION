# AGENTS.md

Context and rules for AI coding agents working in this repository. Humans: see [README.md](README.md).

## What this is

AcompanION: a Windows companion for AION 2 that passively decodes game traffic and shows per-character progress (dailies, tickets, energy, pets) and a combat meter. Rust core (`src-tauri/`) with a Tauri 2 + Angular 21 desktop app.

The protocol decoder, the headless live engine (`state.rs`, `live.rs`, `re live`) and the desktop app (`main.rs`, `app.rs`, `src/app/`, `public/gamedata.json`) are built, including the Pets screens, the movable translucent overlay and the combat meter. The curated game data is partial; `docs/status.md` lists what is verified, what is not and what comes next.

## Read in this order

1. `docs/status.md` — where we stopped and the next concrete steps (source of truth for "what now")
2. `docs/protocol.md` — what is known about the wire protocol, with evidence
3. `docs/decisions.md` — why choices were made (do not re-litigate without new evidence)
4. `docs/architecture.md` — pipeline, modules, state model, desktop app

## Commands

`Taskfile.yml` wraps the common ones (`task check`, `task test`, `task web:test`, `task lint`, `task live`, `task app`, `task app:build`, `task i18n`, `task re -- <args>`; `task` lists them). The raw commands below are what they run; the cargo ones go from the repo root with `--manifest-path src-tauri/Cargo.toml`:

```
cargo test    --manifest-path src-tauri/Cargo.toml
cargo test    --manifest-path src-tauri/Cargo.toml -- --ignored      # needs Npcap runtime
cargo clippy  --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo run     --manifest-path src-tauri/Cargo.toml --bin re -- <devices|record|dump|decode|live> ...
pnpm ng test --watch=false                                           # frontend tests (task web:test)
pnpm tauri dev | pnpm tauri build                                    # app (task app, task app:build)
pnpm i18n                                                            # extract src/locale/messages.xlf (task i18n)
uv run --with lz4 python tools/re/aion_re.py <chain|hist|findstr|findint|frame> ...
```

Expected today: Rust 173 tests pass (plus 2 in the `re` bin), 2 ignored; frontend 212 tests pass; clippy clean. Keep all true. The app needs an elevated terminal to capture; `pnpm ng build`, `pnpm i18n` and `pnpm tauri build --no-bundle` pass (the installer needs the updater signing key and is built by `.github/workflows/release.yml` on a `vX.Y.Z` tag; see README, Releasing).

## Hard rules

- **Passive only.** No memory reads, injection, or sending input to the game, in this codebase or in tooling. Input automation is a separate, future, opt-in spec.
- **Account data never leaves `captures/`.** `*.pcap`, `*.pcapng` and everything under `captures/` are gitignored and must stay out of commits, docs, issues and chat logs. Do not paste raw capture bytes anywhere; protocol notes use short structural examples with placeholders (`<name>`).
- **No new dependencies** without a stated reason. Current production deps: Rust anyhow, clap, etherparse, libloading, serde_json, tauri (the window and IPC, with the `tray-icon` feature; build-dep tauri-build), tauri-plugin-updater (signed self-update from GitHub Releases). Frontend: Angular packages (scaffolded by `ng new`), `@tauri-apps/api` (invoke/listen); dev `@angular/localize` (i18n) and `@tauri-apps/cli` (`pnpm tauri`). No direct `serde`: payloads are `serde_json::Value`. Python tools use only the stdlib plus `lz4`.
- **Smallest change that solves the problem.** Follow existing structure; no opportunistic refactors.
- **Every language, same change.** The app ships in English and Brazilian Portuguese (pt-BR). Any new or changed user-facing text (an `i18n` string) or game name (`public/gamedata.json`) is translated into ALL available languages in the same change: an entry in `src/locale/messages.xlf` (via `task i18n`) AND in every `src/locale/messages.<locale>.xlf`, and every language key (`en`, `pt`) on each name map. A name with no verified translation stays English-only only if it is listed in `src/app/untranslated-names.json`. Frontend tests fail otherwise (`translations.spec.ts`).
- **Rust emits ids and raw values, never display text.** All names, formatting and plurals belong to the frontend (i18n from day one; English first).
- **Commits:** small, imperative, conventional prefixes (`feat:`, `fix:`, `docs:`, `chore:`, `test:`). No co-author trailers and no tool/agent attribution in messages.
- **Patch notes:** a commit that changes something a player can notice also adds a plain-language line under the next version's section in `docs/patch-notes.md` (what the player sees, no jargon). Release notes are built from it.
- **Releases:** a `vX.Y.Z` tag creates a draft (README, Releasing). Before publishing, its notes are rewritten for players in plain language (what changed for the user), never a developer changelog.
- **Branches/PRs:** work on a `feature/*`, `bugfix/*`, `docs/*`, `refactor/*`, `test/*` or `chore/*` branch, never directly on the default branch. Do not open PRs or merge without explicit approval from the maintainer.

## Conventions

- Rust edition 2024, `clippy -D warnings`, unit tests inline in `#[cfg(test)]`. Unsafe/FFI is confined to `src-tauri/src/npcap.rs`; layout-mirror structs are `#[repr(C)]` with `#[allow(dead_code)]`.
- Test fixtures are synthetic. Never derive a fixture from a real capture.
- A known simplification gets a `ponytail:` comment naming its ceiling and upgrade path.
- Frontend: Angular components use `templateUrl` + `styleUrl` with sibling `.html`/`.scss`, never inline; `pnpm`, not npm; every user-facing string has an `i18n` attribute with an `@@` id (`task i18n` after changes, then add its `<target>` to every `src/locale/messages.<locale>.xlf`; the UI language is the Settings choice or the system language, loaded by `src/app/locale.ts`). Counter and class names come from `public/gamedata.json` (game facts only, no account data), with one key per language (`en`, `pt`); `pick()` falls back to `en`, and names still English-only are listed in `src/app/untranslated-names.json` (shrink it, never grow it silently). Python: `uv`.
- The `acompanion` bin has `test = false` (a Tauri-linked test harness may not start on Windows); put testable logic in `src-tauri/src/app.rs`.
- When a protocol finding is confirmed, update `docs/protocol.md` in the same change, with evidence and without account data.

## Workflow used here

brainstorm → spec → plan, kept locally in `.superpowers/` (gitignored); one implementer per task, a reviewer after each task, a whole-branch review at the end. Durable decisions go into `docs/decisions.md` and `docs/status.md`.

## Environment notes

- Live capture needs an **elevated terminal** and Npcap at `C:\Windows\System32\Npcap\wpcap.dll`.
- Start `re record` **before** launching the game when you can: it also records the handshake. A mid-stream capture is still decodable (`tools/re/aion_re.py` resyncs on the server ping).
