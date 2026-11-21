#!/usr/bin/env bash
# Run from any directory. With --mir, use an existing pinned textual MIR dump.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d)
echo "T28.header work directory: $work"
if [[ $# == 0 ]]; then
    mir="$work/percolator.mir"
    (cd "$root" && cargo +1.95.0 rustc --locked --lib --jobs 1 -- --emit="mir=$mir")
elif [[ $# == 2 && $1 == --mir ]]; then
    mir=$2
else
    echo "usage: $0 [--mir path/to/percolator.mir]" >&2
    exit 2
fi
python3 -IB "$here/extract_mir.py" --mir "$mir" --rust-source "$root/src/v16.rs" \
    --output "$work/DepositGraph.lean"
cp "$here/MirContracts.lean" "$here/Deposit.lean" "$work/"
cd "$work"
export LEAN_PATH="$work"
for module in MirContracts DepositGraph Deposit; do
    lean +leanprover/lean4:v4.33.1 -j 1 -M 1024 -DwarningAsError=true \
        -o "$module.olean" "$module.lean"
done
