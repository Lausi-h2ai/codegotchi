#!/bin/sh
set -eu

mode=${1:?signal mode required}
on_interrupt() {
    printf 'FAKE_SIGNAL_INT\r\n'
    printf 'FAKE_SIGNAL_STATE=alive pgid=%s\n' "$(ps -o pgid= -p $$ | tr -d ' ')" >&2
    if [ "$mode" != '--ignore-interrupt' ]; then
        exit 130
    fi
}

on_terminate() {
    printf 'FAKE_SIGNAL_TERM\r\n'
    printf 'FAKE_SIGNAL_STATE=terminating pgid=%s\n' "$(ps -o pgid= -p $$ | tr -d ' ')" >&2
    exit 143
}

trap on_interrupt INT
trap on_terminate TERM
printf 'FAKE_SIGNAL_READY\r\n'
printf 'FAKE_SIGNAL_PID=%s\r\n' "$$"
while :; do
    :
done

printf 'FAKE_SIGNAL_UNEXPECTED_EXIT=1\n' >&2
