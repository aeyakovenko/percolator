# Header write sets and the (entry point x check) split of layer T (engine revision c030aa40)

`MarketGroupV16View::validate_shape` in a default build is the conjunction of the checks H0 to H17
below. The table after it gives, for each deployed entry point with at least one Ok path that ends
without a shape check (classes local, frame, real, mixed and dispatch of
`aristotle/transition_shape_spec.md`), an over-approximation of the header fields an execution can
write: the direct assignments of the entry point and of every function it can reach. Calls are resolved by callee name (and by
receiver type where the call names it), so the set can only be too large, never too small.
Code under `cfg(test)`, `cfg(kani)` or a cargo feature is left out (37 functions).

A check that reads none of the written fields is a **frame** piece for that entry point: it holds
after the call because it held before. The remaining pairs are the real pieces; each names the
fields that have to be argued about.

The header has 39 fields. `validate_shape` reads 25 of them. Never read by it:
`market_group_id`, `resolved_payout_blocker_count`, `materialized_portfolio_count`, `stale_certificate_count`, `b_stale_account_count`, `negative_pnl_account_count`, `risk_epoch`, `asset_set_epoch`, `oracle_epoch`, `funding_epoch`, `mode`, `resolved_slot`, `payout_snapshot`, `payout_snapshot_pnl_pos_tot`.

## The checks

| Check | Statement | Header fields it reads |
|---|---|---|
| H0 | `markets.len() == asset_slot_capacity >= config.max_market_slots` | `asset_slot_capacity`, `config` |
| H1 | `config.try_to_runtime_shape()` is `Ok` | `config` |
| H2 | the four flags decode as booleans | `bankruptcy_hlock_active`, `loss_stale_active`, `payout_snapshot_captured`, `threshold_stress_active` |
| H3 | `recovery_reason` decodes | `recovery_reason` |
| H4 | `resolved_payout_ledger` decodes | `resolved_payout_ledger` |
| H5 | `vault <= MAX_VAULT_TVL` | `vault` |
| H6 | `c_tot <= vault` and `insurance <= vault` | `c_tot`, `insurance`, `vault` |
| H7 | `pnl_matured_pos_tot <= pnl_pos_tot <= pnl_pos_bound_tot` | `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_tot` |
| H8 | `slot_last <= current_slot` | `current_slot`, `slot_last` |
| H9 | `next_market_id != 0` | `next_market_id` |
| H10 | `pnl_pos_bound_tot == amount_from_bound_num(pnl_pos_bound_tot_num)` | `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num` |
| H11 | `pnl_pos_bound_tot_num >= bound_num_from_amount(pnl_pos_tot)` | `pnl_pos_bound_tot_num`, `pnl_pos_tot` |
| H12 | `payout_snapshot_captured == 0` implies `resolved_payout_ledger == EMPTY` | `payout_snapshot_captured`, `resolved_payout_ledger` |
| H13 | `asset_activation_count == 0` implies `last_asset_activation_slot == 0`; otherwise `last_asset_activation_slot <= current_slot` | `asset_activation_count`, `current_slot`, `last_asset_activation_slot` |
| H14 | `c_tot + insurance + backing_provider_earnings_total <= vault` | `backing_provider_earnings_total`, `c_tot`, `insurance`, `vault` |
| H15 | `c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE <= vault` | `backing_provider_earnings_total`, `c_tot`, `insurance`, `source_fresh_backing_total_num`, `vault` |
| H16 | `source_insurance_credit_reserved_total_atoms <= insurance` and `insurance_domain_budget_remaining_total <= insurance` | `insurance`, `insurance_domain_budget_remaining_total`, `source_insurance_credit_reserved_total_atoms` |
| H17 | `pnl_pos_bound_tot_num >= source_claim_bound_total_num` | `pnl_pos_bound_tot_num`, `source_claim_bound_total_num` |

H0 also reads the length of the slice of market slots, which no engine function changes.

## Entry points with an Ok path that ends without a shape check

The class is the one of `aristotle/transition_shape_spec.md`. For a mixed entry point and for the dispatcher the set
covers all of its paths, the checked ones included, so it is larger than what the unchecked paths write.

| Label | Entry point | Class | Header fields it can write | Checks it can affect | Frame checks |
|---|---|---|---|---|---|
| T01 | `accrue_asset_path_to_not_atomic` | local | `current_slot`, `funding_epoch`, `loss_stale_active`, `oracle_epoch`, `resolved_payout_blocker_count`, `slot_last` | H2, H8, H13 | 15 of 18 |
| T02 | `accrue_asset_to_not_atomic` | local | `current_slot`, `funding_epoch`, `loss_stale_active`, `oracle_epoch`, `resolved_payout_blocker_count`, `slot_last` | H2, H8, H13 | 15 of 18 |
| T03 | `activate_empty_asset_slot_not_atomic` | local | `asset_activation_count`, `asset_set_epoch`, `current_slot`, `last_asset_activation_slot`, `next_market_id`, `risk_epoch` | H8, H9, H13 | 15 of 18 |
| T05 | `activate_empty_market_slot_not_atomic` | local | `asset_activation_count`, `asset_set_epoch`, `current_slot`, `last_asset_activation_slot`, `next_market_id`, `risk_epoch` | H8, H9, H13 | 15 of 18 |
| T09 | `advance_resolved_slot_not_atomic` | local | `current_slot` | H8, H13 | 16 of 18 |
| T10 | `advance_terminal_slab_not_atomic` | mixed | `backing_provider_earnings_total`, `current_slot`, `insurance`, `insurance_domain_budget_remaining_total`, `payout_snapshot`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `vault` | H4, H5, H6, H8, H12, H13, H14, H15, H16, H17 | 8 of 18 |
| T15 | `close_resolved_account_not_atomic` | mixed | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `mode`, `negative_pnl_account_count`, `payout_snapshot`, `payout_snapshot_captured`, `payout_snapshot_pnl_pos_tot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `recovery_reason`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `vault` | H2, H3, H4, H5, H6, H7, H10, H11, H12, H14, H15, H16, H17 | 5 of 18 |
| T29 | `deregister_empty_materialized_portfolio_not_atomic` | frame | `materialized_portfolio_count` | none | 18 of 18 |
| T32 | `expire_source_backing_bucket_not_atomic` | real | `backing_provider_earnings_total`, `payout_snapshot`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms` | H4, H12, H14, H15, H16, H17 | 12 of 18 |
| T33 | `finalize_side_reset_not_atomic` | frame | `resolved_payout_blocker_count`, `risk_epoch` | none | 18 of 18 |
| T36 | `forfeit_recovery_leg_not_atomic` | mixed | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `mode`, `negative_pnl_account_count`, `payout_snapshot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `recovery_reason`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms` | H2, H3, H4, H6, H7, H10, H11, H12, H14, H15, H16, H17 | 6 of 18 |
| T37 | `full_account_refresh_not_atomic` | real | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `negative_pnl_account_count`, `payout_snapshot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `stale_certificate_count` | H2, H4, H6, H7, H10, H11, H12, H14, H15, H16, H17 | 7 of 18 |
| T38 | `grow_asset_slot_capacity_not_atomic` | local | `asset_set_epoch`, `asset_slot_capacity`, `config`, `risk_epoch` | H0, H1 | 16 of 18 |
| T42 | `liquidate_account_not_atomic` | real | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `mode`, `negative_pnl_account_count`, `payout_snapshot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `recovery_reason`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `stale_certificate_count` | H2, H3, H4, H6, H7, H10, H11, H12, H14, H15, H16, H17 | 6 of 18 |
| T44 | `permissionless_auto_crank_not_atomic` | dispatch | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `current_slot`, `funding_epoch`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `mode`, `negative_pnl_account_count`, `oracle_epoch`, `payout_snapshot`, `payout_snapshot_captured`, `payout_snapshot_pnl_pos_tot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `recovery_reason`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `resolved_slot`, `risk_epoch`, `slot_last`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `stale_certificate_count`, `vault` | H2, H3, H4, H5, H6, H7, H8, H10, H11, H12, H13, H14, H15, H16, H17 | 3 of 18 |
| T45 | `rebalance_reduce_position_not_atomic` | real | `b_stale_account_count`, `backing_provider_earnings_total`, `bankruptcy_hlock_active`, `c_tot`, `insurance`, `insurance_domain_budget_remaining_total`, `loss_stale_active`, `negative_pnl_account_count`, `payout_snapshot`, `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot`, `resolved_payout_blocker_count`, `resolved_payout_ledger`, `risk_epoch`, `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms`, `stale_certificate_count` | H2, H4, H6, H7, H10, H11, H12, H14, H15, H16, H17 | 7 of 18 |
| T47 | `register_empty_materialized_portfolio_not_atomic` | frame | `materialized_portfolio_count` | none | 18 of 18 |

Pairs for the entry points without a shape check: 106 real, 200 frame (of 306).

## The writers behind those entry points

Every header write of the entry points above happens in one of the functions below; they are the
places a proof has to look at. Some writers keep their checks on their own (the two accrual entry
points, the slot and activation writers, every flag write): for those the lemma is *if the checks
it can affect hold before the call and the call returns `Ok`, they hold after it*. Others only apply
a delta to a total that mirrors per-domain state (`update_source_credit_aggregate_totals`,
`update_backing_aggregate_totals`, `set_domain_insurance_spent_core`) or move value out of one total
(the `c_tot` and `insurance` writers): their checks are kept by the calling function together with
the change the delta mirrors, and H17 by a relation between the header and the accounts. For those
the unit of proof is the caller, and H alone is not an inductive invariant.

Per entry point the composition also needs that every writer's error is propagated (the caller of a
`*_not_atomic` function discards the state on `Err`). One place turns an error into `Ok`: the crank
commits a `RecoveryRequired` error when the same call declared Recovery
(`kernel_commit_declared_liquidation_recovery`); the declaration itself validates the shape.

| Lemma | Writer | Line | Fields it assigns that a check reads | Checks | Reached from |
|---|---|---|---|---|---|
| W01 | `grow_asset_slot_capacity_not_atomic` | 8913 | `asset_slot_capacity`, `config` | H0, H1 | T38 |
| W02 | `activate_empty_market_slot_not_atomic` | 9001 | `asset_activation_count`, `current_slot`, `last_asset_activation_slot`, `next_market_id` | H8, H9, H13 | T03, T05 |
| W03 | `update_source_credit_aggregate_totals` | 9724 | `source_claim_bound_total_num`, `source_fresh_backing_total_num`, `source_insurance_credit_reserved_total_atoms` | H15, H16, H17 | T10, T15, T32, T36, T37, T42, T44, T45 |
| W04 | `update_backing_aggregate_totals` | 9754 | `backing_provider_earnings_total` | H14, H15 | T10, T15, T32, T36, T37, T42, T44, T45 |
| W05 | `credit_post_snapshot_residual_not_atomic` | 10643 | `resolved_payout_ledger` | H4, H12 | T10, T15, T32, T36, T37, T42, T44, T45 |
| W06 | `set_domain_insurance_spent_core` | 12539 | `insurance_domain_budget_remaining_total` | H16 | T10, T15, T36, T37, T42, T44, T45 |
| W07 | `retire_terminal_unbudgeted_insurance_core_not_atomic` | 12872 | `insurance`, `vault` | H5, H6, H14, H15, H16 | T10 |
| W08 | `consume_domain_insurance_for_negative_pnl` | 12953 | `bankruptcy_hlock_active`, `insurance` | H2, H6, H14, H15, H16 | T15, T36, T42, T44 |
| W09 | `create_and_consume_source_credit_from_insurance_parts_not_atomic` | 13127 | `insurance` | H6, H14, H15, H16 | T15, T36, T37, T42, T44, T45 |
| W10 | `apply_terminal_insurance_recredit_not_atomic` | 13272 | `insurance` | H6, H14, H15, H16 | T10 |
| W11 | `set_account_pnl_inner` | 14280 | `pnl_matured_pos_tot`, `pnl_pos_bound_tot`, `pnl_pos_bound_tot_num`, `pnl_pos_tot` | H7, H10, H11, H17 | T15, T36, T37, T42, T44, T45 |
| W12 | `pay_kf_loss_from_free_capital_not_atomic` | 14908 | `c_tot` | H6, H14, H15 | T15, T37, T42, T44, T45 |
| W13 | `reserve_new_capital_backed_loss_for_source_domain_not_atomic` | 15200 | `c_tot` | H6, H14, H15 | T15, T36, T37, T42, T44, T45 |
| W14 | `apply_account_kf_settlement_entry` | 15677 | `loss_stale_active` | H2 | T15, T37, T42, T44, T45 |
| W15 | `collect_account_backing_utilization_fee_for_domain_not_atomic` | 16460 | `c_tot` | H6, H14, H15 | T15, T36, T37, T42, T44, T45 |
| W16 | `accrue_asset_to_not_atomic` | 16931 | `current_slot`, `loss_stale_active`, `slot_last` | H2, H8, H13 | T02, T44 |
| W17 | `accrue_asset_path_to_not_atomic` | 17076 | `current_slot`, `loss_stale_active`, `slot_last` | H2, H8, H13 | T01 |
| W18 | `declare_permissionless_recovery` | 17243 | `recovery_reason` | H3 | T15, T36, T42, T44 |
| W19 | `book_bankruptcy_residual_chunk_internal` | 19723 | `bankruptcy_hlock_active` | H2 | T15, T36, T42, T44 |
| W20 | `liquidate_account_not_atomic` | 20157 | `bankruptcy_hlock_active` | H2 | T42, T44 |
| W21 | `clear_resolved_unattributed_negative_pnl` | 21083 | `bankruptcy_hlock_active` | H2 | T15, T44 |
| W22 | `settle_resolved_bankruptcy_negative_pnl` | 21096 | `bankruptcy_hlock_active` | H2 | T15, T44 |
| W23 | `settle_negative_pnl_from_principal_core_not_atomic` | 21212 | `bankruptcy_hlock_active`, `c_tot` | H2, H6, H14, H15 | T15, T36, T37, T42, T44, T45 |
| W24 | `charge_account_fee_current_not_atomic` | 21249 | `c_tot`, `insurance` | H6, H14, H15, H16 | T15, T42, T44 |
| W25 | `recompute_resolved_payout_rate` | 21337 | `resolved_payout_ledger` | H4, H12 | T15, T44 |
| W26 | `initialize_resolved_payout_ledger_if_needed` | 21345 | `payout_snapshot_captured`, `resolved_payout_ledger` | H2, H4, H12 | T15, T44 |
| W27 | `create_resolved_payout_receipt_if_needed` | 21371 | `resolved_payout_ledger` | H4, H12 | T15, T44 |
| W28 | `apply_released_pnl_conversion_with_consumption_core_not_atomic` | 21529 | `c_tot`, `pnl_matured_pos_tot` | H6, H7, H14, H15 | T15, T44 |
| W29 | `resolve_market_not_atomic` | 22068 | `current_slot`, `loss_stale_active` | H2, H8, H13 | T44 |
| W30 | `advance_resolved_slot_not_atomic` | 22083 | `current_slot` | H8, H13 | T09, T10, T44 |
| W31 | `refine_resolved_unreceipted_bound_not_atomic` | 22584 | `resolved_payout_ledger` | H4, H12 | T15, T44 |
| W32 | `close_resolved_account_not_atomic` | 22615 | `c_tot`, `vault` | H5, H6, H14, H15 | T15, T44 |
| W33 | `settle_forfeited_leg_kf_effects` | 23590 | `loss_stale_active` | H2 | T36, T44 |

33 writers.

Functions that assign a checked field but are reached only from entry points that re-validate the shape themselves (16): `deposit_fresh_counterparty_backing_not_atomic`, `withdraw_fresh_counterparty_backing_not_atomic`, `withdraw_backing_provider_earnings_not_atomic`, `charge_account_backing_fee_not_atomic`, `set_domain_insurance_budget_core`, `deposit_domain_insurance_not_atomic`, `withdraw_domain_insurance_not_atomic`, `credit_account_from_insurance_not_atomic`, `reset_empty_asset_oracle_anchor_not_atomic`, `execute_batch_with_fee_loss_stale_scoped_not_atomic`, `cure_and_cancel_close_with_cert_not_atomic`, `withdraw_not_atomic`, `claim_resolved_payout_topup_core_not_atomic`, `deposit_not_atomic`, `retire_empty_asset_not_atomic`, `restart_empty_asset_preserving_insurance_budget_not_atomic`.

## Which function writes each field, per entry point

### T01 `accrue_asset_path_to_not_atomic`

- `current_slot` (H8, H13): written in `accrue_asset_path_to_not_atomic`
- `funding_epoch` (read by no check): written in `accrue_asset_path_to_not_atomic`
- `loss_stale_active` (H2): written in `accrue_asset_path_to_not_atomic`
- `oracle_epoch` (read by no check): written in `accrue_asset_path_to_not_atomic`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `slot_last` (H8): written in `accrue_asset_path_to_not_atomic`

### T02 `accrue_asset_to_not_atomic`

- `current_slot` (H8, H13): written in `accrue_asset_to_not_atomic`
- `funding_epoch` (read by no check): written in `accrue_asset_to_not_atomic`
- `loss_stale_active` (H2): written in `accrue_asset_to_not_atomic`
- `oracle_epoch` (read by no check): written in `accrue_asset_to_not_atomic`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `slot_last` (H8): written in `accrue_asset_to_not_atomic`

### T03 `activate_empty_asset_slot_not_atomic`

- `asset_activation_count` (H13): written in `activate_empty_market_slot_not_atomic`
- `asset_set_epoch` (read by no check): written in `activate_empty_market_slot_not_atomic`
- `current_slot` (H8, H13): written in `activate_empty_market_slot_not_atomic`
- `last_asset_activation_slot` (H13): written in `activate_empty_market_slot_not_atomic`
- `next_market_id` (H9): written in `activate_empty_market_slot_not_atomic`
- `risk_epoch` (read by no check): written in `activate_empty_market_slot_not_atomic`

### T05 `activate_empty_market_slot_not_atomic`

- `asset_activation_count` (H13): written in `activate_empty_market_slot_not_atomic`
- `asset_set_epoch` (read by no check): written in `activate_empty_market_slot_not_atomic`
- `current_slot` (H8, H13): written in `activate_empty_market_slot_not_atomic`
- `last_asset_activation_slot` (H13): written in `activate_empty_market_slot_not_atomic`
- `next_market_id` (H9): written in `activate_empty_market_slot_not_atomic`
- `risk_epoch` (read by no check): written in `activate_empty_market_slot_not_atomic`

### T09 `advance_resolved_slot_not_atomic`

- `current_slot` (H8, H13): written in `advance_resolved_slot_not_atomic`

### T10 `advance_terminal_slab_not_atomic`

- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `current_slot` (H8, H13): written in `advance_resolved_slot_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `apply_terminal_insurance_recredit_not_atomic`, `retire_terminal_unbudgeted_insurance_core_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `recompute_source_credit_domain_after_mutation`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `vault` (H5, H6, H14, H15): written in `retire_terminal_unbudgeted_insurance_core_not_atomic`

### T15 `close_resolved_account_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `book_bankruptcy_residual_chunk_internal`, `clear_resolved_unattributed_negative_pnl`, `consume_domain_insurance_for_negative_pnl`, `settle_negative_pnl_from_principal_core_not_atomic`, `settle_resolved_bankruptcy_negative_pnl`
- `c_tot` (H6, H14, H15): written in `apply_released_pnl_conversion_with_consumption_core_not_atomic`, `charge_account_fee_current_not_atomic`, `close_resolved_account_not_atomic`, `collect_account_backing_utilization_fee_for_domain_not_atomic`, `pay_kf_loss_from_free_capital_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `charge_account_fee_current_not_atomic`, `consume_domain_insurance_for_negative_pnl`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `apply_account_kf_settlement_entry`
- `mode` (read by no check): written in `declare_permissionless_recovery`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`, `initialize_resolved_payout_ledger_if_needed`
- `payout_snapshot_captured` (H2, H12): written in `initialize_resolved_payout_ledger_if_needed`
- `payout_snapshot_pnl_pos_tot` (read by no check): written in `initialize_resolved_payout_ledger_if_needed`
- `pnl_matured_pos_tot` (H7): written in `apply_released_pnl_conversion_with_consumption_core_not_atomic`, `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `recovery_reason` (H3): written in `declare_permissionless_recovery`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `create_resolved_payout_receipt_if_needed`, `credit_post_snapshot_residual_not_atomic`, `initialize_resolved_payout_ledger_if_needed`, `recompute_resolved_payout_rate`, `refine_resolved_unreceipted_bound_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `recompute_source_credit_domain_after_mutations`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `vault` (H5, H6, H14, H15): written in `close_resolved_account_not_atomic`

### T29 `deregister_empty_materialized_portfolio_not_atomic`

- `materialized_portfolio_count` (read by no check): written in `deregister_empty_materialized_portfolio_not_atomic`

### T32 `expire_source_backing_bucket_not_atomic`

- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `recompute_source_credit_domain_after_mutation`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`

### T33 `finalize_side_reset_not_atomic`

- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `risk_epoch` (read by no check): written in `finalize_side_reset_not_atomic`

### T36 `forfeit_recovery_leg_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `book_bankruptcy_residual_chunk_internal`, `consume_domain_insurance_for_negative_pnl`, `settle_negative_pnl_from_principal_core_not_atomic`
- `c_tot` (H6, H14, H15): written in `collect_account_backing_utilization_fee_for_domain_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `consume_domain_insurance_for_negative_pnl`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `settle_forfeited_leg_kf_effects`
- `mode` (read by no check): written in `declare_permissionless_recovery`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `pnl_matured_pos_tot` (H7): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `recovery_reason` (H3): written in `declare_permissionless_recovery`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`

### T37 `full_account_refresh_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `settle_negative_pnl_from_principal_core_not_atomic`
- `c_tot` (H6, H14, H15): written in `collect_account_backing_utilization_fee_for_domain_not_atomic`, `pay_kf_loss_from_free_capital_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `apply_account_kf_settlement_entry`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `pnl_matured_pos_tot` (H7): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `recompute_source_credit_domain_after_mutations`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `stale_certificate_count` (read by no check): written in `clear_account_stale`

### T38 `grow_asset_slot_capacity_not_atomic`

- `asset_set_epoch` (read by no check): written in `grow_asset_slot_capacity_not_atomic`
- `asset_slot_capacity` (H0): written in `grow_asset_slot_capacity_not_atomic`
- `config` (H0, H1): written in `grow_asset_slot_capacity_not_atomic`
- `risk_epoch` (read by no check): written in `grow_asset_slot_capacity_not_atomic`

### T42 `liquidate_account_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `book_bankruptcy_residual_chunk_internal`, `consume_domain_insurance_for_negative_pnl`, `liquidate_account_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `c_tot` (H6, H14, H15): written in `charge_account_fee_current_not_atomic`, `collect_account_backing_utilization_fee_for_domain_not_atomic`, `pay_kf_loss_from_free_capital_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `charge_account_fee_current_not_atomic`, `consume_domain_insurance_for_negative_pnl`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `apply_account_kf_settlement_entry`
- `mode` (read by no check): written in `declare_permissionless_recovery`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `pnl_matured_pos_tot` (H7): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `recovery_reason` (H3): written in `declare_permissionless_recovery`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `recompute_source_credit_domain_after_mutations`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `stale_certificate_count` (read by no check): written in `clear_account_stale`

### T44 `permissionless_auto_crank_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `book_bankruptcy_residual_chunk_internal`, `clear_resolved_unattributed_negative_pnl`, `consume_domain_insurance_for_negative_pnl`, `liquidate_account_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`, `settle_resolved_bankruptcy_negative_pnl`
- `c_tot` (H6, H14, H15): written in `apply_released_pnl_conversion_with_consumption_core_not_atomic`, `charge_account_fee_current_not_atomic`, `close_resolved_account_not_atomic`, `collect_account_backing_utilization_fee_for_domain_not_atomic`, `pay_kf_loss_from_free_capital_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `current_slot` (H8, H13): written in `accrue_asset_to_not_atomic`, `advance_resolved_slot_not_atomic`, `resolve_market_not_atomic`
- `funding_epoch` (read by no check): written in `accrue_asset_to_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `charge_account_fee_current_not_atomic`, `consume_domain_insurance_for_negative_pnl`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `accrue_asset_to_not_atomic`, `apply_account_kf_settlement_entry`, `resolve_market_not_atomic`, `settle_forfeited_leg_kf_effects`
- `mode` (read by no check): written in `declare_permissionless_recovery`, `resolve_market_not_atomic`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `oracle_epoch` (read by no check): written in `accrue_asset_to_not_atomic`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`, `initialize_resolved_payout_ledger_if_needed`
- `payout_snapshot_captured` (H2, H12): written in `initialize_resolved_payout_ledger_if_needed`
- `payout_snapshot_pnl_pos_tot` (read by no check): written in `initialize_resolved_payout_ledger_if_needed`
- `pnl_matured_pos_tot` (H7): written in `apply_released_pnl_conversion_with_consumption_core_not_atomic`, `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `recovery_reason` (H3): written in `declare_permissionless_recovery`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `create_resolved_payout_receipt_if_needed`, `credit_post_snapshot_residual_not_atomic`, `initialize_resolved_payout_ledger_if_needed`, `recompute_resolved_payout_rate`, `refine_resolved_unreceipted_bound_not_atomic`
- `resolved_slot` (read by no check): written in `resolve_market_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `recompute_source_credit_domain_after_mutations`, `release_source_credit_lien_from_counterparty_core_not_atomic`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `slot_last` (H8): written in `accrue_asset_to_not_atomic`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `stale_certificate_count` (read by no check): written in `clear_account_stale`
- `vault` (H5, H6, H14, H15): written in `close_resolved_account_not_atomic`

### T45 `rebalance_reduce_position_not_atomic`

- `b_stale_account_count` (read by no check): written in `clear_account_b_stale`, `mark_account_b_stale`
- `backing_provider_earnings_total` (H14, H15): written in `update_backing_aggregate_totals`
- `bankruptcy_hlock_active` (H2): written in `settle_negative_pnl_from_principal_core_not_atomic`
- `c_tot` (H6, H14, H15): written in `collect_account_backing_utilization_fee_for_domain_not_atomic`, `pay_kf_loss_from_free_capital_not_atomic`, `reserve_new_capital_backed_loss_for_source_domain_not_atomic`, `settle_negative_pnl_from_principal_core_not_atomic`
- `insurance` (H6, H14, H15, H16): written in `create_and_consume_source_credit_from_insurance_parts_not_atomic`
- `insurance_domain_budget_remaining_total` (H16): written in `set_domain_insurance_spent_core`
- `loss_stale_active` (H2): written in `apply_account_kf_settlement_entry`
- `negative_pnl_account_count` (read by no check): written in `set_account_pnl_after_principal_settlement`, `set_account_pnl_inner`
- `payout_snapshot` (read by no check): written in `credit_post_snapshot_residual_not_atomic`
- `pnl_matured_pos_tot` (H7): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot` (H7, H10): written in `set_account_pnl_inner`
- `pnl_pos_bound_tot_num` (H10, H11, H17): written in `set_account_pnl_inner`
- `pnl_pos_tot` (H7, H11): written in `set_account_pnl_inner`
- `resolved_payout_blocker_count` (read by no check): written in `update_resolved_payout_blocker_total`
- `resolved_payout_ledger` (H4, H12): written in `credit_post_snapshot_residual_not_atomic`
- `risk_epoch` (read by no check): written in `begin_full_drain_reset_inner`, `consume_source_credit_lien_from_counterparty_core_not_atomic`, `create_and_consume_source_credit_from_counterparty_parts_not_atomic`, `create_and_consume_source_credit_from_insurance_parts_not_atomic`, `recompute_source_credit_domain_after_mutation`, `recompute_source_credit_domain_after_mutations`, `release_source_credit_lien_from_counterparty_terminal_not_atomic`, `release_source_credit_lien_from_insurance_core_not_atomic`, `release_source_credit_lien_from_insurance_terminal_not_atomic`, `retire_source_credit_lien_from_counterparty_impaired_not_atomic`, `set_account_pnl_inner`, `sync_side_adl_index`
- `source_claim_bound_total_num` (H17): written in `update_source_credit_aggregate_totals`
- `source_fresh_backing_total_num` (H15): written in `update_source_credit_aggregate_totals`
- `source_insurance_credit_reserved_total_atoms` (H16): written in `update_source_credit_aggregate_totals`
- `stale_certificate_count` (read by no check): written in `clear_account_stale`

### T47 `register_empty_materialized_portfolio_not_atomic`

- `materialized_portfolio_count` (read by no check): written in `register_empty_materialized_portfolio_not_atomic`

