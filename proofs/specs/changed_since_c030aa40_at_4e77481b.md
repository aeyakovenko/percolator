functions: old 762, new 801; types: old 100, new 100

## Functions: 52 changed, 39 new, 0 removed
- changed: `health_cert_after_capital_debit` (line 149)  in ``
- changed: `liquidation_projected_health_deficit_from_parts` (line 526)  in ``
- changed: `add_open_interest_for_new_position` (line 683)  in ``
- changed: `public_user_fund_with_market_slots` (line 6279)  in `impl V16Config`
- changed: `validate_public_user_fund_shape` (line 6697)  in `impl V16Config`
- changed: `new` (line 7184)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `canonical_accrual_price_step_v16` (line 8092)  in ``
- changed: `from_runtime` (line 9481)  in `impl V16ConfigAccount`
- changed: `decode_runtime` (line 9542)  in `impl V16ConfigAccount`
- changed: `is_zero_fill_or_canonical_disabled_slot` (line 10006) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl EngineAssetSlotV16Account`
- changed: `empty_for_market` (line 10073)  in `impl EngineAssetSlotV16Account`
- changed: `activate_empty_market_slot_not_atomic` (line 10481)  in `impl MarketGroupV16HeaderAccount`
- changed: `residual` (line 11278)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `set_source_credit_for_domain` (line 11364)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `adjust_provider_principal_num` (line 11557)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `stranded_loser_backing_owed_to_twin_num` (line 11617)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `add_fresh_counterparty_backing_not_atomic` (line 12035) [cfg(any(kani, feature = "fuzz"))]  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `deposit_fresh_counterparty_backing_not_atomic` (line 12052)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `withdraw_fresh_counterparty_backing_not_atomic` (line 12108)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `decrement_account_source_claim_for_domain_not_atomic` (line 13473)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `incremental_initial_margin_source_credit_needed` (line 14114)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `liquidation_residual_exceeds_single_step_capacity` (line 14671)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_account_source_credit_lien_delta` (line 15434)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_haircut_equity` (line 15577)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `transfer_account_residual_reward_credit` (line 15693)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `set_account_pnl_inner` (line 15885)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `pay_kf_loss_from_free_capital_not_atomic` (line 16493)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_signed_kf_delta_to_pnl` (line 16657)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `reserve_new_capital_backed_loss_for_source_domain_not_atomic` (line 16768)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `book_realized_kf_loss_for_source_domain_not_atomic` (line 16847)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `resolved_loss_backing_capacity_num` (line 16983)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `compute_account_health_cert_with_price_override` (line 17640)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `refresh_account_and_certify_not_atomic` (line 17723)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_b_settlement_chunk_from_leg` (line 18095)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `accrue_asset_to_not_atomic` (line 18436)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `accrue_asset_path_to_not_atomic` (line 18604)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `permissionless_crank_not_atomic` (line 18825)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `refresh_recovery_account_from_committed_state_not_atomic` (line 18929)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `permissionless_auto_crank_not_atomic` (line 19396)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `begin_close_progress_ledger` (line 21062)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `bankruptcy_residual_single_step_capacity` (line 21195)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `liquidate_account_not_atomic` (line 21663)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `rebalance_reduce_position_not_atomic` (line 21845)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `account_no_positive_credit_equity` (line 21923)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `recertify_account_after_source_lien_change` (line 21944)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `recertify_account_after_trade_delta` (line 21972)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `charge_account_fee_current_not_atomic` (line 22738)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `charge_account_fee_after_loss_settlement` (line 22772)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `release_account_source_credit_liens_if_unneeded_core_not_atomic` (line 23114)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `sync_account_fee_to_slot_not_atomic` (line 23420)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `close_resolved_account_not_atomic` (line 24082)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `canonicalize_spent_backing_history_not_atomic` (line 24838)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `kernel_accrual_price_move_within_cap` (line 1309)  in `impl V16Core`
- new: `kernel_accrual_funding_num` (line 1349)  in `impl V16Core`
- new: `kernel_accrual_index_update` (line 1404)  in `impl V16Core`
- new: `kernel_provider_protection_num` (line 1483)  in `impl V16Core`
- new: `kernel_provider_protection_floor_observe` (line 1499)  in `impl V16Core`
- new: `kernel_provider_protection_floor_shift` (line 1512)  in `impl V16Core`
- new: `kernel_charge_account_fee` (line 2166)  in `impl V16Core`
- new: `kernel_pay_kf_loss_from_free_capital` (line 2216)  in `impl V16Core`
- new: `kernel_initial_margin_source_credit_needed` (line 2266)  in `impl V16Core`
- new: `kernel_certified_liq_deficit` (line 2300)  in `impl V16Core`
- new: `kernel_cert_requirements_after_leg_delta` (line 2333)  in `impl V16Core`
- new: `kernel_residual_reward_credit` (line 2388)  in `impl V16Core`
- new: `kernel_pnl_totals_after_increase` (line 2425)  in `impl V16Core`
- new: `kernel_pnl_bounds_after_decrease` (line 2458)  in `impl V16Core`
- new: `kernel_negative_pnl_account_count` (line 2486)  in `impl V16Core`
- new: `kernel_begin_close_ledger` (line 2537)  in `impl V16Core`
- new: `kernel_residual` (line 2582)  in `impl V16Core`
- new: `kernel_no_positive_credit_equity` (line 2623)  in `impl V16Core`
- new: `kernel_haircut_equity` (line 2666)  in `impl V16Core`
- new: `kernel_gain_against_loss_after_support` (line 2713)  in `impl V16Core`
- new: `kernel_capital_backed_loss_reservation` (line 2769)  in `impl V16Core`
- new: `kernel_resolved_loss_backing_capacity` (line 2820)  in `impl V16Core`
- new: `kernel_terminal_provider_refill_capacity` (line 2851)  in `impl V16Core`
- new: `kernel_split_loss_at_capacity` (line 2871)  in `impl V16Core`
- new: `kernel_burn_account_source_claim` (line 2901)  in `impl V16Core`
- new: `kernel_bankruptcy_residual_step_capacity` (line 2940)  in `impl V16Core`
- new: `kernel_stranded_loser_backing_num` (line 2994)  in `impl V16Core`
- new: `kernel_stranded_backing_owed_to_twin` (line 3016)  in `impl V16Core`
- new: `kernel_b_settlement_chunk` (line 3061)  in `impl V16Core`
- new: `kernel_account_source_lien_delta` (line 3177)  in `impl V16Core`
- new: `provider_protection_and_floor_num` (line 10039)  in `impl EngineAssetSlotV16Account`
- new: `observe_provider_protection_before_change` (line 11407)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_is_observed` (line 11452)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `remember_provider_protection_observed` (line 11468)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `shift_provider_protection_floor_num` (line 11495)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_num` (line 11520)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_floor_num` (line 11532)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `accrue_asset_to_admitted_not_atomic` (line 18472)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `accrue_asset_path_to_admitted_not_atomic` (line 18636)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`

## Types: 4 changed, 0 new, 0 removed
- changed: `V16Config`
- changed: `MarketGroupV16ViewMut`
- changed: `V16ConfigAccount`
- changed: `EngineAssetSlotV16Account`
types that are or contain a changed type: ['EngineAssetSlotV16Account', 'Market', 'MarketGroupV16HeaderAccount', 'MarketGroupV16View', 'MarketGroupV16ViewMut', 'V16Config', 'V16ConfigAccount']

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
| K61 | `kernel_provider_protection_num` | new | no | no | YES |
| K62 | `kernel_provider_protection_floor_observe` | new | no | no | YES |
| K63 | `kernel_provider_protection_floor_shift` | new | no | no | YES |
| K64 | `kernel_charge_account_fee` | new | no | no | YES |
| K65 | `kernel_pay_kf_loss_from_free_capital` | new | no | no | YES |
| K66 | `kernel_initial_margin_source_credit_needed` | new | no | no | YES |
| K67 | `kernel_certified_liq_deficit` | new | no | no | YES |
| K68 | `kernel_cert_requirements_after_leg_delta` | new | no | no | YES |
| K69 | `kernel_residual_reward_credit` | new | no | no | YES |
| K70 | `kernel_pnl_totals_after_increase` | new | no | no | YES |
| K71 | `kernel_pnl_bounds_after_decrease` | new | no | no | YES |
| K72 | `kernel_negative_pnl_account_count` | new | no | no | YES |
| K73 | `kernel_begin_close_ledger` | new | no | no | YES |
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
| K92 | `kernel_account_source_lien_delta` | new | new | EngineAssetSlotV16Account, Market, MarketGroupV16HeaderAccount | YES |
| K93 | `canonical_accrual_price_step_v16` | changed | no | no | YES |

kernels that need a new proof: 34: K35 K61 K62 K63 K64 K65 K66 K67 K68 K69 K70 K71 K72 K73 K74 K75 K76 K77 K78 K79 K80 K81 K82 K83 K84 K85 K86 K87 K88 K89 K90 K91 K92 K93
