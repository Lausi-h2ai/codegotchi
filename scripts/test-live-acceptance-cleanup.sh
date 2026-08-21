#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
HARNESS="$SCRIPT_DIR/verify-terminal-room-live.sh"
source "$SCRIPT_DIR/live-acceptance-cleanup.sh"

TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/codegotchi-live-cleanup-test.XXXXXX")
cleanup() {
    rm -rf -- "$TEST_ROOT"
}
trap cleanup EXIT

private_authority="$TEST_ROOT/private-Xauthority"
printf '%s\n' 'private-cookie' >"$private_authority"
chmod 600 "$private_authority"
scrub_private_display_credentials "$private_authority" 1
[[ ! -e $private_authority ]]

inherited_authority="$TEST_ROOT/inherited-Xauthority"
printf '%s\n' 'inherited-cookie' >"$inherited_authority"
chmod 600 "$inherited_authority"
scrub_private_display_credentials "$inherited_authority" 0
[[ -s $inherited_authority ]]

retained_line=$(retained_run_root_report_line "$TEST_ROOT/retained-run")
[[ $retained_line == *"$TEST_ROOT/retained-run"* ]]
[[ $retained_line != *cookie* ]]

hostile_status='$(touch "$TEST_ROOT/command-substitution-fired") `echo hostile`'
status_line=$(private_display_credential_report_line "$hostile_status")
[[ $status_line == *'$(touch'* && $status_line == *'`echo hostile`'* ]]
[[ ! -e "$TEST_ROOT/command-substitution-fired" ]]

grep -Fq 'source "$SCRIPT_DIR/live-acceptance-cleanup.sh"' "$HARNESS"
grep -Fq 'DISPLAY_AUTHORITY_OWNED=1' "$HARNESS"
grep -Fq 'retained_run_root_report_line "$RUN_ROOT"' "$HARNESS"
grep -Fq 'private_display_credential_report_line "$DISPLAY_AUTHORITY_CLEANUP_STATUS"' "$HARNESS"
scrub_line=$(grep -n 'scrub_private_display_credentials "$DISPLAY_AUTHORITY"' "$HARNESS" | cut -d: -f1 | head -n 1)
report_call_line=$(grep -n '^    write_report ' "$HARNESS" | cut -d: -f1 | head -n 1)
[[ $scrub_line =~ ^[0-9]+$ && $report_call_line =~ ^[0-9]+$ ]]
((scrub_line < report_call_line))

printf '%s\n' 'live acceptance cleanup regression: PASS'
