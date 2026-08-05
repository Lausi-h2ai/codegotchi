# Codex-first runnable MVP final broad review

Date: 2026-08-05

Reviewed range: `488c9a1..3a5b566` (109 files, 17,409 additions, 403
deletions), with the implementation plan, all Task 1–6 reports/reviews and
adjudications, the verification record, the ADR, and the current follow-up
backlog.

This review did not modify implementation files, did not modify the backlog,
and did not create a commit.

## Exact verdict

**NOT READY FOR FINAL MVP ACCEPTANCE.**

The supervisor may not sign off the MVP yet. The supervisor-owned real
installed acceptance is still required, and one production-browser recovery
defect requires correction before that run can be accepted. The supervisor
may run the real smoke as a diagnostic once the correction is made, but the
current branch is not an acceptance-ready result.

## Classification summary

- `MVP-blocking`: 2
- `Backlog`: 0 new entries

## MVP-blocking findings

### 1. Recovered browser connections retain a stale error banner

After a state/stream error, the React hook only clears `error` after a
successful feed or clean action at `web/src/useCodeGotchi.ts:70` and
`web/src/useCodeGotchi.ts:84`. Successful snapshots and status changes at
`web/src/useCodeGotchi.ts:44` and `web/src/useCodeGotchi.ts:45` do not clear it.
The WebSocket client reports a successful connection at
`web/src/client.ts:188` and accepts/re-publishes a valid snapshot at
`web/src/client.ts:197`, but has no recovery success signal that clears the
hook error.

This is user-visible false state: a transient initial HTTP failure or stream
failure can leave a `role="alert"` saying the room is unavailable while the
same room is already connected and rendering an authoritative snapshot. It
fails the requested reconnect/error behavior and can mislead the operator
during the real Codex flow.

Targeted production-bundle check, run against the real Rust Task 3 fixture,
aborted only the first `/api/v1/state` request and allowed the WebSocket to
recover:

```text
{"initial_state_aborted":true,"alerts_after_ws_recovery":1}
```

The existing production Playwright reconnect test checks the `Connected`
status at `web/e2e/mvp.spec.ts:168` and the replacement snapshot at
`web/e2e/mvp.spec.ts:177`, but does not assert that the error alert is gone.
This requires a final implementation correction and a regression assertion
before acceptance.

### 2. The final real installed acceptance gate remains unproven

The automated evidence proves the compiled/installed-style launcher with a
fake Codex, the embedded production bundle, authenticated loopback behavior,
persistence/restart, Strict denial/retry/fail-open, and the fixture browser
flow. It does not prove the final integrated command with the installed
Codex:

```text
codegotchi run -- codex
```

The verification record explicitly says no real or paid Codex session ran at
`docs/verification/codex-first-mvp.md:3` and leaves the installed launch and
trust review unchecked at `docs/verification/codex-first-mvp.md:158`, with
real care/reconnect/error, Strict, restart, and cleanup still unchecked at
`docs/verification/codex-first-mvp.md:167` and
`docs/verification/codex-first-mvp.md:174`.

The earlier Task 1 spike is useful evidence for the hook schema/trust/denial
seam (`.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-1-report.md:16`
and `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-1-report.md:176`),
but it was not the final launcher/browser run. In particular,
the report says the actual base Codex config had no existing hooks and that
real execution with an existing user hook was not possible at
`.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-1-report.md:226`;
only the synthetic profile test covered that shape. The committed real
coexistence gate is intentionally ignored and its own record says it was not
completed without an API key at
`docs/adr/0002-codex-hook-profile-integration.md:190` and
`docs/adr/0002-codex-hook-profile-integration.md:198`.

The exact attempted gate in this environment was:

```text
cargo test -p codegotchi-cli --test installed_codex -- --ignored --nocapture
```

It stopped at `crates/codegotchi-cli/tests/installed_codex.rs:90` with:

```text
run the manual gate with OPENAI_API_KEY or CODEX_API_KEY set
```

Therefore real hook trust/coexistence through the final installed launcher,
the embedded browser during that run, real care/clean/reconnect/error
behavior, Strict refusal/recovery, restart persistence, and final cleanup
remain acceptance claims rather than current final evidence. This is an
acceptance blocker, not a claim that the fake/fixture path failed.

## Backlog

`Backlog — no new entries.` The existing
`docs/backlog/codex-first-mvp-followups.md` already contains the optional
reconnect-strengthening, fixture cleanup, WebSocket-token, debug-capability,
PID-identity, hard-kill profile, and ID-less lifecycle follow-ups. No further
optional architecture or polish item is warranted by this review.

## Evidence that passed

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  pass.
- `cargo test --workspace` — 110 passed, 0 failed, 1 intentionally ignored
  manual installed-Codex test; the static-asset tests included a real
  temporary `cargo install` and an installed-binary run outside the
  repository.
- `corepack pnpm test` — 29 frontend tests passed.
- `corepack pnpm lint` — pass.
- `corepack pnpm format:check` — pass.
- `corepack pnpm build` — Vite production build pass.
- `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test` — 7 embedded-production tests passed.
- A focused launcher harness kept a real WebSocket open while the fake Codex
  child was released; the server closed the socket and the launcher exited,
  so no open-stream shutdown hang was found.
- `codex --version` reported `codex-cli 0.146.0`, and
  `codex features list` reported `hooks` stable and enabled. These checks did
  not consume a real session.

`git diff --check 488c9a1..3a5b566` reported only trailing/EOF whitespace in
Task reports and the ADR; it found no implementation whitespace defect. That
documentation hygiene result is not classified as a product finding.

## Acceptance disposition

The branch has strong automated vertical evidence and the launcher shutdown,
auth, persistence, privacy, fail-open, care, Strict, and embedded-asset
contracts are covered. It must not yet be called MVP-complete: first correct
the stale browser error state and add the corresponding recovery assertion,
then run and record the supervisor-owned real installed acceptance with
normal Codex trust (no bypass), an actual pre-existing hook, browser care and
cleanup, Strict refusal/retry, restart, exit/signal behavior, and unchanged
Codex config/credentials.
