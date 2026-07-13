#!/usr/bin/env bash
# Isolated lib-mode Kani verification runner (contract + closure layers).
#
# Layers (select via env):
#   contracts (default): FEATURES=fuzz,contracts KANI_Z="-Z function-contracts"
#       LOG_DIR=kani_contracts CARGO_TARGET_DIR=target/contracts
#   closure:             FEATURES=fuzz,closure   KANI_Z=""
#       LOG_DIR=kani_closure   CARGO_TARGET_DIR=target/closure
#
# Why isolated: -Z function-contracts slows kani-compiler ~5x crate-wide and
# must never touch the main per-proof suite budget; the closure layer skips
# the flag entirely (plain proofs). Each layer keeps its own target dir so
# caches never thrash. The cache is warmed un-timed (--only-codegen) because
# a cold compile alone can exceed any per-check budget.
#
# Usage: [env overrides] bash scripts/contracts_runner.sh [BUDGET_S]
#        (reads $LOG_DIR/proofs.txt; results appended to $LOG_DIR/results.tsv)
set -uo pipefail
cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/contracts}"
FEATURES="${FEATURES:-fuzz,contracts}"
KANI_Z="${KANI_Z--Z function-contracts}"
LOG_DIR="${LOG_DIR:-kani_contracts}"
BUDGET_S="${1:-1800}"
mkdir -p "$LOG_DIR"
RESULT="$LOG_DIR/results.tsv"
[ -f "$RESULT" ] || echo -e "harness\twall_s\tstatus" > "$RESULT"

cleanup() {
    pkill -9 -x cbmc 2>/dev/null
    pkill -9 -x kani-driver 2>/dev/null
}

echo "[$(date +%H:%M:%S)] warming $CARGO_TARGET_DIR cache (un-timed)..."
# shellcheck disable=SC2086
cargo kani $KANI_Z --features "$FEATURES" --only-codegen \
    > "$LOG_DIR/warmup.log" 2>&1 || true
echo "[$(date +%H:%M:%S)] cache warm."

mapfile -t HARNESSES < "$LOG_DIR/proofs.txt"
for h in "${HARNESSES[@]}"; do
    [ -z "$h" ] && continue
    if cut -f1 "$RESULT" | grep -qxF "$h"; then
        echo "[$(date +%H:%M:%S)] $h -> SKIP (done)"; continue
    fi
    cleanup; sleep 1
    logf="$LOG_DIR/${h}.log"
    start=$(date +%s)
    reach_args=()
    # Explicit economic-state covers replace Kani's costly per-assertion reach report.
    [[ "$h" == *_explicit_coverage ]] && reach_args=(--no-assertion-reach-checks)
    # shellcheck disable=SC2086
    if timeout --kill-after=30 "$BUDGET_S" cargo kani $KANI_Z "${reach_args[@]}" \
        --features "$FEATURES" --harness "$h" --output-format terse \
        > "$logf" 2>&1; then
        status="PASS"
    else
        ec=$?
        if [ $ec -eq 124 ] || [ $ec -eq 137 ]; then status="TIMEOUT"; else status="FAIL($ec)"; fi
    fi
    wall=$(( $(date +%s) - start ))
    printf "%s\t%s\t%s\n" "$h" "$wall" "$status" >> "$RESULT"
    printf "[%s] %s -> %s (%ss)\n" "$(date +%H:%M:%S)" "$h" "$status" "$wall"
    cleanup
done
echo "====="; column -t -s$'\t' "$RESULT"
