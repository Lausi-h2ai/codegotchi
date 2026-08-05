# Final focused correction report

Date: 2026-08-05

## Scope

This correction addresses only final-review finding 1: a transient initial
state or stream error remained visible after an authoritative WebSocket
snapshot restored the room. No backend, domain, persistence, launcher, hook,
real-Codex evidence, or backlog files were changed.

## TDD evidence

The regression test was added before the production change:

```text
corepack pnpm --filter @codegotchi/web exec vitest run src/useCodeGotchi.test.tsx
```

Red result: 1 failed, 1 passed. The new recovery assertion received the
stale `{ code: "client_error", message: "transient state failure" }` instead
of `null` after the WebSocket snapshot.

The minimal production change clears the hook error in the existing
`onSnapshot` observer callback. The focused test then passed:

```text
Test Files  1 passed (1)
Tests  2 passed (2)
```

The production-bundle test now aborts only the first `/api/v1/state` request
with Playwright routing, leaves the real Rust WebSocket untouched, asserts the
authoritative room is `Connected`, and requires zero `role=alert` elements.

## Automated results

- `corepack pnpm test`: PASS — 3 files, 30 tests.
- `corepack pnpm lint`: PASS.
- `corepack pnpm format:check`: PASS.
- `corepack pnpm build`: PASS — Vite production bundle built.
- `node web/scripts/embed-web.mjs`: PASS — embedded bytes refreshed.
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS — 110 passed, 0 failed, 1 intentionally
  ignored manual installed-Codex test; doc-tests 0.

The required production browser command was run exactly as follows:

```text
LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test
```

It did not reach Chromium assertions. The Rust fixture exited before ready
because the execution environment denied its loopback bind:

```text
Error: Bind(Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" })
Error: Task 3 backend fixture exited before ready (1)
```

An additional safe retry with `CODEX_SANDBOX_NETWORK_DISABLED` removed had
the same bind denial. The supervisor then ran the exact command outside the
delegated sandbox: all 7 production Playwright tests passed, including the
new initial-state-failure recovery assertion.

## Changed files

- `web/src/useCodeGotchi.ts`
- `web/src/useCodeGotchi.test.tsx`
- `web/e2e/mvp.spec.ts`
- `crates/codegotchi-cli/web-dist/index.html`
- `crates/codegotchi-cli/web-dist/assets/index-BbD4drv9.js`
- removed stale `crates/codegotchi-cli/web-dist/assets/index-Dce04d7P.js`
- this report

The delegated implementer could not stage because its managed `.git` mount
was read-only and reported:

```text
fatal: Unable to create '/home/laurent/codegatchi/.git/index.lock': Read-only file system
```

The supervisor verified the browser gate and created the single correction
commit from the writable supervising environment.
