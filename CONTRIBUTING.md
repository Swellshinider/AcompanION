# Contributing

Read [`AGENTS.md`](AGENTS.md) first (it applies to people as well as coding agents), then [`docs/development.md`](docs/development.md) for setup.

## Workflow

1. Branch from `main` with a prefix: `feature/*`, `bugfix/*`, `docs/*`, `refactor/*`, `test/*` or `chore/*`.
2. Keep the change small and follow the existing structure. No new dependency without a stated reason.
3. Run `task check` (clippy, Rust tests, frontend tests). Run `task i18n` if you changed a user-facing string, and translate it in every `src/locale/messages.<locale>.xlf` (and give game names every language key) in the same change; the frontend tests check this.
4. Commit with a conventional, imperative message (`feat: ...`, `fix: ...`, `docs: ...`, `chore: ...`, `test: ...`).
5. Open a pull request against `main`. `main` only accepts pull requests that pass CI and are squash-merged.

## Ground rules

- **Passive only.** No memory reads, injection or input sent to the game.
- **Account data stays out of git.** Captures live in the gitignored `captures/` folder. Never commit, paste or attach a capture, a state file or a character name.
- Test fixtures are synthetic. Never derive one from a real capture.
- Protocol findings go into [`docs/protocol.md`](docs/protocol.md) in the same change, with evidence and no account data.

## Releases

Releases are tags named `vX.Y.Z` created by the maintainer. Tags are protected: they cannot be moved or deleted once pushed.
