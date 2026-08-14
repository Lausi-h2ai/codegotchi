#!/bin/sh
set -eu

spawn_ledger=${FAKE_LAUNCHER_SPAWN_LEDGER:?FAKE_LAUNCHER_SPAWN_LEDGER must be set}
outer_tty=${CODEGOTCHI_OUTER_TTY:?CODEGOTCHI_OUTER_TTY must be exported by the outer PTY harness}

test -t 0
test -t 1
inner_tty=$(tty)
size=$(stty size)

printf 'pid=%s|size=%s|tty=%s|outer_tty=%s\n' \
    "$$" "$size" "$inner_tty" "$outer_tty" >>"$spawn_ledger"
printf 'FAKE_LAUNCHER_PTY_READY\r\n'
