# Percolator CLI

Command-line interface for deploying, testing, and interacting with the Percolator perpetual exchange protocol on Solana.

## Installation

```bash
# Build the CLI
cargo build --release -p percolator-cli

# Install globally (optional)
cargo install --path cli

# Or run directly
cargo run -p percolator-cli -- --help
```

## Quick Start

```bash
# Set up Solana CLI and create a keypair
solana-keygen new

# Start local validator (optional)
solana-test-validator

# Deploy programs to localnet
percolator deploy --all

# Initialize a new exchange
percolator init --name "Percolator DEX" \
  --insurance-fund 1000000000 \
  --maintenance-margin 500

# Test crisis haircut calculation (uses formally verified model)
percolator crisis test-haircut 1000000 500000 300000 5000000
```

## Commands

### Program Deployment

```bash
# Deploy all programs
percolator deploy --all

# Deploy specific programs
percolator deploy --router --slab

# Deploy to devnet
percolator --network devnet deploy --all
```

### Exchange Management

```bash
# Initialize new exchange
percolator init \
  --name "My Exchange" \
  --insurance-fund 1000000000 \
  --maintenance-margin 500 \
  --initial-margin 1000

# Show exchange status
percolator status <EXCHANGE_PUBKEY>
```

### Matcher/Slab Operations

```bash
# Create a new matcher
percolator matcher create \
  --exchange <EXCHANGE_PUBKEY> \
  --symbol "BTC-USD" \
  --tick-size 100 \
  --lot-size 1000

# List all matchers
percolator matcher list --exchange <EXCHANGE_PUBKEY>

# Show matcher details
percolator matcher info <MATCHER_PUBKEY>
```

### Trading

```bash
# Place a limit order
percolator trade limit \
  --matcher <MATCHER_PUBKEY> \
  --side buy \
  --price 50000.0 \
  --size 1000000

# Place a market order
percolator trade market \
  --matcher <MATCHER_PUBKEY> \
  --side sell \
  --size 1000000

# View order book
percolator trade book <MATCHER_PUBKEY> --depth 20

# List your open orders
percolator trade orders

# Cancel an order
percolator trade cancel <ORDER_ID>
```

### Margin & Collateral

```bash
# Deposit collateral
percolator margin deposit --amount 1000000000

# Withdraw collateral
percolator margin withdraw --amount 500000000

# View margin account
percolator margin show

# Check margin requirements
percolator margin requirements <USER_PUBKEY>
```

### Liquidity Provision

```bash
# Add liquidity
percolator liquidity add \
  --matcher <MATCHER_PUBKEY> \
  --amount 10000000 \
  --price 50000.0

# Remove liquidity
percolator liquidity remove \
  --matcher <MATCHER_PUBKEY> \
  --amount 5000000

# View LP positions
percolator liquidity show
```

### Liquidations

```bash
# Execute liquidation
percolator liquidation execute <USER_PUBKEY>

# List liquidatable accounts
percolator liquidation list --exchange <EXCHANGE_PUBKEY>

# View liquidation history
percolator liquidation history --limit 50
```

### Insurance Fund

```bash
# Add funds to insurance
percolator insurance fund \
  --exchange <EXCHANGE_PUBKEY> \
  --amount 1000000000

# Check insurance balance
percolator insurance balance --exchange <EXCHANGE_PUBKEY>

# View insurance history
percolator insurance history --exchange <EXCHANGE_PUBKEY>
```

### Crisis Management

```bash
# Test haircut calculation (uses formally verified crisis module)
percolator crisis test-haircut \
  <DEFICIT> \
  <WARMING_PNL> \
  <INSURANCE> \
  <EQUITY>

# Example: 1M deficit, 500K warming PnL, 300K insurance, 5M equity
percolator crisis test-haircut 1000000 500000 300000 5000000

# Simulate crisis scenario
percolator crisis simulate \
  --exchange <EXCHANGE_PUBKEY> \
  --deficit 1000000 \
  --dry-run

# View crisis history
percolator crisis history --exchange <EXCHANGE_PUBKEY>
```

### Keeper Operations

```bash
# Start keeper bot
percolator keeper run \
  --exchange <EXCHANGE_PUBKEY> \
  --interval 5

# Monitor only (no execution)
percolator keeper run \
  --exchange <EXCHANGE_PUBKEY> \
  --interval 5 \
  --monitor-only

# View keeper statistics
percolator keeper stats --exchange <EXCHANGE_PUBKEY>
```

### Testing

```bash
# Run all tests
percolator test --all

# Run specific test suites
percolator test --crisis
percolator test --liquidations
percolator test --quick  # Smoke tests only
```

## Network Configuration

The CLI supports three networks:

### Localnet (default)
```bash
percolator --network localnet <command>
```
- RPC: http://127.0.0.1:8899
- Best for development and testing

### Devnet
```bash
percolator --network devnet <command>
```
- RPC: https://api.devnet.solana.com
- Public testnet for integration testing

### Mainnet
```bash
percolator --network mainnet-beta <command>
```
- RPC: https://api.mainnet-beta.solana.com
- Production environment

### Custom RPC
```bash
percolator --url https://my-rpc.com <command>
```

## Configuration Files

### Keypair Location

By default, the CLI uses `~/.config/solana/id.json`. Override with:

```bash
percolator --keypair /path/to/keypair.json <command>
```

### Environment Variables

```bash
# Set default network
export PERCOLATOR_NETWORK=devnet

# Set default RPC URL
export PERCOLATOR_RPC_URL=https://my-rpc.com

# Set default keypair
export PERCOLATOR_KEYPAIR=/path/to/keypair.json
```

## Examples

### Complete Workflow

```bash
# 1. Deploy programs
percolator deploy --all

# 2. Initialize exchange
EXCHANGE=$(percolator init --name "Percolator" --insurance-fund 10000000000 | grep "Exchange:" | awk '{print $2}')

# 3. Create matcher
MATCHER=$(percolator matcher create --exchange $EXCHANGE --symbol "SOL-USD" --tick-size 100 --lot-size 1000 | grep "Matcher:" | awk '{print $2}')

# 4. Deposit collateral
percolator margin deposit --amount 100000000000

# 5. Add liquidity
percolator liquidity add --matcher $MATCHER --amount 50000000000 --price 100.0

# 6. Place orders
percolator trade limit --matcher $MATCHER --side buy --price 99.0 --size 1000000
percolator trade limit --matcher $MATCHER --side sell --price 101.0 --size 1000000

# 7. Start keeper
percolator keeper run --exchange $EXCHANGE --interval 10
```

### Crisis Testing

```bash
# Test various crisis scenarios

# Scenario 1: Insurance covers all
percolator crisis test-haircut 500000 0 1000000 5000000
# Result: No equity haircut

# Scenario 2: Partial insurance coverage
percolator crisis test-haircut 1000000 0 300000 5000000
# Result: 700K equity haircut (20%)

# Scenario 3: Warming PnL + insurance
percolator crisis test-haircut 1000000 500000 300000 5000000
# Result: Burns warming PnL first, then insurance, then equity
```

## Troubleshooting

### Common Issues

**Keypair not found**
```bash
# Create a new keypair
solana-keygen new --outfile ~/.config/solana/id.json
```

**Insufficient balance**
```bash
# Airdrop SOL on devnet
solana airdrop 2 --url devnet
```

**RPC connection failed**
```bash
# Check network connectivity
solana cluster-version --url https://api.devnet.solana.com

# Try custom RPC
percolator --url https://api.devnet.solana.com <command>
```

**Program deployment failed**
```bash
# Ensure programs are built
cargo build-sbf

# Check deployer has enough SOL
solana balance
```

## Development

### Running Tests

```bash
# Unit tests
cargo test -p percolator-cli

# Integration tests with local validator
solana-test-validator &
cargo test -p percolator-cli --test '*'
```

### Building from Source

```bash
# Debug build
cargo build -p percolator-cli

# Release build (optimized)
cargo build --release -p percolator-cli

# Install globally
cargo install --path cli
```

## Architecture

The CLI is organized into modules:

- **`config.rs`** - Network configuration and keypair management
- **`client.rs`** - Solana RPC client utilities
- **`deploy.rs`** - Program deployment via `cargo build-sbf`
- **`exchange.rs`** - Exchange initialization
- **`matcher.rs`** - Matcher/slab operations
- **`liquidity.rs`** - Liquidity provision
- **`trading.rs`** - Order management
- **`margin.rs`** - Collateral operations
- **`liquidation.rs`** - Liquidation execution
- **`insurance.rs`** - Insurance fund management
- **`crisis.rs`** - Crisis simulation (uses formally verified `model_safety` crate)
- **`keeper.rs`** - Keeper bot operations

## Formal Verification

The `crisis test-haircut` command uses the **formally verified** crisis module from `model_safety`. This ensures that:

- ✅ Loss waterfall ordering is correct (warming PnL → insurance → equity)
- ✅ No over-burning of funds
- ✅ Conservation of total balances
- ✅ Bounded haircut ratios
- ✅ Monotonic scale factors

See `crates/model_safety/src/crisis/` for proof implementations.

## License

Apache-2.0

## Contributing

Contributions welcome! Please see the main [README.md](../README.md) for guidelines.
