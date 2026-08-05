#!/bin/sh
set -eu

log_file=${FAKE_CODEX_LOG:?FAKE_CODEX_LOG must be set}

{
    printf 'PID\t%s\n' "$$"
    printf 'CWD\t%s\n' "$(pwd)"
    printf 'CODEX_HOME\t%s\n' "${CODEX_HOME-}"
    printf 'SESSION_FILE\t%s\n' "${CODEGOTCHI_SESSION_FILE-}"
    for argument in "$@"; do
        printf 'ARG\t%s\n' "$argument"
    done
} >>"$log_file"

if [ -n "${FAKE_PROFILE_COPY-}" ]; then
    profile_name=${2-}
    cp "$CODEX_HOME/$profile_name.config.toml" "$FAKE_PROFILE_COPY"
fi

if [ -n "${FAKE_METADATA_COPY-}" ]; then
    cp "$CODEGOTCHI_SESSION_FILE" "$FAKE_METADATA_COPY"
fi

cat >"${FAKE_STDIN_FILE:?FAKE_STDIN_FILE must be set}"
printf '\033[32mfake codex stdout\033[0m\n'
printf '\033[31mfake codex stderr\033[0m\n' >&2

if [ "${FAKE_DEBUG_NEGLECT-0}" = 1 ]; then
    "$CODEGOTCHI_BIN" debug neglect >>"$log_file" 2>&1
fi

if [ -n "${FAKE_SIGNAL_FILE-}" ]; then
    signal_log=${FAKE_SIGNAL_LOG:?FAKE_SIGNAL_LOG must be set}
    trap 'printf "SIGINT\n" >>"$signal_log"; exit 130' INT
    trap 'printf "SIGTERM\n" >>"$signal_log"; exit 143' TERM
    trap 'printf "SIGWINCH\n" >>"$signal_log"' WINCH
    touch "$FAKE_SIGNAL_FILE"
    while :; do
        sleep 1
    done
fi

if [ -n "${FAKE_READY_FILE-}" ]; then
    touch "$FAKE_READY_FILE"
    while [ -e "${FAKE_RELEASE_FILE:?FAKE_RELEASE_FILE must be set}" ]; do
        sleep 0.05
    done
fi

exit "${FAKE_EXIT-0}"
