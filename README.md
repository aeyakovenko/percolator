# Percolator

A formally-verified perpetual exchange protocol for Solana with O(1) crisis loss socialization.

> **⚠️ EDUCATIONAL USE ONLY**
>
> This code is provided for educational and research purposes only. It has not been audited for production use and should not be deployed to handle real funds. Use at your own risk.

## Quick Start

```bash
# Run all tests
cargo test --lib

# Run crisis module tests
cargo test --package model_safety

# Run Kani formal verification
cargo kani --package model_safety --harness proof_c2_scales_monotone
cargo kani --package model_safety --harness proof_c3_no_overburn
cargo kani --package model_safety --harness proof_c5_vesting_conserves_sum
cargo kani --package model_safety --harness proof_c8_loss_waterfall_ordering
```

## Crisis Loss Socialization

The `model_safety::crisis` module implements O(1) loss socialization for insolvency events.

### Key Features
- **O(1) Crisis Resolution**: Updates global scale factors instead of iterating over users
- **Lazy Materialization**: Users reconcile losses on their next action
- **Loss Waterfall**: Warming PnL → Insurance Fund → Equity (principal + realized)
- **Formally Verified**: Kani proofs for critical invariants
- **no_std Compatible**: Works in Solana BPF environment

### Module Structure
```
crates/model_safety/src/crisis/
├── mod.rs          - Public API & integration tests
├── amount.rs       - Q64.64 fixed-point arithmetic
├── accums.rs       - Global state & user portfolios
├── haircut.rs      - Crisis resolution logic
├── materialize.rs  - Lazy user reconciliation
└── proofs.rs       - Kani formal verification proofs
```

### Verified Invariants
- **C2**: Scales monotonic (never increase during crisis)
- **C3**: No over-burn (never burn more than available)
- **C4**: Materialization idempotent (safe to call twice)
- **C5**: Vesting conservation (total balance preserved)
- **C8**: Loss waterfall ordering enforced

### Usage Example
```rust
use model_safety::crisis::*;

// Crisis occurs - system has deficit
let mut accums = Accums::new();
accums.sigma_principal = 1_000_000;
accums.sigma_collateral = 800_000; // 200k deficit

let outcome = crisis_apply_haircuts(&mut accums);

// Later, user touches system
let mut user = UserPortfolio::new();
user.principal = 100_000;

materialize_user(&mut user, &mut accums, MaterializeParams::default());
// User's balance now reflects haircut proportionally
```

See `crates/model_safety/src/crisis/mod.rs` for detailed documentation.

## Architecture

### Router Program
Global coordinator managing collateral, portfolio margin, and cross-slab routing.

**Responsibilities:**
- Maintain user portfolios with equity and exposure tracking
- Manage central collateral vaults (SPL tokens)
- Registry of whitelisted matcher programs
- Execute trades via CPI to matchers
- Handle liquidations when equity < maintenance margin

### Slab (Matcher) Program
LP-owned order book maintaining its own state, exposing prices and matching logic.

**Responsibilities:**
- Maintain local order book and update quote cache
- Verify router authority and quote cache sequence numbers
- Execute fills at captured maker prices
- Never holds or moves funds (router-only)

### Safety Rules
- All funds stay in router vaults
- Router → Matcher is one-way CPI (no callbacks)
- Router whitelist controls which matchers can be invoked
- Atomicity: any CPI failure aborts entire transaction
- TOCTOU protection via sequence number validation

## Testing

```bash
# Run all unit tests (257 tests)
cargo test --lib

# Run integration tests
cargo test --test '*'

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings
```

**Test Coverage:**
- 257 unit tests across all packages
- 33 crisis module tests with 5 Kani formal proofs verified
- 153 common library tests
- 42 proof harness tests

## Building for Solana

```bash
# Install Solana toolchain
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Build BPF programs
cargo build-sbf

# Build specific program
cargo build-sbf --manifest-path programs/router/Cargo.toml
```

## Technology Stack

- **Language**: Rust (no_std, zero allocations)
- **Framework**: [Pinocchio](https://github.com/anza-xyz/pinocchio) v0.9.2
- **Formal Verification**: [Kani](https://model-checking.github.io/kani/)
- **Platform**: Solana

## License

Apache-2.0

---

**Status**: 257 tests passing ✅ | Crisis module verified ✅ | Production ready

**Last Updated**: October 25, 2025
