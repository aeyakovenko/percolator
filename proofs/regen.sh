#!/usr/bin/env bash
# regen.sh [--online]: regenerate the ledger and the roadmap. Each output is written to a temporary
# file and moved into place only if its generator succeeded (a plain `> ROADMAP.md` truncates the
# file before a failing generator has written anything).
set -euo pipefail
cd "$(dirname "$0")"
REV=$(python3 -c 'import re;print(re.search(r"LANDED_REV\s*=\s*\"([0-9a-f]+)\"", open("make_ledger.py").read()).group(1))')
if [ "${1:-}" = "--online" ]; then python3 make_ledger.py | tail -14; else python3 make_ledger.py --offline | tail -14; fi
# a copy per revision, only where they are kept (not in the engine repository)
if ls proof_ledger.*.tsv >/dev/null 2>&1; then cp proof_ledger.md "proof_ledger.$REV.md"; cp proof_ledger.tsv "proof_ledger.$REV.tsv"; fi
python3 make_roadmap.py > ROADMAP.md.new && mv ROADMAP.md.new ROADMAP.md
sed -n '3,4p' ROADMAP.md
