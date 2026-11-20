functions: old 756, new 762; types: old 100, new 100

## Functions: 24 changed, 6 new, 0 removed
- changed: `add_open_interest_for_new_position` (line 622)  in ``
- changed: `kernel_resize_leg_same_side` (line 1628)  in `impl V16Core`
- changed: `kernel_raw_basis_for_adl_effective_quantity` (line 2264)  in `impl V16Core`
- changed: `kernel_begin_full_drain_reset` (line 3262)  in `impl V16Core`
- changed: `kernel_retain_leg_as_pending_obligation` (line 3380)  in `impl V16Core`
- changed: `kernel_clear_leg` (line 3576)  in `impl V16Core`
- changed: `kernel_attach_leg` (line 3767)  in `impl V16Core`
- changed: `default` (line 5468)  in `impl Default for AssetStateV16`
- changed: `plan_delta` (line 6954)  in `impl PositionSlotMapV16`
- changed: `from_runtime` (line 8366)  in `impl AssetStateV16Account`
- changed: `try_to_runtime` (line 8416)  in `impl AssetStateV16Account`
- changed: `asset_account_is_empty_for_activation` (line 8663) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl EngineAssetSlotV16Account`
- changed: `asset_state_is_empty_for_activation` (line 8712)  in `impl EngineAssetSlotV16Account`
- changed: `validate_asset_shape_for_view` (line 9405) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl<'a, T> MarketGroupV16View<'a, T>`
- changed: `require_empty_asset_lifecycle_state_with_policy` (line 18374)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `attach_leg_at_slot` (line 18993)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `retain_leg_as_pending_obligation` (line 19118)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `remaining_effective_oi_for_leg` (line 19160)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `clear_leg_at_slot_inner` (line 19219)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_position_delta_with_lookup_inner` (line 19329)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `resize_leg_same_side_at_slot` (line 19460)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `reduce_matching_open_interest_for_unilateral_close` (line 20096)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `apply_trade_after_refresh_not_atomic` (line 20621)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- changed: `validate_active_leg` (line 24779)  in ``
- new: `kernel_side_adl_index` (line 2318)  in `impl V16Core`
- new: `kernel_sync_side_adl_index` (line 2345)  in `impl V16Core`
- new: `kernel_raw_basis_after_adl_reduction` (line 2427)  in `impl V16Core`
- new: `side_adl_totals_are_coherent` (line 9492) [cfg(any(test, kani, feature = "audit-scan"))]  in `impl<'a, T> MarketGroupV16View<'a, T>`
- new: `effective_abs_q_for_raw_basis` (line 18168)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`
- new: `sync_side_adl_index` (line 19943)  in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`

## Types: 2 changed, 0 new, 0 removed
- changed: `AssetStateV16`
- changed: `AssetStateV16Account`
types that are or contain a changed type: ['AssetStateV16', 'AssetStateV16Account', 'EngineAssetSlotV16Account', 'Market', 'MarketGroupV16View', 'MarketGroupV16ViewMut']

## Kernels of the list
| Label | Function | Own text | Reaches a changed/new function (by name) | Names a changed type (own text or a reached function) | New proof needed |
|---|---|---|---|---|---|
| K01 | `apply_backing_provider_earnings_withdraw` | same | no | no | no |
| K02 | `kernel_mark_kf_stale_cohorts` | same | no | AssetStateV16 | YES |
| K03 | `kernel_settle_kf_stale_cohort` | same | no | AssetStateV16 | YES |
| K04 | `available_backing_num_for_source_credit_state` | same | no | no | no |
| K05 | `prepare_source_positive_claim_burn_delta` | same | no | no | no |
| K06 | `prepare_source_positive_claim_bound_delta` | same | no | no | no |
| K07 | `kernel_resize_leg_same_side` | changed | no | AssetStateV16 | YES |
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
| K26 | `apply_bankruptcy_residual_chunk_to_loss_side` | same | no | AssetStateV16 | YES |
| K27 | `kernel_bresidual_step` | same | no | no | no |
| K28 | `kernel_forfeit_residual_step` | same | no | no | no |
| K29 | `kernel_settle_resolved_pnl_after_booking` | same | no | no | no |
| K30 | `kernel_advance_leg_b_snap` | same | no | no | no |
| K31 | `kernel_normalize_social_loss_carry` | same | no | no | no |
| K32 | `kernel_retain_leg_as_pending_obligation` | changed | no | AssetStateV16 | YES |
| K33 | `kernel_recovery_pending_obligation_release_allowed` | same | no | no | no |
| K34 | `kernel_clear_leg` | changed | no | AssetStateV16 | YES |
| K35 | `kernel_attach_leg` | changed | add_open_interest_for_new_position | AssetStateV16 | YES |
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
| K59 | `kernel_side_adl_index` | new | no | no | YES |
| K60 | `kernel_raw_basis_after_adl_reduction` | new | no | no | YES |

kernels that need a new proof: 9: K02 K03 K07 K26 K32 K34 K35 K59 K60
