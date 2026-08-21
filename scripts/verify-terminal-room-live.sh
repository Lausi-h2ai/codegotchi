#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPOSITORY_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
source "$SCRIPT_DIR/live-acceptance-cleanup.sh"

usage() {
    cat <<'EOF'
Usage: scripts/verify-terminal-room-live.sh [--] [codex arguments...]

Launches the production CodeGotchi terminal room in an xterm with the real
installed Codex executable, drives a bounded fidelity/care checklist, and
writes screenshots and a redacted checklist report.

The default Codex invocation is read-only and never asks for approvals. Pass
arguments after `--` to replace those defaults, for example:

  scripts/verify-terminal-room-live.sh -- --ask-for-approval never --sandbox read-only

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

CODEX_ARGUMENTS=(--disable apps --ask-for-approval never --sandbox read-only)
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
HOST_TTY_STATE_BEFORE=""

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
            printf '%s\n' "- Private display credential cleanup: `$DISPLAY_AUTHORITY_CLEANUP_STATUS`."
        fi
        printf '%s\n' '- Run-owned process cleanup scans the exact per-run environment marker after root cleanup, including descendants reparented after root death.'
        printf '%s\n' '- Process diagnostics intentionally omit command lines so operator arguments and credentials cannot enter the report.'
        printf '%s\n' '- No Codex screen transcript is captured or parsed; checks requiring protocol or product-text receipts remain manual/unavailable and block completion.'
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
    fi
    if [[ ! -x $candidate ]]; then
        if [[ ${CODEGOTCHI_LIVE_NO_BUILD:-0} == 1 ]]; then
            fail "CodeGotchi binary is unavailable at the requested path and CODEGOTCHI_LIVE_NO_BUILD=1"
        fi
        require_command cargo
        log 'building the production CodeGotchi binary'
        if ! cargo build --quiet -p codegotchi-cli --bin codegotchi >"$RUN_ROOT/cargo-build.log" 2>&1; then
            fail 'cargo build failed; the bounded build log is not emitted because it may contain user paths'
        fi
        candidate="$REPOSITORY_ROOT/target/debug/codegotchi"
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
        WINDOW_ID=$(display_command xdotool search --onlyvisible --name "$WINDOW_TITLE" 2>/dev/null | sed -n '1p' || true)
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
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        timeout 10s env DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" import -silent -window "$WINDOW_ID" "$path" >/dev/null 2>&1 || fail "ImageMagick import could not capture the live $label frame"
    elif ! timeout 10s env DISPLAY="$DISPLAY_USED" import -silent -window "$WINDOW_ID" "$path" >/dev/null 2>&1; then
        fail "ImageMagick import could not capture the live $label frame"
    fi
    [[ -s $path ]] || fail "the live $label capture is empty"
    CAPTURED_FRAMES+=("$path")
    record "capture $label" "saved (terminal geometry ${CURRENT_COLUMNS}x${CURRENT_ROWS})"
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
    else
        record "$label raw-mode restoration" 'not verified (run-owned PTY state was unavailable or changed)'
        block_required_gate
    fi
    record "$label terminal controls" 'manual/unavailable: transcript capture is disabled, so cursor, alternate-screen, mouse, focus, and paste cleanup require visual inspection'
    block_required_gate
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

record_codex_checks() {
    record 'ordinary prompt entry and editing/navigation' 'manual/unavailable: no non-text Codex input receipt is exposed and transcript capture is disabled'
    record 'model response' 'manual/unavailable: authoritative runtime state cannot bind a response to an unobserved prompt without Codex text'
    record 'tool activity' 'manual/unavailable: structured runtime activity is not sufficient to prove the requested live tool command without Codex text or a receipt'
    record 'bracketed multiline paste' 'manual/unavailable: paste negotiation and insertion require visual/protocol inspection; no transcript is retained'
    record 'focus out/in' 'manual/unavailable: xterm window focus can be observed, but DEC focus delivery to Codex cannot be proven without a protocol bridge'
    record 'Codex scroll/click behavior' 'manual/unavailable: room non-mutation is not proof that Codex received its negotiated mouse protocol'
    block_required_gate
}

verify_hook_trust() {
    record 'Codex hook trust' 'manual/unavailable: trust selection is not automated without reading or retaining the official Codex review screen'
    block_required_gate
}

verify_approval_probe() {
    local policy
    policy=$(codex_approval_policy)
    if [[ $policy == never && $CUSTOM_CODEX_ARGUMENTS == 0 ]]; then
        record 'approval/review interaction' "manual/unavailable in $CODEX_VERSION with exact command codex --disable apps --ask-for-approval never --sandbox read-only"
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
    record "resize ${CURRENT_COLUMNS}x${rows}" 'manual/unavailable: xterm geometry can be measured, but Codex PTY rectangle and resize handling have no non-text receipt'
    block_required_gate
}

record_care_completion_unavailable() {
    record 'authoritative pet result' 'manual/unavailable: the state snapshot exposes care IDs but not whether the final release was Pet versus PetStroke'
    block_required_gate
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
    if [[ ! $after_width =~ ^[0-9]+$ || ! $after_height =~ ^[0-9]+$ || "$before_width,$before_height" == "$after_width,$after_height" || $resize_hints != *PResizeInc* ]]; then
        record "xterm outer resize ${CURRENT_COLUMNS}x${CURRENT_ROWS}" 'not verified (outer geometry or xterm resize hints did not settle)'
        block_required_gate
    else
        record "xterm outer resize ${CURRENT_COLUMNS}x${CURRENT_ROWS}" "verified by changed xterm geometry and xterm resize hints (${after_width}x${after_height})"
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
    display_command xdotool click --window "$WINDOW_ID" 1 >/dev/null 2>&1 || fail 'xdotool could not click a terminal-room target'
}

full_pet() {
    local column
    cell_move 86 38
    display_command xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the Full pet gesture'
    for column in 80 84 88 92 95; do
        cell_move "$column" 38
        sleep 0.35
    done
    display_command xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the Full pet gesture'
    sleep 0.8
    assert_window_usable
    record 'pet stroke input' 'attempted across five configured cell positions; no pet completion is inferred from the gesture'
}

full_feed() {
    local column
    cell_move 6 41
    display_command xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the stocked-food drag'
    for column in 20 40 60 80 87; do
        cell_move "$column" 40
        sleep 0.12
    done
    cell_move 87 38
    display_command xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the stocked-food drag'
    sleep 0.8
    assert_window_usable
    record 'stocked food drag-to-pet' 'attempted from the initial Full kibble source to the pet hit region'
}

full_clean() {
    cell_click 58 41
    sleep 0.8
    assert_window_usable
    record 'authoritative poop clean' 'attempted against the isolated generated-poop target'
}

full_nap() {
    cell_click 107 39
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

    prepared_kibble=$(state_value prepared kibble)
    after_feed_kibble=$(state_value after-feed kibble)
    prepared_poops=$(state_value prepared poops)
    after_clean_poops=$(state_value after-clean poops)
    after_nap=$(state_value after-nap napping)

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
    if CODEGOTCHI_ENABLE_DEBUG=1 CODEGOTCHI_SESSION_FILE="$METADATA_PATH" "$CODEGOTCHI_EXECUTABLE" debug "$command_name" >"$stdout_path" 2>"$stderr_path"; then
        record "isolated debug $command_name" 'applied without exposing runtime credentials'
    else
        record "isolated debug $command_name" 'not available; care result remains explicitly unclaimed'
    fi
}

normal_exit() {
    send_text '/exit'
    send_key Return
    for _ in {1..30}; do
        if ! window_exists; then
            record 'normal Codex exit' 'xterm closed after the bounded /exit command'
            WINDOW_ID=""
            return 0
        fi
        sleep 1
    done
    record 'normal Codex exit' 'not available (Codex did not close after bounded /exit)'
    block_required_gate
    return 1
}

start_xterm_session() {
    RESTORE_PREFIX="$RUN_ROOT/$WINDOW_TITLE-restore"
    export CODEGOTCHI_LIVE_RESTORE_PREFIX="$RESTORE_PREFIX"
    export CODEGOTCHI_LIVE_CODEGOTCHI_BIN="$CODEGOTCHI_EXECUTABLE"
    export CODEGOTCHI_LIVE_HARNESS=1
    export CODEGOTCHI_LIVE_CODEX_ARGUMENTS_FILE="$CODEGOTCHI_ARGUMENTS_FILE"
    export CODEGOTCHI_LIVE_ARGUMENTS_ROOT="$RUN_ROOT"
    export HOME="$TEMP_HOME"
    export XDG_CONFIG_HOME="$CONFIG_HOME"
    export XDG_CACHE_HOME="$CACHE_HOME"
    export XDG_DATA_HOME="$DATA_HOME"
    export XDG_STATE_HOME="$STATE_HOME"
    export XDG_RUNTIME_DIR="$RUNTIME_HOME"
    export CODEX_HOME="$CODEX_HOME_VALUE"
    export CODEGOTCHI_BROWSER=none
    export CODEGOTCHI_ENABLE_DEBUG=1
    export CODEGOTCHI_REAL_CODEX="$CODEX_EXECUTABLE"
    export TERM=xterm-256color
    if [[ -n $DISPLAY_AUTHORITY ]]; then
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" XAUTHORITY="$DISPLAY_AUTHORITY" xterm \
            -title "$WINDOW_TITLE" \
            -geometry 120x45 \
            -e "$CODEGOTCHI_WRAPPER" \
            >/dev/null 2>&1 &
    else
        CODEGOTCHI_LIVE_RUN_ID="$RUN_ID" DISPLAY="$DISPLAY_USED" xterm \
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
            verify_restoration 'bounded termination' "$RESTORE_PREFIX"
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
    local normal_restore_prefix
    for command_name in xterm xdotool xdpyinfo import timeout sed awk find ps xprop od tr grep basename seq; do
        require_command "$command_name"
    done
    find_codegotchi_binary
    find_codex_binary
    select_codex_home
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

    full_pet
    state_summary after-pet
    full_feed
    state_summary after-feed
    full_clean
    state_summary after-clean
    full_nap
    state_summary after-nap
    verify_care_snapshots
    record_care_completion_unavailable
    capture_frame 'full-live-care'

    record_codex_checks
    verify_hook_trust
    verify_approval_probe
    capture_frame 'full-live-blocked'

    resize_terminal 30
    resize_terminal 21
    resize_terminal 45
    capture_frame 'full-live-final'
    normal_restore_prefix=$RESTORE_PREFIX
    normal_exit || true
    if [[ -n $normal_restore_prefix ]]; then
        verify_restoration 'normal exit' "$normal_restore_prefix"
    fi
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
