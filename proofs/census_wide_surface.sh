#!/usr/bin/env bash
# census_wide_surface.sh <engine-git-dir-or-worktree> <rev>
# Compiler-checked census of what src/v16.rs uses from src/wide_math.rs in a default build.
# A scratch copy of <rev> is built with src/v16.rs importing from census/wide_shim.rs instead of
# wide_math. The shim exposes exactly: five free functions and, of U256, the constant ONE, from_u128,
# from_u64, is_zero, try_into_u128, checked_add/sub/mul/div/rem, equality and ordering. If the build
# succeeds, src/v16.rs reaches nothing else of wide_math (no operator, no I256, no panicking helper).
# Writes specs/wide_surface_census_<rev8>.txt.
set -euo pipefail
cd "$(dirname "$0")"
WT=$1; REV=$2
short=$(git -C "$WT" rev-parse --short=8 "$REV")
tmp=$(mktemp -d)
git -C "$WT" archive "$REV" | tar -x -C "$tmp"
cp census/wide_shim.rs "$tmp/src/wide_shim.rs"
python3 - "$tmp" <<'PY'
import sys
t = sys.argv[1]
p = t + "/src/lib.rs"; s = open(p).read()
a = "#[cfg(not(kani))]\nmod wide_math;\n"
assert s.count(a) == 1, "lib.rs layout changed"
open(p, "w").write(s.replace(a, a + "mod wide_shim;\n"))
p = t + "/src/v16.rs"; s = open(p).read()
a = "use crate::wide_math::{"
assert s.count(a) == 1 and s.count("wide_math") == 1, "v16.rs names wide_math in more than its one use line"
open(p, "w").write(s.replace(a, "use crate::wide_shim::{"))
PY
out="specs/wide_surface_census_$short.txt"
if (cd "$tmp" && CARGO_TARGET_DIR="$tmp/target" cargo check --lib >"$tmp/check.log" 2>&1); then
  { echo "OK $short"
    echo "src/v16.rs sha256 $(git -C "$WT" show "$REV:src/v16.rs" | sha256sum | cut -d' ' -f1)"
    echo "src/wide_math.rs sha256 $(git -C "$WT" show "$REV:src/wide_math.rs" | sha256sum | cut -d' ' -f1)"
    echo "shim sha256 $(sha256sum census/wide_shim.rs | cut -d' ' -f1)"
    echo "build: cargo check --lib, default features, $(cargo --version)"
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$out"
  echo "census OK -> $out"
else
  grep -E "^error" -A6 "$tmp/check.log" | head -40
  echo "census FAILED: src/v16.rs uses something the shim does not expose" >&2
  exit 1
fi
rm -rf "$tmp"
