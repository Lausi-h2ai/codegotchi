# Live provider verification

The screenshots described below are local generated artifacts excluded from Git.
The sanitized JSON summaries and this verification record remain tracked.

This directory contains opt-in evidence from genuine provider requests through
the production `codegotchi run` launcher. The verifier is
[`scripts/verify-harness-provider.py`](../../../../scripts/verify-harness-provider.py).
It uses a private Xvfb/xterm display and tmpfs homes. Pi's OAuth file is copied
only when the caller supplies `--pi-auth-source`; Claude accepts a key only from
the explicitly named environment variable. Credential bytes and session
transcripts are removed with the private temporary home and are never written
to this directory.

The verified Pi run used Pi **0.85.1**, the `openai-codex/gpt-5.5` model, and
the final `target/release/codegotchi` executable with SHA-256
`9220dcd6c4c2d56f803bac0d8c8b5ac1b027c700dc03965a16b00d5509123fcc`.
Session JSONL records, rather than echoed
terminal text, established the assistant markers and the exact native tool
result. The run also observed current backend `thinking` activity before a
real Escape interruption, Pi's `stopReason=aborted` record, the follow-up
answer, three additional turns, and a second process launched with Pi's
`--continue` flag. The resumed answer recalled a private token from the first
session, and both processes exited with status 0. Pi has no separate native
approval prompt in this mode, so that dimension is recorded as
`not_applicable`; the tool call and result were still verified from Pi's native
session record.

The screenshots `pi-startup.png`, `pi-turn-1.png`, `pi-tool-result.png`,
`pi-interrupted.png`, `pi-after-interruption.png`, `pi-long-session.png`, and
`pi-resumed.png` were opened and visually inspected. They show the genuine Pi
conversation, tool result, aborted turn, follow-up, longer retained history,
CodeGotchi room, and resumed answer in the dark terminal theme.

Claude Code was not run against a provider: `claude auth status --json` in the
available Linux configuration reported `loggedIn=false` and no Anthropic API
key was present in the environment. The retained [`claude.json`](claude.json)
records this blocker without attempting login or changing user credentials.
To run it after supplying a key, use an isolated invocation such as (a custom
source variable is translated to Claude's `ANTHROPIC_API_KEY`):

```sh
ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
python3 scripts/verify-harness-provider.py --harness claude \
  --claude-program /path/to/claude
```

After Claude Code login, pass the directory containing its
`.credentials.json` (or the file itself) with
`--claude-auth-source /path/to/.claude`. The verifier copies only that file
into the disposable `CLAUDE_CONFIG_DIR`; it never edits the source.

The verifier's assertions intentionally fail or mark the extended portion
incomplete when session evidence is absent; a marker merely echoed in xterm
cannot pass the provider checks.
