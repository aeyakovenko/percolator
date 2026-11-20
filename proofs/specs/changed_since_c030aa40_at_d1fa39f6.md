functions: old 762, new 772; types: old 100, new 100

## Functions: 10 changed, 10 new, 0 removed
- changed: `new` (line 5887)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `is_zero_fill_or_canonical_disabled_slot` (line 8675) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl EngineAssetSlotV16Account`
- changed: `empty_for_market` (line 8742)  in `impl EngineAssetSlotV16Account`
- changed: `activate_empty_market_slot_not_atomic` (line 9150)  in `impl MarketGroupV16HeaderAccount`
- changed: `set_source_credit_for_domain` (line 10033)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `adjust_provider_principal_num` (line 10226)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `add_fresh_counterparty_backing_not_atomic` (line 10700) [cfg(any(kani, feature = "fuzz"))]  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `deposit_fresh_counterparty_backing_not_atomic` (line 10717)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `withdraw_fresh_counterparty_backing_not_atomic` (line 10773)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `canonicalize_spent_backing_history_not_atomic` (line 23703)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `kernel_provider_protection_num` (line 1285)  in `impl V16Core`
- new: `kernel_provider_protection_floor_observe` (line 1301)  in `impl V16Core`
- new: `kernel_provider_protection_floor_shift` (line 1314)  in `impl V16Core`
- new: `provider_protection_and_floor_num` (line 8708)  in `impl EngineAssetSlotV16Account`
- new: `observe_provider_protection_before_change` (line 10076)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_is_observed` (line 10121)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `remember_provider_protection_observed` (line 10137)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `shift_provider_protection_floor_num` (line 10164)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_num` (line 10189)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `provider_protection_floor_num` (line 10201)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`

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

kernels that need a new proof: 0: 
