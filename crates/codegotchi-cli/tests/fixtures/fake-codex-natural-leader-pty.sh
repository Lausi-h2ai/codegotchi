#!/bin/sh
set -eu

pid_file=${1:?descendant PID file required}
sleep 60 &
printf '%s\n' "$!" >"$pid_file"
printf 'FAKE_NATURAL_LEADER_READY\r\n'
exit 0
