# Specification: market shape is preserved by every public engine entry point (layer T)

`src/v16.rs` has 64 public entry points named `*_not_atomic` on `MarketGroupV16ViewMut` (a few on
the header account). The market shape predicate of a default build is the function

    MarketGroupV16ViewMut::validate_shape(&self) -> V16Result<()>

It reads the market only (`&self`) and returns `Ok(())` exactly when the market header is well
formed: the configuration decodes, the flags are booleans, the vault covers capital and insurance,
the positive-PnL totals and their bound agree, slots and activation counters are ordered, and the
header's aggregate totals pass `validate_header_aggregate_totals`. Several charter invariants are
conjuncts of this predicate, so showing that every entry point preserves it discharges one
obligation of each of them.

`validate_shape` also calls `validate_shape_full_audit_scan`, the scan over every asset slot, but
only when the crate is built with the `audit-scan` feature (or under test or Kani). That scan is a
second, larger predicate. It is not part of a default build and not part of this layer's first
task: it is listed in the ledger as `Txx.scan` and needs the kernel contracts.

## What to prove for each entry point

`Txx.header`: for all inputs and all starting states, if the call returns `Ok(_)`, then
`validate_shape()` evaluated on the market state the call leaves behind returns `Ok(())`.

No assumption about the starting state is needed for the entry points marked **self-checking**: the
body calls `self.validate_shape()` itself, so the obligation is that every path that returns `Ok`
passes through that call and writes nothing to the market afterwards. `validate_shape` takes `&self`,
so evaluating it again on the same state gives the same result. If an entry point has an `Ok` path
that skips the call (for example an early `return Ok(..)`, or a call guarded by a condition), say
so and state exactly which paths skip it and whether those paths write to the market: a path that
skips the check and writes nothing still preserves the predicate if it held at entry, and that
weaker statement (`Txx.header` under the precondition that `validate_shape()` held at entry) is then
the property to prove. Report which form you proved.

Some self-checking entry points are marked "with an early or conditional Ok path": a source scan
found a `return Ok(..)` before the last call to `validate_shape()`, or a call under an `if`. For
those, expect the path analysis described above; the others have no such path in the source.

For the entry points marked **delegating**, the body is a thin wrapper over another entry point of
this table; prove `Txx.header` from that entry point's property.

The entry points marked **unchecked** and **unchecked in production** do not re-validate in a
default build (`validate_shape_audit_scan` is compiled out without the `audit-scan` feature). Their
property needs the precondition that `validate_shape()` held at entry and a real argument about
every write. They are listed for completeness and are not part of the first task.

`Txx.nopanic` is not asked for in this layer.

The column "In a default build" says whether the entry point is compiled into the deployed program.
Some entry points are declared under `cfg(any(kani, feature = "fuzz"))` (or with `test` added): they
are an API for tests and proofs and are absent from a default build. Their properties are not
applicable to the deployed program and are not asked for; they are listed so the table matches the
source.

## The entry points

| Label | Entry point | Line of `fn` in `src/v16.rs` | Kind | In a default build |
|---|---|---|---|---|
| T01 | `accrue_asset_path_to_not_atomic` | 16701 | unchecked in production | yes |
| T02 | `accrue_asset_to_not_atomic` | 16556 | unchecked in production | yes |
| T03 | `activate_empty_asset_slot_not_atomic` | 8854 | delegating | yes |
| T04 | `activate_empty_market_not_atomic` | 23112 | unchecked | no: `cfg(any(test, kani, feature = "fuzz"))` |
| T05 | `activate_empty_market_slot_not_atomic` | 8702 | unchecked | yes |
| T06 | `add_account_source_positive_pnl_not_atomic` | 13879 | self-checking, with an early or conditional Ok path | yes |
| T07 | `add_fresh_counterparty_backing_not_atomic` | 9996 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T08 | `add_source_positive_claim_bound_not_atomic` | 9847 | unchecked | no: `cfg(any(kani, feature = "fuzz"))` |
| T09 | `advance_resolved_slot_not_atomic` | 21646 | unchecked | yes |
| T10 | `advance_terminal_slab_not_atomic` | 13094 | self-checking, with an early or conditional Ok path | yes |
| T11 | `canonicalize_retired_empty_asset_slot_not_atomic` | 23084 | self-checking | yes |
| T12 | `canonicalize_spent_backing_history_not_atomic` | 22932 | self-checking | yes |
| T13 | `charge_account_backing_fee_not_atomic` | 11078 | self-checking, with an early or conditional Ok path | yes |
| T14 | `claim_resolved_payout_topup_not_atomic` | 22136 | self-checking | yes |
| T15 | `close_resolved_account_not_atomic` | 22178 | self-checking, with an early or conditional Ok path | yes |
| T16 | `consume_source_credit_lien_from_counterparty_not_atomic` | 10659 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T17 | `consume_source_credit_lien_from_insurance_not_atomic` | 10891 | self-checking, with an early or conditional Ok path | no: `cfg(any(kani, feature = "fuzz"))` |
| T18 | `convert_released_pnl_to_capital_not_atomic` | 21188 | self-checking, with an early or conditional Ok path | yes |
| T19 | `create_source_credit_lien_from_counterparty_not_atomic` | 10448 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T20 | `create_source_credit_lien_from_insurance_not_atomic` | 10778 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T21 | `credit_account_from_insurance_not_atomic` | 12444 | self-checking, with an early or conditional Ok path | yes |
| T22 | `credit_backing_provider_earnings_not_atomic` | 11042 | self-checking, with an early or conditional Ok path | yes |
| T23 | `credit_domain_insurance_budget_not_atomic` | 12264 | delegating | yes |
| T24 | `credit_domain_insurance_budgets_not_atomic` | 12274 | self-checking | yes |
| T25 | `cure_and_cancel_close_not_atomic` | 21503 | delegating | yes |
| T26 | `deposit_domain_insurance_not_atomic` | 12299 | self-checking | yes |
| T27 | `deposit_fresh_counterparty_backing_not_atomic` | 10012 | self-checking | yes |
| T28 | `deposit_not_atomic` | 22771 | self-checking, with an early or conditional Ok path | yes |
| T29 | `deregister_empty_materialized_portfolio_not_atomic` | 9216 | unchecked in production | yes |
| T30 | `execute_batch_with_fee_loss_stale_scoped_not_atomic` | 20440 | self-checking | yes |
| T31 | `execute_trade_with_fee_loss_stale_scoped_not_atomic` | 20422 | delegating | yes |
| T32 | `expire_source_backing_bucket_not_atomic` | 10233 | unchecked | yes |
| T33 | `finalize_side_reset_not_atomic` | 18084 | unchecked in production | yes |
| T34 | `force_asset_recovery_not_atomic` | 16527 | self-checking | yes |
| T35 | `force_close_recovery_pair_not_atomic` | 20368 | delegating | yes |
| T36 | `forfeit_recovery_leg_not_atomic` | 23265 | self-checking, with an early or conditional Ok path | yes |
| T37 | `full_account_refresh_not_atomic` | 15788 | unchecked | yes |
| T38 | `grow_asset_slot_capacity_not_atomic` | 8614 | unchecked | yes |
| T39 | `impair_account_source_credit_lien_from_insurance_not_atomic` | 21346 | self-checking, with an early or conditional Ok path | no: `cfg(any(kani, feature = "fuzz"))` |
| T40 | `impair_source_credit_lien_from_counterparty_not_atomic` | 10707 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T41 | `impair_source_credit_lien_from_insurance_not_atomic` | 10972 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T42 | `liquidate_account_not_atomic` | 19735 | unchecked in production | yes |
| T43 | `mark_asset_drain_only_not_atomic` | 22809 | self-checking | yes |
| T44 | `permissionless_auto_crank_not_atomic` | 17471 | self-checking, with an early or conditional Ok path | yes |
| T45 | `rebalance_reduce_position_not_atomic` | 19917 | unchecked in production | yes |
| T46 | `recredit_terminal_claim_free_residual_for_asset_not_atomic` | 13076 | self-checking | yes |
| T47 | `register_empty_materialized_portfolio_not_atomic` | 9198 | unchecked in production | yes |
| T48 | `release_account_source_credit_liens_if_unneeded_not_atomic` | 21338 | unchecked | no: `cfg(any(kani, feature = "fuzz"))` |
| T49 | `release_source_credit_lien_from_counterparty_not_atomic` | 10490 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T50 | `release_source_credit_lien_from_insurance_not_atomic` | 10850 | self-checking | no: `cfg(any(kani, feature = "fuzz"))` |
| T51 | `reserve_insurance_credit_not_atomic` | 10717 | self-checking, with an early or conditional Ok path | no: `cfg(any(kani, feature = "fuzz"))` |
| T52 | `reset_empty_asset_oracle_anchor_not_atomic` | 16497 | self-checking | yes |
| T53 | `resolve_market_not_atomic` | 21631 | self-checking | yes |
| T54 | `restart_empty_asset_preserving_insurance_budget_not_atomic` | 22993 | self-checking | yes |
| T55 | `retire_empty_asset_not_atomic` | 22829 | self-checking | yes |
| T56 | `retire_terminal_unbudgeted_insurance_not_atomic` | 12533 | self-checking | yes |
| T57 | `set_asset_raw_oracle_target_not_atomic` | 16436 | delegating | yes |
| T58 | `set_asset_raw_oracle_targets_not_atomic` | 16447 | self-checking | yes |
| T59 | `sync_account_fee_to_slot_not_atomic` | 21513 | self-checking, with an early or conditional Ok path | yes |
| T60 | `withdraw_backing_provider_earnings_not_atomic` | 10981 | self-checking, with an early or conditional Ok path | yes |
| T61 | `withdraw_domain_insurance_not_atomic` | 12370 | self-checking | yes |
| T62 | `withdraw_fresh_counterparty_backing_not_atomic` | 10067 | self-checking | yes |
| T63 | `withdraw_not_atomic` | 21561 | self-checking, with an early or conditional Ok path | yes |
| T64 | `withdraw_resolved_counterparty_backing_surplus_not_atomic` | 10222 | delegating | yes |
