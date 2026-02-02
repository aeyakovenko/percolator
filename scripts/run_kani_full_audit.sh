#!/usr/bin/env bash
# Full Kani audit: run all proofs one-by-one with 10-minute timeout each
set -euo pipefail
cd /home/anatoly/percolator

OUTFILE="/home/anatoly/percolator/kani_audit_full.tsv"
echo -e "proof\ttime_s\tstatus" > "$OUTFILE"

PROOFS=$(grep -A5 'kani::proof' tests/kani.rs | grep '^\s*fn ' | sed 's/.*fn \([a-z_0-9]*\).*/\1/' | sort)

TOTAL=$(echo "$PROOFS" | wc -l)
COUNT=0
PASS=0
FAIL=0

for proof in $PROOFS; do
    COUNT=$((COUNT + 1))
    echo "[$COUNT/$TOTAL] Running: $proof"
    START=$(date +%s)

    if timeout 600 cargo kani --harness "$proof" --output-format terse 2>&1 | tail -3; then
        STATUS="PASS"
        PASS=$((PASS + 1))
    else
        EXIT_CODE=$?
        if [ $EXIT_CODE -eq 124 ]; then
            STATUS="TIMEOUT"
        else
            STATUS="FAIL"
        fi
        FAIL=$((FAIL + 1))
    fi

    END=$(date +%s)
    ELAPSED=$((END - START))
    echo -e "${proof}\t${ELAPSED}\t${STATUS}" >> "$OUTFILE"
    echo "  -> $STATUS (${ELAPSED}s)"
done

echo ""
echo "========================================="
echo "SUMMARY: $PASS passed, $FAIL failed/timeout out of $TOTAL"
echo "Results saved to: $OUTFILE"
echo "========================================="
