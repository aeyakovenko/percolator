#!/usr/bin/env bash
#
# Build router program for Solana BPF
#

set -e

cd "$(dirname "$0")"

echo "Building router program for BPF..."

# Build the BPF program
cargo build-sbf

echo "Router BPF build complete!"
echo "Binary: ../../target/deploy/percolator_router.so"
