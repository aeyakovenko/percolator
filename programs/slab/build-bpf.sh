#!/usr/bin/env bash
#
# Build slab program for Solana BPF
#

set -e

cd "$(dirname "$0")"

echo "Building slab program for BPF..."

# Build the BPF program
cargo build-sbf

echo "Slab BPF build complete!"
echo "Binary: ../../target/deploy/percolator_slab.so"
