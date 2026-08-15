# AGENTS.md

Guidance for Codex agents working in this repository.

## Process cleanup safety (critical)

This project's smoke tests spawn real Codex processes inside `xterm` and
codegotchi wrappers, so `pgrep -af codex` legitimately returns many processes.
Two sessions have already been lost when a cleanup step mistook the running
session's own process tree for a leftover smoke process and killed it.

When cleaning up stray processes, follow these rules:

1. **Identify your own tree first.** Before any `kill`, compute the current
   session's own ancestor chain (walk `/proc/<pid>/stat` PPID from the shell's
   own PID upward) and its descendants. Treat every PID on that chain as
   untouchable, including the `node .../codex` wrapper and the vendored
   `codex` binary.
2. **Never assume `codex` or `codex resume` entries are leftovers.** They are
   often the active session or a session the user intentionally resumed. A
   cmdline containing `codex resume` is a strong signal it may be the active
   session.
3. **Scope matches to concrete artifacts.** Match on markers that only smoke
   runs produce (xterm window titles, codegotchi child PIDs, temp directories
   like `/tmp/codegotchi-*`) instead of a bare `pgrep codex`.
4. **Verify before killing.** Show `ps -o pid,ppid,stat,etime,cmd` for every
   candidate and confirm the parent and elapsed time indicate a leftover, not
   the active session. When in doubt, leave it alone and report instead.
5. **Prefer SIGTERM over SIGKILL.** Use `kill` (TERM) first; use `kill -9`
   only when a verified leftover ignores TERM.
6. **Confirm the session survived.** After cleanup, run a quick command and
   check that the tool returns normally. If the exec is aborted or no result
   comes back, the session may have killed itself — stop cleanup work and
   reassess the harness state before continuing.
