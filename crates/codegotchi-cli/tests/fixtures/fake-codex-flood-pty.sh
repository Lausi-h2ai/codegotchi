#!/bin/sh
set -eu

mode=${1:---interrupt}
trap 'if [ "$mode" = "--ignore-interrupt" ]; then :; else exit 130; fi' INT
trap 'exit 143' TERM
printf 'FAKE_FLOOD_READY\r\n'
while :; do
    printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\r\n'
    if [ "$mode" != "--ignore-interrupt" ]; then
        sleep 0.001
    fi
done
