#!/usr/bin/env bash
#
# Run E2E tests on Surfpool localnet
#
# This script runs the E2E test scenarios from TEST_PLAN.md
#

set -e

cd "$(dirname "$0")/.."

echo "========================================="
echo "Percolator v0 E2E Tests"
echo "========================================="
echo

# Check if programs are deployed
if [ ! -f "deployed_programs.json" ]; then
    echo "Error: Programs not deployed"
    echo "Run: ./scripts/deploy.sh"
    exit 1
fi

# Load program IDs
SLAB_ID=$(jq -r '.programs.slab' deployed_programs.json)
ROUTER_ID=$(jq -r '.programs.router' deployed_programs.json)
ORACLE_ID=$(jq -r '.programs.oracle' deployed_programs.json)

echo "Program IDs:"
echo "  Slab:   $SLAB_ID"
echo "  Router: $ROUTER_ID"
echo "  Oracle: $ORACLE_ID"
echo

# Run tests
echo "Running E2E-1: Atomic Multi-Slab Buy..."
echo "  [TODO: Implement test runner]"
echo

echo "Running E2E-2: Capital Efficiency Proof..."
echo "  [TODO: Implement test runner]"
echo

echo "========================================="
echo "E2E Tests Complete"
echo "========================================="
echo
echo "See TEST_PLAN.md for full test specifications"
echo
