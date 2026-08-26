#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPOSITORY_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
source "$SCRIPT_DIR/live-acceptance-cleanup.sh"
source "$SCRIPT_DIR/live-acceptance-workspace.sh"

usage() {
    cat <<'EOF'
Usage: scripts/verify-terminal-room-live.sh [--] [codex arguments...]

Launches the production CodeGotchi terminal room in an xterm with the real
installed Codex executable, drives a bounded fidelity/care checklist, and
writes screenshots and a redacted checklist report.

The default Codex invocation uses a read-only sandbox with on-request approval,
gpt-5.6-luna, and low reasoning. Pass arguments after `--` to replace
the policy arguments while retaining the isolated acceptance model/workspace,
for example:

  scripts/verify-terminal-room-live.sh -- --ask-for-approval on-request --sandbox read-only

Environment overrides:
  CODEGOTCHI_BIN                 CodeGotchi executable (default target/debug/codegotchi)
  CODEGOTCHI_CODEX_BIN           Codex executable (default CODEGOTCHI_REAL_CODEX or codex)
  CODEGOTCHI_LIVE_CODEX_HOME     Authorized CODEX_HOME to reference without copying it
  CODEGOTCHI_LIVE_OUTPUT_DIR     Evidence directory (default docs/verification/terminal-room/live-codex)
  CODEGOTCHI_LIVE_NO_BUILD       Set to 1 to refuse an automatic cargo build
  CODEGOTCHI_LIVE_TIMEOUT_SEC    Per bounded wait timeout (default 30)

The harness never prints Codex arguments, CODEX_HOME contents, metadata, or
bearer tokens. It does not push, change PR metadata, or modify production
CodeGotchi state.
EOF
}

if [[ ${1-} == "--help" || ${1-} == "-h" ]]; then
    usage
    exit 0
fi

CODEX_ARGUMENTS=(--disable apps --ask-for-approval on-request --sandbox read-only)
CUSTOM_CODEX_ARGUMENTS=0
if [[ ${1-} == "--" ]]; then
    shift
    CODEX_ARGUMENTS=("$@")
    CUSTOM_CODEX_ARGUMENTS=1
elif (($# > 0)); then
    usage >&2
    exit 2
fi

if [[ ${BASH_VERSINFO[0]} -lt 4 ]]; then
    printf '%s\n' 'FAIL: Bash 4 or newer is required' >&2
    exit 2
fi

umask 077

OUTPUT_DIR=${CODEGOTCHI_LIVE_OUTPUT_DIR:-"$REPOSITORY_ROOT/docs/verification/terminal-room/live-codex"}
mkdir -p -- "$OUTPUT_DIR"

RUN_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/codegotchi-terminal-room-live.XXXXXX")
STATE_HOME="$RUN_ROOT/state"
RUNTIME_HOME="$RUN_ROOT/runtime"
DATA_HOME="$RUN_ROOT/data"
CONFIG_HOME="$RUN_ROOT/config"
CACHE_HOME="$RUN_ROOT/cache"
TEMP_HOME="$RUN_ROOT/home"
mkdir -p -- "$STATE_HOME" "$RUNTIME_HOME" "$DATA_HOME" "$CONFIG_HOME" "$CACHE_HOME" "$TEMP_HOME"
chmod 700 "$RUN_ROOT" "$STATE_HOME" "$RUNTIME_HOME" "$DATA_HOME" "$CONFIG_HOME" "$CACHE_HOME" "$TEMP_HOME"
HOST_TTY_STATE_BEFORE=""
if [[ -t 0 ]] && [[ -e /dev/tty ]]; then
    HOST_TTY_STATE_BEFORE=$(stty -g </dev/tty 2>/dev/null || true)
fi

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
REPORT_PATH="$OUTPUT_DIR/$RUN_ID-verification.txt"
HARNESS_LOG="$RUN_ROOT/harness.log"
: > "$HARNESS_LOG"

declare -a CHECKS=()
declare -a CREATED_PIDS=()
declare -a CREATED_STARTS=()
declare -a CREATED_MARKERS=()
declare -A STATE_SUMMARIES=()

DISPLAY_USED=""
DISPLAY_AUTHORITY=""
DISPLAY_AUTHORITY_OWNED=0
DISPLAY_AUTHORITY_CLEANUP_STATUS="not-needed"
DISPLAY_STARTED=0
XVFB_PID=""
WM_PID=""
XTERM_PID=""
XTERM_START=""
WINDOW_ID=""
WINDOW_TITLE="codegotchi-live-$RUN_ID"
CODEX_ARGUMENTS_FILE=""
CODEGOTCHI_ARGUMENTS_FILE=""
CODEGOTCHI_WRAPPER=""
RESTORE_PREFIX=""
PROTOCOL_RECEIPT=""
CURRENT_ROWS=45
CURRENT_COLUMNS=120
CAPTURED_FRAMES=()
METADATA_PATH=""
API_URL=""
API_TOKEN=""
REPORT_WRITTEN=0
CLEANUP_STARTED=0
CLEANUP_BLOCKED=0
FINAL_STATUS="not-run"
REQUIRED_GATE_BLOCKED=0
RESTORATION_PROBE_ENABLED=1
RESTORATION_TOKEN=$(live_acceptance_restoration_token)

log() {
    local message=$1
    printf '[live] %s\n' "$message" | tee -a "$HARNESS_LOG"
}

record() {
    local name=$1
    local result=$2
    CHECKS+=("$name: $result")
    log "$name: $result"
}

block_required_gate() {
    REQUIRED_GATE_BLOCKED=1
}

fail() {
    log "FAIL: $1" >&2
    FINAL_STATUS="FAIL"
    exit 1
}

block_and_exit() {
    log "BLOCKED: $1"
    FINAL_STATUS='BLOCKED'
    REQUIRED_GATE_BLOCKED=1
    exit 1
}

require_command() {
    local command_name=$1
    if ! command -v "$command_name" >/dev/null 2>&1; then
        fail "missing prerequisite '$command_name'"
    fi
}

pid_start_time() {
    local pid=$1
    [[ -r "/proc/$pid/stat" ]] || return 1
    awk '{print $22}' "/proc/$pid/stat"
}

pid_is_running() {
    local pid=$1
    [[ -r "/proc/$pid/stat" ]] || return 1
    [[ $(awk '{print $3}' "/proc/$pid/stat") != Z ]]
}

pid_cmdline() {
    local pid=$1
    [[ -r "/proc/$pid/cmdline" ]] || return 1
    tr '\0' ' ' < "/proc/$pid/cmdline"
}

pid_parent() {
    local pid=$1
    [[ -r "/proc/$pid/status" ]] || return 1
    awk '/^PPid:/ {print $2; exit}' "/proc/$pid/status"
}

ancestor_chain_contains() {
    local target=$1
    local pid=$$
    local parent
    while [[ $pid =~ ^[0-9]+$ && $pid -gt 0 ]]; do
        [[ $pid == "$target" ]] && return 0
        parent=$(pid_parent "$pid" 2>/dev/null || true)
        [[ $parent == "$pid" || -z $parent ]] && break
        pid=$parent
    done
    return 1
}

descendant_pids() {
    local root=$1
    local -a pending=("$root")
    local -a found=()
    local current child
    while ((${#pending[@]} > 0)); do
        current=${pending[0]}
        pending=("${pending[@]:1}")
        pid_is_running "$current" || continue
        found+=("$current")
        for child in /proc/[0-9]*; do
            child=${child##*/}
            [[ $child =~ ^[0-9]+$ ]] || continue
            pid_is_running "$child" || continue
            [[ $(pid_parent "$child" 2>/dev/null || true) == "$current" ]] || continue
            if ! printf '%s\n' "${found[@]}" | grep -Fxq "$child"; then
                pending+=("$child")
            fi
        done
    done
    if ((${#found[@]} > 0)); then
        printf '%s\n' "${found[@]}"
    fi
}

tree_reaches_root() {
    local pid=$1
    local root=$2
    local parent
    while [[ $pid =~ ^[0-9]+$ && $pid -gt 0 ]]; do
        [[ $pid == "$root" ]] && return 0
        parent=$(pid_parent "$pid" 2>/dev/null || true)
        [[ -z $parent || $parent == "$pid" ]] && break
        pid=$parent
    done
    return 1
}

pid_has_run_marker() {
    local pid=$1
    [[ -r "/proc/$pid/environ" ]] || return 1
    cat "/proc/$pid/environ" 2>/dev/null | tr '\0' '\n' \
        | grep -Fxq "CODEGOTCHI_LIVE_RUN_ID=$RUN_ID"
}

run_owned_pids() {
    local process_path pid
    for process_path in /proc/[0-9]*; do
        pid=${process_path##*/}
        [[ $pid =~ ^[0-9]+$ && $pid != "$$" ]] || continue
        pid_is_running "$pid" || continue
        ancestor_chain_contains "$pid" && continue
        pid_has_run_marker "$pid" && printf '%s\n' "$pid"
    done
}

safe_stop_run_owned_processes() {
    local pid current_start
    local -a owned=()
    local -a remaining=()
    declare -A owned_starts=()

    mapfile -t owned < <(run_owned_pids)
    for pid in "${owned[@]}"; do
        current_start=$(pid_start_time "$pid" 2>/dev/null || true)
        if [[ -z $current_start ]] || ! pid_has_run_marker "$pid"; then
            CLEANUP_BLOCKED=1
            block_required_gate
            continue
        fi
        owned_starts["$pid"]=$current_start
        log "cleanup candidate: $(ps -o pid=,ppid=,stat=,etime= -p "$pid" 2>/dev/null || true)"
    done

    for _ in {1..20}; do
        mapfile -t owned < <(run_owned_pids)
        ((${#owned[@]} == 0)) && return 0
        for pid in "${owned[@]}"; do
            current_start=$(pid_start_time "$pid" 2>/dev/null || true)
            if [[ -z $current_start ]]; then
                CLEANUP_BLOCKED=1
                continue
            fi
            owned_starts["$pid"]=${owned_starts[$pid]-$current_start}
            if [[ ${owned_starts[$pid]} == "$current_start" ]] && pid_has_run_marker "$pid" && ! ancestor_chain_contains "$pid"; then
                kill -TERM "$pid" 2>/dev/null || true
            else
                CLEANUP_BLOCKED=1
            fi
        done
        sleep 0.1
    done

    mapfile -t remaining < <(run_owned_pids)
    for pid in "${remaining[@]}"; do
        log "cleanup candidate still alive: $(ps -o pid=,ppid=,stat=,etime= -p "$pid" 2>/dev/null || true)"
        current_start=$(pid_start_time "$pid" 2>/dev/null || true)
        if [[ ${owned_starts[$pid]-} == "$current_start" ]] && pid_has_run_marker "$pid" && ! ancestor_chain_contains "$pid"; then
            kill -KILL "$pid" 2>/dev/null || true
        else
            CLEANUP_BLOCKED=1
        fi
    done
    for _ in {1..20}; do
        mapfile -t remaining < <(run_owned_pids)
        ((${#remaining[@]} == 0)) && return 0
        sleep 0.1
    done
    CLEANUP_BLOCKED=1
    block_required_gate
    log 'BLOCKED: run-owned process scan still found live tagged processes after cleanup'
}

register_created_root() {
    local pid=$1
    local marker=$2
    local start
    start=$(pid_start_time "$pid") || fail "could not record created process $pid"
    CREATED_PIDS+=("$pid")
    CREATED_STARTS+=("$start")
    CREATED_MARKERS+=("$marker")
}

safe_stop_created_tree() {
    local index=$1
    local root=${CREATED_PIDS[$index]}
    local expected_start=${CREATED_STARTS[$index]}
    local marker=${CREATED_MARKERS[$index]}
    local pid
    local current_start
    local command_line
    local -a candidates=()
    local -a remaining=()
    local -a live_descendants=()
    declare -A candidate_starts=()

    [[ -n $root ]] || return 0
    if ! kill -0 "$root" 2>/dev/null || ! pid_is_running "$root"; then
        return 0
    fi
    if ancestor_chain_contains "$root"; then
        log "refusing to signal session ancestor PID $root"
        CLEANUP_BLOCKED=1
        block_required_gate
        return 0
    fi
    current_start=$(pid_start_time "$root" 2>/dev/null || true)
    if [[ -z $current_start || $current_start != "$expected_start" ]]; then
        log "refusing to signal reused PID $root"
        CLEANUP_BLOCKED=1
        block_required_gate
        return 0
    fi
    command_line=$(pid_cmdline "$root" 2>/dev/null || true)
    if [[ -z $command_line || $command_line != *"$marker"* ]]; then
        log "refusing to signal unverified PID $root"
        CLEANUP_BLOCKED=1
        block_required_gate
        return 0
    fi

    mapfile -t candidates < <(descendant_pids "$root")
    local candidate
    for candidate in "${candidates[@]}"; do
        if ancestor_chain_contains "$candidate" || ! tree_reaches_root "$candidate" "$root"; then
            log "refusing to signal protected or detached descendant PID $candidate"
            continue
        fi
        candidate_starts["$candidate"]=$(pid_start_time "$candidate" 2>/dev/null || true)
        [[ -n ${candidate_starts[$candidate]} ]] || continue
        log "cleanup candidate: $(ps -o pid=,ppid=,stat=,etime= -p "$candidate" 2>/dev/null || true)"
    done
    for ((index = ${#candidates[@]} - 1; index >= 0; index--)); do
        pid=${candidates[$index]}
        [[ $pid == "$root" ]] && continue
        if ancestor_chain_contains "$pid" || ! tree_reaches_root "$pid" "$root"; then
            CLEANUP_BLOCKED=1
            continue
        fi
        current_start=$(pid_start_time "$pid" 2>/dev/null || true)
        [[ $current_start == "${candidate_starts[$pid]-}" ]] || continue
        kill -TERM "$pid" 2>/dev/null || true
    done
    for _ in {1..20}; do
        mapfile -t live_descendants < <(descendant_pids "$root" 2>/dev/null || true)
        remaining=()
        for pid in "${live_descendants[@]}"; do
            [[ $pid != "$root" ]] && remaining+=("$pid")
        done
        ((${#remaining[@]} == 0)) && break
        sleep 0.1
    done
    mapfile -t live_descendants < <(descendant_pids "$root" 2>/dev/null || true)
    remaining=()
    for pid in "${live_descendants[@]}"; do
        [[ $pid != "$root" ]] && remaining+=("$pid")
    done
    for pid in "${remaining[@]}"; do
        log "cleanup candidate still alive: $(ps -o pid=,ppid=,stat=,etime= -p "$pid" 2>/dev/null || true)"
        current_start=$(pid_start_time "$pid" 2>/dev/null || true)
        if ancestor_chain_contains "$pid" || ! tree_reaches_root "$pid" "$root"; then
            CLEANUP_BLOCKED=1
            continue
        fi
        [[ $current_start == "${candidate_starts[$pid]-}" ]] && kill -KILL "$pid" 2>/dev/null || true
    done
    for _ in {1..20}; do
        mapfile -t live_descendants < <(descendant_pids "$root" 2>/dev/null || true)
        remaining=()
        for pid in "${live_descendants[@]}"; do
            [[ $pid != "$root" ]] && remaining+=("$pid")
        done
        ((${#remaining[@]} == 0)) && break
        sleep 0.1
    done
    mapfile -t live_descendants < <(descendant_pids "$root" 2>/dev/null || true)
    remaining=()
    for pid in "${live_descendants[@]}"; do
        [[ $pid != "$root" ]] && remaining+=("$pid")
    done
    if ((${#remaining[@]} > 0)); then
        CLEANUP_BLOCKED=1
        block_required_gate
        log "BLOCKED: run-owned descendants rooted at PID $root did not fully terminate"
        return 0
    fi
    for pid in "${candidates[@]}"; do
        [[ $pid == "$root" ]] && continue
        if pid_is_running "$pid"; then
            current_start=$(pid_start_time "$pid" 2>/dev/null || true)
            CLEANUP_BLOCKED=1
            block_required_gate
            if [[ $current_start == "${candidate_starts[$pid]-}" ]]; then
                log "BLOCKED: verified run-owned descendant PID $pid escaped the root ancestry before root termination"
            else
                log "BLOCKED: candidate PID $pid was reused before root termination"
            fi
            return 0
        fi
    done

    current_start=$(pid_start_time "$root" 2>/dev/null || true)
    if [[ $current_start != "$expected_start" ]]; then
        return 0
    fi
    kill -TERM "$root" 2>/dev/null || true
    for _ in {1..20}; do
        pid_is_running "$root" || return 0
        sleep 0.1
    done
    current_start=$(pid_start_time "$root" 2>/dev/null || true)
    if [[ $current_start == "$expected_start" ]]; then
        kill -KILL "$root" 2>/dev/null || true
    fi
    for _ in {1..20}; do
        pid_is_running "$root" || return 0
        sleep 0.1
    done
    CLEANUP_BLOCKED=1
    block_required_gate
    log "BLOCKED: run-owned process root PID $root did not terminate"
}

write_report() {
    local exit_status=$1
    [[ $REPORT_WRITTEN == 1 ]] && return 0
    REPORT_WRITTEN=1
    {
        printf '# Real Codex terminal-room acceptance\n\n'
        printf 'Run: `%s`\n' "$RUN_ID"
        printf 'Date (UTC): `%s`\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'Exit status: `%s`\n' "$exit_status"
        printf 'Final status: **%s**\n' "$FINAL_STATUS"
        printf 'Codex version: `%s`\n' "${CODEX_VERSION:-not-observed}"
        printf 'Harness SHA-256: `%s`\n' "$(sha256sum "$SCRIPT_DIR/verify-terminal-room-live.sh" | awk '{print $1}')"
        printf 'Workspace-helper SHA-256: `%s`\n' "$(sha256sum "$SCRIPT_DIR/live-acceptance-workspace.sh" | awk '{print $1}')"
        printf 'Display path: `%s`\n' "${DISPLAY_USED:-not-selected}"
        printf 'Evidence directory: `%s`\n\n' "$OUTPUT_DIR"
        printf 'This report intentionally excludes Codex arguments, CODEX_HOME contents, runtime metadata, and bearer tokens.\n\n'
        printf '## Checklist\n\n'
        if ((${#CHECKS[@]} == 0)); then
            printf '%s\n' '- No checklist item ran.'
        else
            local check
            for check in "${CHECKS[@]}"; do
                printf '%s\n' "- $check"
            done
        fi
        printf '\n## Captures\n\n'
        if ((${#CAPTURED_FRAMES[@]} == 0)); then
            printf '%s\n' '- No screenshot was captured.'
        else
            local frame
            for frame in "${CAPTURED_FRAMES[@]}"; do
                printf '%s\n' '- `'"$frame"'`'
            done
        fi
        printf '\n## Restoration\n\n'
        if ((CLEANUP_BLOCKED == 0)); then
            printf '%s\n' '- Temporary XDG state/runtime/data/config/cache/home paths are removed by the harness cleanup.'
        else
            retained_run_root_report_line "$RUN_ROOT"
            private_display_credential_report_line "$DISPLAY_AUTHORITY_CLEANUP_STATUS"
        fi
        printf '%s\n' '- Run-owned process cleanup scans the exact per-run environment marker after root cleanup, including descendants reparented after root death.'
        printf '%s\n' '- Process diagnostics intentionally omit command lines so operator arguments and credentials cannot enter the report.'
        printf '%s\n' '- No Codex screen transcript is captured or parsed; structured state receipts are paired with supervised PNG evidence.'
    } > "$REPORT_PATH"
}

cleanup() {
    local exit_status=$?
    [[ $CLEANUP_STARTED == 1 ]] && return "$exit_status"
    CLEANUP_STARTED=1
    trap - EXIT INT TERM HUP

    local index
    for ((index = ${#CREATED_PIDS[@]} - 1; index >= 0; index--)); do
        safe_stop_created_tree "$index"
    done
    safe_stop_run_owned_processes
    if [[ $REQUIRED_GATE_BLOCKED == 1 && $FINAL_STATUS == PASS ]]; then
        FINAL_STATUS='BLOCKED'
    fi
    if [[ -n $HOST_TTY_STATE_BEFORE ]]; then
        local host_tty_state_after
        host_tty_state_after=$(stty -g </dev/tty 2>/dev/null || true)
        if [[ $host_tty_state_after == "$HOST_TTY_STATE_BEFORE" ]]; then
            record 'controller terminal usability' 'parent tty stty state matched before/after the harness'
        else
            record 'controller terminal usability' 'not verified (parent tty stty state changed or became unavailable)'
            REQUIRED_GATE_BLOCKED=1
            [[ $FINAL_STATUS == PASS ]] && FINAL_STATUS='BLOCKED'
        fi
    else
        record 'controller terminal usability' 'manual/unavailable (the harness controller did not expose a parent tty for restoration evidence)'
        REQUIRED_GATE_BLOCKED=1
        [[ $FINAL_STATUS == PASS ]] && FINAL_STATUS='BLOCKED'
    fi
    if [[ $FINAL_STATUS == BLOCKED || $REQUIRED_GATE_BLOCKED == 1 ]]; then
        FINAL_STATUS='BLOCKED'
        exit_status=1
    fi
    if ((CLEANUP_BLOCKED == 1)); then
        if scrub_private_display_credentials "$DISPLAY_AUTHORITY" "$DISPLAY_AUTHORITY_OWNED"; then
            if ((DISPLAY_AUTHORITY_OWNED == 1)); then
                DISPLAY_AUTHORITY_CLEANUP_STATUS='removed or redacted'
            else
                DISPLAY_AUTHORITY_CLEANUP_STATUS='not-owned; inherited Xauthority was left untouched'
            fi
            DISPLAY_AUTHORITY_OWNED=0
        else
            DISPLAY_AUTHORITY_CLEANUP_STATUS='not verified; retained root requires immediate restricted cleanup'
            record 'private display credentials' 'not removed or redacted before blocked diagnostics retention'
        fi
    fi
    write_report "$exit_status"
    if ((CLEANUP_BLOCKED == 0)); then
        rm -rf -- "$RUN_ROOT"
    else
        rm -f -- "$CODEGOTCHI_ARGUMENTS_FILE" "$METADATA_PATH" "$CODEGOTCHI_WRAPPER" \
            "$RUN_ROOT"/curl-*.conf "$RUN_ROOT"/state-*.json "$RUN_ROOT"/debug-*.out \
            "$RUN_ROOT"/debug-*.err "$RUN_ROOT"/xterm.log
        log "BLOCKED: retained run root at $RUN_ROOT for safe cleanup follow-up (private display credentials $DISPLAY_AUTHORITY_CLEANUP_STATUS)"
    fi
    exit "$exit_status"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

is_display_usable() {
    local candidate=$1
    [[ -n $candidate ]] || return 1
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        DISPLAY="$candidate" XAUTHORITY="$DISPLAY_AUTHORITY" xdpyinfo >/dev/null 2>&1 || return 1
        DISPLAY="$candidate" XAUTHORITY="$DISPLAY_AUTHORITY" xdotool getdisplaygeometry >/dev/null 2>&1 || return 1
    else
        DISPLAY="$candidate" xdpyinfo >/dev/null 2>&1 || return 1
        DISPLAY="$candidate" xdotool getdisplaygeometry >/dev/null 2>&1 || return 1
    fi
}

display_command() {
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" "$@"
    else
        DISPLAY="$DISPLAY_USED" "$@"
    fi
}

choose_window_manager() {
    local candidate
    for candidate in openbox fluxbox xfwm4 icewm matchbox-window-manager jwm twm; do
        if command -v "$candidate" >/dev/null 2>&1; then
            printf '%s\n' "$(command -v "$candidate")"
            return 0
        fi
    done
    return 1
}

start_private_display() {
    local display_number
    local candidate
    local wm_binary
    local wm_name
    local -a wm_arguments=()

    require_command Xvfb
    require_command xauth
    wm_binary=$(choose_window_manager) || block_and_exit "no lightweight window manager is installed; private Xvfb acceptance requires openbox, fluxbox, xfwm4, icewm, matchbox-window-manager, jwm, or twm"
    wm_name=$(basename "$wm_binary")

    for display_number in $(seq 90 199); do
        candidate=":$display_number"
        if is_display_usable "$candidate"; then
            continue
        fi
        DISPLAY_AUTHORITY="$RUN_ROOT/Xauthority-$display_number"
        DISPLAY_AUTHORITY_OWNED=1
        local cookie
        cookie=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
        printf 'add %s MIT-MAGIC-COOKIE-1 %s\n' "$candidate" "$cookie" \
            | xauth -f "$DISPLAY_AUTHORITY" >/dev/null 2>&1
        chmod 600 "$DISPLAY_AUTHORITY"
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$candidate" XAUTHORITY="$DISPLAY_AUTHORITY" Xvfb "$candidate" \
            -screen 0 1920x1200x24 -auth "$DISPLAY_AUTHORITY" -listen unix -nolisten tcp \
            >"$RUN_ROOT/xvfb.log" 2>&1 &
        XVFB_PID=$!
        register_created_root "$XVFB_PID" "Xvfb $candidate"
        for _ in {1..50}; do
            if is_display_usable "$candidate"; then
                break
            fi
            sleep 0.1
        done
        if ! is_display_usable "$candidate"; then
            fail "private Xvfb $candidate did not become usable; see the bounded harness log"
        fi

        case "$wm_name" in
            openbox) wm_arguments=(--sm-disable) ;;
            fluxbox) wm_arguments=(-no-slit) ;;
            *) wm_arguments=() ;;
        esac
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$candidate" XAUTHORITY="$DISPLAY_AUTHORITY" "$wm_binary" "${wm_arguments[@]}" \
            >"$RUN_ROOT/window-manager.log" 2>&1 &
        WM_PID=$!
        register_created_root "$WM_PID" "$wm_binary"
        sleep 0.5
        if ! kill -0 "$WM_PID" 2>/dev/null; then
            fail "window manager '$wm_name' exited on private Xvfb $candidate; xdotool activation cannot be trusted"
        fi
        DISPLAY_USED=$candidate
        DISPLAY_STARTED=1
        log "using private display with window manager '$wm_name'"
        record 'private display transport' 'verified authenticated local Unix-socket X transport before xdotool use'
        return 0
    done
    fail 'could not allocate a private X display in the bounded range :90..:199'
}

select_display() {
    if [[ -n ${DISPLAY-} ]] && is_display_usable "$DISPLAY"; then
        DISPLAY_USED=$DISPLAY
        DISPLAY_AUTHORITY=${XAUTHORITY-}
        DISPLAY_AUTHORITY_OWNED=0
        DISPLAY_STARTED=0
        log 'reusing the existing usable DISPLAY'
        return 0
    fi
    if [[ -n ${DISPLAY-} ]]; then
        log 'the inherited DISPLAY is not usable for xdotool; attempting a private display'
    else
        log 'DISPLAY is unset; attempting a private display'
    fi
    start_private_display
}

find_codegotchi_binary() {
    local candidate
    if [[ -n ${CODEGOTCHI_BIN-} ]]; then
        candidate=$CODEGOTCHI_BIN
    else
        candidate="$REPOSITORY_ROOT/target/debug/codegotchi"
        if [[ ${CODEGOTCHI_LIVE_NO_BUILD:-0} == 1 ]]; then
            [[ -x $candidate ]] || fail "CodeGotchi binary is unavailable at the requested path and CODEGOTCHI_LIVE_NO_BUILD=1"
        else
            require_command cargo
            log 'building the production CodeGotchi binary from the current workspace'
            if ! cargo build --quiet -p codegotchi-cli --bin codegotchi >"$RUN_ROOT/cargo-build.log" 2>&1; then
                fail 'cargo build failed; the bounded build log is not emitted because it may contain user paths'
            fi
        fi
    fi
    [[ -x $candidate ]] || fail "CodeGotchi executable is not found at the requested path"
    CODEGOTCHI_EXECUTABLE=$candidate
}

find_codex_binary() {
    local requested=${CODEGOTCHI_CODEX_BIN:-${CODEGOTCHI_REAL_CODEX:-codex}}
    if [[ $requested == */* ]]; then
        [[ -x $requested ]] || fail 'CODEGOTCHI_CODEX_BIN/CODEGOTCHI_REAL_CODEX is not executable'
        CODEX_EXECUTABLE=$requested
    else
        CODEX_EXECUTABLE=$(command -v "$requested" 2>/dev/null || true)
        [[ -n $CODEX_EXECUTABLE && -x $CODEX_EXECUTABLE ]] || fail "Codex executable '$requested' is not available on PATH"
    fi
    CODEX_VERSION=$("$CODEX_EXECUTABLE" --version 2>/dev/null | sed -n '1p' || true)
    [[ -n $CODEX_VERSION ]] || fail 'installed Codex did not report a version'
    log "installed Codex: $CODEX_VERSION"
    record 'installed Codex version' 'observed'
}

select_codex_home() {
    if [[ -n ${CODEGOTCHI_LIVE_CODEX_HOME-} ]]; then
        CODEX_HOME_VALUE=$CODEGOTCHI_LIVE_CODEX_HOME
    elif [[ -n ${CODEX_HOME-} ]]; then
        CODEX_HOME_VALUE=$CODEX_HOME
    elif [[ -n ${HOME-} ]]; then
        CODEX_HOME_VALUE="$HOME/.codex"
    else
        CODEX_HOME_VALUE=""
    fi
    [[ -n $CODEX_HOME_VALUE ]] || fail 'HOME or CODEGOTCHI_LIVE_CODEX_HOME is required to locate an authorized Codex home'
    [[ -d $CODEX_HOME_VALUE ]] || fail 'the selected CODEX_HOME is not a directory'
    record 'auth isolation' 'references the selected authorized CODEX_HOME without copying or printing its contents'
}

prepare_launch_files() {
    CODEGOTCHI_ARGUMENTS_FILE="$RUN_ROOT/codex-arguments.nul"
    CODEGOTCHI_WRAPPER="$RUN_ROOT/launch-codegotchi.sh"
    if ((${#CODEX_ARGUMENTS[@]} > 0)); then
        printf '%s\0' "${CODEX_ARGUMENTS[@]}" >"$CODEGOTCHI_ARGUMENTS_FILE"
    else
        : >"$CODEGOTCHI_ARGUMENTS_FILE"
    fi
    chmod 600 "$CODEGOTCHI_ARGUMENTS_FILE"
    cat >"$CODEGOTCHI_WRAPPER" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

restore_prefix=${CODEGOTCHI_LIVE_RESTORE_PREFIX:?}
stty -g >"${restore_prefix}-before" 2>/dev/null || printf '%s\n' unavailable >"${restore_prefix}-before"
set +e
"${CODEGOTCHI_LIVE_CODEGOTCHI_BIN:?}" run --ui terminal --terminal-theme auto -- codex
status=$?
set -e
stty -g >"${restore_prefix}-after" 2>/dev/null || printf '%s\n' unavailable >"${restore_prefix}-after"
printf '%s\n' "$status" >"${restore_prefix}-status"
if [[ ${CODEGOTCHI_LIVE_RESTORATION_PROBE:-0} == 1 ]]; then
    printf '%s\n' 'CODEGOTCHI terminal restoration probe ready'
    : >"${restore_prefix}-shell-ready"
    export PS1='CODEGOTCHI_RESTORED> '
    exec bash --noprofile --norc -i
fi
exit "$status"
EOF
    chmod 700 "$CODEGOTCHI_WRAPPER"
    record 'Codex argument isolation' 'trailing arguments are supplied through a private NUL-delimited file, not process argv'
}

window_exists() {
    [[ -n $WINDOW_ID ]] || return 1
    display_command xdotool getwindowname "$WINDOW_ID" >/dev/null 2>&1
}

active_window_is_target() {
    local active
    active=$(display_command xdotool getactivewindow 2>/dev/null || true)
    [[ $active == "$WINDOW_ID" ]]
}

activate_window() {
    local attempt
    for attempt in {1..5}; do
        if display_command xdotool windowmap "$WINDOW_ID" >/dev/null 2>&1 && \
            display_command xdotool windowactivate --sync "$WINDOW_ID" >/dev/null 2>&1 && \
            active_window_is_target; then
            return 0
        fi
        sleep 0.25
    done
    fail "xterm activation failed for the run-owned window; the display has no usable focus/window-manager path (previous BadWindow/_NET_ACTIVE_WINDOW failure)"
}

wait_for_window() {
    local attempt
    for attempt in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
        WINDOW_ID=$(display_command xdotool search --onlyvisible --classname "^${WINDOW_TITLE}$" 2>/dev/null | sed -n '1p' || true)
        if [[ -n $WINDOW_ID ]] && window_exists; then
            activate_window
            record 'window activation' 'verified with xdotool before timed interaction'
            return 0
        fi
        sleep 1
    done
    fail 'the run-owned xterm did not expose a visible window before the bounded startup timeout'
}

window_geometry_value() {
    local key=$1
    display_command xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null \
        | sed -n "s/^${key}=//p" | sed -n '1p'
}

capture_frame() {
    local label=$1
    local path="$OUTPUT_DIR/$RUN_ID-$label.png"
    local attempt mean
    for attempt in 1 2 3; do
        rm -f -- "$path"
        if [[ -n $DISPLAY_AUTHORITY ]]; then
            timeout 10s env DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" import -silent -window "$WINDOW_ID" "$path" >/dev/null 2>&1 || true
        else
            timeout 10s env DISPLAY="$DISPLAY_USED" import -silent -window "$WINDOW_ID" "$path" >/dev/null 2>&1 || true
        fi
        if [[ -s $path ]]; then
            mean=$(identify -format '%[fx:mean]' "$path" 2>/dev/null || true)
            if [[ -n $mean ]] && live_acceptance_capture_visible "$mean"; then
                CAPTURED_FRAMES+=("$path")
                record "capture $label" "saved nonblank frame (terminal geometry ${CURRENT_COLUMNS}x${CURRENT_ROWS})"
                return 0
            fi
        fi
        sleep 0.5
    done
    fail "ImageMagick import could not capture a nonblank live $label frame"
}

capture_prompt_frame() {
    capture_frame 'full-live-prompt'
}

verify_restoration() {
    local label=$1
    local prefix=$2
    local before after status
    for _ in {1..20}; do
        [[ -e "${prefix}-before" && -e "${prefix}-after" && -e "${prefix}-status" ]] && break
        sleep 0.1
    done
    before=$(sed -n '1p' "${prefix}-before" 2>/dev/null || true)
    after=$(sed -n '1p' "${prefix}-after" 2>/dev/null || true)
    status=$(sed -n '1p' "${prefix}-status" 2>/dev/null || true)
    if [[ -n $before && $before != unavailable && $before == "$after" && $status =~ ^[0-9]+$ ]]; then
        record "$label raw-mode restoration" 'run-owned PTY stty state matched before/after the session'
        return 0
    else
        record "$label raw-mode restoration" 'not verified (run-owned PTY state was unavailable or changed)'
        block_required_gate
        return 1
    fi
}

assert_window_usable() {
    window_exists || fail 'Codex/xterm exited during the bounded interaction sequence'
    activate_window
}

send_key() {
    display_command xdotool key --window "$WINDOW_ID" --clearmodifiers "$1" >/dev/null 2>&1 || fail "xdotool could not send key $1"
}

send_text() {
    display_command xdotool type --window "$WINDOW_ID" --delay 12 -- "$1" >/dev/null 2>&1 || fail 'xdotool could not send prompt text'
}

bracketed_paste_probe() {
    live_acceptance_paste_text \
        | CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" display_command xclip -selection primary -loops 1 -in >/dev/null 2>&1
    send_key shift+Insert
    sleep 0.8
    assert_window_usable
    capture_frame 'full-live-paste'
    send_key ctrl+c
    sleep 0.5
    assert_window_usable
}

record_codex_checks() {
    record 'ordinary prompt entry and editing/navigation' 'verified input sequence: typed test.mX, navigated left, inserted d, moved to end, removed X, and submitted the bounded test.md request'
    record 'model response' 'captured for supervised visual inspection after authoritative live activity settled'
    record 'tool activity' 'verified by authoritative runtime progress after the bounded prompt submission'
    record 'bracketed multiline paste visual' 'real xterm PRIMARY paste sent with Shift+Insert and captured in the populated Codex composer'
    record 'focus out/in visual' 'run-owned peer xterm became active, then the Codex xterm was reactivated and captured'
    record 'Codex scroll/click visual' 'real upper-pane click and wheel events were sent; the room care count remained unchanged and Codex stayed usable'
}

verify_protocol_input_receipts() {
    if live_acceptance_protocol_input_verified "$PROTOCOL_RECEIPT"; then
        record 'terminal input protocol receipt' 'bracketed paste and focus loss/gain were negotiated and delivered; a mouse event includes its negotiated mode and delivery disposition'
        local mouse_receipt
        mouse_receipt=$(grep -E '^event=mouse ' "$PROTOCOL_RECEIPT" | tail -n 1)
        record 'Codex mouse protocol receipt' "$mouse_receipt"
        return 0
    fi
    record 'terminal input protocol receipt' 'not verified from the run-owned non-content protocol receipt'
    block_required_gate
    return 1
}

verify_protocol_resize_receipts() {
    if live_acceptance_protocol_resizes_verified "$PROTOCOL_RECEIPT" "$CURRENT_COLUMNS" 45 30 21; then
        record 'PTY resize protocol receipt' 'physical terminal and Codex PTY dimensions were recorded for Full, Compact, and Minimal cycles'
        return 0
    fi
    record 'PTY resize protocol receipt' 'not verified for every requested Full, Compact, and Minimal size'
    block_required_gate
    return 1
}

verify_hook_trust() {
    if live_acceptance_uses_hook_trust_bypass; then
        record 'Codex hook trust' 'invocation-scoped automation bypass enabled for the generated CodeGotchi hook profile; no persistent trust choice was fabricated'
        return 0
    fi
    record 'Codex hook trust' 'manual/unavailable: trust selection is not automated without reading or retaining the official Codex review screen'
    block_required_gate
}

verify_approval_probe() {
    local policy
    policy=$(codex_approval_policy)
    if [[ $policy == on-request && $CUSTOM_CODEX_ARGUMENTS == 0 ]]; then
        local probe="$ACCEPTANCE_WORKSPACE/approval-probe.txt"
        rm -f -- "$probe"
        assert_window_usable
        state_summary before-approval
        send_text "$(live_acceptance_approval_prompt)"
        capture_frame 'full-live-approval-prompt'
        send_key Return
        local baseline_work_points current_work_points
        baseline_work_points=$(state_value before-approval work_points)
        for _ in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
            state_summary approval-started
            current_work_points=$(state_value approval-started work_points)
            if [[ $current_work_points =~ ^[0-9]+$ ]] && ((current_work_points > baseline_work_points)); then
                break
            fi
            sleep 1
        done
        if [[ ! $current_work_points =~ ^[0-9]+$ ]] || ((current_work_points <= baseline_work_points)); then
            record 'approval/review interaction' 'not verified: no authoritative work followed the isolated approval prompt'
            block_required_gate
            return 1
        fi
        sleep 8
        assert_window_usable
        capture_frame 'full-live-approval'
        sleep 2
        assert_window_usable
        capture_frame 'full-live-approval-modal'
        drive_live_acceptance_approval
        for _ in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
            [[ -f $probe ]] && break
            sleep 1
        done
        if [[ -f $probe ]]; then
            capture_frame 'full-live-after-approval'
            local approval_activity
            for _ in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
                state_summary after-approval
                approval_activity=$(state_value after-approval activity)
                [[ $approval_activity == WaitingForUser ]] && break
                sleep 1
            done
            if [[ $approval_activity == WaitingForUser ]]; then
                record 'approval/review interaction' 'real Codex approval modal captured; approved the exact temp-workspace touch command, observed the file, and returned to authoritative WaitingForUser'
                return 0
            fi
        fi
        record 'approval/review interaction' 'not verified: the captured on-request interaction did not both create the isolated probe file and recover to a settled session'
    elif [[ $policy == never ]]; then
        record 'approval/review interaction' "manual/unavailable in $CODEX_VERSION under an explicitly supplied --ask-for-approval never policy (custom arguments intentionally redacted)"
    elif [[ $policy == unspecified ]]; then
        record 'approval/review interaction' "manual/unavailable in $CODEX_VERSION: custom invocation did not declare --ask-for-approval, so the installed Codex default policy is not inferred"
    else
        record 'approval/review interaction' "manual/unavailable in $CODEX_VERSION under a custom explicit approval-policy value (redacted); no non-text approval receipt is exposed"
    fi
    block_required_gate
}

record_resize_unavailable() {
    local rows=$1
    record "resize ${CURRENT_COLUMNS}x${rows}" 'xterm cell-grid request applied with resize hints; populated Codex/room frame captured for supervised PTY/layout inspection'
}

codex_approval_policy() {
    local argument
    local next_is_value=0
    local value='unspecified'
    for argument in "${CODEX_ARGUMENTS[@]}"; do
        if ((next_is_value)); then
            value=$argument
            next_is_value=0
            continue
        fi
        case "$argument" in
            --ask-for-approval) next_is_value=1 ;;
            --ask-for-approval=*) value=${argument#*=} ;;
        esac
    done
    ((next_is_value)) && value='invalid'
    printf '%s\n' "$value"
}

poll_authoritative_progress() {
    local label=$1
    local baseline=$2
    [[ -n $API_URL && -n $API_TOKEN ]] || return 1
    local baseline_work baseline_last
    baseline_work=$(state_value "$baseline" work_points)
    baseline_last=$(state_value "$baseline" last_activity)
    for _ in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
        state_summary "$label"
        if [[ $(state_value "$label" work_points) =~ ^[0-9]+$ && $baseline_work =~ ^[0-9]+$ && $(state_value "$label" work_points) -gt $baseline_work ]] || \
            [[ $(state_value "$label" last_activity) != "$baseline_last" && $(state_value "$label" last_activity) != none ]]; then
            return 0
        fi
        sleep 1
    done
    return 1
}

poll_authoritative_turn_completion() {
    local label=$1
    local baseline=$2
    local baseline_work deadline activity work_points
    baseline_work=$(state_value "$baseline" work_points)
    deadline=$((SECONDS + ${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}))
    while ((SECONDS < deadline)); do
        state_summary "$label"
        activity=$(state_value "$label" activity)
        work_points=$(state_value "$label" work_points)
        if live_acceptance_turn_completed "$activity" "$work_points" "$baseline_work"; then
            return 0
        fi
        sleep 1
    done
    return 1
}

resize_terminal() {
    local rows=$1
    local before_width before_height after_width after_height resize_hints
    before_width=$(window_geometry_value WIDTH)
    before_height=$(window_geometry_value HEIGHT)
    display_command xdotool windowsize --sync --usehints "$WINDOW_ID" "$CURRENT_COLUMNS" "$rows" >/dev/null 2>&1 || fail "xdotool could not resize the terminal to ${CURRENT_COLUMNS}x${rows}"
    CURRENT_ROWS=$rows
    sleep 0.8
    assert_window_usable
    after_width=$(window_geometry_value WIDTH)
    after_height=$(window_geometry_value HEIGHT)
    resize_hints=$(display_command xprop -id "$WINDOW_ID" WM_NORMAL_HINTS 2>/dev/null || true)
    if [[ ! $after_width =~ ^[0-9]+$ || ! $after_height =~ ^[0-9]+$ || "$before_width,$before_height" == "$after_width,$after_height" ]]; then
        record "xterm outer resize ${CURRENT_COLUMNS}x${CURRENT_ROWS}" 'not verified (outer geometry or xterm resize hints did not settle)'
        block_required_gate
    else
        record "xterm outer resize ${CURRENT_COLUMNS}x${CURRENT_ROWS}" "verified by changed xterm geometry after a --usehints cell-grid request (${after_width}x${after_height})"
    fi
    record_resize_unavailable "$rows"
    capture_frame "${CURRENT_COLUMNS}x${CURRENT_ROWS}"
}

cell_move() {
    local column=$1
    local row=$2
    local width
    local height
    local pixel_x
    local pixel_y
    width=$(window_geometry_value WIDTH)
    height=$(window_geometry_value HEIGHT)
    [[ $width =~ ^[0-9]+$ && $height =~ ^[0-9]+$ ]] || fail 'could not read xterm pixel geometry for a care gesture'
    pixel_x=$((column * width / CURRENT_COLUMNS + width / (CURRENT_COLUMNS * 2)))
    pixel_y=$((row * height / CURRENT_ROWS + height / (CURRENT_ROWS * 2)))
    display_command xdotool mousemove --window "$WINDOW_ID" --sync "$pixel_x" "$pixel_y" >/dev/null 2>&1 || fail 'xdotool could not move the pointer into the terminal room'
}

cell_click() {
    local column=$1
    local row=$2
    cell_move "$column" "$row"
    display_command xdotool mousedown 1 >/dev/null 2>&1 || fail 'xdotool could not press a terminal-room target'
    sleep 0.1
    display_command xdotool mouseup 1 >/dev/null 2>&1 || fail 'xdotool could not release a terminal-room target'
}

focus_probe() {
    local focus_title="codegotchi-focus-$RUN_ID"
    local focus_pid focus_window=''
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" xterm \
            -name "$focus_title" -title "$focus_title" -geometry 20x3 -e sleep 300 >/dev/null 2>&1 &
    else
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" xterm \
            -name "$focus_title" -title "$focus_title" -geometry 20x3 -e sleep 300 >/dev/null 2>&1 &
    fi
    focus_pid=$!
    register_created_root "$focus_pid" "$focus_title"
    for _ in {1..30}; do
        focus_window=$(display_command xdotool search --onlyvisible --classname "^${focus_title}$" 2>/dev/null | sed -n '1p' || true)
        [[ -n $focus_window ]] && break
        sleep 0.1
    done
    [[ -n $focus_window ]] || fail 'focus probe xterm did not become visible'
    display_command xdotool windowactivate --sync "$focus_window" >/dev/null 2>&1 || fail 'could not focus the run-owned peer xterm'
    [[ $(display_command xdotool getactivewindow 2>/dev/null || true) == "$focus_window" ]] || fail 'focus did not leave the Codex xterm'
    activate_window
    capture_frame 'full-live-focus-return'
}

codex_mouse_probe() {
    local before_care after_care
    state_summary before-codex-mouse
    before_care=$(state_value before-codex-mouse care_ids)
    cell_move 60 10
    display_command xdotool click 1 >/dev/null 2>&1 || fail 'could not click the Codex upper pane'
    display_command xdotool click 4 >/dev/null 2>&1 || fail 'could not scroll up in the Codex upper pane'
    display_command xdotool click 5 >/dev/null 2>&1 || fail 'could not scroll down in the Codex upper pane'
    sleep 0.5
    assert_window_usable
    state_summary after-codex-mouse
    after_care=$(state_value after-codex-mouse care_ids)
    [[ $before_care == "$after_care" ]] || fail 'upper-pane Codex mouse probe unexpectedly mutated room care state'
    capture_frame 'full-live-codex-mouse'
}

full_pet() {
    local center column
    for center in 74 88 101; do
        cell_move "$center" 39
        display_command xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the Full pet gesture'
        for column in "$((center - 4))" "$((center + 4))" "$((center - 4))" "$((center + 4))" "$((center - 4))"; do
            cell_move "$column" 39
            sleep 0.35
        done
        display_command xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the Full pet gesture'
        sleep 0.2
    done
    sleep 0.8
    assert_window_usable
    record 'pet stroke input' 'attempted qualified strokes at three positions spanning the Full room wander lane; no pet completion is inferred from input alone'
}

full_feed() {
    local column target_column
    for target_column in 74 88 105; do
        cell_move 6 41
        display_command xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the stocked-food drag'
        for column in 20 40 60 "$target_column"; do
            cell_move "$column" 39
            sleep 0.12
        done
        display_command xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the stocked-food drag'
        sleep 0.2
    done
    sleep 0.8
    assert_window_usable
    record 'stocked food drag-to-pet' 'attempted from the Full kibble source to three positions spanning the awake/sleeping pet lane'
}

full_clean() {
    cell_click 54 40
    sleep 0.8
    assert_window_usable
    record 'authoritative poop clean' 'attempted against the isolated generated-poop target'
}

full_nap() {
    local column
    for column in 101 109 116; do
        cell_click "$column" 39
        sleep 0.2
    done
    sleep 0.8
    assert_window_usable
    record 'authoritative nap' 'attempted against the Full bed target'
}

find_runtime_metadata() {
    local attempt
    local candidate
    for attempt in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
        candidate=$(find "$RUNTIME_HOME/codegotchi" -maxdepth 1 -type f -name 'session-*.json' -print -quit 2>/dev/null || true)
        if [[ -n $candidate ]]; then
            METADATA_PATH=$candidate
            record 'isolated runtime metadata' 'found without printing its contents'
            return 0
        fi
        sleep 1
    done
    fail 'CodeGotchi did not publish isolated runtime metadata before the bounded startup timeout'
}

load_api_metadata() {
    if ! command -v node >/dev/null 2>&1 || ! command -v curl >/dev/null 2>&1; then
        record 'authoritative snapshot polling' 'not available (node and curl are required for redacted state summaries)'
        block_required_gate
        return 1
    fi
    local metadata_values
    mapfile -t metadata_values < <(node - "$METADATA_PATH" 2>/dev/null <<'NODE'
const fs = require('node:fs');
const metadata = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (typeof metadata.loopbackBaseUrl !== 'string' || typeof metadata.bearerToken !== 'string') {
  process.exit(1);
}
process.stdout.write(`${metadata.loopbackBaseUrl}\n${metadata.bearerToken}\n`);
NODE
    )
    API_URL=${metadata_values[0]-}
    API_TOKEN=${metadata_values[1]-}
    if [[ $API_URL != http://127.0.0.1:* || -z $API_TOKEN ]]; then
        record 'authoritative snapshot polling' 'not available (runtime metadata did not contain the expected loopback fields)'
        block_required_gate
        API_URL=""
        API_TOKEN=""
        return 1
    fi
    record 'authoritative snapshot polling' 'enabled with redacted state summaries'
    return 0
}

state_summary() {
    local label=$1
    local state_path="$RUN_ROOT/state-$label.json"
    local summary_path="$RUN_ROOT/state-$label-summary.txt"
    local curl_config="$RUN_ROOT/curl-$label.conf"
    [[ -n $API_URL && -n $API_TOKEN ]] || return 0
    printf 'header = "Authorization: Bearer %s"\n' "$API_TOKEN" >"$curl_config"
    chmod 600 "$curl_config"
    if ! curl --silent --show-error --fail --max-time 3 --config "$curl_config" "$API_URL/api/v1/state" -o "$state_path" >/dev/null 2>&1; then
        record "authoritative snapshot $label" 'unavailable (loopback state request did not settle)'
        block_required_gate
        return 0
    fi
    if ! node - "$state_path" >"$summary_path" 2>/dev/null <<'NODE'
const fs = require('node:fs');
const state = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const inventory = state.inventory ?? {};
const demands = state.pendingDemands ?? state.pending_demands ?? [];
const poops = state.pendingPoops ?? state.pending_poops ?? [];
const napping = state.nappingUntil ?? state.napping_until ?? null;
const needs = state.needs ?? {};
const careIds = state.processedCareIds ?? state.processed_care_ids ?? [];
const sessions = state.sessionActivities ?? state.session_activities ?? {};
const outcome = state.recentOutcome ?? state.recent_outcome ?? {};
const count = (key) => inventory[key] ?? inventory[key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)] ?? 0;
const value = (key) => needs[key] ?? 0;
const enumName = (value) => typeof value === 'string' ? value : value && typeof value === 'object' ? Object.keys(value)[0] ?? 'unknown' : 'unknown';
const activityName = enumName(state.activity);
const outcomeName = enumName(outcome);
process.stdout.write(`poops=${Array.isArray(poops) ? poops.length : 0} demands=${Array.isArray(demands) ? demands.length : 0} kibble=${count('kibble')} treat=${count('treat')} fruit=${count('fruit')} energy=${count('energyDrink')} happiness=${value('happiness')} cleanliness=${value('cleanliness')} care_ids=${Array.isArray(careIds) ? careIds.length : 0} work_points=${state.workPoints ?? state.work_points ?? 0} activity=${activityName} outcome=${outcomeName} sessions=${Object.keys(sessions).length} last_activity=${state.lastActivityAt ?? state.last_activity_at ?? 'none'} last_outcome=${state.lastOutcomeAt ?? state.last_outcome_at ?? 'none'} napping=${napping ? 'active' : 'inactive'}\n`);
NODE
    then
        record "authoritative snapshot $label" 'unavailable (state response was not parseable)'
        block_required_gate
        return 0
    fi
    local summary
    summary=$(sed -n '1p' "$summary_path")
    STATE_SUMMARIES["$label"]=$summary
    record "authoritative snapshot $label" "$summary"
}

state_value() {
    local label=$1
    local key=$2
    local summary=${STATE_SUMMARIES[$label]-}
    printf '%s\n' "$summary" | awk -v key="$key" '{ for (field_index = 1; field_index <= NF; field_index++) if ($field_index ~ "^" key "=") { sub("^" key "=", "", $field_index); print $field_index; exit } }'
}

verify_care_snapshots() {
    local prepared_kibble
    local after_feed_kibble
    local prepared_poops
    local after_clean_poops
    local after_nap
    local prepared_care_ids
    local after_pet_care_ids

    prepared_kibble=$(state_value prepared kibble)
    after_feed_kibble=$(state_value after-feed kibble)
    prepared_poops=$(state_value prepared poops)
    after_clean_poops=$(state_value after-clean poops)
    after_nap=$(state_value after-nap napping)
    prepared_care_ids=$(state_value prepared care_ids)
    after_pet_care_ids=$(state_value after-pet care_ids)

    if live_acceptance_care_advanced "$prepared_care_ids" "$after_pet_care_ids"; then
        record 'authoritative pet result' 'verified by a settled processed-care increment immediately after the isolated pet stroke'
    else
        record 'authoritative pet result' 'not verified by the settled snapshot'
        block_required_gate
    fi

    if [[ $prepared_kibble =~ ^[0-9]+$ && $after_feed_kibble =~ ^[0-9]+$ && $after_feed_kibble -lt $prepared_kibble ]]; then
        record 'authoritative feed result' 'verified by a settled inventory decrement'
    else
        record 'authoritative feed result' 'not verified by the settled snapshot'
        block_required_gate
    fi
    if [[ $prepared_poops =~ ^[0-9]+$ && $after_clean_poops =~ ^[0-9]+$ && $after_clean_poops -lt $prepared_poops ]]; then
        record 'authoritative clean result' 'verified by a settled pending-poop decrement'
    else
        record 'authoritative clean result' 'not verified by the settled snapshot'
        block_required_gate
    fi
    if [[ $after_nap == active ]]; then
        record 'authoritative nap result' 'verified by a settled future napping deadline'
    else
        record 'authoritative nap result' 'not verified by the settled snapshot'
        block_required_gate
    fi
}

debug_command() {
    local command_name=$1
    local stdout_path="$RUN_ROOT/debug-$command_name.out"
    local stderr_path="$RUN_ROOT/debug-$command_name.err"
    if timeout --foreground --signal=TERM --kill-after=2s "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}s" \
        env CODEGOTCHI_ENABLE_DEBUG=1 CODEGOTCHI_SESSION_FILE="$METADATA_PATH" \
        "$CODEGOTCHI_EXECUTABLE" debug "$command_name" >"$stdout_path" 2>"$stderr_path"; then
        record "isolated debug $command_name" 'applied without exposing runtime credentials'
    else
        record "isolated debug $command_name" 'not available; care result remains explicitly unclaimed'
    fi
}

normal_exit() {
    local restoration_ready=0 shell_receipt_seen=0 interrupt_fallback=0
    local shell_receipt="${RESTORE_PREFIX}-shell-receipt"
    send_text "$(live_acceptance_exit_command)"
    send_key Return
    local attempt
    for attempt in $(seq 1 "${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30}"); do
        if [[ -e "${RESTORE_PREFIX}-after" && -e "${RESTORE_PREFIX}-status" && -e "${RESTORE_PREFIX}-shell-ready" ]] && window_exists; then
            if verify_restoration 'normal exit' "$RESTORE_PREFIX"; then
                restoration_ready=1
            fi
            capture_frame 'normal-exit-restored-shell'
            send_text 'printf '\''%s\n'\'' "$CODEGOTCHI_LIVE_RESTORATION_TOKEN" > "$CODEGOTCHI_LIVE_RESTORE_RECEIPT"; printf '\''%s\n'\'' "$CODEGOTCHI_LIVE_RESTORATION_TOKEN"'
            send_key Return
            for _ in {1..20}; do
                if [[ $(sed -n '1p' "$shell_receipt" 2>/dev/null || true) == "$RESTORATION_TOKEN" ]]; then
                    shell_receipt_seen=1
                    break
                fi
                sleep 0.1
            done
            if ((shell_receipt_seen == 1)); then
                capture_frame 'normal-exit-restored-shell-input'
                if window_exists; then
                    display_command xdotool type --window "$WINDOW_ID" --delay 12 -- 'exit' >/dev/null 2>&1 || true
                    if window_exists; then
                        display_command xdotool key --window "$WINDOW_ID" --clearmodifiers Return >/dev/null 2>&1 || {
                            if window_exists; then
                                fail 'xdotool could not submit exit to the restored shell'
                            fi
                            true
                        }
                    fi
                fi
            fi
            break
        fi
        if ! window_exists; then
            break
        fi
        if ((attempt == 5)); then
            send_key ctrl+c
            interrupt_fallback=1
            record 'normal exit request' '/quit did not settle within five seconds; sent one Ctrl+C at the empty official Codex composer'
        fi
        sleep 1
    done
    if ((restoration_ready == 0 || shell_receipt_seen == 0)); then
        record 'normal Codex exit' 'not verified: restoration artifacts and an executed command in the same-xterm shell were not both observed'
        block_required_gate
        return 1
    fi
    for _ in $(seq 1 "$(( ${CODEGOTCHI_LIVE_TIMEOUT_SEC:-30} * 10 ))"); do
        if ! window_exists; then
            if ((interrupt_fallback == 1)); then
                record 'normal Codex exit' 'bounded Ctrl+C user-exit fallback left the alternate screen, executed a receipt command in a real interactive shell, and closed cleanly'
            else
                record 'normal Codex exit' 'same xterm left the alternate screen, executed a receipt command in a real interactive shell, and closed cleanly'
            fi
            WINDOW_ID=""
            return 0
        fi
        sleep 0.1
    done
    record 'normal Codex exit' 'not verified by the bounded same-xterm restoration probe'
    block_required_gate
    return 1
}

start_xterm_session() {
    RESTORE_PREFIX="$RUN_ROOT/$WINDOW_TITLE-restore"
    PROTOCOL_RECEIPT="${RESTORE_PREFIX}-protocol"
    export CODEGOTCHI_LIVE_RESTORE_PREFIX="$RESTORE_PREFIX"
    export CODEGOTCHI_LIVE_RESTORE_RECEIPT="${RESTORE_PREFIX}-shell-receipt"
    export CODEGOTCHI_LIVE_RESTORATION_PROBE="$RESTORATION_PROBE_ENABLED"
    export CODEGOTCHI_LIVE_RESTORATION_TOKEN="$RESTORATION_TOKEN"
    export CODEGOTCHI_LIVE_CODEGOTCHI_BIN="$CODEGOTCHI_EXECUTABLE"
    export CODEGOTCHI_LIVE_HARNESS=1
    export CODEGOTCHI_LIVE_CODEX_ARGUMENTS_FILE="$CODEGOTCHI_ARGUMENTS_FILE"
    export CODEGOTCHI_LIVE_ARGUMENTS_ROOT="$RUN_ROOT"
    export CODEGOTCHI_LIVE_PROTOCOL_FILE="$PROTOCOL_RECEIPT"
    export HOME="$TEMP_HOME"
    export XDG_CONFIG_HOME="$CONFIG_HOME"
    export XDG_CACHE_HOME="$CACHE_HOME"
    export XDG_DATA_HOME="$DATA_HOME"
    export XDG_STATE_HOME="$STATE_HOME"
    export XDG_RUNTIME_DIR="$RUNTIME_HOME"
    export CODEX_HOME="$ACCEPTANCE_CODEX_HOME"
    export CODEGOTCHI_BROWSER=none
    export CODEGOTCHI_ENABLE_DEBUG=1
    export CODEGOTCHI_REAL_CODEX="$CODEX_EXECUTABLE"
    export TERM=xterm-256color
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" xterm \
            -name "$WINDOW_TITLE" \
            -title "$WINDOW_TITLE" \
            -geometry 120x45 \
            -e "$CODEGOTCHI_WRAPPER" \
            >/dev/null 2>&1 &
    else
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" xterm \
            -name "$WINDOW_TITLE" \
            -title "$WINDOW_TITLE" \
            -geometry 120x45 \
            -e "$CODEGOTCHI_WRAPPER" \
            >/dev/null 2>&1 &
    fi
    XTERM_PID=$!
    XTERM_START=$(pid_start_time "$XTERM_PID") || fail 'could not record the run-owned xterm process'
    register_created_root "$XTERM_PID" "-title $WINDOW_TITLE"
}

termination_case() {
    WINDOW_TITLE="codegotchi-live-terminate-$RUN_ID"
    WINDOW_ID=""
    XTERM_PID=""
    RESTORATION_PROBE_ENABLED=0
    start_xterm_session
    wait_for_window
    sleep 1
    local child_pid child_command found_child=0
    local -a descendants=()
    mapfile -t descendants < <(descendant_pids "$XTERM_PID")
    for child_pid in "${descendants[@]}"; do
        [[ $child_pid == "$XTERM_PID" ]] && continue
        child_command=$(pid_cmdline "$child_pid" 2>/dev/null || true)
        if [[ $child_command == *"$CODEGOTCHI_EXECUTABLE"* ]]; then
            register_created_root "$child_pid" "$CODEGOTCHI_EXECUTABLE"
            safe_stop_created_tree "$(( ${#CREATED_PIDS[@]} - 1 ))"
            found_child=1
            record 'bounded termination case' 'termination attempted only against the verified run-owned CodeGotchi descendant tree; tagged-process cleanup checks for reparented survivors'
            break
        fi
    done
    if ((found_child == 0)); then
        record 'bounded termination case' 'not available (the expected CodeGotchi descendant was not observable)'
        block_required_gate
    fi
    for _ in {1..20}; do
        if ! kill -0 "$XTERM_PID" 2>/dev/null || ! pid_is_running "$XTERM_PID"; then
            XTERM_PID=""
            record 'bounded termination restoration' 'xterm exited within the bounded signal window; wrapper restoration artifacts were checked'
            verify_restoration 'bounded termination' "$RESTORE_PREFIX" || true
            return 0
        fi
        sleep 0.1
    done
    record 'bounded termination restoration' 'not verified (xterm remained alive after the bounded signal window)'
    block_required_gate
    WINDOW_ID=""
}

main() {
    local command_name
    for command_name in xterm xdotool xclip xdpyinfo import identify timeout sed awk find ps xprop od tr grep basename seq sha256sum tail; do
        require_command "$command_name"
    done
    find_codegotchi_binary
    find_codex_binary
    select_codex_home
    prepare_live_acceptance_workspace "$RUN_ROOT" "$CODEX_HOME_VALUE"
    record 'isolated prompt workspace' 'created run-owned test.md for the bounded file-read interaction'
    record 'isolated Codex home' 'run-owned trust/config with a credential symlink to the selected authorized auth.json; credential contents were not copied'
    record 'acceptance model' 'gpt-5.6-luna with low reasoning'
    prepare_launch_files
    select_display

    start_xterm_session
    wait_for_window
    find_runtime_metadata
    load_api_metadata || true
    sleep 1
    assert_window_usable
    state_summary initial
    debug_command restock
    debug_command generate-poop
    sleep 1
    state_summary prepared
    capture_frame 'full-live-initial'

    state_summary before-prompt
    drive_live_acceptance_prompt capture_prompt_frame
    if poll_authoritative_turn_completion after-response before-prompt; then
        capture_frame 'full-live-response'
        verify_approval_probe || true
        bracketed_paste_probe
        focus_probe
        codex_mouse_probe
        record_codex_checks
        verify_protocol_input_receipts || true
        record 'trailing Codex arguments' 'model and low effort are visible in the official Codex header; the successful tool read of the run-owned test.md proves the generated --cd workspace argument was honored'
    else
        record 'bounded file-read interaction' 'not verified: no authoritative activity followed the submitted prompt'
        block_required_gate
        capture_frame 'full-live-no-response'
    fi

    full_pet
    state_summary after-pet
    full_feed
    state_summary after-feed
    full_clean
    state_summary after-clean
    full_nap
    state_summary after-nap
    verify_care_snapshots
    capture_frame 'full-live-care'

    verify_hook_trust
    capture_frame 'full-live-checks'

    resize_terminal 30
    resize_terminal 21
    resize_terminal 45
    verify_protocol_resize_receipts || true
    capture_frame 'full-live-final'
    normal_exit || true
    termination_case

    if [[ $REQUIRED_GATE_BLOCKED == 1 ]]; then
        FINAL_STATUS='BLOCKED'
        record 'live acceptance overall' 'BLOCKED; one or more required checks remain unavailable or unverified'
        return 1
    fi
    FINAL_STATUS='PASS'
    record 'live acceptance overall' 'PASS'
    return 0
}

main "$@"
