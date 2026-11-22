functions: old 782, new 799; types: old 100, new 100

## Functions: 18 changed, 17 new, 0 removed
- changed: `liquidation_projected_health_deficit_from_parts` (line 526)  in ``
- changed: `add_open_interest_for_new_position` (line 683)  in ``
- changed: `validate_public_user_fund_shape` (line 6698)  in `impl V16Config`
- changed: `canonical_accrual_price_step_v16` (line 8101)  in ``
- changed: `residual` (line 11290)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `stranded_loser_backing_owed_to_twin_num` (line 11629)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `decrement_account_source_claim_for_domain_not_atomic` (line 13485)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_account_source_credit_lien_delta` (line 15446)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_haircut_equity` (line 15589)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_signed_kf_delta_to_pnl` (line 16669)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `reserve_new_capital_backed_loss_for_source_domain_not_atomic` (line 16780)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `book_realized_kf_loss_for_source_domain_not_atomic` (line 16859)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `resolved_loss_backing_capacity_num` (line 16995)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_b_settlement_chunk_from_leg` (line 18107)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `accrue_asset_to_not_atomic` (line 18448)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `accrue_asset_path_to_not_atomic` (line 18591)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `bankruptcy_residual_single_step_capacity` (line 21166)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_no_positive_credit_equity` (line 21897)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `kernel_accrual_price_move_within_cap` (line 1309)  in `impl V16Core`
- new: `kernel_accrual_funding_num` (line 1349)  in `impl V16Core`
- new: `kernel_accrual_index_update` (line 1403)  in `impl V16Core`
- new: `kernel_residual` (line 2581)  in `impl V16Core`
- new: `kernel_no_positive_credit_equity` (line 2622)  in `impl V16Core`
- new: `kernel_haircut_equity` (line 2665)  in `impl V16Core`
- new: `kernel_gain_against_loss_after_support` (line 2712)  in `impl V16Core`
- new: `kernel_capital_backed_loss_reservation` (line 2768)  in `impl V16Core`
- new: `kernel_resolved_loss_backing_capacity` (line 2819)  in `impl V16Core`
- new: `kernel_terminal_provider_refill_capacity` (line 2850)  in `impl V16Core`
- new: `kernel_split_loss_at_capacity` (line 2870)  in `impl V16Core`
- new: `kernel_burn_account_source_claim` (line 2900)  in `impl V16Core`
- new: `kernel_bankruptcy_residual_step_capacity` (line 2939)  in `impl V16Core`
- new: `kernel_stranded_loser_backing_num` (line 2993)  in `impl V16Core`
- new: `kernel_stranded_backing_owed_to_twin` (line 3015)  in `impl V16Core`
- new: `kernel_b_settlement_chunk` (line 3060)  in `impl V16Core`
- new: `kernel_account_source_lien_delta` (line 3176)  in `impl V16Core`

## Types: 0 changed, 0 new, 0 removed
types that are or contain a changed type: []

## Kernels of the list
| Label | Function | Own text | Reaches a changed/new function (by name) | Names a changed type (own text or a reached function) | New proof needed |
|---|---|---|---|---|---|
| K01 | `apply_backing_provider_earnings_withdraw` | same | no | no | no |
| K02 | `kernel_mark_kf_stale_cohorts` | same | no | no | no |
| K03 | `kernel_settle_kf_stale_cohort` | same | no | no | no |
| K04 | `available_backing_num_for_source_credit_state` | same | no | no | no |
| K05 | `prepare_source_positive_claim_burn_delta` | same | no | no | no |
| K06 | `prepare_source_positive_claim_bound_delta` | same | no | no | no |
| K07 | `kernel_resize_leg_same_side` | same | no | no | no |
| K08 | `kernel_initial_margin_gate` | same | no | no | no |
| K09 | `kernel_locked_margin_gate` | same | no | no | no |
| K10 | `kernel_accumulate_batch_trade` | same | no | no | no |
| K11 | `kernel_settle_principal` | same | no | no | no |
| K12 | `kernel_consume_insurance_layer` | same | no | no | no |
| K13 | `kernel_resolved_payout_step` | same | no | no | no |
| K14 | `kernel_released_pnl_conversion_partition` | same | no | no | no |
| K15 | `kernel_retain_terminal_source_haircut` | same | no | no | no |
| K16 | `kernel_classify_position_delta` | same | no | no | no |
| K17 | `kernel_trade_side_oi_cap_gate` | same | no | no | no |
| K18 | `kernel_reduce_position_delta` | same | no | no | no |
| K19 | `first_actionable_slot` | same | no | no | no |
| K20 | `select_auto_crank_plan` | same | no | no | no |
| K21 | `build_trade_request_guard_summary` | same | no | no | no |
| K22 | `kernel_cert_is_current` | same | no | no | no |
| K23 | `actionable_summary_from_signals` | same | no | no | no |
| K24 | `kernel_open_close_snapshot_is_stale` | same | no | no | no |
| K25 | `kernel_social_loss_chunk_cap` | same | no | no | no |
| K26 | `apply_bankruptcy_residual_chunk_to_loss_side` | same | no | no | no |
| K27 | `kernel_bresidual_step` | same | no | no | no |
| K28 | `kernel_forfeit_residual_step` | same | no | no | no |
| K29 | `kernel_settle_resolved_pnl_after_booking` | same | no | no | no |
| K30 | `kernel_advance_leg_b_snap` | same | no | no | no |
| K31 | `kernel_normalize_social_loss_carry` | same | no | no | no |
| K32 | `kernel_retain_leg_as_pending_obligation` | same | no | no | no |
| K33 | `kernel_recovery_pending_obligation_release_allowed` | same | no | no | no |
| K34 | `kernel_clear_leg` | same | no | no | no |
| K35 | `kernel_attach_leg` | same | add_open_interest_for_new_position | no | YES |
| K36 | `prepare_counterparty_lien_create_delta` | same | no | no | no |
| K37 | `prepare_counterparty_backing_add_delta` | same | no | no | no |
| K38 | `prepare_counterparty_backing_withdraw_delta` | same | no | no | no |
| K39 | `prepare_counterparty_lien_release_delta` | same | no | no | no |
| K40 | `prepare_counterparty_lien_terminal_release_delta` | same | no | no | no |
| K41 | `prepare_counterparty_lien_consume_delta` | same | no | no | no |
| K42 | `prepare_counterparty_lien_impair_delta` | same | no | no | no |
| K43 | `prepare_counterparty_impaired_lien_retirement_delta` | same | no | no | no |
| K44 | `prepare_insurance_lien_create_delta` | same | no | no | no |
| K45 | `prepare_insurance_lien_release_delta` | same | no | no | no |
| K46 | `prepare_insurance_lien_terminal_release_delta` | same | no | no | no |
| K47 | `prepare_insurance_lien_impair_delta` | same | no | no | no |
| K48 | `health_requirements_from_base_and_target_lag` | same | no | no | no |
| K49 | `debit` | same | no | no | no |
| K50 | `apply_total_delta` | same | no | no | no |
| K51 | `credit_backing_provider_earnings_delta` | same | no | no | no |
| K52 | `set_domain_insurance_spent_delta` | same | no | no | no |
| K53 | `set_domain_insurance_budget_delta` | same | no | no | no |
| K54 | `withdraw_domain_insurance_delta` | same | no | no | no |
| K55 | `credit_account_from_insurance_delta` | same | no | no | no |
| K56 | `retire_terminal_unbudgeted_insurance_delta` | same | no | no | no |
| K57 | `asset_restart_next_counters` | same | no | no | no |
| K58 | `trade_signed_size_deltas` | same | no | no | no |
| K59 | `kernel_side_adl_index` | same | no | no | no |
| K60 | `kernel_raw_basis_after_adl_reduction` | same | no | no | no |
| K61 | `kernel_provider_protection_num` | same | no | no | no |
| K62 | `kernel_provider_protection_floor_observe` | same | no | no | no |
| K63 | `kernel_provider_protection_floor_shift` | same | no | no | no |
| K64 | `kernel_charge_account_fee` | same | no | no | no |
| K65 | `kernel_pay_kf_loss_from_free_capital` | same | no | no | no |
| K66 | `kernel_initial_margin_source_credit_needed` | same | no | no | no |
| K67 | `kernel_certified_liq_deficit` | same | no | no | no |
| K68 | `kernel_cert_requirements_after_leg_delta` | same | no | no | no |
| K69 | `kernel_residual_reward_credit` | same | no | no | no |
| K70 | `kernel_pnl_totals_after_increase` | same | no | no | no |
| K71 | `kernel_pnl_bounds_after_decrease` | same | no | no | no |
| K72 | `kernel_negative_pnl_account_count` | same | no | no | no |
| K73 | `kernel_begin_close_ledger` | same | no | no | no |
| K74 | `liquidation_projected_health_deficit_from_parts` | changed | no | no | YES |
| K75 | `add_open_interest_for_new_position` | changed | no | no | YES |
| K76 | `kernel_accrual_price_move_within_cap` | new | no | no | YES |
| K77 | `kernel_accrual_funding_num` | new | no | no | YES |
| K78 | `kernel_accrual_index_update` | new | no | no | YES |
| K79 | `kernel_residual` | new | no | no | YES |
| K80 | `kernel_no_positive_credit_equity` | new | no | no | YES |
| K81 | `kernel_haircut_equity` | new | no | no | YES |
| K82 | `kernel_gain_against_loss_after_support` | new | no | no | YES |
| K83 | `kernel_capital_backed_loss_reservation` | new | no | no | YES |
| K84 | `kernel_resolved_loss_backing_capacity` | new | no | no | YES |
| K85 | `kernel_terminal_provider_refill_capacity` | new | no | no | YES |
| K86 | `kernel_split_loss_at_capacity` | new | no | no | YES |
| K87 | `kernel_burn_account_source_claim` | new | no | no | YES |
| K88 | `kernel_bankruptcy_residual_step_capacity` | new | no | no | YES |
| K89 | `kernel_stranded_loser_backing_num` | new | no | no | YES |
| K90 | `kernel_stranded_backing_owed_to_twin` | new | no | no | YES |
| K91 | `kernel_b_settlement_chunk` | new | no | no | YES |
| K92 | `kernel_account_source_lien_delta` | new | no | no | YES |
| K93 | `canonical_accrual_price_step_v16` | changed | no | no | YES |

kernels that need a new proof: 21: K35 K74 K75 K76 K77 K78 K79 K80 K81 K82 K83 K84 K85 K86 K87 K88 K89 K90 K91 K92 K93
