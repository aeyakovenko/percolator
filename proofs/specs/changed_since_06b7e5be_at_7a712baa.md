functions: old 804, new 804; types: old 100, new 100

## Functions: 5 changed, 0 new, 0 removed
- changed: `new_dynamic` (line 10375)  in `impl MarketGroupV16HeaderAccount`
- changed: `validate_shape` (line 10694)  in `impl<'a, T> MarketGroupV16View<'a, T>`
- changed: `group_has_position_or_loss_state_for_oracle_reset` (line 18328)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `permissionless_crank_not_atomic` (line 18868)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `h_lock_lane` (line 20448)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`

## Types: 1 changed, 0 new, 0 removed
- changed: `MarketGroupV16HeaderAccount`
types that are or contain a changed type: ['MarketGroupV16HeaderAccount', 'MarketGroupV16View', 'MarketGroupV16ViewMut']

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
| K35 | `kernel_attach_leg` | same | no | no | no |
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
| K74 | `liquidation_projected_health_deficit_from_parts` | same | no | no | no |
| K75 | `add_open_interest_for_new_position` | same | no | no | no |
| K76 | `kernel_accrual_price_move_within_cap` | same | no | no | no |
| K77 | `kernel_accrual_funding_num` | same | no | no | no |
| K78 | `kernel_accrual_index_update` | same | no | no | no |
| K79 | `kernel_residual` | same | no | no | no |
| K80 | `kernel_no_positive_credit_equity` | same | no | no | no |
| K81 | `kernel_haircut_equity` | same | no | no | no |
| K82 | `kernel_gain_against_loss_after_support` | same | no | no | no |
| K83 | `kernel_capital_backed_loss_reservation` | same | no | no | no |
| K84 | `kernel_resolved_loss_backing_capacity` | same | no | no | no |
| K85 | `kernel_terminal_provider_refill_capacity` | same | no | no | no |
| K86 | `kernel_split_loss_at_capacity` | same | no | no | no |
| K87 | `kernel_burn_account_source_claim` | same | no | no | no |
| K88 | `kernel_bankruptcy_residual_step_capacity` | same | no | no | no |
| K89 | `kernel_stranded_loser_backing_num` | same | no | no | no |
| K90 | `kernel_stranded_backing_owed_to_twin` | same | no | no | no |
| K91 | `kernel_b_settlement_chunk` | same | no | no | no |
| K92 | `kernel_account_source_lien_delta` | same | no | MarketGroupV16HeaderAccount | YES |
| K93 | `canonical_accrual_price_step_v16` | same | no | no | no |
| K94 | `kernel_cert_equity_after_capital_charge` | same | no | no | no |

kernels that need a new proof: 1: K92
