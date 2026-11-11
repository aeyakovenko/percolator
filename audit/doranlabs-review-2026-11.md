# Security review — Percolator v16 engine and `percolator-prog`

Reviewer: Doran Labs ([@DoranLabs](https://x.com/DoranLabs)), builder of Predictulator, a prediction-market client over unmodified `percolator-prog`.
Date: 2026-11-11.
Web version with inline code: https://www.doranlabs.si/security

Scope reviewed:

- `percolator-prog` `main` at `5cb331dde354517c6371a8acf92cecb194f3bb73` (`src/v16_program.rs` on `main` is byte-identical to that commit).
- `percolator` engine at `4db11a8cb0053815e23a35d3a7d3edc265d8d866` (the rev `percolator-prog` pins) and at `master` `b22bef7d6122fd7d82680ce9859e8b77fd8ef656`.

Method: manual review of every public instruction handler and the engine kernels it calls, cross-checked against `INVARIANTS.md`, `tests/invariants/README.md` (open rows 411–433 treated as known and not re-reported), and the LiteSVM suites. Findings were traced by hand; no SBF build was run on the reviewing machine, so each finding names the regression we believe reproduces it in the existing `V16CuEnv` harness.

This document is a submission, not a code change. It is published at the maintainer's request.

Summary:

| # | Severity | Where | One line |
| --- | --- | --- | --- |
| 1 | High | engine + wrapper | A backing-bucket authority can withdraw a loser's paid margin before the winner settles. |
| 2 | Medium | `percolator-prog` | A new asset cannot get a live oracle while any account in the market holds positive PnL. |
| 3 | Low | `percolator-prog` | `BatchTradeNoCpi` drops an oracle-profile write that the single route makes. |

---

## Finding 1 — A backing authority can withdraw a loser's paid margin before the winner settles

Severity: high (loss of trader funds by a per-asset authority that is untrusted in a permissionless deployment). Confidence: high at the engine level; the wrapper bypass is traced through code, not executed.

### Summary

When a mark move on asset A creates a loser L and a winner W, cranking L moves L's realized loss out of L's capital and into A's loss-side counterparty backing bucket as *loser-paid* backing (`pay_kf_loss_from_free_capital_not_atomic`). W's positive claim does not enter the domain's `positive_claim_bound_num` until W's own leg is settled. In that window the domain shows `positive_claim_bound_num == 0`, so `expected_source_credit_rate_num_for_state` returns `CREDIT_RATE_SCALE` and `withdraw_fresh_counterparty_backing_not_atomic` releases the loser-paid atoms to the backing authority's token account.

```rust
// engine · expected_source_credit_rate_num_for_state
if state.positive_claim_bound_num == 0 {
    return Ok(CREDIT_RATE_SCALE);   // "fully realizable"
}
let available = available_backing_num_for_source_credit_state(state)?;
// rate = available * SCALE / positive_claim_bound_num, clamped to SCALE
```

The wrapper's Live-mode gate (`live_domain_withdraw_health_or_shutdown_view`) is meant to block this. It does, in a single-asset market, because L's settlement writes `header.loss_stale_active = asset_is_loss_stale_at_slot(A)` and W's stale leg makes that true. But `loss_stale_active` is a last-touched-asset cache (engine comment at `v16.rs` master line ~15645: it "summarizes only the touched asset"), and the wrapper's asset-local fallback checks only whether a slot has elapsed, not the stale counts:

```rust
// program · asset_local_loss_stale_view (the fallback that should catch it)
matches!(asset.lifecycle, ACTIVE | DRAIN_ONLY)
    && asset.slot_last.get() < group.header.current_slot.get()   // false in-slot
    && asset_local_has_position_or_loss_state_view(group, asset_index)
```

Compare the engine's own predicate, which keys off the stale counts with no slot condition:

```rust
// engine · asset_is_loss_stale_at_slot (the correct test)
asset.stale_account_count_long != 0
    || asset.stale_account_count_short != 0
    || (asset_contributes_to_loss_stale_summary(asset)
        && asset.slot_last < current_slot)
```

Touching any second asset B in the same slot rewrites `loss_stale_active` from B's state (the settlement entry runs for every active leg and ends with that write), and because no slot has elapsed, the asset-local check for A is false. The withdrawal then passes every gate.

### Preconditions

- A Live market with at least two active assets A and B.
- The attacker holds A's `backing_bucket_authority`. In a permissionless deployment that is whatever key the asset's activator supplied, so the asset creator holds it. No provider deposit is needed: `loss_bucket_accepts_fresh` returns true for an `Empty` bucket, and `add_loser_paid_counterparty_backing_unchecked` creates a `Fresh` bucket.
- Any mark move on A that puts one side in loss.
- The attacker holds one portfolio with an active leg on B (a self-trade between two attacker portfolios on B is enough).

### Steps (one transaction, so no one cranks W in between)

1. Wait until A's effective price equals its target (otherwise `reject_exposed_target_effective_lag_view` rejects). W and L both carry stale K/F snapshots; `kernel_mark_kf_stale_cohorts` set `stale_account_count_long = stale_account_count_short = 1`.
2. `PermissionlessCrank` on L. `apply_account_kf_settlement_entry` settles L's leg; `pay_kf_loss_from_free_capital_not_atomic` moves `loss` atoms from L's capital into A's loss-side bucket (`capital -= x`, `c_tot -= x`, `fresh_reserved_backing_num += x·BOUND_SCALE`), L's pnl stays 0 so `negative_pnl_account_count` stays 0. At the end of the entry `loss_stale_active = true` because `stale_account_count_long == 1` (W).
3. `PermissionlessCrank` on the attacker's B portfolio in the same slot. The settlement entry runs for every active leg and its last statement sets `loss_stale_active = asset_is_loss_stale_at_slot(B)`, which is false. `header.current_slot` is unchanged.
4. `WithdrawBackingBucket { domain: A_loss_side, amount: x }` signed by A's backing authority, with no ledger account in position 6 (the ledger is optional and is the only place a principal cap exists).
   - `domain_withdrawal_has_active_loss`: `negative_pnl_account_count == 0`, no pending barrier, `threshold_stress_active == false`, `loss_stale_active == false`, no recovery. Passes.
   - `asset_local_loss_stale_view(A)`: `A.slot_last == header.current_slot`. Passes.
   - `reject_exposed_target_effective_lag_view(A)`: target equals effective. Passes.
   - Engine: bucket `Fresh` and unexpired, `fresh_unliened_backing_num >= x`, recomputed `credit_rate_num == CREDIT_RATE_SCALE` because `positive_claim_bound_num == 0`. Passes. `vault -= x`, SPL transfer of `x` to the authority.
5. W is cranked later. Its settlement adds `x·BOUND_SCALE` to A's `positive_claim_bound_num`; available backing is 0, so W's credit rate is 0. L's loss is already crystallized and gone, so W's gain is permanently unrealizable.

Conservation holds throughout (counterparty backing principal and vault both decrease by `x`), which is why no invariant trips. Repeatable on every adverse move; the amount taken per round is the loser-paid backing booked between the loser's and the winner's settlement, which the attacker controls by choosing crank order.

### Code references

Program (`percolator-prog/src/v16_program.rs`, `main` == `5cb331dd`): `handle_withdraw_backing_bucket` (ledger optional; engine call with no principal cap when absent), `live_domain_withdraw_health_or_shutdown_view`, `asset_local_loss_stale_view` (`slot_last < current_slot`), `domain_withdrawal_has_active_loss`.

Engine (pinned `4db11a8c` / master `b22bef7d`): `withdraw_fresh_counterparty_backing_not_atomic`, `expected_source_credit_rate_num_for_state`, `pay_kf_loss_from_free_capital_not_atomic`, `apply_account_kf_settlement_entry` (unconditional `loss_stale_active` write), `asset_is_loss_stale_at_slot`. On master, `release_provider_principal_after` `saturating_sub`s provider principal and continues; the withdrawn amount is still never compared with provider principal.

### Why the suite misses it

`inv_027_protected_principal_seniority.rs::v16_program_loss_stale_reserve_matrix_preserves_senior_stocks_and_flat_exit` advances the second asset at a *later* slot, which makes `slot_last < current_slot` true and the fallback fire. The same-slot variant is not covered. `tests/support/fuzz_model.rs` does not model `WithdrawBackingBucket` (zero occurrences), so the public-sequence generator cannot rediscover it.

### Proposed regression

Same fixture as the `inv_027` test above, but after the mark move on asset 0 converges, crank only the losing portfolio; then, *without warping*, crank a third portfolio holding a current leg on asset 1; then call `inv027_try_withdraw_backing(&mut env, domain_of_asset0_loss_side, None, loser_paid_amount)` and assert it is rejected with market and vault bytes unchanged. On current code the withdrawal succeeds and the vault falls by `loser_paid_amount`.

### Suggested fix

Either closes the wrapper hole; the second also closes it at the engine level.

1. In `live_domain_withdraw_health_or_shutdown_view`, replace the `slot_last < current_slot` asset-local test with the engine predicate: reject when the target asset's `stale_account_count_long != 0 || stale_account_count_short != 0`. Apply the same to `handle_withdraw_insurance_asset`, which uses the same gate.
2. In `withdraw_fresh_counterparty_backing_not_atomic`, cap Live-mode withdrawals at provider principal (`fresh_reserved_backing_num - loser_paid_backing_num`) and reject while the claim side's `stale_account_count` is nonzero. The `provider_principal_*` fields on master make this a two-line check.

---

## Finding 2 — New assets cannot get a live oracle while any account holds positive PnL

Severity: medium (liveness / denial of a documented feature; no funds lost). Confidence: high.

`require_asset_active_for_oracle_reconfiguration_view` (called by `handle_configure_managed_mark` and `handle_configure_hybrid_oracle`, and from `reset_empty_asset_oracle_anchor_view`) gates per-asset oracle configuration on market-wide counters:

```rust
// percolator-prog · require_asset_active_for_oracle_reconfiguration_view
if asset_index >= group.markets.len()
    || group.markets[asset_index].engine.asset.lifecycle != 2
    || asset_local_has_position_or_loss_state_view(group, asset_index)
    || group.header.pnl_pos_tot.get() != 0        // market-wide, not per asset
    || group.header.stale_certificate_count.get() != 0
    || group.header.b_stale_account_count.get() != 0
    || group.header.negative_pnl_account_count.get() != 0
    // ... stress flags
{
    return Err(PercolatorError::EngineLockActive.into());
}
```

`pnl_pos_tot` is the sum of every portfolio's settled positive PnL (`set_account_pnl_inner`); it rises on every settlement that leaves an account in profit and falls only through owner-signed `ConvertReleasedPnl` or later losses. In any market with ongoing trading it is essentially never zero.

A permissionlessly activated asset starts in `ORACLE_MODE_MANUAL` at its initial price (`manual_asset_oracle_profile`), and there is no manual price-push instruction; the only way to give it a live mark is `ConfigureAuthMark` / `ConfigureEwmaMark` / `ConfigureHybridOracle`, all of which call this gate. So the moment one unrelated trader anywhere in the market is up on a position, no new asset can be priced. The activation fee is spent on an asset that can never move. The same gate blocks every mode switch and feed rotation on existing assets.

This contradicts `INV-074` (scope locality; stated adversarial case ADV-09 "global stale flag blocks unrelated domain") and the README's "each asset is reconfigured only by that asset's `oracle_authority`." The engine's own `reset_empty_asset_oracle_anchor_not_atomic` imposes the same global condition via `group_has_position_or_loss_state_for_oracle_reset`; the justification appears to be that the reset writes the market-wide `current_slot`/`slot_last` accrual anchors, which argues for not *rewinding* the global clock, not for refusing to configure an empty asset while other assets hold profit.

The existing test `inv_074_scope_locality.rs::v16_attack_permissionless_oracle_reconfiguration_preserves_unrelated_fee_and_exit_liveness` passes only because it asserts `before.pnl_pos_tot == 0` before reconfiguring. Pushing asset 0's mark by a tick and cranking the winner before the reconfigure should make it fail on current code.

### Suggested fix

Drop `pnl_pos_tot`, `stale_certificate_count`, `b_stale_account_count`, and `negative_pnl_account_count` from this gate, keeping the asset-local emptiness check and the recovery/stress flags. Have `reset_empty_asset_oracle_anchor_view` set only the target asset's `slot_last` and advance `header.current_slot` monotonically (`max`) rather than assigning it, so the global clock never rewinds.

---

## Finding 3 — `BatchTradeNoCpi` drops an oracle-profile write on authority-mark assets

Severity: low (route inconsistency against `INV-047` equivalent-route semantics; no atoms created or destroyed). Confidence: high on the code path; low on any measurable effect.

In `handle_batch_execute_zero_copy`, when `!account_b_matcher_synchronized` the leg loop runs an inline zero-move funding accrual that mutates the local `oracle_profile` (funding-mark checkpoint, `price_move_remainder_bps_num`, and for fresh Hybrid the mark transition) and commits the engine accrual. The loop then hits an early return before the only profile write:

```rust
// percolator-prog · handle_batch_execute_zero_copy (leg loop)
if !profile_updates_mark_from_trade_view(oracle_profile, batch_now_slot) {
    continue;   // AuthMark + non-stale Hybrid take this branch
}
// ... mark/target updates ...
write_oracle_profile_to_view(&mut group, *asset_index, oracle_profile)?;  // skipped
```

For `ORACLE_MODE_AUTH_MARK` and non-stale Hybrid profiles the predicate is false, so the engine accrual is kept while the mutated profile is discarded. The single route (`handle_trade_nocpi_zero_copy`) writes it unconditionally, and the CPI route's wrapper writes the profile itself. Effect: the fractional `price_move_remainder_bps_num` carry for that segment is lost, and on fresh Hybrid the mark-follows-effective transition shifts by one slot, so the price path after a batch differs from the path after the same fills sent singly. Closest tests (`inv_045_public_carry_order.rs`, `inv_045_carry_transport_exit.rs`, `inv_027_funding_admission.rs`) trade only on price-moving or already-current segments, where this branch is not taken.

Fix: move `write_oracle_profile_to_view(&mut group, *asset_index, oracle_profile)?` above the `continue`, or have the accrue helper write the profile back itself as the CPI route does.

---

## Areas reviewed with no findings

Token custody (deposit, withdraw, close portfolio, close slab, insurance and backing top-ups and withdrawals, base-unit mint rotation and swap): canonical-ATA vault pinning, SPL-only token program, exact-amount accounting, atomic CPI-after-engine ordering, and every alias case hold and are covered by named `inv_017/018/021/034/070` tests. Asset lifecycle (activate, reuse, shutdown, retire, restart, authority rotation): generation and frontier binding, cooldowns, fee atomicity, realloc rollback, emptiness checks before retire all hold. Force close, crank, liquidation, resolved payouts: delay-gate clock immutability in Recovery, pair clamps, reward bounds, hint validation, exact-once receipts all hold. Trades and matcher CPI: aliasing, fee consent, epoch binding, return-data validation, side-mode gating, the 14-leg CU envelope all hold. Oracle paths: sequence and epoch binding, price bounds, per-slot cap, shutdown freeze, stale-resolve maturity, hybrid feed validation all hold.
