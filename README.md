# Percolator

A sharded perpetual exchange protocol for Solana, implementing the design from `plan.md`.

## **Design**

### **Router Program**
- Owns all collateral and user portfolios.  
- Computes margin (IM/MM) and equity for every account.  
- Executes trades, settlements, and liquidations.  
- Talks to whitelisted matcher programs via CPI.

### **Matcher (Slab) Program**
- LP-owned program that maintains its own order book (`slab_state` account).  
- Exposes prices and matching logic.  
- Never holds or moves funds.  
- Accepts only Router-authorized CPIs for trade execution.


## **Core Router Responsibilities**
- **Maintain:**
  - `UserPortfolio { equity, exposures, im, mm }` per trader/LP.  
  - Central collateral vaults (SPL tokens).  
  - Registry of whitelisted matchers `(program_id, version_hash, oracle_id, fee_caps, etc.)`.  

- **Before routing any order:**
  - Verify maker/taker margin sufficiency in Router portfolios.  
  - Check matcher program is allow-listed and oracle-aligned.  

- **Execute orders:**
  - Read top-K quotes directly from each matcher’s `QuoteCache`.  
  - Choose best price/quantity split.  
  - CPI → `matcher.commit_fill(side, qty, limit_px, receipt_pda)`.  
  - Read receipts and update both portfolios (PnL, fees, exposures).  

- **Handle liquidations:**
  - Detect under-margin accounts (`equity < MM`).  
  - Read best bids/asks from matchers.  
  - Issue reduce-only `commit_fill` CPIs within liquidation price bands.  
  - Repeat until account is solvent or flat.  


## **Core Matcher Responsibilities**
- Own a single writable `slab_state` account (book + header + quote cache).  
- Maintain local book and update `QuoteCache` (K levels per side).  

- **Verify at CPI entry:**
  - Caller is the Router authority.  
  - `Header.seqno` unchanged since Router read `QuoteCache`.  
  - `limit_px` respected; tick/lot alignment correct.  
  - `receipt_pda.owner == matcher_program_id` and unused.  

- **On `commit_fill`:**
  - Match orders and write one `FillReceipt`  
    (`used=1, seqno_committed, filled_qty, vwap, notional, fee`).  
  - Mutate only its own state + the receipt PDA.  
  - No token transfers, no external CPIs.  


## **Boundary & Safety Rules**
- All funds stay in Router vaults; matchers never see token accounts.  
- Router → Matcher is one-way CPI; matchers never call back.  
- Router whitelist controls which matcher programs can be invoked.  
- Router and matchers share a fixed data layout for `Header` and `QuoteCache`; version hash checked at registration.  
- **Atomicity:** if any CPI fails, the transaction aborts and no Router state changes.  
- **TOCTOU protection:** `seqno` mismatch causes CPI failure (no partial fills).  
- **Oracle alignment:** Router skips matchers whose `mark_px` deviates > ε from oracle.  
- Optional per-matcher exposure cap `E_max` to bound router-routed notional.  

## **Capital & Margin Flow**
- Every participant (trader or LP) deposits collateral into Router vaults → creates a `UserPortfolio`.  
- LP’s quotes on their matcher are backed by the LP’s Router-held equity.  
- Router checks LP margin before routing to their matcher.  
- Trade PnL, fees, and margin adjustments occur only inside Router state.  

## **Liquidation / Deleveraging**
- **Trigger:** `equity < MM`.  
- Router builds reduce-only orders using current exposures.  
- Reads quotes from matchers (bids/asks) and executes via `commit_fill`.  
- Applies liquidation fee and updates portfolios.  
- If equity still < MM, repeats; if equity ≤ 0, records bad debt (v0) for later settlement.  


## Architecture

Percolator consists of two main on-chain programs:

### 1. Router Program
The global coordinator managing collateral, portfolio margin, and cross-slab routing.

**Program ID:** `RoutR1VdCpHqj89WEMJhb6TkGT9cPfr1rVjhM3e2YQr`

**State structures:**
- `Vault` - Collateral custody per asset mint
- `Escrow` - Per (user, slab, mint) pledges with anti-replay nonces
- `Cap` (Capability) - Time-limited, scoped debit authorization tokens (max 2 minutes TTL)
- `Portfolio` - Cross-margin tracking with exposure aggregation across slabs
- `SlabRegistry` - Governance-controlled registry with version validation

**PDA Derivations:**
- Vault: `[b"vault", mint]`
- Escrow: `[b"escrow", user, slab, mint]`
- Capability: `[b"cap", user, slab, mint, nonce_u64]`
- Portfolio: `[b"portfolio", user]`
- Registry: `[b"registry"]`

### 2. Slab Program
LP-run perp engines with 10 MB state budget, fully self-contained matching and settlement.

**Program ID:** `SLabZ6PsDLh2X6HzEoqxFDMqCVcJXDKCNEYuPzUvGPk`

**State structures:**
- `SlabHeader` - Metadata, risk params, anti-toxicity settings
- `Instrument` - Contract specs, oracle prices, funding rates, book heads
- `Order` - Price-time sorted orders with reservation tracking
- `Position` - User positions with VWAP entry prices
- `Reservation` - Reserve-commit two-phase execution state
- `Slice` - Sub-order fragments locked during reservation
- `Trade` - Ring buffer of executed trades
- `AggressorEntry` - Anti-sandwich tracking per batch

**PDA Derivations:**
- Slab State: `[b"slab", market_id]`
- Authority: `[b"authority", slab]`

## Key Features Implemented

### ✅ Memory Management
- **10 MB budget** strictly enforced at compile time
- O(1) freelist-based allocation for all pools
- Zero allocations after initialization
- Pool sizes (tuned to fit within 10 MB):
  - Accounts: 5,000
  - Orders: 30,000
  - Positions: 30,000
  - Reservations: 4,000
  - Slices: 16,000
  - Trades: 10,000 (ring buffer)
  - Instruments: 32
  - DLP accounts: 100
  - Aggressor entries: 4,000

### ✅ Matching Engine
- **Price-time priority** with strict FIFO at same price level
- **Reserve operation**: Walk book, lock slices, calculate VWAP/worst price
- **Commit operation**: Execute at captured maker prices
- **Cancel operation**: Release reservations
- **Pending queue promotion**: Non-DLP orders wait one batch epoch
- **Order book management**: Insert, remove, promote with proper linking

### ✅ Risk Management
- **Local (slab) margin**: IM/MM calculated per position
- **Global (router) margin**: Cross-slab portfolio netting
- Equity calculation with unrealized PnL and funding payments
- Pre-trade margin checks
- Liquidation detection

### ✅ Capability Security
- Time-limited caps (max 2 minutes TTL)
- Scoped to (user, slab, mint) triplet
- Anti-replay with nonces
- Remaining amount tracking
- Automatic expiry checks

### ✅ Fixed-Point Math
- 6-decimal precision for prices
- VWAP calculations
- PnL computation
- Funding payment tracking
- Margin calculations in basis points

### ✅ PDA Derivation Helpers
- Router: Vault, Escrow, Capability, Portfolio, Registry PDAs
- Slab: Slab State, Authority PDAs
- Verification functions for account validation
- Comprehensive seed management

### ✅ Instruction Dispatching
- 6 instruction types: Reserve, Commit, Cancel, BatchOpen, Initialize, AddInstrument
- Discriminator-based routing
- Error handling for invalid instructions
- Account validation framework ready

### ✅ Anti-Toxicity Infrastructure
- Batch windows (`batch_ms`)
- Delayed maker posting (pending → live promotion)
- JIT penalty detection
- Kill band parameters
- Freeze levels configuration
- Aggressor roundtrip guard (ARG) data structures

### ✅ BPF Build Support
- Panic handlers for no_std builds
- `panic = "abort"` configuration
- Pinocchio integration for zero-dependency Solana programs

## Test Coverage

**53 tests passing** across all packages:

### percolator-common (27 tests)
- ✅ VWAP calculations (single/multiple fills, zero quantity)
- ✅ PnL calculations (long/short profit/loss, no change)
- ✅ Funding payment calculations
- ✅ Tick/lot alignment and rounding
- ✅ Margin calculations (IM/MM, scaling with quantity/price)
- ✅ Type defaults (Side, TimeInForce, MakerClass, OrderState, Order, Position)

### percolator-router (7 tests)
- ✅ Vault pledge/unpledge operations
- ✅ Escrow credit/debit with nonce validation
- ✅ Capability lifecycle (creation, usage, expiry)
- ✅ Capability TTL capping (max 2 minutes)
- ✅ Portfolio exposure tracking
- ✅ Portfolio margin aggregation
- ✅ Registry operations (add/validate slabs)

### percolator-slab (19 tests)
- ✅ Pool allocation/free operations
- ✅ Pool capacity limits and reuse
- ✅ Header validation and monotonic IDs
- ✅ JIT penalty detection
- ✅ Timestamp updates
- ✅ Book sequence numbers
- ✅ Reserve operation with max charge calculation
- ✅ Margin requirement calculations
- ✅ Slab size constraint (≤10 MB)

**Note:** PDA tests require Solana syscalls and are marked `#[cfg(target_os = "solana")]`. They will be tested in integration tests with Surfpool.

## Building and Testing

### Build
```bash
# Build all programs (libraries)
cargo build

# Build in release mode
cargo build --release

# Build specific package
cargo build --package percolator-slab
```

### Testing
```bash
# Run all tests
cargo test

# Run only library tests
cargo test --lib

# Run tests for specific package
cargo test --package percolator-common
cargo test --package percolator-router
cargo test --package percolator-slab

# Run specific test
cargo test test_vwap_calculation

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode (faster)
cargo test --release
```

**Integration and Property Tests:**

The `tests/` directory contains templates for integration tests and property-based tests. These are currently disabled (code commented out) and serve as documentation until Surfpool is available. See [`tests/README.md`](tests/README.md) for details on:
- Integration test scenarios (15+ tests across 3 files)
- Property-based invariant tests
- Setup instructions for Surfpool
- How to enable and run the tests

### Build for Solana BPF
```bash
# Install Solana toolchain (if not already installed)
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Build BPF programs
cargo build-sbf

# Build specific program
cargo build-sbf --manifest-path programs/slab/Cargo.toml
cargo build-sbf --manifest-path programs/router/Cargo.toml
```

## Surfpool Integration

[Surfpool](https://github.com/txtx/surfpool) provides a local Solana test validator with mainnet state access for realistic integration testing.

### Setup Surfpool

```bash
# Clone surfpool
git clone https://github.com/txtx/surfpool
cd surfpool

# Install dependencies
npm install

# Start local validator
npm run validator
```

### Integration Test Structure

Create `tests/integration/` directory for surfpool-based tests:

```rust
// tests/integration/test_reserve_commit.rs
use surfpool::prelude::*;
use percolator_slab::*;
use percolator_router::*;

#[surfpool::test]
async fn test_reserve_and_commit_flow() {
    // Initialize test environment
    let mut context = SurfpoolContext::new().await;

    // Deploy programs
    let router_program = context.deploy_program("percolator_router").await;
    let slab_program = context.deploy_program("percolator_slab").await;

    // Initialize slab state (10 MB account)
    let slab_pda = derive_slab_pda(b"BTC-PERP", &slab_program.id());
    context.create_account(&slab_pda, 10 * 1024 * 1024, &slab_program.id()).await;

    // Initialize router accounts
    let vault_pda = derive_vault_pda(&usdc_mint, &router_program.id());
    // ... setup vault, escrow, portfolio

    // Test reserve operation
    let reserve_ix = create_reserve_instruction(/* ... */);
    context.send_transaction(&[reserve_ix]).await.unwrap();

    // Verify reservation created
    let slab_state = context.get_account::<SlabState>(&slab_pda).await;
    assert!(slab_state.reservations.used() > 0);

    // Test commit operation
    let commit_ix = create_commit_instruction(/* ... */);
    context.send_transaction(&[commit_ix]).await.unwrap();

    // Verify trade executed
    assert_eq!(slab_state.trade_count, 1);
}
```

### Running Integration Tests

```bash
# Start surfpool validator (terminal 1)
cd surfpool && npm run validator

# Run integration tests (terminal 2)
cargo test --test integration

# Run specific integration test
cargo test --test integration test_reserve_and_commit_flow
```

### Example Test Scenarios

1. **Order Matching**
   - Place limit orders on both sides
   - Execute market order
   - Verify VWAP calculation and position updates

2. **Reserve-Commit Flow**
   - Reserve liquidity for aggregator order
   - Verify slices locked correctly
   - Commit at reserved prices
   - Check trades executed at expected prices

3. **Cross-Slab Portfolio**
   - Open positions on multiple slabs
   - Verify router aggregates exposures
   - Check cross-margin calculation

4. **Capability Security**
   - Create time-limited cap
   - Use cap to debit escrow
   - Verify expiry enforcement

5. **Anti-Toxicity**
   - Post pending order
   - Open batch window
   - Verify promotion after epoch
   - Test JIT penalty application

6. **Liquidation**
   - Open underwater position
   - Trigger liquidation
   - Verify position closure and PnL settlement

## Design Invariants (from plan.md)

**Safety:**
1. Slabs cannot access Router vaults directly
2. Slabs can only debit via unexpired, correctly scoped Caps
3. Total debits ≤ min(cap.remaining, escrow.balance)
4. No cross-contamination: slab cannot move funds for (user', slab') ≠ (user, slab)

**Matching:**
1. Price-time priority strictly maintained
2. Reserved qty ≤ available qty always
3. Book links acyclic and consistent
4. Pending orders never match before promotion

**Risk:**
1. IM monotone: increasing exposure increases margin
2. Portfolio IM ≤ Σ slab IMs (convexity not double-counted)
3. Liquidation triggers only when equity < MM

**Anti-Toxicity:**
1. Kill band: reject if mark moved > threshold
2. JIT penalty: DLP orders posted after batch_open get no rebate
3. ARG: roundtrip trades within batch are taxed/clipped

## Current Status

### ✅ Completed
- Core data structures (Router & Slab)
- Memory pools with O(1) freelists
- Order book management (insert, remove, promote)
- Reserve operation (lock slices, calculate VWAP)
- Commit operation (execute trades at maker prices)
- Risk calculations (equity, IM/MM, liquidation checks)
- Capability system (time-limited scoped debits)
- Fixed-point math utilities (VWAP, PnL, margin)
- Compile-time size constraints (10 MB enforced)
- PDA derivation helpers (all account types)
- Instruction dispatching framework
- BPF build support (panic handlers, no_std)
- Comprehensive unit tests (53 tests passing)
- Integration test templates with Surfpool (3 test files with 15+ scenarios)
- Property-based test framework with invariant checks

### 🚧 In Progress
- Integration testing infrastructure (Surfpool setup and runbook development)
- Solana build tooling setup (cargo build-sbf installation)

### 📋 Next Steps (Priority Order)

**Phase 1: Complete Core Program Logic**
- Implement instruction handler bodies (account validation, deserialization)
- Complete anti-toxicity mechanism integration (kill band, JIT penalty, ARG)
- Implement funding rate updates (time-weighted calculations)
- Implement liquidation execution (position closure, PnL settlement)
- Add account initialization helpers

**Phase 2: Build and Deploy**
- Set up Solana Platform Tools for BPF builds
- Build programs with `cargo build-sbf`
- Deploy to local test validator for manual testing
- Measure CU (Compute Unit) consumption and optimize

**Phase 3: Advanced Testing**
- Complete integration tests (Option B: traditional Solana testing or Option A: Surfpool once runbook format is clarified)
- Uncomment and run property-based tests
- Add fuzz tests for instruction parsing and edge cases
- Implement chaos/soak tests (24-72h load testing)

**Phase 4: Multi-Slab Coordination**
- Router orchestration (multi-slab reserve/commit atomicity)
- Cross-slab portfolio margin calculations
- Global liquidation coordination

**Phase 5: Production Readiness**
- Slab-level insurance pools (v1 feature)
- Client SDK (TypeScript/Rust)
- CLI tools for LP operations
- Operational runbooks and monitoring
- Security audits
- Documentation and examples

### Architecture Notes

**v0 Simplifications:**
- ✅ No router-level insurance pool (each slab manages its own isolated insurance fund)
- Individual slabs will implement their own insurance pools in v1
- This maintains full isolation between slabs and simplifies router logic

## Technology Stack

- **Framework**: [Pinocchio](https://github.com/anza-xyz/pinocchio) v0.9.2 - Zero-dependency Solana SDK
- **Testing**: [Surfpool](https://github.com/txtx/surfpool) - Local Solana test validator with mainnet state
- **Language**: Rust (no_std, zero allocations, panic = abort)

## Surfpool Integration Status

### Current Status
Surfpool is installed and configured, but full integration testing is pending due to challenges with Pinocchio-based programs.

### Challenges
1. **Runbook Format**: Surfpool uses `txtx` runbooks (`.tx` files) with a Terraform-inspired declarative syntax. The exact action syntax for Pinocchio programs (non-Anchor) is not well-documented.
2. **Auto-generation**: Surfpool's automatic runbook generation appears optimized for Anchor projects. Pinocchio-based programs may require manually crafted runbooks.
3. **Build Tooling**: `cargo build-sbf` is needed to compile programs for Solana BPF target, but isn't available via standard `cargo install`.

### Files Created
- `Surfpool.toml` - Manifest configuration for Percolator programs
- `.surfpool/runbooks/test_basic.tx` - Basic connectivity test runbook (template)

### Next Steps for Surfpool Integration
1. **Option A - Manual Runbooks**: Research txtx documentation at `docs.txtx.sh` to understand proper action syntax for Pinocchio programs
2. **Option B - Traditional Testing**: Use standard Solana testing tools with local validator:
   - Build programs with `cargo build-sbf` (requires Solana Platform Tools installation)
   - Deploy to local test validator with `solana program deploy`
   - Write integration tests using `solana-program-test` crate
3. **Option C - Anchor Wrapper**: Create minimal Anchor wrappers around Pinocchio programs for Surfpool compatibility

For now, the project focuses on comprehensive unit testing (53 tests passing) while integration test infrastructure is being developed.

## References

- [Plan Document](./plan.md) - Full protocol specification
- [Pinocchio Docs](https://docs.rs/pinocchio/)
- [Surfpool](https://github.com/txtx/surfpool)
- [Solana Cookbook](https://solanacookbook.com/)

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

### Testing
```bash
# Run all crisis module tests (33 unit tests)
cargo test --package model_safety

# Run Kani formal verification proofs
cargo kani --package model_safety --harness proof_c2_scales_monotone
cargo kani --package model_safety --harness proof_c3_no_overburn
cargo kani --package model_safety --harness proof_c5_vesting_conserves_sum
cargo kani --package model_safety --harness proof_c8_loss_waterfall_ordering

# Install Kani (if not already installed)
cargo install --locked kani-verifier
cargo kani setup
```

### Verified Invariants
- C2: Scales monotonic (never increase)
- C3: No over-burn (never burn more than available)
- C4: Materialization idempotent (safe to call twice)
- C5: Vesting conservation (sum preserved)
- C8: Loss waterfall ordering enforced

### Usage Example
```rust
use model_safety::crisis::*;

// Crisis occurs
let mut accums = Accums::new();
accums.sigma_principal = 1_000_000;
accums.sigma_collateral = 800_000; // 200k deficit

let outcome = crisis_apply_haircuts(&mut accums);

// Later, user touches system
let mut user = UserPortfolio::new();
user.principal = 100_000;

materialize_user(&mut user, &mut accums, MaterializeParams::default());
// User's balance now reflects haircut
```

See `crates/model_safety/src/crisis/mod.rs` for detailed documentation.

## License

Apache-2.0

---

**Status**: Core infrastructure complete ✅ | Crisis module verified ✅ | 86 tests passing ✅

**Last Updated**: October 25, 2025
