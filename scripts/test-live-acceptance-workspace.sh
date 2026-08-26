#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
WORKSPACE_HELPER="$SCRIPT_DIR/live-acceptance-workspace.sh"

TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/codegotchi-live-workspace-test.XXXXXX")
cleanup() {
    rm -rf -- "$TEST_ROOT"
}
trap cleanup EXIT

source "$WORKSPACE_HELPER"

AUTHORIZED_CODEX_HOME="$TEST_ROOT/authorized-codex-home"
mkdir -p -- "$AUTHORIZED_CODEX_HOME"
printf '%s\n' 'fake-auth-for-path-testing-only' > "$AUTHORIZED_CODEX_HOME/auth.json"
chmod 600 "$AUTHORIZED_CODEX_HOME/auth.json"

CODEX_ARGUMENTS=(--disable apps --ask-for-approval on-request --sandbox read-only)
prepare_live_acceptance_workspace "$TEST_ROOT/run" "$AUTHORIZED_CODEX_HOME"

[[ -f "$ACCEPTANCE_WORKSPACE/test.md" ]]
[[ $(sed -n '1p' "$ACCEPTANCE_WORKSPACE/test.md") == 'My grandma is a wonderful woman.' ]]
[[ $ACCEPTANCE_PROMPT == 'Read this file called test.md and tell me what it says.' ]]

expected_arguments=(
    --disable apps
    --ask-for-approval on-request
    --sandbox read-only
    --model gpt-5.6-luna
    --config 'model_reasoning_effort="low"'
    --dangerously-bypass-hook-trust
    --cd "$ACCEPTANCE_WORKSPACE"
)
[[ ${#CODEX_ARGUMENTS[@]} == ${#expected_arguments[@]} ]]
for index in "${!expected_arguments[@]}"; do
    [[ ${CODEX_ARGUMENTS[$index]} == "${expected_arguments[$index]}" ]]
done

[[ -L "$ACCEPTANCE_CODEX_HOME/auth.json" ]]
[[ $(readlink "$ACCEPTANCE_CODEX_HOME/auth.json") == "$AUTHORIZED_CODEX_HOME/auth.json" ]]
expected_trust_config=$(printf '[projects."%s"]\ntrust_level = "trusted"' "$ACCEPTANCE_WORKSPACE")
[[ $(sed -n '1,2p' "$ACCEPTANCE_CODEX_HOME/config.toml") == "$expected_trust_config" ]]
[[ $(stat -c '%a' "$ACCEPTANCE_CODEX_HOME/config.toml") == 600 ]]

BUFFER=''
CURSOR=0
SUBMITTED=0
KEY_LOG=()
send_text() {
    local value=$1
    BUFFER="${BUFFER:0:CURSOR}${value}${BUFFER:CURSOR}"
    CURSOR=$((CURSOR + ${#value}))
}
send_key() {
    KEY_LOG+=("$1")
    case "$1" in
        Left) ((CURSOR > 0)) && CURSOR=$((CURSOR - 1)) ;;
        End) CURSOR=${#BUFFER} ;;
        BackSpace)
            if ((CURSOR > 0)); then
                BUFFER="${BUFFER:0:CURSOR-1}${BUFFER:CURSOR}"
                CURSOR=$((CURSOR - 1))
            fi
            ;;
        Return) SUBMITTED=1 ;;
        *) printf 'unexpected key: %s\n' "$1" >&2; exit 1 ;;
    esac
}

drive_live_acceptance_prompt

[[ $BUFFER == "$ACCEPTANCE_PROMPT" ]]
[[ $SUBMITTED == 1 ]]
KEY_LOG=()
BUFFER=''
CURSOR=0
drive_live_acceptance_approval
[[ ${#KEY_LOG[@]} == 1 ]]
[[ ${KEY_LOG[0]} == Return ]]
[[ -z $BUFFER ]]
live_acceptance_uses_hook_trust_bypass
live_acceptance_turn_completed WaitingForUser 6 0
if live_acceptance_turn_completed Active 6 0; then
    printf '%s\n' 'active Codex work was incorrectly treated as a completed turn' >&2
    exit 1
fi
if live_acceptance_turn_completed WaitingForUser 0 0; then
    printf '%s\n' 'a waiting state without post-prompt work was incorrectly accepted' >&2
    exit 1
fi
live_acceptance_care_advanced 3 5
if live_acceptance_care_advanced 5 5; then
    printf '%s\n' 'an unchanged processed-care count was incorrectly accepted' >&2
    exit 1
fi
expected_paste=$'This is a bracketed multiline paste check.\nThe second line must stay in the same Codex composer submission.'
[[ $(live_acceptance_paste_text) == "$expected_paste" ]]
live_acceptance_capture_visible 0.953536
if live_acceptance_capture_visible 0; then
    printf '%s\n' 'a fully black capture was incorrectly accepted as visible evidence' >&2
    exit 1
fi
[[ $(live_acceptance_restoration_token) == 'terminal-restoration-ok' ]]
[[ $(live_acceptance_exit_command) == '/quit' ]]
[[ $(live_acceptance_approval_prompt) == 'Request my approval to run this exact command outside the read-only sandbox, then run it only if approved: touch approval-probe.txt' ]]

PROTOCOL_RECEIPT="$TEST_ROOT/protocol.tsv"
printf '%s\n' \
    'event=paste bracketed=true delivered=true' \
    'event=focus-lost reporting=true delivered=true' \
    'event=focus-gained reporting=true delivered=true' \
    'event=mouse tracking=Disabled encoding=Default delivered=false' \
    'resize physical=120x45 codex-pty=120x31 room=Full' \
    'resize physical=120x30 codex-pty=120x23 room=Compact' \
    'resize physical=120x21 codex-pty=120x18 room=Minimal' >"$PROTOCOL_RECEIPT"
live_acceptance_protocol_input_verified "$PROTOCOL_RECEIPT"
live_acceptance_protocol_resizes_verified "$PROTOCOL_RECEIPT" 120 45 30 21
if live_acceptance_protocol_input_verified "$TEST_ROOT/missing"; then
    printf '%s\n' 'a missing protocol receipt was incorrectly accepted' >&2
    exit 1
fi

printf '%s\n' 'live acceptance workspace regression: PASS'
