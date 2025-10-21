#!/usr/bin/env bash
#
# Build oracle program for Solana BPF
#

set -e

cd "$(dirname "$0")"

echo "Building oracle program for BPF..."

# Build the BPF program
cargo build-sbf

echo "Oracle BPF build complete!"
echo "Binary: ../../target/deploy/percolator_oracle.so"
