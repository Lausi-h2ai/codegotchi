# Task 5 — exact launcher, installed embedded UI, and lifecycle cleanup

Model: `gpt-5.6-luna`; reasoning: `max`.

Base commit: `885977f` plus accepted Task 4 correction/re-review through current
HEAD. Implement only the Task 5 vertical increment. Use TDD. Do not spawn or
delegate to any agent; you are the sole primary implementer.

The required user command is exactly:

```text
codegotchi run -- codex [ordinary Codex arguments...]
```

It must start the persistent authoritative runtime, serve the already-built
Task 3 UI from the installed Rust binary, open or clearly surface the UI, and
run the real Codex process transparently with the Task 1 hook profile.

## Exact relevant files

Create:

- `crates/codegotchi-cli/src/launcher.rs`
- `crates/codegotchi-cli/src/assets.rs`
- `crates/codegotchi-cli/tests/process_wrapper.rs`
- `crates/codegotchi-cli/tests/static_assets.rs`
- `crates/codegotchi-cli/tests/fixtures/fake-codex.sh`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-5-report.md`

Modify only as needed:

- `Cargo.toml` and `crates/codegotchi-cli/Cargo.toml`
- `crates/codegotchi-cli/src/{lib.rs,cli.rs,main.rs,server.rs,runtime_metadata.rs,codex_profile.rs}`
- `crates/codegotchi-cli/web-dist/**` only by rebuilding and running the existing embed script
- `.gitignore`
- `README.md`

A small `crates/codegotchi-cli/build.rs` is allowed if it is the narrowest
way to generate a compile-time embedded asset table from `web-dist`; document
it. Do not modify React source, domain source, Task 4 policy behavior, the
persistence schema, CI, architecture docs, or Task 6 acceptance files.

## Consumed interfaces

- `TemporaryCodexProfile` creates one additive `$CODEX_HOME/<unique>.config.toml`,
  injects `--profile <unique>`, and preserves the base config/auth state.
- `RuntimeMetadataV1` plus `CODEGOTCHI_SESSION_FILE` is the hook discovery
  contract. Metadata is create-new mode 0600 and contains no prompts.
- `AuthoritativeRuntime`, `SqliteStore::open_for_repository`, and
  `RunningServer` are the authoritative persistence/server services.
- The Task 3 bundle authenticates from `/#token=<token>`, removes the fragment,
  uses same-origin HTTP, and passes the token as the WebSocket subprotocol.
- Task 4 mode/debug commands work inside the Codex child environment and must
  remain unchanged.

## Required launcher behavior

### Parse and validate before mutation

- Accept only `run -- codex [arguments...]`; preserve every trailing argument
  byte-for-byte and in order after the injected profile arguments.
- Reject missing `--`, unsupported agents, and explicit `-p`, `--profile`,
  `--profile=...`, or attached short-profile forms with an actionable conflict
  before creating runtime/profile files.
- Locate Codex through `CODEGOTCHI_REAL_CODEX` when set (test seam), otherwise
  search `PATH`. Reject missing/non-executable candidates and reject a canonical
  path or symlink that resolves to the running CodeGotchi executable.

### Repository, persistence, and runtime discovery

- Identify the current Git worktree root with read-only Git discovery and fall
  back to the canonical current directory when it is not a Git repository.
- Store one SQLite database below `$XDG_STATE_HOME/codegotchi/`, falling back
  to `$HOME/.local/state/codegotchi/`. Use a stable repository identifier so
  the same repository reloads the same pet while different repositories remain
  distinct. Seed a deterministic pet identity/name only when no snapshot exists.
- Create a mode-0700 CodeGotchi runtime directory below `$XDG_RUNTIME_DIR`,
  falling back to the user state directory. Write a unique
  `session-<uuid>.json` using the existing 0600 create-new metadata function.
- Generate a high-entropy, RFC-6455-subprotocol-safe token (at least two UUIDv4
  values encoded without unsafe characters). Bind remains loopback-only.
- Detect/remove only stale CodeGotchi-owned `session-*.json` files in that
  protected directory; never remove an active session or unrelated file.
- On normal/error/signal paths remove only the owned metadata and profile;
  preserve the SQLite state. Abnormal leftovers must be safe and discoverable
  as stale on the next start.

### Embedded UI and browser

- Embed every committed file under `crates/codegotchi-cli/web-dist/` into the
  executable at compile time. The installed binary must not read the repo or
  require Vite/pnpm at runtime.
- Serve `/`, hashed JS/CSS assets with correct MIME types, and SPA fallback.
  Unknown `/api/v1/*` routes must retain typed API errors rather than serving
  HTML. Keep state-changing API authentication unchanged.
- Start the server before Codex. Construct the UI URL with the token only in
  the fragment. Always print one concise local UI line; attempt browser launch
  using an explicit test override, native Linux helpers, and WSL `cmd.exe`
  where available. Browser-launch failure is a clear warning plus surfaced URL,
  not a reason to kill an otherwise usable Codex session. `CODEGOTCHI_BROWSER=none`
  must suppress browser spawning in automated tests.

### Transparent Codex process

- Resolve the current absolute CodeGotchi executable and use it in the hook
  command so an installed launcher works outside the repository.
- Create a unique additive profile in the user's existing `CODEX_HOME` (or
  `$HOME/.codex`) without copying/moving credentials or changing base config.
- Launch Codex directly with inherited stdin/stdout/stderr and no PTY unless a
  demonstrated real terminal defect requires one. Preserve color/interactive
  behavior and current working directory.
- Forward SIGINT, SIGTERM, and SIGWINCH while waiting; direct inherited terminal
  ownership should remain transparent. Return Codex's numeric exit status,
  including conventional signal status on Unix.
- Shut down the server, clean owned temporary files, and leave persisted pet
  state after Codex exits or child spawn/wait fails.

## Mandatory TDD and acceptance tests

### `process_wrapper.rs`

Use the installed test binary plus the fake Codex executable; routine tests
must not invoke real/paid Codex. Cover:

1. exact trailing argument order after the generated profile, stdin read,
   visible stdout/stderr, ANSI bytes, current directory, metadata/profile env,
   and exit-code preservation;
2. missing Codex, recursive/symlink resolution, malformed command shape, and
   every profile-conflict form before runtime mutation;
3. child spawn failure cleanup and browser-launch failure remaining nonfatal;
4. SIGINT and SIGTERM forwarding plus runtime/profile cleanup; SIGWINCH should
   be forwarded or explicitly proven transparent with inherited foreground TTY;
5. checksums/bytes for existing `$CODEX_HOME/config.toml` and auth/credential
   files remain identical; only the unique generated profile is removed;
6. runtime metadata is 0600, its parent is 0700, token is high entropy and
   syntax-safe, stale owned metadata is cleaned, unrelated/active files remain;
7. SQLite state remains and the second launcher run for the same repository
   reloads the same pet identity and cared/simulation state.

Tests may add narrowly scoped public test seams, but no production fixture
endpoint or arbitrary mutation endpoint.

### `static_assets.rs`

- Start the production server and fetch `/`, every referenced hashed asset,
  and a client-side SPA path; assert byte identity/MIME and no Vite development
  dependency.
- Assert unknown API paths still return typed JSON and state-changing routes
  remain authenticated.
- Install with
  `cargo install --path crates/codegotchi-cli --root <temporary-root>` and run
  the installed binary from a directory outside the repository with the fake
  Codex override/browser disabled. Prove the UI is served without `web-dist`,
  pnpm, or a second process. Keep this test practical; an explicit ignored
  installation test is not acceptable for the core packaging gate.

### Verification and documentation

- Rebuild frontend and refresh `web-dist` with the existing script; confirm the
  embedded files exactly match the build and contain no Vite dev URL.
- Update README from the obsolete Phase 1/2 status to the exact install,
  `codegotchi run -- codex`, first-run `/hooks` trust, printed/opened UI,
  persistence/runtime locations, Strict mode, guarded demo commands, cleanup,
  and all current gates. Be honest that Strict is not a security boundary.
- Run Rust fmt, strict workspace clippy, workspace tests; frontend lint/test/
  format/build; Playwright if the server/static integration changes browser
  delivery. Record exact results.

## Explicit exclusions

- No PTY abstraction without a demonstrated failure.
- No generic coding-agent SDK, Claude/Cursor support, daemon, MCP, remote
  access, cloud sync, telemetry, command interception, native packaging, or
  unrelated refactor.
- Do not bypass Codex hook trust, modify auth credentials, copy user config, or
  claim the profile is the only matching hook.
- Do not implement Task 6's full restart/strict browser harness or final
  verification report in this task.

## Report

Write `task-5-report.md` with RED/GREEN evidence, exact changed files, install
proof, process/asset test results, cleanup/config checks, and limitations. Do
not commit; the supervisor will review and commit the bounded result.
