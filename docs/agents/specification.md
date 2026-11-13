# AcompanION specification

## Intent

A Windows companion for AION 2 players who want to track progress across characters and read their combat results. The app passively decodes game traffic and keeps account data locally. The dashboard, pets, radar, overlays and combat meter are implemented; curated game data and live verification remain partial.

See the [architecture](../architecture.md) for implemented behavior and the [current status](status.md) for verified work, limitations and next steps.

## Current deliverable

Complete the pending live-validation pass in [Status — Not verified live yet](status.md#not-verified-live-yet), fixing issues found in the existing app before expanding its scope.

Acceptance criteria:

- Confirm each pending behavior in an elevated desktop session with the game, or leave it explicitly unchecked with the limitation recorded in status.
- Preserve already verified behavior and validate fixes with the narrowest relevant automated checks, followed by `task check` before committing.
- Record confirmed wire findings in [protocol.md](../protocol.md) and changed rationale in [decisions.md](../decisions.md), without account data.

This document does not claim those live checks have passed. The detailed checklist stays in status rather than being duplicated here.

## Accepted decisions and boundaries

- Passive capture only: no memory reads, injection or input sent to the game. Any future input automation needs its own separate, opt-in specification.
- Reuse the Rust core, Tauri desktop shell and Angular frontend; follow [AGENTS.md](../../AGENTS.md) for dependencies, privacy, localization and validation rules.
- English and Brazilian Portuguese ship together. Rust emits identifiers and raw values; the frontend owns display text.
- Keep architecture, protocol evidence and decision rationale in the shared human references. Keep durable intent and ongoing validation in `docs/agents/`; `.superpowers/` remains local scratch material.

The [decision log](../decisions.md) holds the rationale and superseded choices. Future build suggestions, macros and automation remain deferred as listed in status; this migration creates no specifications for them.

## Open questions

The actionable [protocol unknowns](status.md#open-protocol-unknowns) and [next steps](status.md#next-steps) remain in status, with supporting evidence in the protocol reference. Resolve them through deliberate experiments rather than treating hypotheses as implemented behavior.
