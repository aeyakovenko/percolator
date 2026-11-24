# Specification: market shape is preserved by every public engine entry point (layer T, engine 06b7e5be)

`src/v16.rs` has 64 public entry points named `*_not_atomic` on `MarketGroupV16ViewMut` (a few on
the header account); 50 of them are compiled into a default build. The market shape predicate of
a default build is the function

    MarketGroupV16ViewMut::validate_shape(&self) -> V16Result<()>

It reads the market only (`&self`) and returns `Ok(())` exactly when eighteen checks on the market
header pass. They are listed as H0 to H17 in `aristotle/header_write_sets.md`: the configuration
decodes, the flags are booleans, the vault covers capital, insurance, provider earnings and
recoverable backing, the positive-PnL totals and their bound agree, slots and activation counters
are ordered. Write H(m) for "`validate_shape()` returns `Ok(())` on m".

`validate_shape` also calls `validate_shape_full_audit_scan`, the scan over every asset slot, but
only when the crate is built with the `audit-scan` feature (or under test or Kani). That scan is a
second, larger predicate. It is not part of a default build and not part of this layer's header
half: it is listed in the ledger as `Txx.scan`.

## Why H has to be proved and is not simply checked

Most entry points end in `self.validate_shape()`, and the program repeats the check after many
instructions. But not after all of them, and some entry points check only in audit builds
(`validate_shape_audit_scan` is an empty function in a default build). The program instructions
that rely on the engine alone are:

- PermissionlessCrank in Live and Recovery mode, whenever the account's position vector is unchanged
  (a refresh, a B-chunk settlement, a step of a pending close, a Recovery refresh): the handler
  validates the shape only `if position_changed`;
- RebalanceReduce: its handler passes `wrapper_post_validate = false`, with the comment that the
  engine transition owns the post-state contract and repeating the audit would exceed the compute
  budget;
- FinalizeResetSide, InitPortfolio, ClosePortfolio and CloseSlab, whose engine calls are of class
  frame or local below.

If one of those paths could return `Ok` and leave a header that fails a check, the next instruction
that does validate would fail, and so would every one after it. So for these paths the property
below is the only thing between a defect and a market that can no longer be used.

## What to prove

`Txx.header`: for all inputs and all starting states m with H(m), if the call returns `Ok(_)`, then
H holds of the market state the call leaves behind. How much work that is depends on the class of
the entry point. The class was established by reading every Ok path of every deployed entry point;
the column "Found when read" says what was seen. Confirm the class from the source before using it,
and report any entry point for which it is wrong.

- **final** (28): every Ok path ends in `self.validate_shape()` after its last write to the
  market; an early `return Ok(..)` writes nothing. The property needs no assumption about the
  starting state. To prove: each Ok path either passes that final call with no market write after
  it, or writes nothing at all (then H at exit is H at entry).
- **delegates** (5): the body is a call to an entry point of class final, plus read-only
  checks. To prove: the property of the callee, and that nothing is written after it returns.
- **local** (6): no shape check. The checks the function can affect follow from guards inside
  the function itself, for every starting state with H. A real proof, but a small one.
- **frame** (3): no shape check. The function writes no header field that a check reads, so H
  at exit is H at entry. To prove: the set of header fields each Ok path writes.
- **mixed** (3): final on most Ok paths; the listed paths end without a check and have the
  class given in the note.
- **real** (4): no shape check in a default build. These move value between capital, insurance,
  provider earnings, backing and positive PnL, and the checks H6, H7, H10, H11 and H14 to H17 relate
  those totals. The proof needs the contracts of the kernels they call and, for H17 and for the
  totals that mirror per-domain state, invariants that relate the header to the market slots and to
  the accounts; H alone is not inductive for them. They are split by writer in
  `aristotle/header_write_sets.md` and are not part of the first tasks.
- **dispatch** (1): chooses a plan and calls other functions. Its property is the conjunction
  of the properties of the functions each branch calls; four of those are internal functions of
  class real (`refresh_account_and_certify_not_atomic`, `settle_account_b_chunk`,
  `advance_pending_close_residual_not_atomic`, `refresh_recovery_account_from_committed_state_not_atomic`).

`Txx.nopanic` is not asked for in this layer.

The column "In a default build" says whether the entry point is compiled into the deployed program.
Entry points declared under `cfg(any(kani, feature = "fuzz"))` (or with `test` added) are an API
for tests and proofs. Their properties are not applicable to the deployed program and are not asked
for; they are listed so the table matches the source.

## The entry points

| Label | Entry point | Line of `fn` in `src/v16.rs` | Class | Found when read | In a default build |
|---|---|---|---|---|---|
| T01 | `accrue_asset_path_to_not_atomic` | 18651 | local | writes `current_slot = now_slot` after refusing `now_slot < current_slot`, and `slot_last = asset.slot_last <= now_slot`; `loss_stale_active` from `encode_bool`. Ends in the audit-only scan Re-read 2026-11-21 at 80f8f0b4: refactor batch 9 moved the segment arithmetic into kernels; the header writes are unchanged | yes |
| T02 | `accrue_asset_to_not_atomic` | 18483 | local | writes `current_slot = max(current_slot, now_slot)` and `slot_last = asset.slot_last <= now_slot`; `loss_stale_active` from `encode_bool`. Ends in the audit-only scan Re-read 2026-11-21 at 80f8f0b4: refactor batch 9 moved the segment arithmetic into kernels; the header writes are unchanged | yes |
| T03 | `activate_empty_asset_slot_not_atomic` | 10673 | local | a call to T05 | yes |
| T04 | `activate_empty_market_not_atomic` | 25232 | not deployed |  | no: `cfg(any(test, kani, feature = "fuzz"))` |
| T05 | `activate_empty_market_slot_not_atomic` | 10519 | local | header-account method: writes `next_market_id`, `current_slot`, `asset_activation_count`, `last_asset_activation_slot` and two epochs | yes |
| T06 | `add_account_source_positive_pnl_not_atomic` | 15897 | final | early `Ok` for `amount == 0` before any write | yes |
| T07 | `add_fresh_counterparty_backing_not_atomic` | 12073 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T08 | `add_source_positive_claim_bound_not_atomic` | 11924 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T09 | `advance_resolved_slot_not_atomic` | 23752 | local | one write, `current_slot = kernel_advance_resolved_slot(..)` | yes |
| T10 | `advance_terminal_slab_not_atomic` | 15168 | mixed | validates at entry, then calls T09. The two `ScanProgress` returns end without a check after T09's write only, so they reduce to T09; the other three Ok paths end in the check | yes |
| T11 | `canonicalize_retired_empty_asset_slot_not_atomic` | 25204 | final |  | yes |
| T12 | `canonicalize_spent_backing_history_not_atomic` | 25040 | final | first applies retirement's normalization to the asset's two buckets (a bucket write each, no header field); every Ok path still ends in the check | yes |
| T13 | `charge_account_backing_fee_not_atomic` | 13158 | final | early `Ok(0)` for a zero fee before any write | yes |
| T14 | `claim_resolved_payout_topup_not_atomic` | 24242 | final |  | yes |
| T15 | `close_resolved_account_not_atomic` | 24284 | mixed | seven Ok paths end in the check, and the return after a source preparation is checked inside the callee. The two `ProgressOnly` returns after `settle_negative_pnl_from_principal_not_atomic`, `settle_resolved_bankruptcy_negative_pnl` and `detach_solvent_active_legs_for_resolved_close` end without a check: class real. The program's CloseResolved handler validates the shape after the call | yes |
| T16 | `consume_source_credit_lien_from_counterparty_not_atomic` | 12739 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T17 | `consume_source_credit_lien_from_insurance_not_atomic` | 12971 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T18 | `convert_released_pnl_to_capital_not_atomic` | 23239 | final | the check is skipped only for `converted == 0`, which the core returns only before any write (`released == 0`) | yes |
| T19 | `create_source_credit_lien_from_counterparty_not_atomic` | 12528 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T20 | `create_source_credit_lien_from_insurance_not_atomic` | 12858 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T21 | `credit_account_from_insurance_not_atomic` | 14518 | final | early `Ok` for `amount == 0` | yes |
| T22 | `credit_backing_provider_earnings_not_atomic` | 13122 | final | early `Ok` for `amount == 0` | yes |
| T23 | `credit_domain_insurance_budget_not_atomic` | 14338 | delegates | a call to T24 | yes |
| T24 | `credit_domain_insurance_budgets_not_atomic` | 14348 | final |  | yes |
| T25 | `cure_and_cancel_close_not_atomic` | 23551 | final | a read-only preflight, T37 (class real, no check of its own), then `cure_and_cancel_close_with_cert_not_atomic`, whose only Ok path ends in the check; that check sees T37's writes too | yes |
| T26 | `deposit_domain_insurance_not_atomic` | 14373 | final |  | yes |
| T27 | `deposit_fresh_counterparty_backing_not_atomic` | 12090 | final | after the principal adjustment it shifts the domain's provider-protection floor (a slot field); ends in the check | yes |
| T28 | `deposit_not_atomic` | 24877 | final | early `Ok` for `amount == 0` | yes |
| T29 | `deregister_empty_materialized_portfolio_not_atomic` | 11111 | frame | writes only `materialized_portfolio_count` | yes |
| T30 | `execute_batch_with_fee_loss_stale_scoped_not_atomic` | 22507 | final | the check follows the restore of `loss_stale_active` | yes |
| T31 | `execute_trade_with_fee_loss_stale_scoped_not_atomic` | 22489 | delegates | a call to T30 | yes |
| T32 | `expire_source_backing_bucket_not_atomic` | 12313 | real | writes the backing and source-credit totals and, after the payout snapshot, the payout ledger | yes |
| T33 | `finalize_side_reset_not_atomic` | 20066 | frame | early `Ok` for a side that is already Normal, before any write; otherwise writes one asset record, `resolved_payout_blocker_count` and `risk_epoch`, and calls `validate_header_aggregate_totals` (H14 to H17) in every build | yes |
| T34 | `force_asset_recovery_not_atomic` | 18454 | final | both Ok arms end in the check | yes |
| T35 | `force_close_recovery_pair_not_atomic` | 22435 | delegates | every Ok path ends in one of two calls to T31 | yes |
| T36 | `forfeit_recovery_leg_not_atomic` | 25385 | mixed | three Ok paths end in the check. The return for a leg with nothing left to settle ends without one, after `begin_full_drain_reset_inner`, then `retain_leg_as_pending_obligation` or `clear_forfeited_leg` and `begin_zero_oi_residue_resets`: class frame, to be confirmed from the write sets of those four functions | yes |
| T37 | `full_account_refresh_not_atomic` | 17750 | real | a call to `refresh_account_and_certify_not_atomic`, which ends in the audit-only scan | yes |
| T38 | `grow_asset_slot_capacity_not_atomic` | 10431 | local | header-account method: writes `asset_slot_capacity`, `config` and two epochs | yes |
| T39 | `impair_account_source_credit_lien_from_insurance_not_atomic` | 23394 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T40 | `impair_source_credit_lien_from_counterparty_not_atomic` | 12787 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T41 | `impair_source_credit_lien_from_insurance_not_atomic` | 13052 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T42 | `liquidate_account_not_atomic` | 21804 | real | both Ok paths end in the audit-only scan | yes |
| T43 | `mark_asset_drain_only_not_atomic` | 24915 | final | `Ok` for an asset that is already DrainOnly, without a write | yes |
| T44 | `permissionless_auto_crank_not_atomic` | 19443 | dispatch | Recovery: T36 then the audit-only scan, or T53. Released obligation: T36, or `clear_leg` and `begin_zero_oi_residue_resets`, then the audit-only scan. Recovery leg: `refresh_recovery_account_from_committed_state_not_atomic` (audit-only). ReleaseSourceLiens: its core ends in the check. Refresh: T02 then `refresh_account_and_certify_not_atomic` (audit-only). SettleBChunk: `settle_account_b_chunk` (no market check). Liquidate: T42, then T02. AdvanceClose: `advance_pending_close_residual_not_atomic` (audit-only); its `RecoveryRequired` error is turned into `Ok` when the call declared Recovery, which validates the shape at the declaration. DeclareRecovery: validates. CloseResolved: T09, then T32 and the check, or T15. The program's crank handler validates the shape afterwards only when the position vector changed | yes |
| T45 | `rebalance_reduce_position_not_atomic` | 21986 | real | ends in the audit-only scan; the program's RebalanceReduce handler does not validate afterwards either (compute budget) | yes |
| T46 | `recredit_terminal_claim_free_residual_for_asset_not_atomic` | 15150 | final | validates at entry and at exit | yes |
| T47 | `register_empty_materialized_portfolio_not_atomic` | 11093 | frame | writes only `materialized_portfolio_count` | yes |
| T48 | `release_account_source_credit_liens_if_unneeded_not_atomic` | 23386 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T49 | `release_source_credit_lien_from_counterparty_not_atomic` | 12570 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T50 | `release_source_credit_lien_from_insurance_not_atomic` | 12930 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T51 | `reserve_insurance_credit_not_atomic` | 12797 | not deployed |  | no: `cfg(any(kani, feature = "fuzz"))` |
| T52 | `reset_empty_asset_oracle_anchor_not_atomic` | 18424 | final |  | yes |
| T53 | `resolve_market_not_atomic` | 23737 | final |  | yes |
| T54 | `restart_empty_asset_preserving_insurance_budget_not_atomic` | 25113 | final |  | yes |
| T55 | `retire_empty_asset_not_atomic` | 24935 | final | both Ok arms end in the check | yes |
| T56 | `retire_terminal_unbudgeted_insurance_not_atomic` | 14607 | final | validates at entry and at exit | yes |
| T57 | `set_asset_raw_oracle_target_not_atomic` | 18354 | delegates | a call to T58 | yes |
| T58 | `set_asset_raw_oracle_targets_not_atomic` | 18367 | final | a changed target also restarts the price-cap anchor (`fund_px_last`, an asset field) before the check | yes |
| T59 | `sync_account_fee_to_slot_not_atomic` | 23561 | final | early `Ok(0)` when no fee is due, before any write; the skip of a settlement with nothing to settle ends in the same check | yes |
| T60 | `withdraw_backing_provider_earnings_not_atomic` | 13061 | final | early `Ok` for `amount == 0` | yes |
| T61 | `withdraw_domain_insurance_not_atomic` | 14444 | final |  | yes |
| T62 | `withdraw_fresh_counterparty_backing_not_atomic` | 12146 | final | after the principal release it shifts the domain's provider-protection floor (a slot field); ends in the check | yes |
| T63 | `withdraw_not_atomic` | 23667 | final | early `Ok` for `amount == 0` | yes |
| T64 | `withdraw_resolved_counterparty_backing_surplus_not_atomic` | 12302 | delegates | a read-only check, then a call to T62 | yes |

## The real class, split into local statements

The checks an entry point of class real can affect fall into six groups. Each group has a statement
that mentions only the header and the one account the call works on, so it can be proved one
function at a time; none needs a sum over all accounts. Write `stack` for
`c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE`
(integer division), the left side of H15.

- **R-a (H2, H3):** every write of a flag is 0, 1 or `encode_bool(..)`, and `recovery_reason` is
  written only by `from_runtime(Some(reason))`.
- **R-b (H5, H6, H14, H15):** `stack` after the call is at most `stack` before it plus the change of
  `vault`. None of the real entry points writes `vault`, so for them: `stack` does not increase.
  This is the value-conservation statement; it follows from the kernels that move value (each
  lowers one component by at least what it adds to another) and has to account for the rounding of
  backing held in `BOUND_SCALE` units. H5 and H6 follow from H15 and an unchanged `vault`.
- **R-c (H16):** `insurance - insurance_domain_budget_remaining_total` does not decrease, and
  `source_insurance_credit_reserved_total_atoms <= insurance_domain_budget_remaining_total` is
  preserved. (The second needs the per-domain relation between a domain's reserved credit and its
  remaining budget, which the audit scan checks: state it as a hypothesis on the touched domain.)
- **R-d (H7, H10, H11):** `pnl_pos_tot`, `pnl_pos_bound_tot_num` and `pnl_pos_bound_tot` are written
  only by `set_account_pnl_inner`, which keeps `pnl_pos_bound_tot == amount_from_bound_num(num)` and
  `num >= bound_num_from_amount(pnl_pos_tot)` by construction (a sum on an increase, a `max` on a
  decrease) and clamps `pnl_matured_pos_tot` to `pnl_pos_tot`. `amount_from_bound_num` rounds up, so
  `pnl_pos_tot <= pnl_pos_bound_tot` follows.
- **R-e (H17):** H17 is not inductive on its own. Let `U(account)` be
  `bound_num_from_amount(max(pnl, 0))` minus the sum of the account's `source_claim_bound_num` over
  its source domains. The local statement is: `U(account) >= 0` is preserved, and
  `(pnl_pos_bound_tot_num - source_claim_bound_total_num) - U(account)` does not decrease. With that
  quantity non-negative at the start (it is the slack owed to all other accounts), H17 follows for
  every reachable state. The step that needs care is a fall of positive PnL in
  `set_account_pnl_inner` with a nonzero `preburned_source_claim_num`: the caller must have taken
  exactly that amount out of the account's claims and out of the domain totals before the call.
- **R-f (H4, H12):** the payout ledger is written only through `from_runtime` of a ledger value, and
  only after `payout_snapshot_captured` was set, except in `initialize_resolved_payout_ledger_if_needed`,
  which sets the flag and the ledger together.

For each of T32, T37, T42, T45, and for the four internal functions the dispatcher reaches, the
header property is the conjunction of R-a to R-f over the writers that function can reach
(`aristotle/header_write_sets.md` lists them). R-b and R-e carry the weight.

## Tasks

- Structural (33 entry points, classes final and delegates): T06, T11, T12, T13, T14, T18, T21, T22, T23, T24, T25, T26, T27, T28, T30, T31, T34, T35, T43, T46, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64.
- Local (6): T01, T02, T03, T05, T09, T38; and through them the two unchecked paths of T10.
- Frame (3): T29, T33, T47; and the unchecked path of T36.
- Real (4): T32, T37, T42, T45; the unchecked paths of T15; and the four internal functions named
  under dispatch.
- Dispatch: T44, once the functions it calls are done.
