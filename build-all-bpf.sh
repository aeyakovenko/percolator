#!/usr/bin/env bash
#
# Build all Percolator programs for Solana BPF
#

set -e

cd "$(dirname "$0")"

echo "========================================="
echo "Building all Percolator BPF programs"
echo "========================================="
echo

# Build slab program
echo ">>> Building slab program..."
cd programs/slab
./build-bpf.sh
cd ../..
echo

# Build router program
echo ">>> Building router program..."
cd programs/router
./build-bpf.sh
cd ../..
echo

# Build oracle program
echo ">>> Building oracle program..."
cd programs/oracle
./build-bpf.sh
cd ../..
echo

echo "========================================="
echo "All builds complete!"
echo "========================================="
echo
echo "Binaries located in: ./target/deploy/"
ls -lh target/deploy/*.so 2>/dev/null || echo "  (no .so files found yet - run cargo build-sbf first)"
echo
