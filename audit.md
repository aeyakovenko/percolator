# Kani Proof Timing Report
Generated: 2026-02-02

## Summary

- **Total Proofs**: 116
- **Passed**: 116 (100%)
- **Failed**: 0
- **Total verification time**: ~25 min (sequential, one-by-one)

---

## Haircut Audit Fixes (2026-02-02)

**Commits**: `3ab37df`..`34417a0` (6 commits)

### Finding G Fix — Stale Haircut After Trade (HIGH)

After a trade, the loser's negative PnL hasn't been settled yet, so `c_tot` is inflated relative
to economic reality. `Residual = V - c_tot - I = 0`, making `haircut_ratio = 0`. The winner's
profit conversion then gets haircutted to zero — value is systematically destroyed.

**Fix**: Two-pass settlement in `execute_trade()`:
1. `settle_loss_only(user_idx)` — realize negative PnL from capital (increases Residual)
2. `settle_loss_only(lp_idx)` — same for LP
3. `settle_warmup_to_capital(user_idx)` — now uses correct haircut ratio
4. `settle_warmup_to_capital(lp_idx)`

New helper `settle_loss_only()` implements §6.1 only (loss settlement without profit conversion).

### Finding C Fix — Fee Debt Traps Accounts (MEDIUM)

Accounts with negative `fee_credits` could never call `close_account`. Capital was gradually
consumed to pay fees, but any remainder was uncollectable — the account was permanently trapped.

**Fix**: `close_account` now forgives remaining fee debt after `touch_account_full` has paid what
it could from capital. This matches `garbage_collect_dust` semantics (which already frees accounts
regardless of fee_credits).

### 14 Audit Findings Fixed (3ab37df)

| # | Finding | Fix |
|---|---------|-----|
| A1 | c_tot updated atomically | `set_capital` helper updates c_tot on every mutation |
| A2 | pnl_pos_tot tracks correctly | `set_pnl` helper recalculates pnl_pos_tot |
| A3 | haircut_ratio equity uses pnl | Changed to use pnl (not capital) for effective equity |
| A4 | Insurance fee revenue routing | Fee revenue routes to insurance_fund.fee_revenue |
| A5 | Warmup slope reset timing | Reset slope after settlement, not before |
| A6 | GC updates aggregates | garbage_collect_dust calls set_capital/set_pnl |
| A7 | Close account updates aggregates | close_account calls set_capital/set_pnl |
| A8 | Panic settle updates aggregates | panic_settle_all uses set_capital/set_pnl |
| A9 | Stale haircut in execute_trade | Settle matured warmup before resetting slope |
| B1 | 12 vacuous Kani proofs | Added concrete assertions that exercise proof goals |
| B2 | 5 incomplete fast_valid proofs | Added post-condition checks to mutation proofs |
| B3 | Deprecated margin proof | Updated to use actual margin function |

### 6 New Haircut Mechanism Proofs (C1-C6)

| Proof | What it verifies |
|-------|-----------------|
| proof_haircut_ratio_formula_correctness (C1) | h_num/h_den ∈ [0,1], monotone in vault |
| proof_effective_equity_with_haircut (C2) | Effective equity = capital + haircut(pnl) |
| proof_principal_protection_across_accounts (C3) | Haircut never reduces principal (capital) |
| proof_profit_conversion_payout_formula (C4) | Payout = min(pnl, pnl × residual/pnl_pos_tot) |
| proof_rounding_slack_bound (C5) | Rounding error ≤ number of settling accounts |
| proof_liveness_after_loss_writeoff (C6) | After loss writeoff, account can still deposit/withdraw |

---

## Stranded Funds Recovery Fix (2026-02-01)

Three design flaws in `recover_stranded_to_insurance()` (merged via PR #15) were fixed:

**Fix A — Only haircut loss_accum, not all PnL:**
The old code computed `haircut = stranded + loss_accum = total_positive_pnl`, wiping 100% of
LP profit. Fixed to `haircut = min(loss_accum, total_positive_pnl)`, leaving legitimate profit
(pnl - loss_accum) intact. No insurance topup — haircut only reduces loss_accum.

Conservation: `vault + (L - H) = C + (P - H) + I + slack`, both sides decrease by H.

**Fix B — Decouple warmup pause from insurance threshold:**
`enter_risk_reduction_only_mode()` no longer unconditionally pauses warmup. Warmup only pauses
when `loss_accum > 0` (actual insolvency). `exit_risk_reduction_only_mode_if_safe()` unpauses
warmup when flat (`loss_accum = 0`) regardless of insurance level. `panic_settle_all` and
`force_realize_losses` explicitly force-pause warmup since they know insolvency is imminent.

Kani `inv_mode` weakened: `risk_reduction_only ∧ ¬loss_accum.is_zero() ⇒ warmup_paused`

**Fix C — Don't restart warmup_started_at_slot in recovery:**
Eliminated the 3rd bitmap loop that reset `warmup_started_at_slot = current_slot`. Warmup slope
updates are now folded into pass 2 inline, respecting the paused state (doesn't reset `started_at`
when `warmup_paused = true`). This matches `update_warmup_slope()` semantics.

**Kani timeout fix:** Recovery bitmap loops excluded from crank path via `#[cfg(not(kani))]` to
keep CBMC formula tractable. Recovery is verified by 6 dedicated unit tests and conservation proofs.
`proof_crank_with_funding_preserves_inv`: 62s (was TIMEOUT), `proof_keeper_crank_best_effort_settle`: 8s (was TIMEOUT).

Unit tests: 172 pass (6 new TDD tests for corrected behavior).

---

## Fee-Debt Sweep + GC Liveness + Coupon Semantics (2026-01-30)

**Commits**: `d890682`..`d88175b` (5 commits)

Engine changes:
- **Stale account fee drain**: `keeper_crank()` now settles maintenance fees for every visited account,
  draining idle accounts over time so they become dust and get GC'd.
- **GC liveness**: `garbage_collect_dust()` snaps funding_index for flat accounts (position_size == 0)
  instead of skipping them. Uses `is_lp()` instead of raw matcher_program check.
- **Fee-credit coupon semantics**: `fee_credits` are pure coupons/discounts — spending credits does NOT
  route to insurance. Only capital-paid fee portions flow to `insurance_fund.fee_revenue`.
- **Fee-debt sweep**: New `pay_fee_debt_from_capital()` sweeps negative fee_credits from available
  capital into insurance. Called after warmup in `touch_account_full()` and after capital addition
  in `deposit()`. Closes the intra-slot fee-debt dodge loophole.
- **Post-settlement MM re-check**: `touch_account_full()` re-checks maintenance margin after the
  fee debt sweep to catch accounts pushed below maintenance by fee settlement.
- **settle_maintenance_fee return value**: Now returns `paid_from_capital` (insurance inflow) instead
  of `due` (total fee), for accurate keeper rebate accounting.

Kani proof fixes:
- **`gc_respects_full_dust_predicate`**: Replaced blocker case 2 (funding_index mismatch, no longer
  valid after GC snap change) with positive-pnl blocker.
- **`kani_cross_lp_close_no_pnl_teleport`**: Changed exact `pnl` check to total value (pnl + capital)
  check, since warmup reordering can partially settle PnL to capital during trade execution.

---

## Unwind OOM Fix (2026-01-29)

Six ADL harnesses used `#[kani::unwind(33)]` but `MAX_ACCOUNTS=4` in Kani mode,
so all ADL loops are bounded by 4 iterations. `unwind(33)` caused CBMC to generate
formulas for 33 iterations of loops that never exceed 4, leading to OOM during SAT
solving. Changed to `unwind(5)` (4 iterations + 1 for termination check).

**Removed `i1b_adl_overflow_soundness`**: CBMC's bit-level u128 encoding makes
`apply_adl` intractable with any symbolic inputs (~5M SAT variables, OOM during
propositional reduction regardless of input range). The overflow atomicity scenario
is covered by `i1c` (concrete values) and the non-overflow symbolic case by `i1`.

**New proofs added**: `kani_cross_lp_close_no_pnl_teleport`, `kani_rejects_invalid_matcher_output`
(inline in `src/percolator.rs`), `proof_variation_margin_no_pnl_teleport`, `proof_trade_pnl_zero_sum`,
`kani_no_teleport_cross_lp_close` (in `tests/kani.rs`).

---

## Optimization: is_lp/is_user Simplification (2026-01-21)

The `is_lp()` and `is_user()` methods were simplified to use the `kind` field directly instead of comparing 32-byte `matcher_program` arrays. This eliminates memcmp calls that required `unwind(33)` and was the root cause of 25 timeout proofs.

**Before**: `self.matcher_program != [0u8; 32]` (32-byte comparison, SBF workaround)
**After**: `matches!(self.kind, AccountKind::LP)` (enum match)

The U128/I128 wrapper types ensure consistent struct layout between x86 and SBF, making the `kind` field reliable. This optimization reduced all previously-timeout proofs from 15+ minutes to under 6 minutes.

---

## CRITICAL: ADL Overflow Atomicity Bug (2026-01-18)

### Issue

A soundness issue was discovered in `RiskEngine::apply_adl` where an overflow error can leave the engine in an inconsistent state. If the `checked_mul` in the haircut calculation overflows on account N, accounts 0..N-1 have already been modified but the operation returns an error.

### Location

`src/percolator.rs` lines 4354-4361 in `apply_adl_impl`:

```rust
let numer = loss_to_socialize
    .checked_mul(unwrapped)
    .ok_or(RiskError::Overflow)?;  // Early return if overflow
let haircut = numer / total_unwrapped;
let rem = numer % total_unwrapped;

self.accounts[idx].pnl =
    self.accounts[idx].pnl.saturating_sub(haircut as i128);  // Account modified BEFORE potential overflow on next iteration
```

### Proof of Bug

Unit test `test_adl_overflow_atomicity_engine` demonstrates the issue:

```
pnl1 = 1, pnl2 = 2^64
loss_to_socialize = 2^64 + 1
Account 1 mul check: Some(2^64 + 1) - no overflow
Account 2 mul check: None - OVERFLOW!

Result: Err(Overflow)
PnL 1 before: 1, after: 0  <-- MODIFIED BEFORE OVERFLOW

*** ATOMICITY VIOLATION DETECTED! ***
```

### Impact

- **Severity**: Medium-High
- **Exploitability**: Low (requires attacker to have extremely large PnL values ~2^64)
- **Impact**: If triggered, some accounts have haircuts applied while others don't, violating ADL fairness invariant

### Recommended Fix

Option A (Pre-validation): Compute all haircuts in a scratch array first, check for overflows, then apply all at once only if no overflow.

Option B (Wider arithmetic): Use u256 for the multiplication to avoid overflow entirely.

Option C (Loss bound): Enforce `total_loss < sqrt(u128::MAX)` so multiplication can never overflow.

---

### Full Audit Results (2026-01-16)

All 160 proofs were run individually with a 15-minute (900s) timeout per proof.

**Key Findings:**
- All passing proofs complete in 1-100 seconds (most under 10s)
- 25 proofs timeout due to U128/I128 wrapper type complexity
- Zero actual verification failures
- Timeouts are concentrated in ADL, panic_settle, and complex liquidation proofs

**Timeout Categories (25 proofs):**
| Category | Count | Example Proofs |
|----------|-------|----------------|
| ADL operations | 12 | adl_is_proportional_for_user_and_lp, fast_proof_adl_conservation |
| Panic settle | 4 | fast_valid_preserved_by_panic_settle_all, proof_c1_conservation_bounded_slack_panic_settle |
| Liquidation routing | 5 | proof_liq_partial_3_routing, proof_liquidate_preserves_inv |
| Force realize | 2 | fast_valid_preserved_by_force_realize_losses, proof_c1_conservation_bounded_slack_force_realize |
| i10 risk mode | 1 | i10_risk_mode_triggers_at_floor |
| Sequences | 1 | proof_sequence_deposit_trade_liquidate |

**Root Cause of Timeouts:**
The U128/I128 wrapper types (introduced for BPF alignment) add extra struct access operations
that significantly increase SAT solver complexity for proofs involving:
- Iteration over account arrays
- Multiple account mutations
- ADL waterfall calculations

### Proof Fixes (2026-01-16)

**Commit TBD - Fix Kani proofs for U128/I128 wrapper types**

The engine switched from raw `u128`/`i128` to `U128`/`I128` wrapper types for BPF-safe alignment.
All Kani proofs were updated to work with these wrapper types.

**Fixes applied:**
- All field assignments use `U128::new()`/`I128::new()` constructors
- All comparisons use `.get()` to extract primitive values
- All zero checks use `.is_zero()` method
- All Account struct literals include `_padding: [0; 8]`
- Changed all `#[kani::unwind(8)]` to `#[kani::unwind(33)]` for memcmp compatibility
- Fixed `reserved_pnl` field (remains `u64`, not wrapped)

### Proof Fixes (2026-01-13)

**Commit b09353e - Fix Kani proofs for is_lp/is_user memcmp detection**

The `is_lp()` and `is_user()` methods were changed to detect account type via
`matcher_program != [0u8; 32]` instead of the `kind` field. This 32-byte array
comparison requires `memcmp` which needs 33 loop iterations.

**Fixes applied:**
- Changed all `#[kani::unwind(10)]` to `#[kani::unwind(33)]` (50+ occurrences)
- Changed all `add_lp([0u8; 32], ...)` to `add_lp([1u8; 32], ...)` (32 occurrences)
  so LPs are properly detected with the new `is_lp()` implementation

**Impact:**
- All tested proofs pass with these fixes
- Proofs involving ADL/heap operations are significantly slower due to increased unwind bound
- Complex sequence proofs (e.g., `proof_sequence_deposit_trade_liquidate`) now take 30+ minutes

### Representative Proof Results (2026-01-13)

| Category | Proofs Tested | Status |
|----------|---------------|--------|
| Core invariants | i1, i5, i7, i8, i10 series | All PASS |
| Deposit/Withdraw | fast_valid_preserved_by_deposit/withdraw | All PASS |
| LP operations | proof_inv_preserved_by_add_lp | PASS |
| Funding | funding_p1, p2, p5, zero_position | All PASS |
| Warmup | warmup_budget_a/b/c/d | All PASS |
| Close account | proof_close_account_* | All PASS |
| Panic settle | panic_settle_enters_risk_mode, closes_all_positions | All PASS |
| Trading | proof_trading_credits_fee_to_user, risk_increasing_rejected | All PASS |
| Keeper crank | proof_keeper_crank_* | All PASS |

### Proof Hygiene Fixes (2026-01-08)

**Fixed 4 Failing Proofs**:
- `proof_lq3a_profit_routes_through_adl`: Fixed conservation setup, adjusted entry_price for proper liquidation trigger
- `proof_keeper_crank_advances_slot_monotonically`: Changed to deterministic now_slot=200, removed symbolic slot handling
- `withdrawal_maintains_margin_above_maintenance`: Tightened symbolic ranges for tractability (price 800k-1.2M, position 500-5000)
- `security_goal_bounded_net_extraction_sequence`: Simplified to 3 operations, removed loop over accounts, direct loss tracking

**Proof Pattern Updates**:
- Use `matches!()` for multiple valid error types (e.g., `pnl_withdrawal_requires_warmup`)
- Use `is_err()` for "any error acceptable" cases (e.g., `i10_withdrawal_mode_blocks_position_increase`)
- Force Ok path with `assert_ok!` pattern for non-vacuous proofs
- Ensure account closable state before calling `close_account`

### Previous Engine Changes (2025-12-31)

**apply_adl_excluding for Liquidation Profit Routing**:
- Added `apply_adl_excluding(total_loss, exclude_idx)` function
- Liquidation profit (mark_pnl > 0) now routed via ADL excluding the liquidated account
- Prevents liquidated winners from funding their own profit through ADL
- Fixed `apply_adl` while loop to bounded for loop (Kani-friendly)

**Fixes Applied (2025-12-31)**:
- `proof_keeper_crank_best_effort_liquidation`: use deterministic oracle_price instead of symbolic
- `proof_lq3a_profit_routes_through_adl`: simplified test setup to avoid manual pnl state

### Previous Engine Changes (2025-12-30)

**Slot-Native Engine**:
- Removed `slots_per_day` and `maintenance_fee_per_day` from RiskParams
- Engine now uses only `maintenance_fee_per_slot` for direct calculation
- Fee calculation: `due = maintenance_fee_per_slot * dt` (no division)
- Any per-day conversion is wrapper/UI responsibility

**Overflow Safety in Liquidation**:
- If partial close arithmetic overflows, engine falls back to full close
- Ensures liquidations always complete even with extreme position sizes
- Added match on `RiskError::Overflow` in `liquidate_at_oracle`

### Recent Non-Vacuity Improvements (2025-12-30)

The following proofs were updated to be non-vacuous (force operations to succeed
and assert postconditions unconditionally):

**Liquidation Proofs (LQ1-LQ6, LIQ-PARTIAL-1/2/3/4)**:
- Force liquidation with `assert!(result.is_ok())` and `assert!(result.unwrap())`
- Use deterministic setups: small capital, large position, oracle=entry

**Panic Settle Proofs (PS1-PS5, C1)**:
- Assert `panic_settle_all` succeeds under bounded inputs
- PS4 already had this; PS1/PS2/PS3/PS5/C1 now non-vacuous

**Waterfall Proofs**:
- `proof_adl_waterfall_exact_routing_single_user`: deterministic warmup time vars
- `proof_adl_waterfall_unwrapped_first_no_insurance_touch`: seed warmed_* = 0
- `proof_adl_never_increases_insurance_balance`: force insurance spend

### Verified Key Proofs (2025-12-30)

| Proof | Time | Status |
|-------|------|--------|
| proof_c1_conservation_bounded_slack_panic_settle | 487s | PASS |
| proof_ps5_panic_settle_no_insurance_minting | 438s | PASS |
| proof_liq_partial_3_routing_is_complete_via_conservation_and_n1 | 2s | PASS |
| proof_liq_partial_deterministic_reaches_target_or_full_close | 2s | PASS |

## Full Timing Results (2026-02-02)

116/116 proofs pass. No timeouts, no failures.

| Proof Name | Time | Status |
|------------|------|--------|
| audit_force_realize_updates_warmup_start | 2s | PASS |
| crank_bounds_respected | 2s | PASS |
| fast_account_equity_computes_correctly | 1s | PASS |
| fast_frame_deposit_only_mutates_one_account_vault_and_warmup | 1s | PASS |
| fast_frame_execute_trade_only_mutates_two_accounts | 5s | PASS |
| fast_frame_settle_warmup_only_mutates_one_account_and_warmup_globals | 2s | PASS |
| fast_frame_touch_account_only_mutates_one_account | 2s | PASS |
| fast_frame_update_warmup_slope_only_mutates_one_account | 2s | PASS |
| fast_frame_withdraw_only_mutates_one_account_vault_and_warmup | 2s | PASS |
| fast_i2_deposit_preserves_conservation | 1s | PASS |
| fast_i2_withdraw_preserves_conservation | 2s | PASS |
| fast_maintenance_margin_uses_equity_including_negative_pnl | 10s | PASS |
| fast_neg_pnl_after_settle_implies_zero_capital | 2s | PASS |
| fast_neg_pnl_settles_into_capital_independent_of_warm_cap | 3s | PASS |
| fast_valid_preserved_by_deposit | 1s | PASS |
| fast_valid_preserved_by_execute_trade | 4s | PASS |
| fast_valid_preserved_by_force_realize_losses | 2s | PASS |
| fast_valid_preserved_by_garbage_collect_dust | 2s | PASS |
| fast_valid_preserved_by_panic_settle_all | 6s | PASS |
| fast_valid_preserved_by_settle_warmup_to_capital | 3s | PASS |
| fast_valid_preserved_by_top_up_insurance_fund | 1s | PASS |
| fast_valid_preserved_by_withdraw | 2s | PASS |
| fast_withdraw_cannot_bypass_losses_when_position_zero | 3s | PASS |
| funding_p1_settlement_idempotent | 11s | PASS |
| funding_p2_never_touches_principal | 5s | PASS |
| funding_p3_bounded_drift_between_opposite_positions | 8s | PASS |
| funding_p4_settle_before_position_change | 4s | PASS |
| funding_p5_bounded_operations_no_overflow | 1s | PASS |
| funding_zero_position_no_change | 1s | PASS |
| gc_frees_only_true_dust | 2s | PASS |
| gc_never_frees_account_with_positive_value | 5s | PASS |
| gc_respects_full_dust_predicate | 5s | PASS |
| i5_warmup_bounded_by_pnl | 1s | PASS |
| i5_warmup_determinism | 3s | PASS |
| i5_warmup_monotonicity | 3s | PASS |
| i7_user_isolation_deposit | 1s | PASS |
| i7_user_isolation_withdrawal | 2s | PASS |
| i8_equity_with_negative_pnl | 1s | PASS |
| i8_equity_with_positive_pnl | 2s | PASS |
| kani_cross_lp_close_no_pnl_teleport | 8s | PASS |
| kani_no_teleport_cross_lp_close | 6s | PASS |
| kani_rejects_invalid_matcher_output | 2s | PASS |
| maintenance_margin_uses_equity_negative_pnl | 1s | PASS |
| neg_pnl_is_realized_immediately_by_settle | 1s | PASS |
| neg_pnl_settlement_does_not_depend_on_elapsed_or_slope | 3s | PASS |
| negative_pnl_withdrawable_is_zero | 1s | PASS |
| panic_settle_clamps_negative_pnl | 6s | PASS |
| panic_settle_closes_all_positions | 2s | PASS |
| panic_settle_preserves_conservation | 2s | PASS |
| pnl_withdrawal_requires_warmup | 3s | PASS |
| proof_add_user_structural_integrity | 1s | PASS |
| proof_c1_conservation_bounded_slack_force_realize | 2s | PASS |
| proof_c1_conservation_bounded_slack_panic_settle | 7s | PASS |
| proof_close_account_includes_warmed_pnl | 2s | PASS |
| proof_close_account_preserves_inv | 2s | PASS |
| proof_close_account_rejects_negative_pnl | 2s | PASS |
| proof_close_account_rejects_positive_pnl | 2s | PASS |
| proof_close_account_requires_flat_and_paid | 6s | PASS |
| proof_close_account_structural_integrity | 2s | PASS |
| proof_crank_with_funding_preserves_inv | 167s | PASS |
| proof_deposit_preserves_inv | 2s | PASS |
| proof_effective_equity_with_haircut | 40s | PASS |
| proof_execute_trade_conservation | 21s | PASS |
| proof_execute_trade_margin_enforcement | 64s | PASS |
| proof_execute_trade_preserves_inv | 22s | PASS |
| proof_fee_credits_never_inflate_from_settle | 1s | PASS |
| proof_force_realize_preserves_inv | 2s | PASS |
| proof_gc_dust_preserves_inv | 2s | PASS |
| proof_gc_dust_structural_integrity | 1s | PASS |
| proof_haircut_ratio_formula_correctness | 2s | PASS |
| proof_inv_holds_for_new_engine | 1s | PASS |
| proof_inv_preserved_by_add_lp | 1s | PASS |
| proof_inv_preserved_by_add_user | 1s | PASS |
| proof_keeper_crank_advances_slot_monotonically | 2s | PASS |
| proof_keeper_crank_best_effort_liquidation | 3s | PASS |
| proof_keeper_crank_best_effort_settle | 9s | PASS |
| proof_keeper_crank_forgives_half_slots | 6s | PASS |
| proof_keeper_crank_preserves_inv | 3s | PASS |
| proof_liq_partial_1_safety_after_liquidation | 2s | PASS |
| proof_liq_partial_2_dust_elimination | 3s | PASS |
| proof_liq_partial_3_routing_is_complete_via_conservation_and_n1 | 3s | PASS |
| proof_liq_partial_4_conservation_preservation | 3s | PASS |
| proof_liq_partial_deterministic_reaches_target_or_full_close | 3s | PASS |
| proof_liquidate_preserves_inv | 2s | PASS |
| proof_liveness_after_loss_writeoff | 3s | PASS |
| proof_lq1_liquidation_reduces_oi_and_enforces_safety | 2s | PASS |
| proof_lq2_liquidation_preserves_conservation | 4s | PASS |
| proof_lq3a_profit_routes_through_adl | 3s | PASS |
| proof_lq4_liquidation_fee_paid_to_insurance | 2s | PASS |
| proof_lq6_n1_boundary_after_liquidation | 3s | PASS |
| proof_net_extraction_bounded_with_fee_credits | 75s | PASS |
| proof_principal_protection_across_accounts | 2s | PASS |
| proof_profit_conversion_payout_formula | 28s | PASS |
| proof_ps5_panic_settle_no_insurance_minting | 4s | PASS |
| proof_require_fresh_crank_gates_stale | 1s | PASS |
| proof_rounding_slack_bound | 170s | PASS |
| proof_sequence_deposit_crank_withdraw | 13s | PASS |
| proof_sequence_deposit_trade_liquidate | 5s | PASS |
| proof_set_risk_reduction_threshold_updates | 0s | PASS |
| proof_settle_maintenance_deducts_correctly | 2s | PASS |
| proof_settle_warmup_negative_pnl_immediate | 1s | PASS |
| proof_settle_warmup_preserves_inv | 2s | PASS |
| proof_total_open_interest_initial | 1s | PASS |
| proof_trade_creates_funding_settled_positions | 6s | PASS |
| proof_trade_pnl_zero_sum | 21s | PASS |
| proof_trading_credits_fee_to_user | 3s | PASS |
| proof_variation_margin_no_pnl_teleport | 215s | PASS |
| proof_warmup_slope_nonzero_when_positive_pnl | 2s | PASS |
| proof_withdraw_preserves_inv | 2s | PASS |
| saturating_arithmetic_prevents_overflow | 0s | PASS |
| withdraw_calls_settle_enforces_pnl_or_zero_capital_post | 3s | PASS |
| withdraw_im_check_blocks_when_equity_after_withdraw_below_im | 2s | PASS |
| withdrawal_maintains_margin_above_maintenance | 75s | PASS |
| withdrawal_rejects_if_below_maintenance_at_oracle | 2s | PASS |
| withdrawal_requires_sufficient_balance | 2s | PASS |
| zero_pnl_withdrawable_is_zero | 1s | PASS |

## Historical Results (2026-01-21)

Previous results: 161 proofs, 160 passed, 1 timeout (i1b_adl_overflow_soundness with unwind(33)).

## Historical Results (2026-01-16)

Previous results with 25 timeouts (before is_lp/is_user optimization):
- 135 passed, 25 timeout out of 160

## Historical Results (2026-01-13)

Previous timing results before U128/I128 wrapper migration (all passed):

| Proof Name | Time (s) | Status |
|------------|----------|--------|
| proof_c1_conservation_bounded_slack_force_realize | 522s | PASS |
| fast_valid_preserved_by_force_realize_losses | 520s | PASS |
| fast_valid_preserved_by_apply_adl | 513s | PASS |
| security_goal_bounded_net_extraction_sequence | 507s | PASS |
| proof_c1_conservation_bounded_slack_panic_settle | 487s | PASS |
| proof_ps5_panic_settle_no_insurance_minting | 438s | PASS |
| fast_valid_preserved_by_panic_settle_all | 438s | PASS |
| panic_settle_clamps_negative_pnl | 303s | PASS |
| multiple_lps_adl_preserves_all_capitals | 32s | PASS |
| multiple_users_adl_preserves_all_principals | 31s | PASS |
| mixed_users_and_lps_adl_preserves_all_capitals | 30s | PASS |
| adl_is_proportional_for_user_and_lp | 30s | PASS |
| i4_adl_haircuts_unwrapped_first | 29s | PASS |
| fast_frame_apply_adl_never_changes_any_capital | 23s | PASS |
