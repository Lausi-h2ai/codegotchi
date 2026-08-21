scrub_private_display_credentials() {
    local authority_path=$1
    local owned=$2

    [[ $owned == 1 && -n $authority_path ]] || return 0
    if [[ -e $authority_path ]]; then
        if ! rm -f -- "$authority_path" 2>/dev/null && [[ -e $authority_path ]]; then
            : >"$authority_path" 2>/dev/null || return 1
            chmod 600 "$authority_path" 2>/dev/null || return 1
        fi
    fi
    [[ ! -e $authority_path || ! -s $authority_path ]]
}

retained_run_root_report_line() {
    local run_root=$1

    printf '%s\n' "- BLOCKED: retained run root: \`$run_root\` (cleanup could not prove full process-tree termination)."
}
