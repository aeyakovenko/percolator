# Specification: no call into `wide_math.rs` can panic (layer P, engine revision cb78f1f0)

The arithmetic layer established the exact panic condition of every function of `src/wide_math.rs`
(labels A1 to G5 of `aristotle/wide_math_spec.md`). `src/v16.rs` calls five of those functions, at
22 call sites. This layer closes the gap between the two: at each call site, the callee's panic
condition cannot hold. The crate is built with `overflow-checks = true` and the program aborts the
transaction on a panic, so a reachable panic is a path on which an operation can never complete.

## What `src/v16.rs` can reach in `wide_math.rs` (checked by the compiler)

`src/v16.rs` names `wide_math` once, in a `use` line that imports five free functions and the type
`U256`. Which methods and operators of `U256` it uses was established by a build, not by reading:
a copy of this revision compiles (`cargo check --lib`, default features) with that `use` line
pointed at a stand-in module that exposes only the items in the table below
(`census_wide_surface.sh`, result in `specs/wide_surface_census_cb78f1f0.txt`). So `src/v16.rs` uses no
arithmetic operator of `U256` (`+ - * / %`, shifts), nothing of `I256`, and no other helper.

| `U256` item | Arithmetic label | Can it panic |
|---|---|---|
| `U256::from_u128`, `U256::from_u64` | A1 | no (proved) |
| `U256::ONE`, `is_zero` | A2 | no (proved) |
| `try_into_u128` | A3 | no (proved) |
| `==`, `<=`, `>`, `>=` (`partial_cmp` and `cmp` are proved; the comparison operators are the core library's provided methods over `partial_cmp`) | A4 | no (proved) |
| `checked_add` | B1 | no (proved) |
| `checked_sub` | B2 | no (proved) |
| `checked_mul` | B3 | no (proved) |
| `checked_div`, `checked_rem` | B4 | no (proved) |

Every one of these was proved in the arithmetic layer to return without panicking for all inputs
(`checked_div` and `checked_rem` return `None` for a zero divisor). So a panic in `wide_math.rs`
can only start in one of the five free functions, at one of the call sites listed below.

## The callees and their panic conditions (proved, task a3)

| Callee | Label | Panic condition |
|---|---|---|
| `checked_mul_div_ceil_u256` | D7 | never panics |
| `mul_div_floor_u256_with_rem` | D5 | panics exactly when `val(d) = 0` or `floor(val(a)*val(b)/val(d)) >= 2^256` |
| `wide_mul_div_floor_u128` | F3 | panics exactly when `d = 0` or `floor(a*b/d) >= 2^128` |
| `floor_div_signed_conservative_i128` | E3 | panics exactly when `d = 0`, or `n = i128::MIN` and `d = 1` |
| `wide_signed_mul_div_floor_from_k_pair` | E5 | panics exactly when `den = 0` or `abs(floor(abs_basis*(k_now-k_then)/den)) >= 2^127` |

## The call sites

Class **local**: the surrounding code of the enclosing function excludes the panic condition for
every input and every state, with no assumption. Class **leg** and class **ledger**: the panic
condition is excluded by a property of the data the function receives, which the engine establishes
elsewhere; those properties are the pieces PL1, PL2 and PR1 to PR4 below. Class **none**: the callee
never panics.

| Label | Line in `src/v16.rs` | Enclosing function | Callee | Class | Why the panic condition cannot hold (to be proved) |
|---|---|---|---|---|---|
| P01 | 79 | `backing_domain_fee_split_for_lien_delta_num` | `wide_mul_div_floor_u128` (F3) | local | `d` is the constant `MAX_MARGIN_BPS`; `insurance_share_bps <= MAX_MARGIN_BPS` is checked above, so the quotient is at most `total_fee` |
| P02 | 772 | `mul_div_ceil_u128_or_wide` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P03 | 842 | `source_lien_fee_after_backing_release` | `wide_mul_div_floor_u128` (F3) | local | `backing_before != 0` and `backing_after < backing_before` on the path that reaches the call, so the quotient is at most `fee_revenue` |
| P04 | 919 | `crystallize_source_lien_fee_for_effective` | `wide_mul_div_floor_u128` (F3) | local | `0 < impaired_effective < live_effective` on the path that reaches the call, so the quotient is at most `live_fee` |
| P05 | 1219 | `kernel_adl_scaled_accrual_index_deltas` | `floor_div_signed_conservative_i128` (E3) | local | `d` is one of the constants `FUNDING_DEN`, `POS_SCALE` or `ADL_ONE * POS_SCALE`, all greater than 1 |
| P06 | 1220 | `kernel_adl_scaled_accrual_index_deltas` | `floor_div_signed_conservative_i128` (E3) | local | `d` is one of the constants `FUNDING_DEN`, `POS_SCALE` or `ADL_ONE * POS_SCALE`, all greater than 1 |
| P07 | 2598 | `kernel_resolved_receipt_can_still_gain` | `wide_mul_div_floor_u128` (F3) | ledger | `best_rate_num <= current_payout_rate_den` by the `min`, so the quotient is at most the claim face; `d = current_payout_rate_den`, which is nonzero by the ledger-rate invariant (PR1 to PR4) |
| P08 | 5486 | `checked_mul_div_ceil_to_u128` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P09 | 14708 | `haircut_effective_support` | `wide_mul_div_floor_u128` (F3) | local | `junior_bound != 0` and `residual < junior_bound` on the path that reaches the call, so the quotient is at most `face_claim` |
| P10 | 15152 | `face_claim_to_burn_for_support` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P11 | 15458 | `net_slot_face_at_par_not_atomic` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P12 | 15596 | `twin_loser_paid_share_num` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P13 | 16286 | `leg_kf_delta_components_for_settlement_from_asset` | `wide_signed_mul_div_floor_from_k_pair` (E5) | leg | for a leg accepted by `validate_active_leg`: `a_basis == ADL_ONE`, so `den = ADL_ONE * POS_SCALE = 10^21 != 0`, and `abs(basis_pos_q) <= MAX_POSITION_ABS_Q = 10^14`, so the quotient's magnitude is below `10^14 * 2^128 / 10^21 < 2^127`; with `basis_pos_q == 0` the fast path returns before the call (PL1, PL2) |
| P14 | 16300 | `leg_kf_delta_components_for_settlement_from_asset` | `wide_signed_mul_div_floor_from_k_pair` (E5) | leg | for a leg accepted by `validate_active_leg`: `a_basis == ADL_ONE`, so `den = ADL_ONE * POS_SCALE = 10^21 != 0`, and `abs(basis_pos_q) <= MAX_POSITION_ABS_Q = 10^14`, so the quotient's magnitude is below `10^14 * 2^128 / 10^21 < 2^127`; with `basis_pos_q == 0` the fast path returns before the call (PL1, PL2) |
| P15 | 22107 | `resolved_receipt_claimable_against_ledger` | `wide_mul_div_floor_u128` (F3) | ledger | `d = current_payout_rate_den >= 1` and `current_payout_rate_num <= current_payout_rate_den` by the ledger-rate invariant (PR1 to PR4), so the quotient is at most the claim face |
| P16 | 25086 | `risk_notional_ceil` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P17 | 25163 | `trade_notional_floor` | `mul_div_floor_u256_with_rem` (D5) | local | `d` is the constant `POS_SCALE`; `a` and `b` come from `u128` values, so `a*b < 2^256` |
| P18 | 25190 | `checked_fee_bps` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P19 | 25546 | `loss_weight_for_basis` | `checked_mul_div_ceil_u256` (D7) | none | the callee never panics (D7): nothing to prove at this site |
| P20 | 25569 | `scaled_adl_delta_fast` | `floor_div_signed_conservative_i128` (E3) | local | `d` is one of the constants `FUNDING_DEN`, `POS_SCALE` or `ADL_ONE * POS_SCALE`, all greater than 1 |
| P21 | 25574 | `scaled_adl_delta_fast` | `floor_div_signed_conservative_i128` (E3) | local | `d` is one of the constants `FUNDING_DEN`, `POS_SCALE` or `ADL_ONE * POS_SCALE`, all greater than 1 |
| P22 | 25606 | `f_snap_after_settlement` | `mul_div_floor_u256_with_rem` (D5) | local | `abs_basis != 0` on the path that reaches the call; `a` and `b` come from `u128` values, so `a*b < 2^256` |

## What to prove

`Pxx` for a site of class **local**: for all inputs and all states, no execution of the enclosing
function reaches the call with arguments that satisfy the callee's panic condition.

`Pxx` for the two sites of class **leg** (both in `leg_kf_delta_components_for_settlement_from_asset(asset, leg)`):
for all `asset` and all `leg` for which `validate_active_leg(leg)` returns `Ok(())`, no execution
reaches the call with arguments that satisfy the callee's panic condition. Then:

- PL1: `PortfolioLegV16Account::try_to_runtime` returns `Ok(leg)` with `leg.active` only if
  `validate_active_leg(leg)` returns `Ok(())`.
- PL2: every function of a default build that calls `leg_kf_delta_components_for_settlement_from_asset`
  passes a leg that it obtained from `PortfolioLegV16Account::try_to_runtime` and that is active on
  the path that reaches the call. List the callers you found. (Callers under `cfg(kani)` are not in
  a default build.)

`Pxx` for the two sites of class **ledger**: for all inputs with
`1 <= ledger.current_payout_rate_den` and `ledger.current_payout_rate_num <= ledger.current_payout_rate_den`,
no execution reaches the call with arguments that satisfy the callee's panic condition. (For
`kernel_resolved_receipt_can_still_gain` the precondition `current_payout_rate_den != 0` alone may
be enough; say which precondition you needed.) Then the ledger-rate invariant

    R(header): payout_snapshot_captured != 0  implies
               1 <= resolved_payout_ledger.current_payout_rate_den  and
               resolved_payout_ledger.current_payout_rate_num <= resolved_payout_ledger.current_payout_rate_den

in four pieces:

- PR1: `kernel_recompute_resolved_payout_rate(ledger)` returns `Ok(l)` only with
  `1 <= l.current_payout_rate_den` and `l.current_payout_rate_num <= l.current_payout_rate_den`,
  for every input ledger.
- PR2: every function of a default build that writes `header.resolved_payout_ledger` either leaves
  `current_payout_rate_num` and `current_payout_rate_den` as they were, or returns `Ok` only after a
  call to `recompute_resolved_payout_rate` that follows its last write of the ledger. List the
  writers you found and say which case each is.
- PR3: `header.payout_snapshot_captured` is written only by the market's initialization (to 0, with
  the ledger `EMPTY`) and by `initialize_resolved_payout_ledger_if_needed` (to 1), which returns
  `Ok` only after `recompute_resolved_payout_rate`. So R holds initially and is preserved by every
  function that returns `Ok`.
- PR4: a receipt with `present == true` is written to an account only by
  `create_resolved_payout_receipt_if_needed`, after `initialize_resolved_payout_ledger_if_needed`
  returned `Ok` in the same call; and every caller of `resolved_receipt_claimable_against_ledger` and
  of `kernel_resolved_receipt_can_still_gain` in a default build passes the ledger it read from
  `header.resolved_payout_ledger`. State the conclusion you can draw: whether a call with
  `receipt.present` can meet a ledger that violates R, and under which assumption about the
  account (for example: the receipt was written by this market).

If a panic condition is reachable at any site, that is the result to report: give the state and
inputs. Do not add a precondition silently; if a site needs more than its class states, say exactly
what. The transaction-level consequence of a panic is an abort without state change, so a reachable
panic is a liveness defect, not a loss of funds; report it all the same.

Sites to prove: P01, P03, P04, P05, P06, P07, P09, P13, P14, P15, P17, P20, P21, P22 (14 sites), then PL1, PL2, PR1, PR2, PR3, PR4.

