#!/bin/sh
set -eu

log_file=${FAKE_COMPOSED_LOG:?FAKE_COMPOSED_LOG must be set}
{
    printf 'argc=%s\n' "$#"
    index=1
    for argument in "$@"; do
        printf 'arg[%s]=%s\n' "$index" "$argument"
        index=$((index + 1))
    done
    printf 'env=%s\n' "${FAKE_COMPOSED_ENV-}"
    printf 'size=%s\n' "$(stty size)"
} >"$log_file"

stty -icanon -echo min 0 time 50
printf '\033[?1h\033[?2004h\033[?1004h\033[?1000h\033[?1006hFAKE_COMPOSED_READY\r\n'

# Up/application-cursor, bracketed paste, focus, and SGR mouse bytes are
# supplied by the production session loop. Capture their exact wire form.
dd bs=1 count=30 2>/dev/null | od -An -tx1 -v -w30 | tr -d ' \\n' >>"$log_file"
printf '\n' >>"$log_file"
sleep 1
printf 'resized-size=%s\n' "$(stty size)" >>"$log_file"
exit 0
