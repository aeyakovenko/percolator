module

/-!
# Independent mathematical model for five `V16Core` kernel contracts

This file gives a mathematical reading of the `kani::ensures` (and, for
`kernel_consume_insurance_layer`, `kani::requires`) clauses of five associated
functions of `V16Core` in `src/v16.rs`:

* `prepare_counterparty_backing_withdraw_delta`
* `prepare_counterparty_backing_add_delta`
* `prepare_counterparty_lien_create_delta`
* `kernel_consume_insurance_layer`
* `kernel_resolved_payout_step`

Conventions:

* Rust unsigned integers are modelled by `Nat`, `i128` by `Int`; the range
  predicates `Bucket.WF`, `Source.WF` (and `U64`, `U128`, `I128`) state that a
  value is representable in its Rust type.
* `wrapping_add` / `wrapping_sub` on `u128` are arithmetic modulo `2^128`
  (`wrapAdd128`, `wrapSub128`).
* Plain `+` / `-` appearing inside an `ensures` clause are read as exact
  integer arithmetic (they are only evaluated in the `Ok` case).
* Parameter names denote the values at function entry.  For functions returning
  `V16Result<T>`, the `_ensures` predicate constrains the `Ok` payload; the
  `Err(_) => true` branch is trivially satisfied and is handled elsewhere.
* Derived `PartialEq` on the `Copy` structs is structural equality, i.e. `=`.
-/

@[expose] public section

namespace Spec.Kernel

/-- `2^64`: one more than `u64::MAX`. -/
def two64 : Nat := 2 ^ 64

/-- `2^128`: one more than `u128::MAX`. -/
def two128 : Nat := 2 ^ 128

/-- The value fits in a Rust `u64`. -/
def U64 (x : Nat) : Prop := x < two64

/-- The value fits in a Rust `u128`. -/
def U128 (x : Nat) : Prop := x < two128

/-- The value fits in a Rust `i128` (`i128::MIN ≤ x ≤ i128::MAX`). -/
def I128 (x : Int) : Prop := -(2 ^ 127 : Int) ≤ x ∧ x < (2 ^ 127 : Int)

/-- Rust `a.wrapping_add(b)` on `u128`: addition modulo `2^128`. -/
def wrapAdd128 (a b : Nat) : Nat := (a + b) % two128

/-- Rust `a.wrapping_sub(b)` on `u128`: subtraction modulo `2^128`
(result in `[0, 2^128)`). -/
def wrapSub128 (a b : Nat) : Nat := (((a : Int) - (b : Int)) % (two128 : Int)).toNat

/-- Rust `enum BackingBucketStatusV16 { Empty, Fresh, Expired, Impaired }`. -/
inductive Status where
  | Empty
  | Fresh
  | Expired
  | Impaired
  deriving DecidableEq, Repr

/-- Rust `struct BackingBucketV16` (all fields; `u64`/`u128` fields as `Nat`). -/
structure Bucket where
  /-- `market_id: u64` -/
  market_id : Nat
  /-- `fresh_unliened_backing_num: u128` -/
  fresh_unliened_backing_num : Nat
  /-- `valid_liened_backing_num: u128` -/
  valid_liened_backing_num : Nat
  /-- `consumed_liened_backing_num: u128` -/
  consumed_liened_backing_num : Nat
  /-- `impaired_liened_backing_num: u128` -/
  impaired_liened_backing_num : Nat
  /-- `utilization_fee_earnings: u128` -/
  utilization_fee_earnings : Nat
  /-- `expiry_slot: u64` -/
  expiry_slot : Nat
  /-- `status: BackingBucketStatusV16` -/
  status : Status
  deriving DecidableEq, Repr

/-- Every field of a `BackingBucketV16` value is in range of its Rust type. -/
def Bucket.WF (b : Bucket) : Prop :=
  U64 b.market_id ∧
  U128 b.fresh_unliened_backing_num ∧
  U128 b.valid_liened_backing_num ∧
  U128 b.consumed_liened_backing_num ∧
  U128 b.impaired_liened_backing_num ∧
  U128 b.utilization_fee_earnings ∧
  U64 b.expiry_slot

/-- Rust `struct SourceCreditStateV16` (all fields; `u64`/`u128` fields as `Nat`). -/
structure Source where
  /-- `positive_claim_bound_num: u128` -/
  positive_claim_bound_num : Nat
  /-- `exact_positive_claim_num: u128` -/
  exact_positive_claim_num : Nat
  /-- `fresh_reserved_backing_num: u128` -/
  fresh_reserved_backing_num : Nat
  /-- `spent_backing_num: u128` -/
  spent_backing_num : Nat
  /-- `provider_receivable_num: u128` -/
  provider_receivable_num : Nat
  /-- `valid_liened_backing_num: u128` -/
  valid_liened_backing_num : Nat
  /-- `impaired_liened_backing_num: u128` -/
  impaired_liened_backing_num : Nat
  /-- `insurance_credit_reserved_num: u128` -/
  insurance_credit_reserved_num : Nat
  /-- `valid_liened_insurance_num: u128` -/
  valid_liened_insurance_num : Nat
  /-- `impaired_liened_insurance_num: u128` -/
  impaired_liened_insurance_num : Nat
  /-- `credit_rate_num: u128` -/
  credit_rate_num : Nat
  /-- `credit_epoch: u64` -/
  credit_epoch : Nat
  deriving DecidableEq, Repr

/-- Every field of a `SourceCreditStateV16` value is in range of its Rust type. -/
def Source.WF (s : Source) : Prop :=
  U128 s.positive_claim_bound_num ∧
  U128 s.exact_positive_claim_num ∧
  U128 s.fresh_reserved_backing_num ∧
  U128 s.spent_backing_num ∧
  U128 s.provider_receivable_num ∧
  U128 s.valid_liened_backing_num ∧
  U128 s.impaired_liened_backing_num ∧
  U128 s.insurance_credit_reserved_num ∧
  U128 s.valid_liened_insurance_num ∧
  U128 s.impaired_liened_insurance_num ∧
  U128 s.credit_rate_num ∧
  U64 s.credit_epoch

/-- K38. `ensures` of `V16Core::prepare_counterparty_backing_withdraw_delta(bucket,
source, amount)` for an `Ok((b, s))` result: either `amount == 0` and both values
are returned unchanged, or `fresh_unliened_backing_num` and
`fresh_reserved_backing_num` both drop by `amount` (wrapping), while the bucket's
valid/consumed liened backing and the source's valid liened backing, spent backing
and provider receivable are unchanged. -/
def withdraw_ensures (bucket : Bucket) (source : Source) (amount : Nat)
    (b : Bucket) (s : Source) : Prop :=
  (amount = 0 ∧ b = bucket ∧ s = source) ∨
  (b.fresh_unliened_backing_num = wrapSub128 bucket.fresh_unliened_backing_num amount ∧
   s.fresh_reserved_backing_num = wrapSub128 source.fresh_reserved_backing_num amount ∧
   b.valid_liened_backing_num = bucket.valid_liened_backing_num ∧
   b.consumed_liened_backing_num = bucket.consumed_liened_backing_num ∧
   s.valid_liened_backing_num = source.valid_liened_backing_num ∧
   s.spent_backing_num = source.spent_backing_num ∧
   s.provider_receivable_num = source.provider_receivable_num)

/-- K37. `ensures` of `V16Core::prepare_counterparty_backing_add_delta(bucket, source,
amount, current_slot, expiry_slot)` for an `Ok((b, s))` result: the bucket's
`fresh_unliened_backing_num` and the source's `fresh_reserved_backing_num` rise by
`amount` (wrapping); valid liened backing (bucket and source) and spent backing are
unchanged; the provider receivable and consumed liened backing do not increase, and
the receivable decrease equals the consumed decrease (both as `wrapping_sub`). -/
def add_ensures (bucket : Bucket) (source : Source) (amount _current_slot _expiry_slot : Nat)
    (b : Bucket) (s : Source) : Prop :=
  b.fresh_unliened_backing_num = wrapAdd128 bucket.fresh_unliened_backing_num amount ∧
  s.fresh_reserved_backing_num = wrapAdd128 source.fresh_reserved_backing_num amount ∧
  b.valid_liened_backing_num = bucket.valid_liened_backing_num ∧
  s.valid_liened_backing_num = source.valid_liened_backing_num ∧
  s.spent_backing_num = source.spent_backing_num ∧
  s.provider_receivable_num ≤ source.provider_receivable_num ∧
  b.consumed_liened_backing_num ≤ bucket.consumed_liened_backing_num ∧
  wrapSub128 source.provider_receivable_num s.provider_receivable_num =
    wrapSub128 bucket.consumed_liened_backing_num b.consumed_liened_backing_num

/-- K36. `ensures` of `V16Core::prepare_counterparty_lien_create_delta(bucket, source,
current_slot, amount)` for an `Ok((b, s))` result: either `amount == 0` and both
values are returned unchanged, or `amount` moves from the bucket's fresh unliened
backing (wrapping sub) to its valid liened backing (wrapping add), the source's valid
liened backing rises by `amount` (wrapping), and the bucket's consumed liened backing
and the source's fresh reserved and spent backing are unchanged. -/
def lien_create_ensures (bucket : Bucket) (source : Source) (_current_slot amount : Nat)
    (b : Bucket) (s : Source) : Prop :=
  (amount = 0 ∧ b = bucket ∧ s = source) ∨
  (b.fresh_unliened_backing_num = wrapSub128 bucket.fresh_unliened_backing_num amount ∧
   b.valid_liened_backing_num = wrapAdd128 bucket.valid_liened_backing_num amount ∧
   b.consumed_liened_backing_num = bucket.consumed_liened_backing_num ∧
   s.valid_liened_backing_num = wrapAdd128 source.valid_liened_backing_num amount ∧
   s.fresh_reserved_backing_num = source.fresh_reserved_backing_num ∧
   s.spent_backing_num = source.spent_backing_num)

/-- K12. `requires` of `V16Core::kernel_consume_insurance_layer`:
`pnl < 0 && pnl > i128::MIN`. -/
def consume_insurance_requires (pnl : Int) : Prop :=
  pnl < 0 ∧ pnl > -(2 ^ 127 : Int)

/-- K12. `ensures` of `V16Core::kernel_consume_insurance_layer(domain_available,
insurance, spent, pnl)` for an `Ok((used, new_insurance, new_spent, new_pnl))`
result, with `residual = pnl.unsigned_abs()`: `used = min(residual,
domain_available)` (written as the `if residual < domain_available` choice),
`used ≤ domain_available`, `used ≤ residual`, `new_insurance = insurance - used`,
`new_spent = spent + used`, `new_insurance.wrapping_add(used) == insurance`,
`new_pnl = pnl + used`, and `new_pnl ≤ 0`. -/
def consume_insurance_ensures (domain_available insurance spent : Nat) (pnl : Int)
    (used new_insurance new_spent : Nat) (new_pnl : Int) : Prop :=
  let residual := pnl.natAbs
  used = (if residual < domain_available then residual else domain_available) ∧
  used ≤ domain_available ∧
  used ≤ residual ∧
  (new_insurance : Int) = (insurance : Int) - (used : Int) ∧
  new_spent = spent + used ∧
  wrapAdd128 new_insurance used = insurance ∧
  new_pnl = pnl + (used : Int) ∧
  new_pnl ≤ 0

/-- K13. `ensures` of `V16Core::kernel_resolved_payout_step(claimable, vault)` for the
returned pair `(payout, new_vault)`: `payout = min(claimable, vault)` (written as the
`if claimable < vault` choice), `payout ≤ claimable`, `payout ≤ vault`,
`new_vault = vault - payout`, and `new_vault.wrapping_add(payout) == vault`. -/
def resolved_payout_ensures (claimable vault : Nat) (payout new_vault : Nat) : Prop :=
  payout = (if claimable < vault then claimable else vault) ∧
  payout ≤ claimable ∧
  payout ≤ vault ∧
  (new_vault : Int) = (vault : Int) - (payout : Int) ∧
  wrapAdd128 new_vault payout = vault

end Spec.Kernel

end
