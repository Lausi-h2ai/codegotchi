# Codex-first MVP follow-ups

These findings are explicitly deferred because they are not required for the mandatory runnable vertical slice.

- Investigate a stateful lifecycle occurrence ledger if a future Codex release still omits an occurrence ID and real duplicate `SessionEnd` deliveries need to be distinguished from hook replay. Codex 0.146 fixes `SessionEnd.reason` to `other`, so the MVP safely deduplicates identical deliveries.
