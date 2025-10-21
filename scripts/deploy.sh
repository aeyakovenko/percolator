#!/usr/bin/env bash
#
# Deploy Percolator programs to Surfpool localnet
#

set -e

cd "$(dirname "$0")/.."

echo "========================================="
echo "Deploying Percolator to Surfpool"
echo "========================================="
echo

# Check if solana CLI is available
if ! command -v solana &> /dev/null; then
    echo "Error: solana CLI not found"
    echo "Please install Solana CLI: https://docs.solana.com/cli/install-solana-cli-tools"
    exit 1
fi

# Check if programs are built
if [ ! -f "target/deploy/percolator_slab.so" ]; then
    echo "Error: Programs not built"
    echo "Run: make build-bpf"
    exit 1
fi

# Set cluster to localnet
solana config set --url http://localhost:8899

echo "Deploying programs..."
echo

# Deploy slab program
echo ">>> Deploying slab program..."
SLAB_PROGRAM_ID=$(solana program deploy target/deploy/percolator_slab.so | grep "Program Id:" | awk '{print $3}')
echo "Slab Program ID: $SLAB_PROGRAM_ID"
echo

# Deploy router program
echo ">>> Deploying router program..."
ROUTER_PROGRAM_ID=$(solana program deploy target/deploy/percolator_router.so | grep "Program Id:" | awk '{print $3}')
echo "Router Program ID: $ROUTER_PROGRAM_ID"
echo

# Deploy oracle program
echo ">>> Deploying oracle program..."
ORACLE_PROGRAM_ID=$(solana program deploy target/deploy/percolator_oracle.so | grep "Program Id:" | awk '{print $3}')
echo "Oracle Program ID: $ORACLE_PROGRAM_ID"
echo

# Save program IDs
cat > deployed_programs.json <<EOF
{
  "network": "localnet",
  "deployed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "programs": {
    "slab": "$SLAB_PROGRAM_ID",
    "router": "$ROUTER_PROGRAM_ID",
    "oracle": "$ORACLE_PROGRAM_ID"
  }
}
EOF

echo "========================================="
echo "Deployment complete!"
echo "========================================="
echo
echo "Program IDs saved to: deployed_programs.json"
cat deployed_programs.json
echo
