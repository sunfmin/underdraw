#!/bin/sh
# The gate: what must pass before anything lands, the same locally and in CI.
set -e
cd "$(dirname "$0")"
# The whole gate runs on rustup's nightly, the one cargo-flux is built for,
# whatever the shell that started it has on its PATH: a Homebrew `cargo`
# ahead of rustup's breaks when Homebrew moves a library under it.
PATH="$HOME/.cargo/bin:$PATH"
RUSTUP_TOOLCHAIN=nightly-2026-09-25
export PATH RUSTUP_TOOLCHAIN
cargo fmt --all --check
# Flux builds under a directory of its own, so it checks the pure crate while
# everything below runs; what it came to is read at the end.
flux_log=$(mktemp)
./verify.sh >"$flux_log" 2>&1 &
flux_pid=$!
# Flux has usually ended by then, and a kill that finds nothing fails: under
# `set -e` that would be the gate's exit, whatever passed.
trap 'kill "$flux_pid" 2>/dev/null || true; rm -f "$flux_log"' EXIT
cargo clippy --workspace --all-targets --quiet -- -D warnings
cargo nextest run --workspace --cargo-quiet --status-level fail --final-status-level fail --show-progress none
# Every function of the pure crate is checked: none is taken on trust. Flux
# says so itself in the summary that ends its run on a crate; the pure crate
# is checked last, so its summary is the last one.
wait "$flux_pid" && flux_status=0 || flux_status=$?
summary=$(grep '^summary\.' "$flux_log" | tail -1)
if [ "$flux_status" -ne 0 ]; then
    cat "$flux_log" >&2
    exit "$flux_status"
fi
case "$summary" in
*"; 0 trusted; 0 ignored."*) echo "gate: Flux: $summary" ;;
*)
    echo "gate: the pure crate must not exempt anything from Flux, and Flux reports: ${summary:-no summary}" >&2
    exit 1
    ;;
esac
