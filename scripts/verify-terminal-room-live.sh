#!/usr/bin/env bash

set -Eeuo pipefail
IFS=$'\n\t'

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPOSITORY_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

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
  CODEGOTCHI_LIVE_TRUST_HOOKS    Set to 1 to choose Codex's disposable trust-all option

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
if [[ ${1-} == "--" ]]; then
    shift
    CODEX_ARGUMENTS=("$@")
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
DISPLAY_STARTED=0
XVFB_PID=""
WM_PID=""
XTERM_PID=""
XTERM_START=""
WINDOW_ID=""
WINDOW_TITLE="codegotchi-live-$RUN_ID"
CURRENT_ROWS=45
CURRENT_COLUMNS=120
CAPTURED_FRAMES=()
METADATA_PATH=""
API_URL=""
API_TOKEN=""
REPORT_WRITTEN=0
CLEANUP_STARTED=0
FINAL_STATUS="not-run"
REQUIRED_GATE_BLOCKED=0

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

pid_cmdline() {
    local pid=$1
    [[ -r "/proc/$pid/cmdline" ]] || return 1
    tr '\0' ' ' < "/proc/$pid/cmdline"
}

ancestor_chain_contains() {
    local target=$1
    local pid=$$
    local parent
    while [[ $pid =~ ^[0-9]+$ && $pid -gt 0 ]]; do
        [[ $pid == "$target" ]] && return 0
        [[ -r "/proc/$pid/stat" ]] || break
        parent=$(awk '{print $4}' "/proc/$pid/stat")
        [[ $parent == "$pid" || -z $parent ]] && break
        pid=$parent
    done
    return 1
}

register_created_pid() {
    local pid=$1
    local marker=$2
    local start
    start=$(pid_start_time "$pid") || fail "could not record created process $pid"
    CREATED_PIDS+=("$pid")
    CREATED_STARTS+=("$start")
    CREATED_MARKERS+=("$marker")
}

safe_stop_created_pid() {
    local index=$1
    local pid=${CREATED_PIDS[$index]}
    local expected_start=${CREATED_STARTS[$index]}
    local marker=${CREATED_MARKERS[$index]}
    local current_start
    local command_line

    [[ -n $pid ]] || return 0
    if ! kill -0 "$pid" 2>/dev/null; then
        return 0
    fi
    if ancestor_chain_contains "$pid"; then
        log "refusing to signal session ancestor PID $pid"
        return 0
    fi
    current_start=$(pid_start_time "$pid" 2>/dev/null || true)
    if [[ -z $current_start || $current_start != "$expected_start" ]]; then
        log "refusing to signal reused PID $pid"
        return 0
    fi
    command_line=$(pid_cmdline "$pid" 2>/dev/null || true)
    if [[ -z $command_line || $command_line != *"$marker"* ]]; then
        log "refusing to signal unverified PID $pid"
        return 0
    fi

    log "cleanup candidate: $(ps -o pid=,ppid=,stat=,etime=,cmd= -p "$pid" 2>/dev/null || true)"
    kill -TERM "$pid" 2>/dev/null || true
    for _ in {1..20}; do
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.1
    done
    log "cleanup candidate still alive: $(ps -o pid=,ppid=,stat=,etime=,cmd= -p "$pid" 2>/dev/null || true)"
    current_start=$(pid_start_time "$pid" 2>/dev/null || true)
    command_line=$(pid_cmdline "$pid" 2>/dev/null || true)
    if [[ $current_start == "$expected_start" && $command_line == *"$marker"* ]] && ! ancestor_chain_contains "$pid"; then
        kill -KILL "$pid" 2>/dev/null || true
    fi
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
        printf '%s\n' '- Temporary XDG state/runtime/data/config/cache/home paths are removed by the harness cleanup.'
        printf '%s\n' '- Only PIDs recorded from this run are considered for cleanup; no broad Codex/process matching is used.'
    } > "$REPORT_PATH"
}

cleanup() {
    local exit_status=$?
    [[ $CLEANUP_STARTED == 1 ]] && return "$exit_status"
    CLEANUP_STARTED=1
    trap - EXIT INT TERM HUP

    local index
    for ((index = ${#CREATED_PIDS[@]} - 1; index >= 0; index--)); do
        safe_stop_created_pid "$index"
    done
    write_report "$exit_status"
    rm -rf -- "$RUN_ROOT"
    exit "$exit_status"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

is_display_usable() {
    local candidate=$1
    [[ -n $candidate ]] || return 1
    DISPLAY="$candidate" xdpyinfo >/dev/null 2>&1 || return 1
    DISPLAY="$candidate" xdotool getdisplaygeometry >/dev/null 2>&1 || return 1
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
    wm_binary=$(choose_window_manager) || fail "no lightweight window manager is installed; private Xvfb acceptance requires openbox, fluxbox, xfwm4, icewm, matchbox-window-manager, jwm, or twm"
    wm_name=$(basename "$wm_binary")

    for display_number in $(seq 90 199); do
        candidate=":$display_number"
        if is_display_usable "$candidate"; then
            continue
        fi
        DISPLAY="$candidate" Xvfb "$candidate" -screen 0 1920x1200x24 -ac -nolisten tcp -nolisten unix >"$RUN_ROOT/xvfb.log" 2>&1 &
        XVFB_PID=$!
        register_created_pid "$XVFB_PID" "Xvfb $candidate"
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
        DISPLAY="$candidate" "$wm_binary" "${wm_arguments[@]}" >"$RUN_ROOT/window-manager.log" 2>&1 &
        WM_PID=$!
        register_created_pid "$WM_PID" "$wm_binary"
        sleep 0.5
        if ! kill -0 "$WM_PID" 2>/dev/null; then
            fail "window manager '$wm_name' exited on private Xvfb $candidate; xdotool activation cannot be trusted"
        fi
        DISPLAY_USED=$candidate
        DISPLAY_STARTED=1
        log "using private display with window manager '$wm_name'"
        return 0
    done
    fail 'could not allocate a private X display in the bounded range :90..:199'
}

select_display() {
    if [[ -n ${DISPLAY-} ]] && is_display_usable "$DISPLAY"; then
        DISPLAY_USED=$DISPLAY
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

window_exists() {
    [[ -n $WINDOW_ID ]] || return 1
    DISPLAY="$DISPLAY_USED" xdotool getwindowname "$WINDOW_ID" >/dev/null 2>&1
}

active_window_is_target() {
    local active
    active=$(DISPLAY="$DISPLAY_USED" xdotool getactivewindow 2>/dev/null || true)
    [[ $active == "$WINDOW_ID" ]]
}

activate_window() {
    local attempt
    for attempt in {1..5}; do
        if DISPLAY="$DISPLAY_USED" xdotool windowmap "$WINDOW_ID" >/dev/null 2>&1 && \
            DISPLAY="$DISPLAY_USED" xdotool windowactivate --sync "$WINDOW_ID" >/dev/null 2>&1 && \
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
        WINDOW_ID=$(DISPLAY="$DISPLAY_USED" xdotool search --onlyvisible --name "$WINDOW_TITLE" 2>/dev/null | sed -n '1p' || true)
        if [[ -n $WINDOW_ID ]] && window_exists; then
            register_created_pid "$XTERM_PID" "-title $WINDOW_TITLE"
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
    DISPLAY="$DISPLAY_USED" xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null \
        | sed -n "s/^${key}=//p" | sed -n '1p'
}

capture_frame() {
    local label=$1
    local path="$OUTPUT_DIR/$RUN_ID-$label.png"
    if ! timeout 10s env DISPLAY="$DISPLAY_USED" import -silent -window "$WINDOW_ID" "$path" >/dev/null 2>&1; then
        fail "ImageMagick import could not capture the live $label frame"
    fi
    [[ -s $path ]] || fail "the live $label capture is empty"
    CAPTURED_FRAMES+=("$path")
    record "capture $label" "saved (terminal geometry ${CURRENT_COLUMNS}x${CURRENT_ROWS})"
}

assert_window_usable() {
    window_exists || fail 'Codex/xterm exited during the bounded interaction sequence'
    activate_window
}

send_key() {
    DISPLAY="$DISPLAY_USED" xdotool key --window "$WINDOW_ID" --clearmodifiers "$1" >/dev/null 2>&1 || fail "xdotool could not send key $1"
}

send_text() {
    DISPLAY="$DISPLAY_USED" xdotool type --window "$WINDOW_ID" --delay 12 -- "$1" >/dev/null 2>&1 || fail 'xdotool could not send prompt text'
}

resize_terminal() {
    local rows=$1
    DISPLAY="$DISPLAY_USED" xdotool windowsize --sync --usehints "$WINDOW_ID" "$CURRENT_COLUMNS" "$rows" >/dev/null 2>&1 || fail "xdotool could not resize the terminal to ${CURRENT_COLUMNS}x${rows}"
    CURRENT_ROWS=$rows
    sleep 0.8
    assert_window_usable
    capture_frame "${CURRENT_COLUMNS}x${CURRENT_ROWS}"
    record "resize ${CURRENT_COLUMNS}x${CURRENT_ROWS}" 'xterm remained active and the production session remained alive'
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
    DISPLAY="$DISPLAY_USED" xdotool mousemove --window "$WINDOW_ID" --sync "$pixel_x" "$pixel_y" >/dev/null 2>&1 || fail 'xdotool could not move the pointer into the terminal room'
}

cell_click() {
    local column=$1
    local row=$2
    cell_move "$column" "$row"
    DISPLAY="$DISPLAY_USED" xdotool click --window "$WINDOW_ID" 1 >/dev/null 2>&1 || fail 'xdotool could not click a terminal-room target'
}

full_pet() {
    local column
    cell_move 86 38
    DISPLAY="$DISPLAY_USED" xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the Full pet gesture'
    for column in 80 84 88 92 95; do
        cell_move "$column" 38
        sleep 0.35
    done
    DISPLAY="$DISPLAY_USED" xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the Full pet gesture'
    sleep 0.8
    assert_window_usable
    record 'qualifying pet stroke' 'attempted with >1,500 ms hold and >120 backend-distance cell path'
}

full_feed() {
    local column
    cell_move 6 41
    DISPLAY="$DISPLAY_USED" xdotool mousedown 1 >/dev/null 2>&1 || fail 'could not start the stocked-food drag'
    for column in 20 40 60 80 87; do
        cell_move "$column" 40
        sleep 0.12
    done
    cell_move 87 38
    DISPLAY="$DISPLAY_USED" xdotool mouseup 1 >/dev/null 2>&1 || fail 'could not release the stocked-food drag'
    sleep 0.8
    assert_window_usable
    record 'stocked food drag-to-pet' 'attempted from the initial Full kibble source to the pet hit region'
}

full_clean() {
    cell_click 62 41
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
    [[ -n $API_URL && -n $API_TOKEN ]] || return 0
    if ! curl --silent --show-error --fail --max-time 3 -H "Authorization: Bearer $API_TOKEN" "$API_URL/api/v1/state" -o "$state_path" >/dev/null 2>&1; then
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
const count = (key) => inventory[key] ?? inventory[key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)] ?? 0;
process.stdout.write(`poops=${Array.isArray(poops) ? poops.length : 0} demands=${Array.isArray(demands) ? demands.length : 0} kibble=${count('kibble')} treat=${count('treat')} fruit=${count('fruit')} energy=${count('energyDrink')} napping=${napping ? 'active' : 'inactive'}\n`);
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
    local prepared_demands
    local after_pet_demands

    prepared_kibble=$(state_value prepared kibble)
    after_feed_kibble=$(state_value after-feed kibble)
    prepared_poops=$(state_value prepared poops)
    after_clean_poops=$(state_value after-clean poops)
    after_nap=$(state_value after-nap napping)
    prepared_demands=$(state_value prepared demands)
    after_pet_demands=$(state_value after-pet demands)

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
    if [[ $prepared_demands =~ ^[0-9]+$ && $after_pet_demands =~ ^[0-9]+$ && $after_pet_demands -lt $prepared_demands ]]; then
        record 'authoritative pet result' 'verified by a settled affection-demand decrement'
    else
        record 'authoritative pet result' 'not verified (the isolated run had no observable affection demand)'
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
    local -a environment_values
    environment_values=(
        "HOME=$TEMP_HOME"
        "XDG_CONFIG_HOME=$CONFIG_HOME"
        "XDG_CACHE_HOME=$CACHE_HOME"
        "XDG_DATA_HOME=$DATA_HOME"
        "XDG_STATE_HOME=$STATE_HOME"
        "XDG_RUNTIME_DIR=$RUNTIME_HOME"
        "CODEX_HOME=$CODEX_HOME_VALUE"
        "CODEGOTCHI_BROWSER=none"
        "CODEGOTCHI_ENABLE_DEBUG=1"
        "CODEGOTCHI_REAL_CODEX=$CODEX_EXECUTABLE"
        "TERM=xterm-256color"
    )
    DISPLAY="$DISPLAY_USED" xterm \
        -title "$WINDOW_TITLE" \
        -geometry 120x45 \
        -e env "${environment_values[@]}" "$CODEGOTCHI_EXECUTABLE" run \
        --ui terminal --terminal-theme auto -- codex "${CODEX_ARGUMENTS[@]}" \
        >"$RUN_ROOT/xterm.log" 2>&1 &
    XTERM_PID=$!
    XTERM_START=$(pid_start_time "$XTERM_PID") || fail 'could not record the run-owned xterm process'
    register_created_pid "$XTERM_PID" "-title $WINDOW_TITLE"
}

termination_case() {
    WINDOW_TITLE="codegotchi-live-terminate-$RUN_ID"
    WINDOW_ID=""
    XTERM_PID=""
    start_xterm_session
    wait_for_window
    sleep 1
    local child_pid
    child_pid=$(pgrep -P "$XTERM_PID" | sed -n '1p' || true)
    if [[ -n $child_pid ]]; then
        local child_command
        child_command=$(pid_cmdline "$child_pid" 2>/dev/null || true)
        if [[ $child_command == *"$CODEGOTCHI_EXECUTABLE"* ]]; then
            register_created_pid "$child_pid" "$CODEGOTCHI_EXECUTABLE"
            safe_stop_created_pid "$(( ${#CREATED_PIDS[@]} - 1 ))"
            record 'bounded termination case' 'SIGTERM sent only to the run-owned CodeGotchi child'
        else
            record 'bounded termination case' 'not available (run-owned xterm child was not the expected CodeGotchi executable)'
        fi
    else
        record 'bounded termination case' 'not available (run-owned xterm child was not observable)'
    fi
    for _ in {1..20}; do
        if ! kill -0 "$XTERM_PID" 2>/dev/null; then
            XTERM_PID=""
            return 0
        fi
        sleep 0.1
    done
    record 'bounded termination restoration' 'xterm remained alive after the bounded signal window'
    WINDOW_ID=""
}

main() {
    local command_name
    for command_name in xterm xdotool import timeout sed awk find ps; do
        require_command "$command_name"
    done
    find_codegotchi_binary
    find_codex_binary
    select_codex_home
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

    if [[ ${CODEGOTCHI_LIVE_TRUST_HOOKS:-0} == 1 ]]; then
        send_key 2
        send_key Return
        sleep 2
        record 'Codex hook trust' 'selected the disposable trust-all option by explicit operator opt-in'
    else
        record 'Codex hook trust' 'not attempted; set CODEGOTCHI_LIVE_TRUST_HOOKS=1 only for a disposable authorized session'
        block_required_gate
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

    send_text 'Reply with the single word READY and do not use tools.'
    send_key ctrl+a
    send_key End
    send_key Left
    send_key Right
    send_key Return
    sleep 3
    assert_window_usable
    record 'ordinary prompt entry and editing/navigation' 'sent without reading Codex screen text'

    if command -v xclip >/dev/null 2>&1 || command -v xsel >/dev/null 2>&1; then
        record 'bracketed multiline paste' 'clipboard tool is available; paste probe requires operator review of the negotiated Codex mode'
    else
        record 'bracketed multiline paste' 'not available (xclip/xsel is not installed; no clipboard state was changed)'
        block_required_gate
    fi

    DISPLAY="$DISPLAY_USED" xdotool windowminimize "$WINDOW_ID" >/dev/null 2>&1 || true
    sleep 0.5
    activate_window
    record 'focus out/in' 'window-level focus transition observed; Codex focus reporting is not text-scraped'

    cell_move 60 10
    DISPLAY="$DISPLAY_USED" xdotool click --window "$WINDOW_ID" 4 >/dev/null 2>&1 || true
    DISPLAY="$DISPLAY_USED" xdotool click --window "$WINDOW_ID" 1 >/dev/null 2>&1 || true
    assert_window_usable
    record 'Codex scroll/click behavior' 'pointer probe sent where current Codex negotiated mouse reporting'

    send_text 'Run pwd with the shell tool, then reply TOOL_DONE.'
    send_key Return
    sleep 5
    assert_window_usable
    record 'model response and tool activity' 'bounded prompt sent; window remained active without screen scraping'
    record 'approval/review interaction' 'not available in this safe default invocation (--ask-for-approval never)'
    block_required_gate
    capture_frame 'full-live-populated'

    resize_terminal 30
    resize_terminal 21
    resize_terminal 45
    capture_frame 'full-live-final'
    normal_exit || true
    termination_case

    if [[ $REQUIRED_GATE_BLOCKED == 1 ]]; then
        FINAL_STATUS='BLOCKED'
        record 'live acceptance overall' 'BLOCKED; one or more required checks remain unavailable or unverified'
    else
        FINAL_STATUS='PASS'
        record 'live acceptance overall' 'PASS'
    fi
}

main "$@"
