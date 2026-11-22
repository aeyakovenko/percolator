# Specification: the engine's kernel contracts (engine revision d1fa39f6)

`src/v16.rs` carries 63 functions annotated with contract attributes:

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
| K02 | `kernel_mark_kf_stale_cohorts` | 1120 | 0 | 1 | 0 |
| K03 | `kernel_settle_kf_stale_cohort` | 1167 | 0 | 1 | 0 |
| K04 | `available_backing_num_for_source_credit_state` | 1250 | 0 | 1 | 0 |
| K05 | `prepare_source_positive_claim_burn_delta` | 1542 | 1 | 1 | 0 |
| K06 | `prepare_source_positive_claim_bound_delta` | 1580 | 1 | 1 | 0 |
| K07 | `kernel_resize_leg_same_side` | 1686 | 1 | 1 | 0 |
| K08 | `kernel_initial_margin_gate` | 1743 | 0 | 1 | 0 |
| K09 | `kernel_locked_margin_gate` | 1770 | 1 | 1 | 0 |
| K10 | `kernel_accumulate_batch_trade` | 1807 | 0 | 1 | 0 |
| K11 | `kernel_settle_principal` | 1922 | 1 | 1 | 0 |
| K12 | `kernel_consume_insurance_layer` | 1963 | 1 | 1 | 0 |
| K13 | `kernel_resolved_payout_step` | 1999 | 0 | 1 | 0 |
| K14 | `kernel_released_pnl_conversion_partition` | 2051 | 0 | 1 | 0 |
| K15 | `kernel_retain_terminal_source_haircut` | 2084 | 0 | 1 | 0 |
| K16 | `kernel_classify_position_delta` | 2246 | 0 | 1 | 0 |
| K17 | `kernel_trade_side_oi_cap_gate` | 2555 | 0 | 1 | 0 |
| K18 | `kernel_reduce_position_delta` | 2647 | 1 | 1 | 0 |
| K19 | `first_actionable_slot` | 2705 | 0 | 1 | 0 |
| K20 | `select_auto_crank_plan` | 2854 | 0 | 1 | 0 |
| K21 | `build_trade_request_guard_summary` | 2902 | 0 | 1 | 0 |
| K22 | `kernel_cert_is_current` | 2933 | 0 | 1 | 0 |
| K23 | `actionable_summary_from_signals` | 2967 | 0 | 1 | 0 |
| K24 | `kernel_open_close_snapshot_is_stale` | 2997 | 0 | 1 | 0 |
| K25 | `kernel_social_loss_chunk_cap` | 3017 | 0 | 1 | 0 |
| K26 | `apply_bankruptcy_residual_chunk_to_loss_side` | 3067 | 0 | 1 | 1 |
| K27 | `kernel_bresidual_step` | 3148 | 1 | 1 | 0 |
| K28 | `kernel_forfeit_residual_step` | 3184 | 0 | 1 | 0 |
| K29 | `kernel_settle_resolved_pnl_after_booking` | 3218 | 1 | 1 | 0 |
| K30 | `kernel_advance_leg_b_snap` | 3259 | 0 | 1 | 0 |
| K31 | `kernel_normalize_social_loss_carry` | 3294 | 0 | 1 | 0 |
| K32 | `kernel_retain_leg_as_pending_obligation` | 3438 | 0 | 1 | 0 |
| K33 | `kernel_recovery_pending_obligation_release_allowed` | 3498 | 0 | 1 | 0 |
| K34 | `kernel_clear_leg` | 3634 | 0 | 1 | 0 |
| K35 | `kernel_attach_leg` | 3825 | 1 | 1 | 0 |
| K36 | `prepare_counterparty_lien_create_delta` | 3898 | 0 | 1 | 0 |
| K37 | `prepare_counterparty_backing_add_delta` | 3941 | 0 | 1 | 0 |
| K38 | `prepare_counterparty_backing_withdraw_delta` | 3995 | 0 | 1 | 0 |
| K39 | `prepare_counterparty_lien_release_delta` | 4232 | 0 | 1 | 0 |
| K40 | `prepare_counterparty_lien_terminal_release_delta` | 4276 | 0 | 1 | 0 |
| K41 | `prepare_counterparty_lien_consume_delta` | 4309 | 0 | 1 | 0 |
| K42 | `prepare_counterparty_lien_impair_delta` | 4363 | 0 | 1 | 0 |
| K43 | `prepare_counterparty_impaired_lien_retirement_delta` | 4414 | 0 | 1 | 0 |
| K44 | `prepare_insurance_lien_create_delta` | 4495 | 0 | 1 | 0 |
| K45 | `prepare_insurance_lien_release_delta` | 4539 | 0 | 1 | 0 |
| K46 | `prepare_insurance_lien_terminal_release_delta` | 4581 | 0 | 1 | 0 |
| K47 | `prepare_insurance_lien_impair_delta` | 4628 | 0 | 1 | 0 |
| K48 | `health_requirements_from_base_and_target_lag` | 4730 | 0 | 1 | 0 |
| K49 | `debit` | 7486 | 0 | 1 | 1 |
| K50 | `apply_total_delta` | 9847 | 0 | 1 | 0 |
| K51 | `credit_backing_provider_earnings_delta` | 11725 | 0 | 1 | 0 |
| K52 | `set_domain_insurance_spent_delta` | 12855 | 1 | 1 | 0 |
| K53 | `set_domain_insurance_budget_delta` | 12953 | 1 | 1 | 0 |
| K54 | `withdraw_domain_insurance_delta` | 13048 | 0 | 1 | 0 |
| K55 | `credit_account_from_insurance_delta` | 13129 | 0 | 1 | 0 |
| K56 | `retire_terminal_unbudgeted_insurance_delta` | 13192 | 0 | 1 | 0 |
| K57 | `asset_restart_next_counters` | 18626 | 0 | 1 | 0 |
| K58 | `trade_signed_size_deltas` | 19226 | 0 | 1 | 0 |
| K59 | `kernel_side_adl_index` | 2376 | 1 | 1 | 0 |
| K60 | `kernel_raw_basis_after_adl_reduction` | 2485 | 1 | 1 | 0 |
| K61 | `kernel_provider_protection_num` | 1285 | 0 | 1 | 0 |
| K62 | `kernel_provider_protection_floor_observe` | 1301 | 0 | 1 | 0 |
| K63 | `kernel_provider_protection_floor_shift` | 1314 | 0 | 1 | 0 |

## Priority, if the run cannot finish all of them

1. The lien and backing lifecycle: K36 to K47, K04, K05, K06.
2. Payout, insurance and principal steps: K11 to K15, K49 to K56.
3. Bankruptcy residual and social loss: K25 to K31.
4. Position and leg kernels: K07, K16 to K18, K32 to K35, K58.
5. The rest.

## What changed in engine revision c030aa40

`src/v16.rs` is uploaded again with this revision. Proofs made against the earlier revision stay
valid for every function whose body, contract and reachable callees did not change and whose types
did not change. Nine functions need work: six with new or changed text, and three whose text is the
same but which take the asset record, which gained two fields.

- K59 `kernel_side_adl_index` and K60 `kernel_raw_basis_after_adl_reduction` are new. Each side of an
  asset now keeps the sum of its legs' raw basis (`raw_basis_sum_long_q`, `raw_basis_sum_short_q`),
  and the side's A index is derived from that sum and the side's effective open interest.
- K07 `kernel_resize_leg_same_side`, K32 `kernel_retain_leg_as_pending_obligation`, K34
  `kernel_clear_leg` and K35 `kernel_attach_leg` maintain the new sum, and their contracts gained
  clauses about it. K35 also requires and ensures `a_basis == ADL_ONE`.
- K02 `kernel_mark_kf_stale_cohorts`, K03 `kernel_settle_kf_stale_cohort` and K26
  `apply_bankruptcy_residual_chunk_to_loss_side` are textually unchanged, but they take or return
  `AssetStateV16`, which now has the fields `raw_basis_sum_long_q` and `raw_basis_sum_short_q`. Their
  properties have to be established again on the new record (their contracts say which fields move;
  the two new fields must be among the ones that do not).

No other listed function needs work. A function-by-function comparison of the two revisions (every
`fn` with its attributes and body, and every `struct` and `enum`) found 24 functions with changed
text, 6 new ones and 0 removed, and two changed types (`AssetStateV16` and its account form
`AssetStateV16Account`). Apart from the nine named above, no kernel of this list has changed text, calls
a changed or new function (directly or through its callees, matched by name), or takes, returns or
names a changed type. K50 to K58 are associated functions of the market view without a `self`
parameter: they take scalars only, so the larger asset record does not reach them.

Two consequences of K59 and K60 are worth proving as their own properties, for all inputs with
`raw_basis_sum_q <= 2 * MAX_OI_SIDE_Q` (the engine never holds more raw basis on a side):

- K59.exposure: when `kernel_side_adl_index(oi, raw_sum, a)` returns `Ok(A)` with `raw_sum > 0` and
  `oi > 0`, then `A * raw_sum <= ADL_ONE * oi < (A + 1) * raw_sum`. (This is the contract's own
  clause; it is the statement that the legs' exact exposure never exceeds the side's effective open
  interest and is short of it by less than `raw_sum / ADL_ONE`.)
- K60.progress: when `kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)` returns
  `Ok(next_raw)`, then `1 <= next_raw < raw` and `oi - (e - e2) <= (raw_sum - raw) + next_raw`,
  and if `oi == raw_sum` then `next_raw == e2`.

## What changed in engine revision d1fa39f6 (since c030aa40)

`src/v16.rs` is uploaded again with this revision. Nothing proved for `c030aa40` has to be proved
again in this layer: a function-by-function comparison of the two revisions (every `fn` with its
attributes and body, and every `struct` and `enum`) found 10 functions with changed text, 10 new
ones and 0 removed, and 2 changed types (`MarketGroupV16ViewMut`, `EngineAssetSlotV16Account`). No kernel of this list has changed
text, calls a changed or new function (directly or through its callees, matched by name), or
takes, returns or names a changed type. The changed types are the market view and the per-asset
account slot; kernels take scalars and the runtime records, which did not change.

Three kernels are new. They carry the provider-protection floor of a source domain: provider
protection is the provider principal that neither a claim on the domain nor an unrepaid
consumption has taken, and the floor is the lowest protection the domain has had, moved only by
the provider's own deposits and withdrawals. All amounts are `u128` and none of the three can fail.

- K61 `kernel_provider_protection_num(principal, reserved, claims)` returns
  `min(principal, reserved - claims)` with the difference floored at zero. Contract: the result is
  at most `principal`; if `reserved >= claims` it is at most `reserved - claims` and equals one of
  the two; otherwise it is 0.
- K62 `kernel_provider_protection_floor_observe(floor, protection)` returns `min(floor, protection)`.
  Contract: the result is at most both and equals one of them.
- K63 `kernel_provider_protection_floor_shift(floor, amount, add)` returns `floor + amount` or
  `floor - amount`, saturating. Contract: when `add`, the result is at least `floor` and exceeds it
  by exactly `amount` unless it is `u128::MAX`; otherwise it is at most `floor` and below it by
  exactly `amount` unless it is 0.

Four consequences are worth proving as their own properties, for all inputs:

- K61.realization: consuming `x <= reserved` of backing together with burning `x` of claims leaves
  the protection unchanged when `claims >= x`
  (`protection(p, r - x, c - x) == protection(p, r, c)`).
- K61.monotone: protection does not fall when `reserved` rises or `claims` falls, and does not rise
  when a claim is added.
- K62.minimum: observing a sequence of protections in any order leaves the minimum of the starting
  floor and all of them.
- K63.inverse: a deposit of `a` followed by a withdrawal of `a` leaves the floor unchanged unless the
  deposit saturated. With `floor <= principal`, adding `a` to both keeps `principal - floor`
  unchanged unless an addition overflows, and subtracting `a <= principal` from the principal and
  shifting the floor down by `a` keeps it unchanged exactly when `a <= floor` (otherwise the floor
  stops at 0 and the difference becomes `principal - a`).

## What changed in engine revision cb78f1f0 (since d1fa39f6)

The first two batches of the refactor that moves the logic of the engine's methods into pure kernels.
No kernel listed above changed (specs/changed_since_d1fa39f6_at_cb78f1f0.md: own text, callees and types),
so every proof above holds for cb78f1f0 as it stands. Ten kernels are new. Each has a contract that
states its result exactly, error cases included, and Kani verifies it for every input with a
`contract_check_*` harness, so they are not tasks for the prover:

| Label | Kernel | Kani harness (all inputs) |
|---|---|---|
| K64 | `kernel_charge_account_fee` | `contract_check_kernel_charge_account_fee` |
| K65 | `kernel_pay_kf_loss_from_free_capital` | `contract_check_kernel_pay_kf_loss_from_free_capital` |
| K66 | `kernel_initial_margin_source_credit_needed` | `contract_check_kernel_initial_margin_source_credit_needed` |
| K67 | `kernel_certified_liq_deficit` | `contract_check_kernel_certified_liq_deficit` |
| K68 | `kernel_cert_requirements_after_leg_delta` | `contract_check_kernel_cert_requirements_after_leg_delta` |
| K69 | `kernel_residual_reward_credit` | `contract_check_kernel_residual_reward_credit` |
| K70 | `kernel_pnl_totals_after_increase` | `contract_check_kernel_pnl_totals_after_increase` |
| K71 | `kernel_pnl_bounds_after_decrease` | `contract_check_kernel_pnl_bounds_after_decrease` |
| K72 | `kernel_negative_pnl_account_count` | `contract_check_kernel_negative_pnl_account_count` |
| K73 | `kernel_begin_close_ledger` | `contract_check_kernel_begin_close_ledger` |

## What changed in engine revision 80f8f0b4 (since cb78f1f0)

Batches 3 to 9 of the refactor, and config admission refusing a public B chunk above MAX_VAULT_TVL.
specs/changed_since_cb78f1f0_at_80f8f0b4.md flags K35 (`kernel_attach_leg`) because its callee
`add_open_interest_for_new_position` changed text; the only change is the new `cfg_attr(all(kani, feature = "contracts"))`
contract attribute, so the deployed text is unchanged and the proofs of K35 hold as they stand. No other kernel
listed above changed. Twenty functions are new to this list. Fourteen have contracts that Kani verifies for every input:

| Label | Function | Kani harness (all inputs) |
|---|---|---|
| K74 | `liquidation_projected_health_deficit_from_parts` | `contract_check_liquidation_projected_health_deficit_from_parts` |
| K75 | `add_open_interest_for_new_position` | `contract_check_add_open_interest_for_new_position` |
| K79 | `kernel_residual` | `contract_check_kernel_residual` |
| K80 | `kernel_no_positive_credit_equity` | `contract_check_kernel_no_positive_credit_equity` |
| K81 | `kernel_haircut_equity` | `contract_check_kernel_haircut_equity` |
| K82 | `kernel_gain_against_loss_after_support` | `contract_check_kernel_gain_against_loss_after_support` |
| K83 | `kernel_capital_backed_loss_reservation` | `contract_check_kernel_capital_backed_loss_reservation` |
| K84 | `kernel_resolved_loss_backing_capacity` | `contract_check_kernel_resolved_loss_backing_capacity` |
| K85 | `kernel_terminal_provider_refill_capacity` | `contract_check_kernel_terminal_provider_refill_capacity` |
| K86 | `kernel_split_loss_at_capacity` | `contract_check_kernel_split_loss_at_capacity` |
| K87 | `kernel_burn_account_source_claim` | `contract_check_kernel_burn_account_source_claim` |
| K89 | `kernel_stranded_loser_backing_num` | `contract_check_kernel_stranded_loser_backing_num` |
| K90 | `kernel_stranded_backing_owed_to_twin` | `contract_check_kernel_stranded_backing_owed_to_twin` |
| K92 | `kernel_account_source_lien_delta` | `contract_check_kernel_account_source_lien_delta` |

Six are checked by Kani only for bounded inputs (the wide products and divisions put full-width checks
beyond the solver); their contracts are stated for every input and go to the prover:

| Label | Function | Kani harness (bounded) | Bound | Full width |
|---|---|---|---|---|
| K76 | `kernel_accrual_price_move_within_cap` | `contract_check_kernel_accrual_price_move_within_cap` | 16-bit prices, 4-bit cap and segment length | to be proved at full width (task k4, written, not yet run) |
| K77 | `kernel_accrual_funding_num` | `contract_check_kernel_accrual_funding_num` | rate near zero or near either end of i128; segment length and price near zero or near u64::MAX | to be proved at full width (task k4) |
| K78 | `kernel_accrual_index_update` | `contract_check_kernel_accrual_index_update` | 8-bit A indices and price move, a 16-bit or whole-unit funding amount, indices near zero or near either end of i128 | to be proved at full width (task k4) |
| K88 | `kernel_bankruptcy_residual_step_capacity` | `contract_check_kernel_bankruptcy_residual_step_capacity` | B index within 255 of its maximum, the other inputs 8-bit | proved at full width by the prover (task k3: contract, no panic, no index overflow, zero exactly at exhausted headroom, a nonzero step is always booked); final check pending |
| K91 | `kernel_b_settlement_chunk` | `contract_check_kernel_b_settlement_chunk` | 16-bit inputs | proved at full width by the prover (task k1: contract, no panic, progress, exact refusals) |
| K93 | `canonical_accrual_price_step_v16` | `contract_check_canonical_accrual_price_step_v16` | 8-bit inputs | proved at full width by the prover (task k2: contract, no panic, the summed movement envelope, reaching the target) |
