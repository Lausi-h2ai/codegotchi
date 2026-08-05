# Task 5 report — exact launcher, installed UI, and lifecycle cleanup

Date: 2026-08-05

## Result

The Task 5 vertical increment is implemented without a commit. The installed
`codegotchi` binary accepts the exact launcher shape
`codegotchi run -- codex [ordinary Codex arguments...]`, starts the
authoritative loopback runtime first, serves the committed Task 3 bundle from
compile-time embedded bytes, launches Codex with an additive temporary hook
profile, forwards terminal signals while waiting, returns Codex's numeric exit
status, and cleans only the metadata/profile owned by that run.

## TDD evidence

The new process and static-asset tests were added before the implementation.
The initial RED focused run failed as expected: `run` was not yet accepted by
the CLI, the production server still returned the old JSON 404 for UI routes,
and the first installed-binary fixture exposed the incorrect install working
directory. The implementation and test-fixture corrections turned those
failures into the GREEN results below.

Focused GREEN results:

- `cargo test -p codegotchi-cli --test process_wrapper`: 11 passed.
- `cargo test -p codegotchi-cli --test static_assets`: 2 passed, including a
  real `cargo install` and an installed-binary run from outside the repository.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  passed.
- `cargo test --workspace`: passed. The existing manual installed-Codex gate
  remains intentionally ignored; its non-manual companion passed.

The process-wrapper coverage verifies exact trailing argument order after the
generated profile, raw stdin/stdout/stderr and ANSI bytes, current directory,
metadata/profile environment, exit status, malformed command and all profile
conflicts before mutation, missing/non-executable/self-resolving Codex paths,
recursive symlinks, spawn cleanup, nonfatal browser failure, SIGINT/SIGTERM/
SIGWINCH forwarding, additive config/credential byte identity, metadata modes
and token syntax, stale-session cleanup, preservation of unrelated/active
files, and SQLite pet-state reload/distinction across repositories.

## Embedded UI and install proof

The existing frontend build/embed flow was run:

- `corepack pnpm build`: passed.
- `node web/scripts/embed-web.mjs`: passed.
- Byte-for-byte comparisons of `web/dist/index.html` and every generated CSS/
  JS asset against `crates/codegotchi-cli/web-dist`: passed.
- Checks for Vite development markers (`@vite/client`, `import.meta.hot`, and
  local Vite URLs): passed; none are present.
- `corepack pnpm test`: passed, 29 tests.
- `corepack pnpm lint`: passed.
- `corepack pnpm format:check`: passed.

`build.rs` walks every file under `web-dist`, generates a sorted asset table,
and uses `include_bytes!` at compile time. The installed-binary test installs
with:

```text
cargo install --path crates/codegotchi-cli --root <temporary-root> --locked --offline
```

It runs the installed executable from a directory outside the repository with
`CODEGOTCHI_REAL_CODEX` pointed at the fake Codex and
`CODEGOTCHI_BROWSER=none`. The test confirms the UI line and fake Codex output,
and confirms the install root contains neither `web-dist` nor pnpm/runtime
frontend dependencies. The production-server test also verifies `/`, every
referenced hashed asset and MIME type, SPA fallback, typed unknown API 404s,
and unchanged authentication for state-changing routes.

## Cleanup, persistence, and configuration checks

- Validation resolves the command shape, Codex candidate, and running
  CodeGotchi executable before creating state, runtime, metadata, or profile
  files.
- Repository identity is derived from the canonical Git worktree root, with a
  canonical current-directory fallback. State is kept in
  `$XDG_STATE_HOME/codegotchi/state.sqlite`, or
  `$HOME/.local/state/codegotchi/state.sqlite`.
- Runtime metadata is created with the existing create-new 0600 writer below a
  mode-0700 CodeGotchi runtime directory. The token is two UUIDv4 simple forms
  separated by a safe hyphen and is used only in the URL fragment.
- Existing `CODEX_HOME/config.toml` and auth/credential files retain their
  checksums. Only the unique additive `codegotchi-<uuid>.config.toml` is
  created and later removed.
- Normal exit, spawn/wait failure, and forwarded termination clean the owned
  metadata/profile while preserving SQLite state. The next run removes only
  stale valid CodeGotchi session metadata; unrelated and active files remain.
- The launcher uses the canonical installed executable in the hook command,
  inherits terminal streams directly without a PTY, and keeps the current
  working directory and ordinary Codex arguments intact.

## Exact changed files

- `Cargo.toml` and generated `Cargo.lock` (Tokio signal support).
- `README.md`.
- `crates/codegotchi-cli/build.rs`.
- `crates/codegotchi-cli/src/assets.rs` and `src/launcher.rs`.
- `crates/codegotchi-cli/src/cli.rs`, `src/lib.rs`, `src/main.rs`, and
  `src/server.rs`.
- `crates/codegotchi-cli/tests/process_wrapper.rs`.
- `crates/codegotchi-cli/tests/static_assets.rs`.
- `crates/codegotchi-cli/tests/fixtures/fake-codex.sh`.
- This report.

`crates/codegotchi-cli/web-dist/**` was rebuilt and compared with the frontend
build; the committed bytes were already identical, so it has no final Git
diff. No React source, domain code, persistence schema, CI, architecture, or
Task 6 acceptance file was changed.

## Remaining limitation

`corepack pnpm playwright:test` was attempted. The environment could not start
Playwright's Chromium because the system library `libnspr4.so` is unavailable
(`1 failed, 6 did not run`). No repository change can provide that host-level
library. The real/paid Codex trust and interactive browser check remains a
manual gate, as required; Strict mode remains a fail-open pet-care feature and
is not a security boundary.
