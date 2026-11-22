#!/usr/bin/env bash
# stage_rev.sh <engine-worktree> <rev> <base-rev>
# Put engine revision <rev> into the Aristotle project directory (./engine): every file that differs
# from <base-rev>, the revision the project directory holds now, plus the specs generated for <rev>
# (specs/*_<rev8>.md must exist: see ROLL_TO_<rev8>.md). Prints the list of staged files; submits nothing.
set -euo pipefail
cd "$(dirname "$0")"
WT=$1; REV=$2; BASE=$3
short=$(git -C "$WT" rev-parse --short=8 "$REV")
bshort=$(git -C "$WT" rev-parse --short=8 "$BASE")
[ "$(git -C "$WT" show "$BASE:src/v16.rs" | sha256sum | cut -c1-64)" = "$(sha256sum engine/src/v16.rs | cut -c1-64)" ] \
  || [ "$(git -C "$WT" show "$REV:src/v16.rs" | sha256sum | cut -c1-64)" = "$(sha256sum engine/src/v16.rs | cut -c1-64)" ] \
  || { echo "engine/src/v16.rs is neither $bshort nor $short" >&2; exit 4; }
files=()
while read -r f; do
  mkdir -p "engine/$(dirname "$f")"
  git -C "$WT" show "$REV:$f" > "engine/$f"
  files+=("engine/$f")
done < <(git -C "$WT" diff --name-only "$BASE" "$REV")
# SPECS overrides the specs staged (default: all four), e.g. when a generator is not updated for <rev>
for s in ${SPECS:-kernel_contracts_spec transition_shape_spec header_write_sets wide_math_callsites_spec}; do
  [ -s "specs/${s}_$short.md" ] || { echo "specs/${s}_$short.md is missing" >&2; exit 4; }
  cp "specs/${s}_$short.md" "engine/aristotle/$s.md"
done
# the project tree must now be exactly <rev> for every tracked source file
for f in src/v16.rs src/v16_kani_api.rs src/v16_proofs.rs src/wide_math.rs src/lib.rs Cargo.toml spec.md; do
  [ "$(git -C "$WT" show "$REV:$f" | sha256sum | cut -c1-64)" = "$(sha256sum "engine/$f" | cut -c1-64)" ] || { echo "engine/$f is not $short" >&2; exit 5; }
done
echo "staged $short over $bshort: ${#files[@]} changed files and the specs ${SPECS:-(all four)}"
printf '%s\n' "${files[@]}" > "staged_files_$short.txt"
echo "changed files listed in staged_files_$short.txt"
