# Documentation index

## Human guides and shared references

| Document | What it is | Keep updated when |
| --- | --- | --- |
| [development.md](development.md) | Setup, source builds, tasks, capture workflow, game data, translations and releases | Commands or workflow change |
| [contributing.md](contributing.md) | Contribution workflow, validation and ground rules | Contribution policy changes |
| [architecture.md](architecture.md) | Pipeline, modules, state and threading model | A module, boundary or data flow changes |
| [protocol.md](protocol.md) | Wire protocol findings with evidence | A finding is confirmed or disproved |
| [decisions.md](decisions.md) | Decision log with rationale | A decision is made or changed |
| [patch-notes.md](patch-notes.md) / [pt-BR](patch-notes.pt-BR.md) | Player-facing changes, shown in the app and used for release notes | A commit changes something a player can notice |
| [Analysis helper](../tools/re/README.md) | Python tools for offline protocol analysis | Tool usage changes |

## Agent context

Start at [AGENTS.md](../AGENTS.md) for repository rules and the reading order.

| Document | What it is | Keep updated when |
| --- | --- | --- |
| [agents/specification.md](agents/specification.md) | Overall intent, current deliverable, acceptance criteria and links to accepted decisions | Scope, intent or the next deliverable changes |
| [agents/status.md](agents/status.md) | What is verified, pending live checks, known limits and next steps | End of every working session |

If upcoming work spans multiple deliverables, keep feature specifications in `agents/<feature>.md` and link them from the overall specification. Local `.superpowers/` material is scratch work; durable knowledge belongs in the tracked documents above.

## Conventions

- Technical docs are in English. Both root READMEs and player patch notes have Brazilian Portuguese versions.
- No account data, tokens, character names or raw capture bytes in any document; use placeholders.
- Status is the source of truth for what comes next; specifications describe intent, and shared references describe implementation and evidence.
- Root [CONTRIBUTING.md](../CONTRIBUTING.md) remains GitHub's entry point to the contribution guide. [SECURITY.md](../SECURITY.md) and [LICENSE](../LICENSE) remain repository policies.
