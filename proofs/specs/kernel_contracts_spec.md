# Specification: the engine's kernel contracts

`src/v16.rs` carries 58 functions annotated with contract attributes:

    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(P))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &T| Q))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::modifies(place))]

They are the engine's own statements of what each accounting kernel guarantees: exact deltas,
conservation, caps and disjoint classifications. Please prove each of them for all inputs, with no
bound on the values.

## How to read a contract

- `requires(P)` is a precondition on the arguments at entry.
- `ensures(|result| Q)` says that `Q` holds of the returned value. Inside `Q`, a parameter name
  denotes the value the caller passed, that is, the value at entry, also where the function body
  mutates or rebinds a by-value parameter (several kernels take `mut` parameters by value).
- `modifies(place)` says that only the named place may change.
- The attributes are compiled only with `cfg(kani)` and the `contracts` feature, so they are not
  part of the deployed build. Verify the deployed function bodies (no `cfg(kani)`, default
  features, this crate's release profile, which has `overflow-checks = true`) against them. If an
  attribute refers to a helper that exists only under `cfg(kani)`, read the helper's body as the
  definition of the predicate.

## What to prove for each function

For each function `Kxx` in the table:

1. `Kxx.contract`: for all inputs that satisfy every `requires` of the function, if the call
   returns then every `ensures` of the function holds, and nothing outside its `modifies` clauses
   (where it has any) changes.
2. `Kxx.nopanic`: for all inputs that satisfy every `requires` of the function, no execution of
   the call panics, including by arithmetic overflow. If it can panic, report the property as
   refuted with the input. This is a statement about panics only: it does not ask for a proof
   that the call returns (termination is outside what is proved here; say in the summary
   whether the body has any loop or recursion).

If a contract is false as written, report it as refuted with a concrete counterexample. Do not
weaken a contract silently; say so if you had to add a precondition.

## The functions

| Label | Function | Line of `fn` in `src/v16.rs` | `requires` | `ensures` | `modifies` |
|---|---|---|---|---|---|
| K01 | `apply_backing_provider_earnings_withdraw` | 132 | 0 | 1 | 0 |
| K02 | `kernel_mark_kf_stale_cohorts` | 1112 | 0 | 1 | 0 |
| K03 | `kernel_settle_kf_stale_cohort` | 1159 | 0 | 1 | 0 |
| K04 | `available_backing_num_for_source_credit_state` | 1242 | 0 | 1 | 0 |
| K05 | `prepare_source_positive_claim_burn_delta` | 1476 | 1 | 1 | 0 |
| K06 | `prepare_source_positive_claim_bound_delta` | 1514 | 1 | 1 | 0 |
| K07 | `kernel_resize_leg_same_side` | 1614 | 1 | 1 | 0 |
| K08 | `kernel_initial_margin_gate` | 1665 | 0 | 1 | 0 |
| K09 | `kernel_locked_margin_gate` | 1692 | 1 | 1 | 0 |
| K10 | `kernel_accumulate_batch_trade` | 1729 | 0 | 1 | 0 |
| K11 | `kernel_settle_principal` | 1844 | 1 | 1 | 0 |
| K12 | `kernel_consume_insurance_layer` | 1885 | 1 | 1 | 0 |
| K13 | `kernel_resolved_payout_step` | 1921 | 0 | 1 | 0 |
| K14 | `kernel_released_pnl_conversion_partition` | 1973 | 0 | 1 | 0 |
| K15 | `kernel_retain_terminal_source_haircut` | 2006 | 0 | 1 | 0 |
| K16 | `kernel_classify_position_delta` | 2168 | 0 | 1 | 0 |
| K17 | `kernel_trade_side_oi_cap_gate` | 2278 | 0 | 1 | 0 |
| K18 | `kernel_reduce_position_delta` | 2370 | 1 | 1 | 0 |
| K19 | `first_actionable_slot` | 2428 | 0 | 1 | 0 |
| K20 | `select_auto_crank_plan` | 2577 | 0 | 1 | 0 |
| K21 | `build_trade_request_guard_summary` | 2625 | 0 | 1 | 0 |
| K22 | `kernel_cert_is_current` | 2656 | 0 | 1 | 0 |
| K23 | `actionable_summary_from_signals` | 2690 | 0 | 1 | 0 |
| K24 | `kernel_open_close_snapshot_is_stale` | 2720 | 0 | 1 | 0 |
| K25 | `kernel_social_loss_chunk_cap` | 2740 | 0 | 1 | 0 |
| K26 | `apply_bankruptcy_residual_chunk_to_loss_side` | 2790 | 0 | 1 | 1 |
| K27 | `kernel_bresidual_step` | 2871 | 1 | 1 | 0 |
| K28 | `kernel_forfeit_residual_step` | 2907 | 0 | 1 | 0 |
| K29 | `kernel_settle_resolved_pnl_after_booking` | 2941 | 1 | 1 | 0 |
| K30 | `kernel_advance_leg_b_snap` | 2982 | 0 | 1 | 0 |
| K31 | `kernel_normalize_social_loss_carry` | 3017 | 0 | 1 | 0 |
| K32 | `kernel_retain_leg_as_pending_obligation` | 3152 | 0 | 1 | 0 |
| K33 | `kernel_recovery_pending_obligation_release_allowed` | 3203 | 0 | 1 | 0 |
| K34 | `kernel_clear_leg` | 3328 | 0 | 1 | 0 |
| K35 | `kernel_attach_leg` | 3505 | 1 | 1 | 0 |
| K36 | `prepare_counterparty_lien_create_delta` | 3576 | 0 | 1 | 0 |
| K37 | `prepare_counterparty_backing_add_delta` | 3619 | 0 | 1 | 0 |
| K38 | `prepare_counterparty_backing_withdraw_delta` | 3673 | 0 | 1 | 0 |
| K39 | `prepare_counterparty_lien_release_delta` | 3910 | 0 | 1 | 0 |
| K40 | `prepare_counterparty_lien_terminal_release_delta` | 3954 | 0 | 1 | 0 |
| K41 | `prepare_counterparty_lien_consume_delta` | 3987 | 0 | 1 | 0 |
| K42 | `prepare_counterparty_lien_impair_delta` | 4041 | 0 | 1 | 0 |
| K43 | `prepare_counterparty_impaired_lien_retirement_delta` | 4092 | 0 | 1 | 0 |
| K44 | `prepare_insurance_lien_create_delta` | 4173 | 0 | 1 | 0 |
| K45 | `prepare_insurance_lien_release_delta` | 4217 | 0 | 1 | 0 |
| K46 | `prepare_insurance_lien_terminal_release_delta` | 4259 | 0 | 1 | 0 |
| K47 | `prepare_insurance_lien_impair_delta` | 4306 | 0 | 1 | 0 |
| K48 | `health_requirements_from_base_and_target_lag` | 4408 | 0 | 1 | 0 |
| K49 | `debit` | 7102 | 0 | 1 | 1 |
| K50 | `apply_total_delta` | 9321 | 0 | 1 | 0 |
| K51 | `credit_backing_provider_earnings_delta` | 11018 | 0 | 1 | 0 |
| K52 | `set_domain_insurance_spent_delta` | 12148 | 1 | 1 | 0 |
| K53 | `set_domain_insurance_budget_delta` | 12246 | 1 | 1 | 0 |
| K54 | `withdraw_domain_insurance_delta` | 12341 | 0 | 1 | 0 |
| K55 | `credit_account_from_insurance_delta` | 12422 | 0 | 1 | 0 |
| K56 | `retire_terminal_unbudgeted_insurance_delta` | 12485 | 0 | 1 | 0 |
| K57 | `asset_restart_next_counters` | 17907 | 0 | 1 | 0 |
| K58 | `trade_signed_size_deltas` | 18505 | 0 | 1 | 0 |

## Priority, if the run cannot finish all of them

1. The lien and backing lifecycle: K36 to K47, K04, K05, K06.
2. Payout, insurance and principal steps: K11 to K15, K49 to K56.
3. Bankruptcy residual and social loss: K25 to K31.
4. Position and leg kernels: K07, K16 to K18, K32 to K35, K58.
5. The rest.
