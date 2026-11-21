functions: old 762, new 782; types: old 100, new 100

## Functions: 21 changed, 20 new, 0 removed
- changed: `health_cert_after_capital_debit` (line 149)  in ``
- changed: `new` (line 6311)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `is_zero_fill_or_canonical_disabled_slot` (line 9099) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl EngineAssetSlotV16Account`
- changed: `empty_for_market` (line 9166)  in `impl EngineAssetSlotV16Account`
- changed: `activate_empty_market_slot_not_atomic` (line 9574)  in `impl MarketGroupV16HeaderAccount`
- changed: `set_source_credit_for_domain` (line 10457)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `adjust_provider_principal_num` (line 10650)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `add_fresh_counterparty_backing_not_atomic` (line 11124) [cfg(any(kani, feature = "fuzz"))]  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `deposit_fresh_counterparty_backing_not_atomic` (line 11141)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `withdraw_fresh_counterparty_backing_not_atomic` (line 11197)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `incremental_initial_margin_source_credit_needed` (line 13205)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `transfer_account_residual_reward_credit` (line 14832)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `set_account_pnl_inner` (line 15024)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `pay_kf_loss_from_free_capital_not_atomic` (line 15632)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `compute_account_health_cert_with_price_override` (line 16803)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `refresh_account_and_certify_not_atomic` (line 16886)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `begin_close_progress_ledger` (line 20224)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `recertify_account_after_source_lien_change` (line 21144)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `recertify_account_after_trade_delta` (line 21172)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `charge_account_fee_current_not_atomic` (line 21938)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `canonicalize_spent_backing_history_not_atomic` (line 24047)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `kernel_provider_protection_num` (line 1283)  in `impl V16Core`
- new: `kernel_provider_protection_floor_observe` (line 1299)  in `impl V16Core`
- new: `kernel_provider_protection_floor_shift` (line 1312)  in `impl V16Core`
- new: `kernel_charge_account_fee` (line 1966)  in `impl V16Core`
- new: `kernel_pay_kf_loss_from_free_capital` (line 2016)  in `impl V16Core`
- new: `kernel_initial_margin_source_credit_needed` (line 2066)  in `impl V16Core`
- new: `kernel_certified_liq_deficit` (line 2100)  in `impl V16Core`
- new: `kernel_cert_requirements_after_leg_delta` (line 2133)  in `impl V16Core`
- new: `kernel_residual_reward_credit` (line 2188)  in `impl V16Core`
- new: `kernel_pnl_totals_after_increase` (line 2225)  in `impl V16Core`
- new: `kernel_pnl_bounds_after_decrease` (line 2258)  in `impl V16Core`
- new: `kernel_negative_pnl_account_count` (line 2286)  in `impl V16Core`
- new: `kernel_begin_close_ledger` (line 2337)  in `impl V16Core`
- new: `provider_protection_and_floor_num` (line 9132)  in `impl EngineAssetSlotV16Account`
- new: `observe_provider_protection_before_change` (line 10500)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_is_observed` (line 10545)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `remember_provider_protection_observed` (line 10561)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `shift_provider_protection_floor_num` (line 10588)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_num` (line 10613)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_floor_num` (line 10625)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`

## Types: 2 changed, 0 new, 0 removed
- changed: `MarketGroupV16ViewMut`
- changed: `EngineAssetSlotV16Account`
types that are or contain a changed type: ['EngineAssetSlotV16Account', 'Market', 'MarketGroupV16View', 'MarketGroupV16ViewMut']

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

kernels that need a new proof: 13: K61 K62 K63 K64 K65 K66 K67 K68 K69 K70 K71 K72 K73
