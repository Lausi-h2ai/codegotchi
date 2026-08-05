# Codex-first MVP follow-ups

These findings are explicitly deferred because they are not required for the mandatory runnable vertical slice.

- Investigate a stateful lifecycle occurrence ledger if a future Codex release still omits an occurrence ID and real duplicate `SessionEnd` deliveries need to be distinguished from hook replay. Codex 0.146 fixes `SessionEnd.reason` to `other`, so the MVP safely deduplicates identical deliveries.
- Define and validate a syntax-safe browser WebSocket token contract, or encode tokens before using them as RFC 6455 subprotocol values; cover URL and subprotocol edge cases.
- Make the Task 3 real-backend fixture shut down gracefully or clean its unique SQLite `-wal` and `-shm` sidecars after abrupt termination.
- Strengthen the Playwright reconnect scenario by mutating backend state while the stream is disconnected and asserting that the replacement snapshot contains the change.
