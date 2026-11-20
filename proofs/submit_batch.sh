#!/usr/bin/env bash
# submit_batch.sh <name> <prompt-file> [file-under-./engine ...]
# Start the next Aristotle task on the project (only when no task is running: `continue` hands
# the prompt to a running task). Prints the task id; the answer is kept in continue_<name>.json.
# Then start the cap watcher:  WATCH_INTERVAL=120 ./watch_run.sh <project> <task> <cap> run_<name>.log
set -euo pipefail
cd "$(dirname "$0")"
NAME=$1; PROMPT=$2; shift 2
# The project id is account data: ARISTOTLE_PROJECT, or the file .aristotle_project next to this script.
P=${ARISTOTLE_PROJECT:-$(cat .aristotle_project 2>/dev/null || true)}
[ -n "$P" ] || { echo "set ARISTOTLE_PROJECT (or write the id to .aristotle_project)" >&2; exit 2; }
export ARISTOTLE_API_KEY="${ARISTOTLE_API_KEY:-$(tr -d '\r\n' < ~/.aristotle)}"
A=$(command -v aristotle || echo ~/.local/bin/aristotle)
running=$($A show "$P" --json 2>/dev/null | python3 -c 'import json,sys
try:
    d=json.load(sys.stdin); print((d.get("project") or {}).get("status_name") or "?")
except Exception: print("?")')
if [ "$running" = "RUNNING" ]; then echo "project still RUNNING: not submitting" >&2; exit 3; fi
args=(continue "$P" --prompt-file "$PROMPT" --json --no-questions)
if [ "$#" -gt 0 ]; then args+=(--files "$@" --project-dir ./engine); fi
"$A" "${args[@]}" > "continue_$NAME.json" 2> "continue_$NAME.err"
python3 -c "import json;print(json.load(open('continue_$NAME.json'))['task']['agent_task_id'])"
