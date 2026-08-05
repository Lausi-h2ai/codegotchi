# Task 1 supervisor adjudication

Date: 2026-08-05

Task 1 is approved after the permitted second focused correction.

## MVP-blocking

- Resolved: generated profiles invoke the installed `codegotchi` binary name.
- Resolved: prompt and tool event IDs use the official turn/tool occurrence IDs.
- Resolved: persisted executable metadata is restricted to a bounded allowlist.
- Resolved: fixtures use the Codex 0.146 `tool_use_id` and `tool_response` fields.
- Resolved: the installed-Codex denial harness parses the typed event envelope and proves the denied disposable `cargo` command did not execute.
- Resolved: lifecycle IDs incorporate the official `SessionStart.source` discriminator.

## Backlog

- Codex 0.146 supplies no occurrence ID for identical repeated `SessionEnd` payloads and fixes `reason` to `other`. The adapter therefore treats an identical delivery as an idempotent replay. A stateful occurrence ledger is deferred unless a real MVP integration failure demonstrates it is needed.

The remaining lifecycle limitation is not load-bearing for the vertical MVP: the installed spike observed session boundaries, while all turn and tool events use official occurrence IDs. No further Task 1 correction round is authorized.
