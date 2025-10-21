# Percolator v0 Test Plan (Surfpool Integration)

This document outlines the comprehensive test plan for validating the v0 Router + Slab Perp DEX architecture.

## Test Strategy

### Phase 1: Unit Tests (Current - Rust `#[cfg(test)]`)
- ✅ Test individual instruction handlers
- ✅ Test state structures (Portfolio, SlabHeader, QuoteCache)
- ✅ Test margin calculations
- ✅ Test fee calculations
- ⏳ Test receipt generation
- ⏳ Test TOCTOU safety logic

### Phase 2: Integration Tests (Next - `solana-program-test`)
- Build mock Solana runtime environment
- Test CPI interactions between router and slab
- Test multi-instruction transactions
- Test account state transitions

### Phase 3: Deployment Tests (Final - Surfpool Localnet)
- Full end-to-end tests with deployed programs
- Real transaction building and execution
- Performance and compute unit testing
- Determinism and soak testing

---

## Prerequisites (Phase 3)

### Binaries & IDLs
- [ ] `router.so`, `router.json` (IDL)
- [ ] `slab.so`, `slab.json` (IDL)
- [ ] `oracle.so`, `oracle.json` (IDL)

### Fixed Layouts
- [x] `SlabHeader` - 256 bytes
- [x] `QuoteCache` - 256 bytes (K=4 levels)
- [x] `FillReceipt` - fixed size
- [x] `Portfolio` - tracks exposures
- [x] `Vault` - collateral management

### Constants
```rust
SCALE = 1_000_000
IMR = 0.10 (10%)
MMR = 0.05 (5%)
TAKER_BPS = 5 (0.05%)
EPSILON_PX_TICKS = 1
CONTRACT_SIZE = SCALE
```

### Keypairs
- `router_admin`
- `lp_A`, `lp_B` (slab LPs)
- `user_U` (trader)

### Infrastructure
- Surfpool localnet running
- Test harness for tx building/execution
- Account state inspection utilities

---

## Test Scenarios

### E2E-1: Atomic Multi-Slab Buy (Happy Path)

**Goal**: Router splits order across multiple slabs in one transaction

**Setup**:
- Oracle: $60,000
- Slab A asks: [(59,900, 5), (60,000, 10)]
- Slab B asks: [(59,950, 8), (60,050, 8)]

**Action**: Buy +10 @ limit $60,000

**Expected Split**:
- Slab A: +5 @ $59,900
- Slab B: +5 @ $59,950

**Assertions**:
- [ ] Each `FillReceipt.used == 1`
- [ ] `vwap_px <= limit_px` (buy order)
- [ ] `notional = filled_qty * contract_size * vwap / SCALE`
- [ ] `fee = notional * taker_bps / 10_000`
- [ ] Router IM calculated on net exposure
- [ ] `seqno_snapshot == Header.seqno` (no drift)

**Pass Criteria**: All receipts valid, portfolio updated with net +10 exposure

---

### E2E-2: Capital Efficiency Proof (Netting to ~0 IM)

**Goal**: THE KEY TEST - Prove net exposure = 0 → IM ≈ 0

**Setup**:
- Oracle: $60,000
- Slab A & B with liquidity on both sides

**Action**: In one transaction:
1. Buy +10 (split across A/B)
2. Sell -10 (split across A/B)

**Expected**:
- Individual fills succeed
- Net exposure = +10 + (-10) = 0
- IM_router ≈ 0 (not $10,000!)

**Assertions**:
- [ ] `sum(filled_qty) ≈ 0` (signed sum)
- [ ] `IM_router ≈ 0` (allow tiny rounding)
- [ ] Per-slab exposures are non-zero (proves cross-slab netting)
- [ ] Capital efficiency = ∞ (zero capital for zero net exposure)

**Pass Criteria**: This proves the core thesis! Net exposure reduces IM to ~0.

---

### E2E-3: TOCTOU Safety (Seqno Drift)

**Goal**: Router must fail if book changes after reading cache

**Setup**:
- Router reads `seqno_snapshot = S` from QuoteCache
- Before CPI, artificially bump `Header.seqno` (simulate another fill)

**Action**: Proceed with commit_fill

**Expected**: Slab rejects CPI with seqno mismatch error

**Assertions**:
- [ ] Transaction fails
- [ ] No receipts written (`FillReceipt.used == 0`)
- [ ] No router state changes
- [ ] Slab state unchanged

**Pass Criteria**: TOCTOU guard prevents stale reads

---

### E2E-4: Price Limit Protection

**Goal**: Slab must not fill beyond user's limit

**Setup**:
- Best ask: $60,100

**Action**: Buy +5 @ limit $60,000 (below best ask)

**Expected**: Fill respects limit

**Assertions**:
- [ ] `filled_qty == 0` OR partial fill with `vwap_px <= limit_px`
- [ ] Router aborts if required quantity not filled

**Pass Criteria**: Limit price strictly enforced

---

### E2E-5: Partial Failure Rollback (All-or-Nothing)

**Goal**: If one CPI fails, entire transaction rolls back

**Setup**:
- Make Slab B fail (e.g., remove liquidity between read and CPI)

**Action**: Router splits across A and B in one tx

**Expected**: Entire transaction fails atomically

**Assertions**:
- [ ] No `FillReceipt.used == 1` on any slab
- [ ] No router portfolio changes
- [ ] Slab A book unchanged
- [ ] Slab B book unchanged

**Pass Criteria**: Atomic all-or-nothing semantics

---

### E2E-6: Oracle Alignment Gate

**Goal**: Exclude mis-aligned slabs from execution

**Setup**:
- Router oracle: $60,000
- Slab A mark_px: $60,000 (aligned)
- Slab B mark_px: $60,003 (misaligned by 3 ticks)
- Tolerance: ε = 1 tick

**Action**: Router attempts to include both slabs

**Expected**: Router excludes Slab B

**Assertions**:
- [ ] Slab B not used for execution
- [ ] If Slab A alone can't fill → error "insufficient aligned liquidity"
- [ ] Slab A fills succeed if sufficient

**Pass Criteria**: Misaligned slab excluded; correct error handling

---

### E2E-7: Compute Budget Sanity

**Goal**: Ensure tx fits within Solana compute units

**Setup**:
- 8 slabs (A..H), each with 2 ask levels
- K=4 quote cache levels per slab

**Action**: Buy +16, router reads 8 caches, splits, CPIs to N slabs

**Expected**: Transaction succeeds within limits

**Assertions**:
- [ ] Transaction completes successfully
- [ ] CU usage < 1.2M
- [ ] Localnet time < 150ms

**Pass Criteria**: Under compute and time limits

---

## Edge & Negative Tests

### N-1: K-Levels Clamp
- Seed >4 levels per side
- Verify only top K=4 appear in QuoteCache
- Router never reads beyond K

### N-2: Tick/Lot Alignment
- Send unaligned qty and limit_px
- Slab must floor/ceil per rules
- Receipt shows aligned values

### N-3: Fee Calculation
- Verify `fee == taker_bps * notional / 10_000`
- Tolerance: ±1 unit (rounding)

### N-4: Bad Accounts Ordering
- Intentionally misorder accounts for CPI
- Must return deterministic error
- No partial writes

### N-5: Receipt Reuse Prevention
- Try to use same FillReceipt PDA twice in one tx
- Second write must be rejected
- Transaction fails atomically

---

## Determinism & Soak Tests

### D-1: Deterministic Replay
- Run E2E-1 fifty times with identical inputs
- Compute state hash: {portfolio, headers, caches, receipts}
- All hashes must be identical

### D-2: 50-tx Burst
- Run 50 sequential buys of +2 split across A/B
- Assertions:
  - `Header.seqno` increments exactly once per commit
  - No account grows past allocated size
  - Median tx time stable

---

## Test Output Format

For each test, emit JSON:

```json
{
  "test": "E2E-2 capital efficiency",
  "inputs": {
    "oracle_px": 60000000000,
    "books": {
      "slabA": {"asks": [[59900000000, 5000000]]},
      "slabB": {"asks": [[59950000000, 5000000]]}
    },
    "orders": [
      {"side": "Buy", "qty": 10000000, "limit_px": 60000000000},
      {"side": "Sell", "qty": 10000000, "limit_px": 60000000000}
    ]
  },
  "preState": {
    "routerPortfolio": "hash_abc123",
    "slabA.seqno": 12,
    "slabB.seqno": 9
  },
  "receipts": [
    {
      "slab": "A",
      "filled_qty": 10000000,
      "vwap_px": 59900000000,
      "fee": 2995,
      "seqno_committed": 13
    },
    {
      "slab": "B",
      "filled_qty": -10000000,
      "vwap_px": 60010000000,
      "fee": 3001,
      "seqno_committed": 10
    }
  ],
  "postState": {
    "routerPortfolio": "hash_def456",
    "slabA.seqno": 13,
    "slabB.seqno": 10
  },
  "assertions": {
    "net_exposure": 0,
    "im_router": 0,
    "capital_efficiency": "infinite"
  },
  "pass": true
}
```

Also save:
- Raw account dumps (hex + decoded)
- Transaction logs
- CU usage

---

## Pass/Fail Gate for v0

All of the following must be true:

- ✅ All E2E-1 through E2E-7 pass
- ✅ **E2E-2 (Capital Efficiency)**: IM_router ≈ 0 when net = 0
- ✅ **E2E-3 (TOCTOU)**: Safe failure with no state changes
- ✅ **E2E-6 (Oracle Alignment)**: Misaligned slab excluded
- ✅ No settlement in slab: Only receipts written, no token transfers
- ✅ Determinism (D-1): Identical hashes across replays
- ✅ Soak (D-2): No growth/leaks, seqno increments match
- ✅ All edge tests (N-1 through N-5) pass

---

## Current Status

### Phase 1: Unit Tests (In Progress)

**Completed**:
- [x] SlabHeader initialization and validation
- [x] QuoteCache updates
- [x] FillReceipt write/read
- [x] Portfolio exposure tracking
- [x] Margin calculation logic
- [x] Fee calculation
- [x] Seqno increment tracking

**Remaining**:
- [ ] Add TOCTOU seqno validation test
- [ ] Add price limit enforcement test
- [ ] Add oracle alignment logic test
- [ ] Add K-levels clamp test
- [ ] Add tick/lot alignment test

### Phase 2: Integration Tests (Not Started)

Need to set up:
- [ ] `solana-program-test` framework
- [ ] Mock CPI environment
- [ ] Multi-program testing
- [ ] Account state management

### Phase 3: Surfpool Deployment (Not Started)

Need to:
- [ ] Implement oracle program
- [ ] Build deployment scripts
- [ ] Create test harness
- [ ] Set up Surfpool localnet
- [ ] Implement transaction builder
- [ ] Add account inspection utilities

---

## Next Steps

1. **Complete Phase 1 unit tests** (1-2 hours)
   - Add missing test scenarios to existing test modules
   - Verify all logic components work correctly

2. **Set up Phase 2 integration tests** (4-8 hours)
   - Add `solana-program-test` dependency
   - Create test harness for CPI interactions
   - Implement E2E-1 and E2E-2 in mock environment

3. **Prepare for Phase 3** (16+ hours)
   - Implement minimal oracle program
   - Build Surfpool deployment scripts
   - Create transaction builder utility
   - Run full E2E test suite

---

## Key Insights

The test plan is structured in phases because:

1. **Phase 1 (Unit)** proves the math and logic are correct
2. **Phase 2 (Integration)** proves CPIs work in mock environment
3. **Phase 3 (Deployment)** proves everything works on actual Solana

**The core thesis (E2E-2) can be validated in Phase 1** - we don't need full deployment to prove `net_exposure = 0 → IM = 0`!

This phased approach allows us to:
- Catch bugs early (in unit tests)
- Iterate quickly (no deployment cycle)
- Build confidence progressively
- Prepare for final validation on Surfpool
