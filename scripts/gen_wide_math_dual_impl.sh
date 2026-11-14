#!/usr/bin/env bash
# Regenerates tests/fixtures_wide_math_{kani,host}_impl.rs from
# src/wide_math.rs for tests/wide_math_differential_conformance.rs.
# Rerun after any edit to src/wide_math.rs; the sync test fails on drift.
set -euo pipefail
cd "$(dirname "$0")/.."

sed -e 's/#\[cfg(kani)\]/#[cfg(all())]/g' \
    -e 's/#\[cfg(not(kani))\]/#[cfg(any())]/g' \
    src/wide_math.rs > tests/fixtures_wide_math_kani_impl.rs

sed -e 's/#\[cfg(kani)\]/#[cfg(any())]/g' \
    -e 's/#\[cfg(not(kani))\]/#[cfg(all())]/g' \
    src/wide_math.rs > tests/fixtures_wide_math_host_impl.rs

echo "regenerated tests/fixtures_wide_math_{kani,host}_impl.rs"
