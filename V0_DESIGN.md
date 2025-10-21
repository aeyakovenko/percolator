# Percolator v0 - Minimal Viable Architecture

## Strategic Pivot (January 2025)

**Decision**: Simplify to prove the core thesis with minimal complexity.

## What v0 Proves

1. **Router coordination works** - separate component reads from multiple slabs, splits orders, CPIs atomically
2. **Portfolio netting works** - IM calculated on net exposure across all slabs (not per-slab)
3. **Capital efficiency is real** - long slab A + short slab B = ~0 IM requirement
4. **Oracle alignment prevents basis** - shared marks across slabs guarantee basis-free netting
5. **TOCTOU safety** - seqno matching ensures no race conditions

## Architecture

### Slab Program (Minimal Per-LP DEX)

**Single Account** (~4KB, not 10MB):
```rust
pub struct SlabState {
    pub header: SlabHeader,       // 256 bytes - metadata + seqno
    pub quote_cache: QuoteCache,  // 256 bytes - best 4 levels bid/ask
    pub book: BookArea,           // 3KB - price-time queues
}
```

**One Instruction**:
```rust
pub fn commit_fill(
    side: Side,
    qty: i64,           // desired quantity (1e6 scale)
    limit_px: i64,      // worst acceptable price
    receipt_account: &mut FillReceipt,
    signer: router_id,  // router-only access
) -> Result<()>
```

**No**:
- ❌ Per-user accounts
- ❌ Reservation/hold system
- ❌ Escrow/capability tokens
- ❌ Multi-phase commit
- ❌ On-slab settlement

### Router Program (Coordination + Portfolio)

**Responsibilities**:
1. Read QuoteCache directly from slab accounts (byte offsets)
2. Split user orders across N slabs (greedy on price × qty)
3. CPI to each slab's commit_fill in one transaction
4. Aggregate FillReceipts
5. Update user portfolio (net exposures across all slabs)
6. Check margin on **net exposure** (capital efficiency!)
7. Hold all tokens (no per-slab escrow)

**Key Accounts**:
```rust
pub struct UserPortfolio {
    pub exposures: Vec<(SlabId, InstrumentId, i64)>,  // net positions
    pub equity: i128,
    pub im_required: u128,  // computed on NET exposure!
}

pub struct Vault {
    pub mint: Pubkey,
    pub balance: u128,
    pub token_account: Pubkey,
}
```

## The Killer Demo

```
Initial State:
  User: 10,000 USDC, 0 positions

Atomic Transaction:
  1. Router reads QuoteCache from Slab A and Slab B
  2. Router splits: +1 BTC on Slab A @ $50,000
                    -1 BTC on Slab B @ $50,010
  3. Router CPIs to both slabs
  4. Both fill, receipts returned
  5. Router updates portfolio:
       Exposures: [(A, BTC, +1.0), (B, BTC, -1.0)]
       Net exposure: 0
       IM required: ~$0 (not $10,000!)
       Locked in: $10 arb
  6. Transaction succeeds

Result: Capital efficiency ∞ (zero capital for zero net exposure)
```

## What We Removed (vs. Complex Design)

| Component | Complex | v0 | Why Removed |
|-----------|---------|-----|-------------|
| Slab account size | 10MB multi-pool | 4KB single account | No per-user state needed |
| Instructions | 6 (reserve, commit, cancel, batch, init, add_instrument) | 1 (commit_fill) | Direct commit is sufficient |
| Settlement | Escrow + Cap tokens | Router vaults only | Centralizes complexity |
| Margin | Per-slab | Router net exposure | Proves capital efficiency |
| Liquidation | Multi-slab autonomous | Router reduce-only stub | Simplifies v0 |
| Funding | Periodic payments | None | Not needed for demo |
| Insurance | Fund + DLP | None | Not needed for demo |

## What We Kept (Still Valuable)

- ✅ Instruction data deserialization helpers (percolator-common)
- ✅ Error types
- ✅ PDA derivation patterns
- ✅ Test infrastructure
- ✅ Portfolio tracking
- ✅ Vault management

## Implementation Timeline

- **Week 1**: Minimal slab (SlabHeader + QuoteCache + BookArea + commit_fill)
- **Week 2**: Router coordination (read quotes, CPI, aggregate, margin check)
- **Week 3**: 7 critical tests (atomic split, TOCTOU, capital efficiency, etc.)
- **Week 4**: Integration, audit prep, documentation

vs. 8-12 weeks for complex design.

## The 7 Critical Tests

1. **Atomic router split** - Lock N slabs, read caches, CPI commits, aggregate in one tx
2. **TOCTOU safety** - seqno matching prevents races
3. **Price limit enforcement** - vwap ≤ limit
4. **Capital efficiency demo** - Long A + Short B = ~0 IM (THE PROOF!)
5. **Oracle alignment gate** - Reject if mark_px drift > ε
6. **Failure rollback** - All-or-nothing on insufficient qty
7. **Compute budget** - K=4 levels, M slabs fits in budget

## Migration to v1

When v0 proves the model, add:
- Per-slab settlement (escrow/caps)
- Reservation/hold for multi-tx workflows
- Funding rate mechanism
- Insurance layer
- Autonomous liquidations

But **none of that is needed to prove the thesis**.

## Status

### ✅ Completed

**Phase 1: Architecture simplification** (Commits: bf073d7, e98fb0d, 5db64f5, 99b071e)
- [x] Complex design implemented (research phase - preserved in git history)
- [x] v0 design documented
- [x] Slab simplification complete
  - [x] Removed pools, multi-account state, matching logic (~2,000 LOC removed)
  - [x] Created minimal SlabState (~4KB: Header + QuoteCache + BookArea)
  - [x] Added QuoteCache (best 4 bid/ask levels)
  - [x] Added FillReceipt structure
- [x] Router simplification complete
  - [x] Removed escrow.rs, cap.rs state files
  - [x] Removed multi_reserve, multi_commit, liquidate instructions
  - [x] Kept: Portfolio, Vault, Registry, Initialize, Deposit/Withdraw

**Phase 2: Core implementation** (Commits: 7796b90, b862881, 94a67ba)
- [x] Router entrypoint simplified to v0 instruction set
  - [x] 5 instructions: Initialize, InitializePortfolio, Deposit, Withdraw, ExecuteCrossSlab
  - [x] Removed 4 complex instructions (MultiReserve, MultiCommit, Liquidate, InitializeEscrow)
- [x] commit_fill implementation (slab)
  - [x] Order validation (qty > 0, limit_px > 0)
  - [x] v0 instant fill simulation at limit price
  - [x] Notional and fee calculation
  - [x] QuoteCache updates after fills
  - [x] FillReceipt writing
- [x] execute_cross_slab implementation (router)
  - [x] Portfolio exposure tracking across slabs
  - [x] Net exposure calculation (key to capital efficiency!)
  - [x] IM calculation based on net exposure (IM = 0 when net = 0!)
  - [x] Margin requirement checking

**Phase 3: CPI integration** (Current session)
- [x] Slab entrypoint simplified to v0 instruction set
  - [x] 2 instructions: Initialize, CommitFill
  - [x] Removed 4 complex instructions (Reserve, Commit, Cancel, BatchOpen, AddInstrument)
  - [x] Added contract_size parameter to slab initialization
- [x] Router authority PDA for CPI signing
  - [x] Added derive_authority_pda() function (seeds: ["authority"])
  - [x] Router authority used for signing CPIs to slabs
- [x] CPI implementation in execute_cross_slab
  - [x] Build commit_fill instruction data (discriminator + side + qty + limit_px)
  - [x] Account validation (router_authority must match PDA)
  - [x] CPI invocation to slab's commit_fill
  - [x] Error handling (CpiFailed error added)

### 🚧 In Progress

- [ ] Add 7 critical v0 tests

### 📊 Code Reduction

- **Before**: ~5,000 LOC (complex multi-pool design)
- **After cleanup**: ~2,000 LOC
- **Target for v0**: ~1,000 LOC total

### 🎯 Next Session

1. Add the 7 critical v0 tests:
   - **Capital efficiency test** (long A + short B = ~0 IM) - THE KEY PROOF
   - Atomic split test (execute on multiple slabs)
   - Portfolio netting test
   - Margin requirement test
   - Order validation test
   - Price/quantity limits test
   - Receipt aggregation test
2. Wire up real CPI from router to slab (currently stubbed)
3. End-to-end integration test

---

**Core Insight**: The complex multi-pool design was solving problems we don't have in v0. This minimal design proves capital efficiency (the core value prop) in ~1000 LOC vs ~5000 LOC.

## Current Architecture (v0)

```
Router Program (~800 LOC)
├── State
│   ├── Portfolio (net exposures across slabs)
│   ├── Vault (collateral management)
│   └── Registry (governance)
├── Instructions
│   ├── Initialize (setup registry)
│   ├── InitializePortfolio (create user portfolio)
│   ├── Deposit/Withdraw (vault operations)
│   └── ExecuteCrossSlab (main v0 - splits, CPIs, aggregates)

Slab Program (~600 LOC)
├── State
│   ├── SlabHeader (256B - metadata, seqno, offsets)
│   ├── QuoteCache (256B - best 4 levels each side)
│   └── BookArea (3KB - price-time queues stub)
├── Instructions
│   ├── Initialize (setup slab with instrument params)
│   └── CommitFill (execute fill, write receipt)

Common Library (~600 LOC)
├── Types (Side, PercolatorError, etc.)
├── Instruction helpers (InstructionReader)
└── Account helpers (validation, borrowing)
```
