#!/bin/sh
set -eu

# The hosted PTY must answer these requests on the child input stream before
# this fixture emits its marker. A raw slave makes the byte-for-byte contract
# observable without line buffering or echo.
stty -icanon -echo min 1 time 0

read_exact() {
    expected=$1
    length=${#expected}
    actual_file=$(mktemp)
    expected_file=$(mktemp)
    trap 'rm -f "$actual_file" "$expected_file"' EXIT
    printf '%s' "$expected" >"$expected_file"
    # GNU dd's fullblock flag is important here: a PTY read may legally
    # return fewer bytes than requested even while the remaining response is
    # already queued. The Linux-only integration test must not turn that
    # short read into a false negative or wait forever for a missing reply.
    if ! timeout --foreground 5s \
        dd if=/dev/stdin bs="$length" count=1 iflag=fullblock status=none \
        >"$actual_file" 2>/dev/null; then
        printf 'FAKE_QUERY_TIMEOUT expected=' >&2
        printf '%s' "$expected" | od -An -tx1 >&2
        exit 42
    fi
    if ! cmp -s "$actual_file" "$expected_file"; then
        printf 'FAKE_QUERY_MISMATCH expected=' >&2
        printf '%s' "$expected" | od -An -tx1 >&2
        printf 'FAKE_QUERY_MISMATCH actual=' >&2
        od -An -tx1 "$actual_file" >&2
        exit 42
    fi
    rm -f "$actual_file" "$expected_file"
}

printf '\033[6n\033[c'
cpr=$(printf '\033[1;1R')
da=$(printf '\033[?1;2c')
read_exact "$cpr"
read_exact "$da"
printf 'FAKE_QUERY_ROUTE_READY\r\n'
