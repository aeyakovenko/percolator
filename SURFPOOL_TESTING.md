# Surfpool Testing Guide

This document describes how to test Percolator v0 on Surfpool localnet.

## Prerequisites

1. **Solana CLI** installed and configured
   ```bash
   sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
   ```

2. **Solana localnet** running
   ```bash
   solana-test-validator
   ```

3. **Programs built** for BPF
   ```bash
   make build-bpf
   # OR
   cargo build-sbf
   ```

## Quick Start

### 1. Build all programs
```bash
make build-bpf
```

This will compile all three programs to BPF:
- `target/deploy/percolator_slab.so`
- `target/deploy/percolator_router.so`
- `target/deploy/percolator_oracle.so`

### 2. Start Surfpool localnet
```bash
solana-test-validator
```

In another terminal:
```bash
solana config set --url http://localhost:8899
```

### 3. Deploy programs
```bash
./scripts/deploy.sh
```

This will:
- Deploy all three programs to localnet
- Save program IDs to `deployed_programs.json`

### 4. Run E2E tests
```bash
./scripts/test_e2e.sh
```

This will run the E2E test scenarios from `TEST_PLAN.md`.

## Test Scenarios

See `TEST_PLAN.md` for the complete list of test scenarios.

Key tests:
- **E2E-1**: Atomic multi-slab buy (happy path)
- **E2E-2**: Capital efficiency proof (net = 0 → IM = 0)
- **E2E-3**: TOCTOU safety (seqno drift)
- **E2E-4**: Price limit protection
- **E2E-5**: Partial failure rollback
- **E2E-6**: Oracle alignment gate
- **E2E-7**: Compute budget sanity

## Integration Test Harness

The Rust integration test harness is located at:
- `tests/surfpool_harness.rs`

This provides utilities for:
- Setting up test context with all programs
- Creating and funding test accounts
- Initializing oracle accounts
- Updating oracle prices
- Building transactions

Run integration tests:
```bash
cargo test --test surfpool_harness
```

## Manual Testing

### Initialize an oracle
```bash
# TODO: Add solana CLI commands for manual oracle initialization
```

### Initialize a slab
```bash
# TODO: Add solana CLI commands for manual slab initialization
```

### Execute a cross-slab trade
```bash
# TODO: Add solana CLI commands for cross-slab trade execution
```

## Debugging

### View program logs
```bash
solana logs
```

### Inspect account data
```bash
solana account <ACCOUNT_PUBKEY>
```

### Check program info
```bash
solana program show <PROGRAM_ID>
```

## Next Steps

1. Implement full E2E test scenarios in `scripts/test_e2e.sh`
2. Add TypeScript/JavaScript SDK for transaction building
3. Create test fixtures for common scenarios
4. Add performance benchmarking (compute units)
5. Add determinism testing (50-tx replay)

## References

- [TEST_PLAN.md](TEST_PLAN.md) - Complete test plan with all scenarios
- [PHASE1_TEST_RESULTS.md](PHASE1_TEST_RESULTS.md) - Phase 1 unit test results
- [V0_DESIGN.md](V0_DESIGN.md) - V0 architecture and design decisions
