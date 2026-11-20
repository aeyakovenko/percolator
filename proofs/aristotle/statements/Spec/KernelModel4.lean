module

public import Spec.KernelModel3

/-!
# Independent mathematical model for the source-credit, lien and backing delta kernels

A mathematical reading of the `kani::requires` and `kani::ensures` clauses of these associated
functions of `V16Core` in `src/v16.rs` (K36, K37 and K38 are in `Spec.KernelModel`):

* K04 `available_backing_num_for_source_credit_state`
* K05 `prepare_source_positive_claim_burn_delta`
* K06 `prepare_source_positive_claim_bound_delta`
* K39 `prepare_counterparty_lien_release_delta`
* K40 `prepare_counterparty_lien_terminal_release_delta`
* K41 `prepare_counterparty_lien_consume_delta`
* K42 `prepare_counterparty_lien_impair_delta`
* K43 `prepare_counterparty_impaired_lien_retirement_delta`
* K44 `prepare_insurance_lien_create_delta`
* K45 `prepare_insurance_lien_release_delta`
* K46 `prepare_insurance_lien_terminal_release_delta`
* K47 `prepare_insurance_lien_impair_delta`
* K48 `health_requirements_from_base_and_target_lag`

Conventions (as in `Spec.KernelModel`):

* Rust unsigned integers are modelled by `Nat`; `U64`, `U128` say a value is representable in
  its Rust type, and `Bucket.WF`, `Source.WF`, `Reservation.WF` say so of every field.
* `wrapping_add` / `wrapping_sub` on `u128` are `wrapAdd128` / `wrapSub128` (arithmetic modulo
  `2^128`); `a.min(b)` is `min a b`.
* Plain `+` / `-` inside an `ensures` clause are read as exact integer arithmetic.
* Parameter names denote the values the caller passed, also where the body mutates the
  by-value parameter (`mut bucket`, `mut source`, `mut reservation`). The `_ensures` predicate
  constrains the `Ok` payload; the `Err(_) => true` arm needs no predicate.
* Derived `PartialEq` on these `Copy` structs is structural equality, i.e. `=`.
-/

@[expose] public section

namespace Spec.Kernel

/-- Rust `struct InsuranceCreditReservationV16` (all fields; `u64`/`u128` fields as `Nat`). -/
structure Reservation where
  /-- `insurance_credit_reserved_num: u128` -/
  insurance_credit_reserved_num : Nat
  /-- `valid_liened_insurance_num: u128` -/
  valid_liened_insurance_num : Nat
  /-- `impaired_liened_insurance_num: u128` -/
  impaired_liened_insurance_num : Nat
  /-- `consumed_insurance_num: u128` -/
  consumed_insurance_num : Nat
  /-- `source_credit_epoch: u64` -/
  source_credit_epoch : Nat
  deriving DecidableEq, Repr

/-- Every field of an `InsuranceCreditReservationV16` value is in range of its Rust type. -/
def Reservation.WF (r : Reservation) : Prop :=
  U128 r.insurance_credit_reserved_num ∧
  U128 r.valid_liened_insurance_num ∧
  U128 r.impaired_liened_insurance_num ∧
  U128 r.consumed_insurance_num ∧
  U64 r.source_credit_epoch

/-! ## K04 `available_backing_num_for_source_credit_state(state)` -/

/-- K04. `ensures` for an `Ok(a)` result: `fresh_reserved_backing_num >=
valid_liened_backing_num`, `insurance_credit_reserved_num >=
valid_liened_insurance_num.wrapping_add(impaired_liened_insurance_num)`, and
`a == fresh_reserved_backing_num.wrapping_sub(valid_liened_backing_num) +
(insurance_credit_reserved_num - valid_liened_insurance_num - impaired_liened_insurance_num)`
(the outer `+` and `-` read as exact integer arithmetic). -/
def available_backing_ensures (state : Source) (a : Nat) : Prop :=
  state.fresh_reserved_backing_num ≥ state.valid_liened_backing_num ∧
  state.insurance_credit_reserved_num ≥
    wrapAdd128 state.valid_liened_insurance_num state.impaired_liened_insurance_num ∧
  (a : Int) = (wrapSub128 state.fresh_reserved_backing_num state.valid_liened_backing_num : Int) +
    ((state.insurance_credit_reserved_num : Int) - (state.valid_liened_insurance_num : Int) -
      (state.impaired_liened_insurance_num : Int))

/-! ## K05 `prepare_source_positive_claim_burn_delta(source, face_burn_num)` -/

/-- K05. `requires`: `source.exact_positive_claim_num <= source.positive_claim_bound_num &&
face_burn_num <= source.positive_claim_bound_num`. -/
def claim_burn_requires (source : Source) (face_burn_num : Nat) : Prop :=
  source.exact_positive_claim_num ≤ source.positive_claim_bound_num ∧
  face_burn_num ≤ source.positive_claim_bound_num

/-- K05. `ensures` for an `Ok(s)` result: the claim bound drops by `face_burn_num`, the exact
claim drops by `face_burn_num.min(exact_positive_claim_num)` (both `wrapping_sub`), and every
other field of the source state is unchanged. -/
def claim_burn_ensures (source : Source) (face_burn_num : Nat) (s : Source) : Prop :=
  s.positive_claim_bound_num = wrapSub128 source.positive_claim_bound_num face_burn_num ∧
  s.exact_positive_claim_num =
    wrapSub128 source.exact_positive_claim_num (min face_burn_num source.exact_positive_claim_num) ∧
  s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
  s.spent_backing_num = source.spent_backing_num ∧
  s.provider_receivable_num = source.provider_receivable_num ∧
  s.valid_liened_backing_num = source.valid_liened_backing_num ∧
  s.impaired_liened_backing_num = source.impaired_liened_backing_num ∧
  s.insurance_credit_reserved_num = source.insurance_credit_reserved_num ∧
  s.valid_liened_insurance_num = source.valid_liened_insurance_num ∧
  s.impaired_liened_insurance_num = source.impaired_liened_insurance_num ∧
  s.credit_rate_num = source.credit_rate_num ∧
  s.credit_epoch = source.credit_epoch

/-! ## K06 `prepare_source_positive_claim_bound_delta(source, claim_bound_num, exact_claim_num)` -/

/-- K06. `requires`: `exact_claim_num <= claim_bound_num`. -/
def claim_bound_requires (claim_bound_num exact_claim_num : Nat) : Prop :=
  exact_claim_num ≤ claim_bound_num

/-- K06. `ensures` for an `Ok(s)` result: the claim bound rises by `claim_bound_num` and the
exact claim by `exact_claim_num` (both `wrapping_add`); fresh reserved backing, valid liened
backing, spent backing and the credit rate are unchanged. -/
def claim_bound_ensures (source : Source) (claim_bound_num exact_claim_num : Nat) (s : Source) :
    Prop :=
  s.positive_claim_bound_num = wrapAdd128 source.positive_claim_bound_num claim_bound_num ∧
  s.exact_positive_claim_num = wrapAdd128 source.exact_positive_claim_num exact_claim_num ∧
  s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
  s.valid_liened_backing_num = source.valid_liened_backing_num ∧
  s.spent_backing_num = source.spent_backing_num ∧
  s.credit_rate_num = source.credit_rate_num

/-! ## K39, K40 counterparty lien release (live and terminal) -/

/-- K39 and K40. `ensures` of `prepare_counterparty_lien_release_delta(bucket, source,
current_slot, amount)` and of `prepare_counterparty_lien_terminal_release_delta(bucket, source,
amount)` (the two clauses are identical) for an `Ok((b, s))` result: either `amount == 0` and
both values are returned unchanged, or `amount` moves from the bucket's valid liened backing
(`wrapping_sub`) to its fresh unliened backing (`wrapping_add`), the source's valid liened
backing drops by `amount` (`wrapping_sub`), and the bucket's consumed liened backing and the
source's fresh reserved backing, spent backing and provider receivable are unchanged. -/
def lien_release_ensures (bucket : Bucket) (source : Source) (amount : Nat) (b : Bucket)
    (s : Source) : Prop :=
  (amount = 0 ∧ b = bucket ∧ s = source) ∨
  (b.valid_liened_backing_num = wrapSub128 bucket.valid_liened_backing_num amount ∧
   b.fresh_unliened_backing_num = wrapAdd128 bucket.fresh_unliened_backing_num amount ∧
   b.consumed_liened_backing_num = bucket.consumed_liened_backing_num ∧
   s.valid_liened_backing_num = wrapSub128 source.valid_liened_backing_num amount ∧
   s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
   s.spent_backing_num = source.spent_backing_num ∧
   s.provider_receivable_num = source.provider_receivable_num)

/-! ## K41 `prepare_counterparty_lien_consume_delta(bucket, source, amount)` -/

/-- K41. `ensures` for an `Ok((b, s))` result: the bucket's valid liened backing drops by
`amount` and its consumed liened backing rises by `amount`, its fresh unliened backing is
unchanged; the source's valid liened backing and fresh reserved backing drop by `amount`, and
its spent backing and provider receivable rise by `amount` (all `wrapping_*`). -/
def lien_consume_ensures (bucket : Bucket) (source : Source) (amount : Nat) (b : Bucket)
    (s : Source) : Prop :=
  b.valid_liened_backing_num = wrapSub128 bucket.valid_liened_backing_num amount ∧
  b.consumed_liened_backing_num = wrapAdd128 bucket.consumed_liened_backing_num amount ∧
  b.fresh_unliened_backing_num = bucket.fresh_unliened_backing_num ∧
  s.valid_liened_backing_num = wrapSub128 source.valid_liened_backing_num amount ∧
  s.fresh_reserved_backing_num = wrapSub128 source.fresh_reserved_backing_num amount ∧
  s.spent_backing_num = wrapAdd128 source.spent_backing_num amount ∧
  s.provider_receivable_num = wrapAdd128 source.provider_receivable_num amount

/-! ## K42 `prepare_counterparty_lien_impair_delta(bucket, source, amount)` -/

/-- K42. `ensures` for an `Ok((b, s))` result: either `amount == 0` and both values are
returned unchanged, or the bucket's valid liened backing drops by `amount` and its impaired
liened backing rises by `amount`, its fresh unliened backing is unchanged; the source's valid
liened backing and fresh reserved backing drop by `amount`, its impaired liened backing rises by
`amount`, and its spent backing is unchanged (all `wrapping_*`). -/
def lien_impair_ensures (bucket : Bucket) (source : Source) (amount : Nat) (b : Bucket)
    (s : Source) : Prop :=
  (amount = 0 ∧ b = bucket ∧ s = source) ∨
  (b.valid_liened_backing_num = wrapSub128 bucket.valid_liened_backing_num amount ∧
   b.impaired_liened_backing_num = wrapAdd128 bucket.impaired_liened_backing_num amount ∧
   b.fresh_unliened_backing_num = bucket.fresh_unliened_backing_num ∧
   s.valid_liened_backing_num = wrapSub128 source.valid_liened_backing_num amount ∧
   s.fresh_reserved_backing_num = wrapSub128 source.fresh_reserved_backing_num amount ∧
   s.impaired_liened_backing_num = wrapAdd128 source.impaired_liened_backing_num amount ∧
   s.spent_backing_num = source.spent_backing_num)

/-! ## K43 `prepare_counterparty_impaired_lien_retirement_delta(bucket, source, amount)` -/

/-- K43. `ensures` for an `Ok((b, s))` result: either `amount == 0` and both values are
returned unchanged, or the impaired liened backing of the bucket and of the source drop by
`amount` (`wrapping_sub`), and the bucket's fresh unliened, valid liened and consumed liened
backing and the source's fresh reserved backing, valid liened backing, spent backing, provider
receivable, insurance credit reserved, valid liened insurance and impaired liened insurance are
unchanged. -/
def impaired_retirement_ensures (bucket : Bucket) (source : Source) (amount : Nat) (b : Bucket)
    (s : Source) : Prop :=
  (amount = 0 ∧ b = bucket ∧ s = source) ∨
  (b.impaired_liened_backing_num = wrapSub128 bucket.impaired_liened_backing_num amount ∧
   s.impaired_liened_backing_num = wrapSub128 source.impaired_liened_backing_num amount ∧
   b.fresh_unliened_backing_num = bucket.fresh_unliened_backing_num ∧
   b.valid_liened_backing_num = bucket.valid_liened_backing_num ∧
   b.consumed_liened_backing_num = bucket.consumed_liened_backing_num ∧
   s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
   s.valid_liened_backing_num = source.valid_liened_backing_num ∧
   s.spent_backing_num = source.spent_backing_num ∧
   s.provider_receivable_num = source.provider_receivable_num ∧
   s.insurance_credit_reserved_num = source.insurance_credit_reserved_num ∧
   s.valid_liened_insurance_num = source.valid_liened_insurance_num ∧
   s.impaired_liened_insurance_num = source.impaired_liened_insurance_num)

/-! ## K44 `prepare_insurance_lien_create_delta(reservation, source, amount)` -/

/-- K44. `ensures` for an `Ok((r, s))` result: either `amount == 0` and both values are
returned unchanged, or the valid liened insurance of the reservation and of the source rise by
`amount` (`wrapping_add`), and the reservation's insurance credit reserved and impaired liened
insurance and the source's fresh reserved backing and spent backing are unchanged. -/
def insurance_lien_create_ensures (reservation : Reservation) (source : Source) (amount : Nat)
    (r : Reservation) (s : Source) : Prop :=
  (amount = 0 ∧ r = reservation ∧ s = source) ∨
  (r.valid_liened_insurance_num = wrapAdd128 reservation.valid_liened_insurance_num amount ∧
   r.insurance_credit_reserved_num = reservation.insurance_credit_reserved_num ∧
   r.impaired_liened_insurance_num = reservation.impaired_liened_insurance_num ∧
   s.valid_liened_insurance_num = wrapAdd128 source.valid_liened_insurance_num amount ∧
   s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
   s.spent_backing_num = source.spent_backing_num)

/-! ## K45 `prepare_insurance_lien_release_delta(reservation, source, amount)` -/

/-- K45. `ensures` for an `Ok((r, s))` result: either `amount == 0` and both values are
returned unchanged, or the valid liened insurance of the reservation and of the source drop by
`amount` (`wrapping_sub`), and the reservation's insurance credit reserved and impaired liened
insurance and the source's insurance credit reserved and fresh reserved backing are
unchanged. -/
def insurance_lien_release_ensures (reservation : Reservation) (source : Source) (amount : Nat)
    (r : Reservation) (s : Source) : Prop :=
  (amount = 0 ∧ r = reservation ∧ s = source) ∨
  (r.valid_liened_insurance_num = wrapSub128 reservation.valid_liened_insurance_num amount ∧
   r.insurance_credit_reserved_num = reservation.insurance_credit_reserved_num ∧
   r.impaired_liened_insurance_num = reservation.impaired_liened_insurance_num ∧
   s.valid_liened_insurance_num = wrapSub128 source.valid_liened_insurance_num amount ∧
   s.insurance_credit_reserved_num = source.insurance_credit_reserved_num ∧
   s.fresh_reserved_backing_num = source.fresh_reserved_backing_num)

/-! ## K46 `prepare_insurance_lien_terminal_release_delta(reservation, source, amount)` -/

/-- K46. `ensures` for an `Ok((r, s))` result: either `amount == 0` and both values are
returned unchanged, or, with `vr = if reservation.valid_liened_insurance_num < amount
{ reservation.valid_liened_insurance_num } else { amount }` and `ir = amount.wrapping_sub(vr)`,
the valid liened insurance of the reservation and of the source drop by `vr`, their impaired
liened insurance drops by `ir`, and their insurance credit reserved drops by `amount` (all
`wrapping_sub`). -/
def insurance_lien_terminal_release_ensures (reservation : Reservation) (source : Source)
    (amount : Nat) (r : Reservation) (s : Source) : Prop :=
  (amount = 0 ∧ r = reservation ∧ s = source) ∨
  (let vr := if reservation.valid_liened_insurance_num < amount then
      reservation.valid_liened_insurance_num else amount
   let ir := wrapSub128 amount vr
   r.valid_liened_insurance_num = wrapSub128 reservation.valid_liened_insurance_num vr ∧
   r.impaired_liened_insurance_num = wrapSub128 reservation.impaired_liened_insurance_num ir ∧
   r.insurance_credit_reserved_num = wrapSub128 reservation.insurance_credit_reserved_num amount ∧
   s.valid_liened_insurance_num = wrapSub128 source.valid_liened_insurance_num vr ∧
   s.impaired_liened_insurance_num = wrapSub128 source.impaired_liened_insurance_num ir ∧
   s.insurance_credit_reserved_num = wrapSub128 source.insurance_credit_reserved_num amount)

/-! ## K47 `prepare_insurance_lien_impair_delta(reservation, source, amount)` -/

/-- K47. `ensures` for an `Ok((r, s))` result: either `amount == 0` and both values are
returned unchanged, or the valid liened insurance of the reservation and of the source drop by
`amount` (`wrapping_sub`), their impaired liened insurance rises by `amount` (`wrapping_add`),
and the reservation's insurance credit reserved is unchanged. -/
def insurance_lien_impair_ensures (reservation : Reservation) (source : Source) (amount : Nat)
    (r : Reservation) (s : Source) : Prop :=
  (amount = 0 ∧ r = reservation ∧ s = source) ∨
  (r.valid_liened_insurance_num = wrapSub128 reservation.valid_liened_insurance_num amount ∧
   r.impaired_liened_insurance_num = wrapAdd128 reservation.impaired_liened_insurance_num amount ∧
   r.insurance_credit_reserved_num = reservation.insurance_credit_reserved_num ∧
   s.valid_liened_insurance_num = wrapSub128 source.valid_liened_insurance_num amount ∧
   s.impaired_liened_insurance_num = wrapAdd128 source.impaired_liened_insurance_num amount)

/-! ## K48 `health_requirements_from_base_and_target_lag(base_initial, base_maintenance,
risk_notional, target_lag_penalty)` -/

/-- K48. `ensures` for an `Ok((im, mm, rn))` result: each of `base_initial`,
`base_maintenance` and `risk_notional` is raised by exactly `target_lag_penalty`
(`wrapping_add`). -/
def health_requirements_ensures (base_initial base_maintenance risk_notional
    target_lag_penalty : Nat) (im mm rn : Nat) : Prop :=
  im = wrapAdd128 base_initial target_lag_penalty ∧
  mm = wrapAdd128 base_maintenance target_lag_penalty ∧
  rn = wrapAdd128 risk_notional target_lag_penalty

end Spec.Kernel

end
