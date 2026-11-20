#!/usr/bin/env bash
# watch_run.sh <project-id> <task-id> <max-credits-for-this-run> <log>
# Polls an Aristotle run until it ends. Cancels the task if it has spent more than the cap.
set -u
P=$1; T=$2; CAP=$3; LOG=$4
export ARISTOTLE_API_KEY="${ARISTOTLE_API_KEY:-$(tr -d '\r\n' < ~/.aristotle)}"
A=$(command -v aristotle || echo ~/.local/bin/aristotle)
bal() { $A balance --json 2>/dev/null | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print(d["balance"]["credits"]/d["balance"]["credit_display_config"]["small_credits_per_credit"])
except Exception: print("nan")'; }
START=$(bal); echo "$(date -u +%H:%M:%S) start balance $START credits, cap $CAP" >> "$LOG"
while true; do
  S=$($A show "$P" --task "$T" --json 2>/dev/null | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); t=d.get("task") or {}; c=d.get("property_counts") or {}
    print(t.get("status","?"), c.get("proved",0), c.get("refuted",0), c.get("in_progress",0), c.get("unresolved",0))
except Exception: print("? 0 0 0 0")')
  B=$(bal)
  SPENT=$(python3 -c "print(round($START-$B,2) if '$B'!='nan' and '$START'!='nan' else 'nan')")
  echo "$(date -u +%H:%M:%S) status/proved/refuted/in_progress/unresolved: $S | balance $B | spent $SPENT" >> "$LOG"
  case "$S" in
    COMPLETE*|COMPLETE_WITH_ERRORS*|OUT_OF_BUDGET*|FAILED*|CANCELED*) echo "run ended: $S, spent $SPENT credits" >> "$LOG"; break;;
  esac
  if python3 -c "import sys; sys.exit(0 if ('$SPENT'!='nan' and float('$SPENT')>float('$CAP')) else 1)"; then
    echo "$(date -u +%H:%M:%S) cap exceeded: cancelling task $T" >> "$LOG"; $A cancel "$T" >> "$LOG" 2>&1; sleep 20
  fi
  sleep ${WATCH_INTERVAL:-120}
done
tail -3 "$LOG"
