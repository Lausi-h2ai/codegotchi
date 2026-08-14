#!/bin/sh
set -eu

printf 'FAKE_CODEX_READY\033[31mANSI_RED\033[0m\r\n'
printf 'FAKE_CODEX_ARG_COUNT=%s\r\n' "$#"
argument_index=1
for argument in "$@"; do
    printf 'FAKE_CODEX_ARG[%s]=<%s>\r\n' "$argument_index" "$argument"
    argument_index=$((argument_index + 1))
done
printf 'FAKE_CODEX_CODEX_HOME=<%s>\r\n' "${CODEX_HOME-}"
printf 'FAKE_CODEX_SESSION_FILE=<%s>\r\n' "${CODEGOTCHI_SESSION_FILE-}"

IFS= read -r input
printf 'FAKE_CODEX_INPUT=<%s>\r\n' "$input"
printf 'FAKE_CODEX_SIZE=<%s>\r\n' "$(stty size)"
exit 23
