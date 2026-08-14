#!/bin/sh
set -eu

mode=${1:?signal mode required}
printf 'FAKE_SIGNAL_READY\r\n'

on_interrupt() {
    printf 'FAKE_SIGNAL_INT\r\n'
    if [ "$mode" != '--ignore-interrupt' ]; then
        exit 130
    fi
}

on_terminate() {
    printf 'FAKE_SIGNAL_TERM\r\n'
    exit 143
}

trap on_interrupt INT
trap on_terminate TERM
while :; do
    :
done
