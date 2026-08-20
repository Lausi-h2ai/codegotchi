# Task 6 theme documentation reconciliation

Updated `README.md` to document the implemented `--terminal-theme` launcher
option, its exact five values, placement before `--`, default `auto`, and
working command examples. The note also records that themes are
presentation-only and do not change pet state.

Evidence: `parse_launch_request` and its launcher tests cover all five values,
both separated and equals forms, defaulting, and invalid/duplicate options.
No product code or PR metadata was changed.

## Verification

- `cargo test -p codegotchi-cli launcher::tests --lib` — PASS (20 tests).
- `corepack pnpm --dir web exec prettier --check ../README.md` — PASS.
- `corepack pnpm format:check` — PASS.
- `git diff --check` — PASS.
- `cargo fmt --all -- --check` remains blocked by unrelated pre-existing drift
  in `crates/codegotchi-cli/src/terminal/room.rs` and its tests; those files
  were left untouched.
