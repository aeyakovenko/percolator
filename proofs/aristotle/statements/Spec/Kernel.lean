module

public import Spec.Rust.Kernel
public import Spec.KernelModel
public import SepLogic.Public

/-!
# Public contracts for five `V16Core` kernels of `src/v16.rs`

For each of `prepare_counterparty_backing_withdraw_delta` (K38),
`prepare_counterparty_backing_add_delta` (K37), `prepare_counterparty_lien_create_delta` (K36),
`kernel_consume_insurance_layer` (K12) and `kernel_resolved_payout_step` (K13), two properties
of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModel`, in
  which parameter names denote the values passed by the caller).
* `Kxx.nopanic` (`*_nopanic` below): for all inputs satisfying the `requires`, the call does
  not panic (it never unwinds; overflow checks are on in the release profile).

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. Contracts describe partial correctness: they
say what holds if a call returns and whether it may unwind; they do not say that it terminates.

For K12 and K13, the core library functions that the kernels call and that are not themselves
verified here are covered by the explicit contracts below (`MinU128Contract`, ...), which those
properties take as premises. K36, K37 and K38 take no such premise: every function they call
(the derived `PartialEq` of `BackingBucketStatusV16` and the core library functions) is covered
by a proof of its body.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Values -/

/-- The Rust discriminant of a `BackingBucketStatusV16`
(`Empty = 0`, `Fresh = 1`, `Expired = 2`, `Impaired = 3`). -/
def statusDisc : Status → Int
  | .Empty => 0
  | .Fresh => 1
  | .Expired => 2
  | .Impaired => 3

/-- The Rust value of a `BackingBucketV16`, fields in declaration order. -/
def bucketValue {PROP : Type} [BI PROP] (S : Vocab PROP) (b : Bucket) : S.Value :=
  S.tupleVal [S.intVal b.market_id, S.intVal b.fresh_unliened_backing_num,
    S.intVal b.valid_liened_backing_num, S.intVal b.consumed_liened_backing_num,
    S.intVal b.impaired_liened_backing_num, S.intVal b.utilization_fee_earnings,
    S.intVal b.expiry_slot, S.variantVal (statusDisc b.status) S.unitVal]

/-- The Rust value of a `SourceCreditStateV16`, fields in declaration order. -/
def sourceValue {PROP : Type} [BI PROP] (S : Vocab PROP) (s : Source) : S.Value :=
  S.tupleVal [S.intVal s.positive_claim_bound_num, S.intVal s.exact_positive_claim_num,
    S.intVal s.fresh_reserved_backing_num, S.intVal s.spent_backing_num,
    S.intVal s.provider_receivable_num, S.intVal s.valid_liened_backing_num,
    S.intVal s.impaired_liened_backing_num, S.intVal s.insurance_credit_reserved_num,
    S.intVal s.valid_liened_insurance_num, S.intVal s.impaired_liened_insurance_num,
    S.intVal s.credit_rate_num, S.intVal s.credit_epoch]

/-- The `V16Error` with discriminant `d` (`InvalidConfig = 0`, ..., `CounterUnderflow = 11`). -/
def v16ErrorValue {PROP : Type} [BI PROP] (S : Vocab PROP) (d : Nat) : S.Value :=
  S.variantVal d S.unitVal

/-- `Ok(x)`; also `ControlFlow::Continue(x)`, which has the same discriminant `0`. -/
def okValue {PROP : Type} [BI PROP] (S : Vocab PROP) (x : S.Value) : S.Value :=
  S.variantVal 0 (S.tupleVal [x])

/-- `Err(x)`; also `ControlFlow::Break(x)`, which has the same discriminant `1`. -/
def errValue {PROP : Type} [BI PROP] (S : Vocab PROP) (x : S.Value) : S.Value :=
  S.variantVal 1 (S.tupleVal [x])

/-- `Some(x)` -/
def someValue {PROP : Type} [BI PROP] (S : Vocab PROP) (x : S.Value) : S.Value :=
  S.variantVal 1 (S.tupleVal [x])

/-- `None` -/
def noneValue {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Value :=
  S.variantVal 0 (S.tupleVal [])

/-- A call outcome predicate for the `contract` properties: a normal return satisfies `P`;
unwinding is not constrained (it is not a return). -/
def ifReturns {PROP : Type} [BI PROP] (S : Vocab PROP) (P : S.Value → Prop) :
    CallOutcome S.Value → PROP
  | .returned v => iprop(⌊P v⌋)
  | .unwound => iprop(True)

/-- A `V16Result<(BackingBucketV16, SourceCreditStateV16)>` value that is either `Ok((b, s))`
for in-range `b`, `s` with `ens b s`, or some `Err(e)` (the `Err(_) => true` arm). -/
def BucketSourceResult {PROP : Type} [BI PROP] (S : Vocab PROP)
    (ens : Bucket → Source → Prop) (v : S.Value) : Prop :=
  (∃ b s, b.WF ∧ s.WF ∧
    v = okValue S (S.tupleVal [bucketValue S b, sourceValue S s]) ∧ ens b s) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- The bytes of the promoted constant `&BackingBucketStatusV16::Fresh` that
`prepare_counterparty_backing_withdraw_delta` and `prepare_counterparty_lien_create_delta`
compare `bucket.status` with: the single discriminant byte `1`. -/
def freshConstant {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  S.global Mir.Kernel.Global.«const [1 bytes] 165f0ff0fda1eb79» 1 [1]

/-! ## Contracts of the core library functions called by the kernels

Each is the documented behaviour of the standard-library function at the argument values the
kernels pass; each says the call returns that value and does not panic. They are assumptions
of the properties below, not verified here. -/

/-- `<u128 as Ord>::min(a, b)` returns `min(a, b)`. -/
def MinU128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Nat, U128 a → U128 b →
    ⊢ S.call Mir.Kernel.Fn.«<u128 as core::cmp::Ord>::min»
        [(S.intVal a, S.argTy Mir.Kernel.Fn.«<u128 as core::cmp::Ord>::min» 0),
         (S.intVal b, S.argTy Mir.Kernel.Fn.«<u128 as core::cmp::Ord>::min» 1)]
        (S.returns fun w => iprop(⌊w = S.intVal (min a b : Nat)⌋))

/-- `u128::checked_add(a, b)` returns `Some(a + b)` if `a + b < 2^128`, else `None`. -/
def CheckedAddU128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Nat, U128 a → U128 b →
    ⊢ S.call Mir.Kernel.Fn.«core::num::<impl u128>::checked_add»
        [(S.intVal a, S.argTy Mir.Kernel.Fn.«core::num::<impl u128>::checked_add» 0),
         (S.intVal b, S.argTy Mir.Kernel.Fn.«core::num::<impl u128>::checked_add» 1)]
        (S.returns fun w => iprop(⌊w = if a + b < 2 ^ 128 then someValue S (S.intVal (a + b : Nat))
          else noneValue S⌋))

/-- `i128::checked_add(a, b)` returns `Some(a + b)` if `a + b` is an `i128`, else `None`. -/
def CheckedAddI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Int, I128 a → I128 b →
    ⊢ S.call Mir.Kernel.Fn.«core::num::<impl i128>::checked_add»
        [(S.intVal a, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::checked_add» 0),
         (S.intVal b, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::checked_add» 1)]
        (S.returns fun w => iprop(⌊w = if -(2 ^ 127 : Int) ≤ a + b ∧ a + b < 2 ^ 127 then someValue S (S.intVal (a + b))
          else noneValue S⌋))

/-- `i128::unsigned_abs(x)` returns `|x|` (as a `u128`). -/
def UnsignedAbsI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::num::<impl i128>::unsigned_abs»
        [(S.intVal x, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::unsigned_abs» 0)]
        (S.returns fun w => iprop(⌊w = S.intVal (x.natAbs : Nat)⌋))

/-- `<i128 as TryFrom<u128>>::try_from(x)` returns `Ok(x)` when `x < 2^127`. -/
def TryFromU128I128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Nat, x < 2 ^ 127 →
    ⊢ S.call Mir.Kernel.Fn.«core::convert::num::<impl core::convert::TryFrom<u128> for i128>::try_from»
        [(S.intVal x, S.argTy
          Mir.Kernel.Fn.«core::convert::num::<impl core::convert::TryFrom<u128> for i128>::try_from» 0)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` of `kernel_consume_insurance_layer`, returns `Ok(x)`. -/
def MapErrContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_consume_insurance_layer::{closure#0}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_consume_insurance_layer::{closure#0}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_consume_insurance_layer::{closure#0}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- `Option::<u128>::ok_or(Some(x), e)` returns `Ok(x)` and `ok_or(None, e)` returns `Err(e)`,
for every `V16Error` `e`. -/
def OkOrU128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ (x d : Nat), U128 x → d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>»
        [(someValue S (S.intVal x),
            S.argTy Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>»
        [(noneValue S,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<u128>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋)))

/-- `Option::<i128>::ok_or(Some(x), e)` returns `Ok(x)` and `ok_or(None, e)` returns `Err(e)`,
for every `V16Error` `e`. -/
def OkOrI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ (x : Int) (d : Nat), I128 x → d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>»
        [(someValue S (S.intVal x),
            S.argTy Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>»
        [(noneValue S,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Kernel.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋)))

/-- The `?` operator's `<Result<u128, V16Error> as Try>::branch(r)`: `Ok(x)` gives
`ControlFlow::Continue(x)` and `Err(e)` gives `ControlFlow::Break(Err(e))`. -/
def BranchU128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ x : Nat, U128 x →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<u128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (errValue S (v16ErrorValue S d))⌋)))

/-- The `?` operator's `<Result<i128, V16Error> as Try>::branch(r)`: `Ok(x)` gives
`ControlFlow::Continue(x)` and `Err(e)` gives `ControlFlow::Break(Err(e))`. -/
def BranchI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (errValue S (v16ErrorValue S d))⌋)))

/-- The `?` operator's `from_residual(Err(e))` for
`V16Result<(BackingBucketV16, SourceCreditStateV16)>` returns `Err(e)` (the conversion
`From<V16Error> for V16Error` is the identity). -/
def FromResidualBucketSourceContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<(percolator::v16::BackingBucketV16, percolator::v16::SourceCreditStateV16), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<(percolator::v16::BackingBucketV16, percolator::v16::SourceCreditStateV16), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋))

/-- The `?` operator's `from_residual(Err(e))` for `V16Result<(u128, u128, u128, i128)>` returns
`Err(e)`. -/
def FromResidualTuple4Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<(u128, u128, u128, i128), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<(u128, u128, u128, i128), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋))

/-! ## K38 `prepare_counterparty_backing_withdraw_delta` -/

/-- The call `V16Core::prepare_counterparty_backing_withdraw_delta(bucket, source, amount)`
with postcondition `Φ`. -/
def callWithdraw {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_withdraw_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_withdraw_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_withdraw_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_withdraw_delta» 2)] Φ

/-- K38.contract: if `prepare_counterparty_backing_withdraw_delta(bucket, source, amount)`
returns, its result is `Ok((b, s))` satisfying the `ensures` (`withdraw_ensures`), or an
`Err(_)`. -/
def prepare_counterparty_backing_withdraw_delta_contract : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then, on normal return of the call,
      callWithdraw S bucket source amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K38, or an `Err(_)`.
        BucketSourceResult S (withdraw_ensures bucket source amount) v)

/-- K38.nopanic: `prepare_counterparty_backing_withdraw_delta(bucket, source, amount)` does not
panic. -/
def prepare_counterparty_backing_withdraw_delta_nopanic : Property := fun S =>
  -- For any bucket, source state and amount,
  ∀ (bucket : Bucket) (source : Source) (amount : Nat),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then the call does not unwind (panic).
      callWithdraw S bucket source amount (S.returns fun _ => iprop(True))

/-! ## K37 `prepare_counterparty_backing_add_delta` -/

/-- The call `V16Core::prepare_counterparty_backing_add_delta(bucket, source, amount,
current_slot, expiry_slot)` with postcondition `Φ`. -/
def callAdd {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (amount current_slot expiry_slot : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta» 1),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta» 2),
     (S.intVal current_slot, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta» 3),
     (S.intVal expiry_slot, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_backing_add_delta» 4)] Φ

/-- K37.contract: if `prepare_counterparty_backing_add_delta(bucket, source, amount,
current_slot, expiry_slot)` returns, its result is `Ok((b, s))` satisfying the `ensures`
(`add_ensures`), or an `Err(_)`. -/
def prepare_counterparty_backing_add_delta_contract : Property := fun S =>
  -- For any bucket, source state, amount and slots,
  ∀ (bucket : Bucket) (source : Source) (amount current_slot expiry_slot : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount → U64 current_slot → U64 expiry_slot →
    ⊢ -- then, on normal return of the call,
      callAdd S bucket source amount current_slot expiry_slot (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K37, or an `Err(_)`.
        BucketSourceResult S (add_ensures bucket source amount current_slot expiry_slot) v)

/-- K37.nopanic: `prepare_counterparty_backing_add_delta(..)` does not panic. -/
def prepare_counterparty_backing_add_delta_nopanic : Property := fun S =>
  -- For any bucket, source state, amount and slots,
  ∀ (bucket : Bucket) (source : Source) (amount current_slot expiry_slot : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U128 amount → U64 current_slot → U64 expiry_slot →
    ⊢ -- then the call does not unwind (panic).
      callAdd S bucket source amount current_slot expiry_slot (S.returns fun _ => iprop(True))

/-! ## K36 `prepare_counterparty_lien_create_delta` -/

/-- The call `V16Core::prepare_counterparty_lien_create_delta(bucket, source, current_slot,
amount)` with postcondition `Φ`. -/
def callLienCreate {PROP : Type} [BI PROP] (S : Vocab PROP) (bucket : Bucket) (source : Source)
    (current_slot amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_create_delta»
    [(bucketValue S bucket, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_create_delta» 0),
     (sourceValue S source, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_create_delta» 1),
     (S.intVal current_slot, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_create_delta» 2),
     (S.intVal amount, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::prepare_counterparty_lien_create_delta» 3)] Φ

/-- K36.contract: if `prepare_counterparty_lien_create_delta(bucket, source, current_slot,
amount)` returns, its result is `Ok((b, s))` satisfying the `ensures`
(`lien_create_ensures`), or an `Err(_)`. -/
def prepare_counterparty_lien_create_delta_contract : Property := fun S =>
  -- For any bucket, source state, slot and amount,
  ∀ (bucket : Bucket) (source : Source) (current_slot amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U64 current_slot → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then, on normal return of the call,
      callLienCreate S bucket source current_slot amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((b, s))` with the `ensures` of K36, or an `Err(_)`.
        BucketSourceResult S (lien_create_ensures bucket source current_slot amount) v)

/-- K36.nopanic: `prepare_counterparty_lien_create_delta(..)` does not panic. -/
def prepare_counterparty_lien_create_delta_nopanic : Property := fun S =>
  -- For any bucket, source state, slot and amount,
  ∀ (bucket : Bucket) (source : Source) (current_slot amount : Nat),
    -- PRECONDITIONS
    -- if every value is in range of its Rust type (no `requires`),
    bucket.WF → source.WF → U64 current_slot → U128 amount →
    -- and the promoted constant `&BackingBucketStatusV16::Fresh` is in read-only memory,
    freshConstant S ⊢
      -- then the call does not unwind (panic).
      callLienCreate S bucket source current_slot amount (S.returns fun _ => iprop(True))

/-! ## K12 `kernel_consume_insurance_layer` -/

/-- The call `V16Core::kernel_consume_insurance_layer(domain_available, insurance, spent, pnl)`
with postcondition `Φ`. -/
def callConsume {PROP : Type} [BI PROP] (S : Vocab PROP) (domain_available insurance spent : Nat)
    (pnl : Int) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_consume_insurance_layer»
    [(S.intVal domain_available, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_consume_insurance_layer» 0),
     (S.intVal insurance, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_consume_insurance_layer» 1),
     (S.intVal spent, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_consume_insurance_layer» 2),
     (S.intVal pnl, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_consume_insurance_layer» 3)] Φ

/-- The core library contracts `kernel_consume_insurance_layer` relies on. (`u128::checked_sub`
is verified, not assumed.) -/
def ConsumeInsuranceCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  UnsignedAbsI128Contract S ∧ MinU128Contract S ∧ CheckedAddU128Contract S ∧
    TryFromU128I128Contract S ∧ MapErrContract S ∧ CheckedAddI128Contract S ∧
    OkOrU128Contract S ∧ OkOrI128Contract S ∧ BranchU128Contract S ∧ BranchI128Contract S ∧
    FromResidualTuple4Contract S

/-- A `V16Result<(u128, u128, u128, i128)>` value that is either
`Ok((used, new_insurance, new_spent, new_pnl))` for in-range values satisfying `ens`, or some
`Err(e)`. -/
def ConsumeResult {PROP : Type} [BI PROP] (S : Vocab PROP)
    (ens : Nat → Nat → Nat → Int → Prop) (v : S.Value) : Prop :=
  (∃ (used new_insurance new_spent : Nat) (new_pnl : Int),
    U128 used ∧ U128 new_insurance ∧ U128 new_spent ∧ I128 new_pnl ∧
    v = okValue S (S.tupleVal [S.intVal used, S.intVal new_insurance, S.intVal new_spent,
      S.intVal new_pnl]) ∧
    ens used new_insurance new_spent new_pnl) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- K12.contract: under its `requires` (`pnl < 0 && pnl > i128::MIN`), if
`kernel_consume_insurance_layer(domain_available, insurance, spent, pnl)` returns, its result
is `Ok((used, new_insurance, new_spent, new_pnl))` satisfying the `ensures`
(`consume_insurance_ensures`), or an `Err(_)`. -/
def kernel_consume_insurance_layer_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  ConsumeInsuranceCoreContracts S →
  -- for any arguments,
  ∀ (domain_available insurance spent : Nat) (pnl : Int),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 domain_available → U128 insurance → U128 spent → I128 pnl →
    -- and they satisfy the `requires`,
    consume_insurance_requires pnl →
    ⊢ -- then, on normal return of the call,
      callConsume S domain_available insurance spent pnl (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(..)` with the `ensures` of K12, or an `Err(_)`.
        ConsumeResult S (consume_insurance_ensures domain_available insurance spent pnl) v)

/-- K12.nopanic: under its `requires`, `kernel_consume_insurance_layer(..)` does not panic. -/
def kernel_consume_insurance_layer_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  ConsumeInsuranceCoreContracts S →
  -- for any arguments,
  ∀ (domain_available insurance spent : Nat) (pnl : Int),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 domain_available → U128 insurance → U128 spent → I128 pnl →
    -- and they satisfy the `requires`,
    consume_insurance_requires pnl →
    ⊢ -- then the call does not unwind (panic).
      callConsume S domain_available insurance spent pnl (S.returns fun _ => iprop(True))

/-! ## K13 `kernel_resolved_payout_step` -/

/-- The call `V16Core::kernel_resolved_payout_step(claimable, vault)` with postcondition `Φ`. -/
def callPayout {PROP : Type} [BI PROP] (S : Vocab PROP) (claimable vault : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_resolved_payout_step»
    [(S.intVal claimable, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_resolved_payout_step» 0),
     (S.intVal vault, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_resolved_payout_step» 1)] Φ

/-- K13.contract: if `kernel_resolved_payout_step(claimable, vault)` returns
`(payout, new_vault)`, the `ensures` (`resolved_payout_ensures`) holds. -/
def kernel_resolved_payout_step_contract : Property := fun S =>
  -- ASSUMED: the contract of `<u128 as Ord>::min`,
  MinU128Contract S →
  -- for any `claimable` and `vault`,
  ∀ (claimable vault : Nat),
    -- PRECONDITIONS
    -- if both are `u128`s (no `requires`),
    U128 claimable → U128 vault →
    ⊢ -- then, on normal return of the call,
      callPayout S claimable vault (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a pair of `u128`s `(payout, new_vault)`
        ∃ payout new_vault : Nat, U128 payout ∧ U128 new_vault ∧
          v = S.tupleVal [S.intVal payout, S.intVal new_vault] ∧
          -- satisfying the `ensures` of K13.
          resolved_payout_ensures claimable vault payout new_vault)

/-- K13.nopanic: `kernel_resolved_payout_step(claimable, vault)` does not panic. -/
def kernel_resolved_payout_step_nopanic : Property := fun S =>
  -- ASSUMED: the contract of `<u128 as Ord>::min`,
  MinU128Contract S →
  -- for any `claimable` and `vault`,
  ∀ (claimable vault : Nat),
    -- PRECONDITIONS
    -- if both are `u128`s (no `requires`),
    U128 claimable → U128 vault →
    ⊢ -- then the call does not unwind (panic).
      callPayout S claimable vault (S.returns fun _ => iprop(True))

end Spec.Kernel

end
