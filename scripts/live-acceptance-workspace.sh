#!/usr/bin/env bash

prepare_live_acceptance_workspace() {
    local run_root=$1
    local authorized_codex_home=$2
    local authorized_auth="$authorized_codex_home/auth.json"

    ACCEPTANCE_WORKSPACE="$run_root/workspace"
    ACCEPTANCE_CODEX_HOME="$run_root/codex-home"
    ACCEPTANCE_PROMPT='Read this file called test.md and tell me what it says.'
    if [[ $ACCEPTANCE_WORKSPACE == *'"'* || $ACCEPTANCE_WORKSPACE == *$'\n'* ]]; then
        printf '%s\n' 'live acceptance workspace path cannot be represented safely in a Codex config override' >&2
        return 1
    fi
    [[ -f $authorized_auth ]] || {
        printf '%s\n' 'authorized Codex auth.json is required for the isolated live acceptance home' >&2
        return 1
    }
    mkdir -p -- "$ACCEPTANCE_WORKSPACE" "$ACCEPTANCE_CODEX_HOME"
    chmod 700 "$ACCEPTANCE_WORKSPACE" "$ACCEPTANCE_CODEX_HOME"
    printf '%s\n' 'My grandma is a wonderful woman.' > "$ACCEPTANCE_WORKSPACE/test.md"
    chmod 600 "$ACCEPTANCE_WORKSPACE/test.md"
    ln -s -- "$authorized_auth" "$ACCEPTANCE_CODEX_HOME/auth.json"
    printf '[projects."%s"]\ntrust_level = "trusted"\n' "$ACCEPTANCE_WORKSPACE" > "$ACCEPTANCE_CODEX_HOME/config.toml"
    chmod 600 "$ACCEPTANCE_CODEX_HOME/config.toml"

    CODEX_ARGUMENTS+=(
        --model gpt-5.6-luna
        --config 'model_reasoning_effort="low"'
        --dangerously-bypass-hook-trust
        --cd "$ACCEPTANCE_WORKSPACE"
    )
}

drive_live_acceptance_prompt() {
    local before_submit=${1:-:}

    send_text 'Read this file called test.mX'
    send_key Left
    send_text 'd'
    send_key End
    send_key BackSpace
    send_text ' and tell me what it says.'
    "$before_submit"
    send_key Return
}

drive_live_acceptance_approval() {
    send_key Return
}

live_acceptance_uses_hook_trust_bypass() {
    local argument
    for argument in "${CODEX_ARGUMENTS[@]}"; do
        [[ $argument == --dangerously-bypass-hook-trust ]] && return 0
    done
    return 1
}

live_acceptance_turn_completed() {
    local activity=$1
    local work_points=$2
    local baseline_work_points=$3

    [[ $activity == WaitingForUser ]] &&
        [[ $work_points =~ ^[0-9]+$ ]] &&
        [[ $baseline_work_points =~ ^[0-9]+$ ]] &&
        ((work_points > baseline_work_points))
}

live_acceptance_care_advanced() {
    local before=$1
    local after=$2

    [[ $before =~ ^[0-9]+$ ]] &&
        [[ $after =~ ^[0-9]+$ ]] &&
        ((after > before))
}

live_acceptance_paste_text() {
    printf '%s\n%s\n' \
        'This is a bracketed multiline paste check.' \
        'The second line must stay in the same Codex composer submission.'
}

live_acceptance_capture_visible() {
    local mean=$1
    awk -v mean="$mean" 'BEGIN { exit !(mean > 0.01) }'
}

live_acceptance_restoration_token() {
    printf '%s\n' 'terminal-restoration-ok'
}

live_acceptance_exit_command() {
    printf '%s\n' '/quit'
}

live_acceptance_approval_prompt() {
    printf '%s\n' 'Request my approval to run this exact command outside the read-only sandbox, then run it only if approved: touch approval-probe.txt'
}

live_acceptance_protocol_input_verified() {
    local receipt=$1

    [[ -f $receipt ]] &&
        grep -Fxq 'event=paste bracketed=true delivered=true' "$receipt" &&
        grep -Fxq 'event=focus-lost reporting=true delivered=true' "$receipt" &&
        grep -Fxq 'event=focus-gained reporting=true delivered=true' "$receipt" &&
        grep -Eq '^event=mouse tracking=(Disabled|Press|PressRelease|ButtonMotion|AnyMotion) encoding=(Default|Utf8|Sgr) delivered=(true|false)$' "$receipt"
}

live_acceptance_protocol_resizes_verified() {
    local receipt=$1
    local columns=$2
    shift 2
    local rows

    [[ -f $receipt ]] || return 1
    for rows in "$@"; do
        grep -Eq "^resize physical=${columns}x${rows} codex-pty=${columns}x[0-9]+ room=(Full|Compact|Minimal)$" "$receipt" || return 1
    done
}
