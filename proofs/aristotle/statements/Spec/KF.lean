module

public import Spec.KFLemmas
public import Spec.KFModel
public import Spec.Kernel

/-!
# Part 1: funding settlement of one leg (engine revision `bfd4d0c4`)

Public contracts for the free functions `scaled_adl_delta_fast(abs_basis_q, a_basis, then, now)
-> Option<i128>` and `f_snap_after_settlement(abs_basis, a_basis, f_snap, f_now, f_delta) ->
V16Result<i128>` of `src/v16.rs`, as compiled in the program `Mir.KF` (handles in
`Spec.Rust.KF`): default features, no `cfg(kani)`, release profile with `overflow-checks = true`
and `panic = "unwind"`.

Exact integers throughout (`Spec.KFModel`): `den a_basis = a_basis * POS_SCALE` (the mathematical
product) and `fl abs_basis a_basis x y = floor(abs_basis * (y - x) / den)`, rounded toward minus
infinity. When `den = 0` the quotient is read as `0` (Lean's convention for division by zero);
the request leaves that case undefined.

Inputs range over the whole of their Rust types (`U128`, `I128` of `Spec.KFModel`).

Every property is a partial-correctness statement: `S.returns P` says that **no execution
panics** and that `P` holds of the result if the call returns. Nothing is claimed about
termination (the functions have no loops; the division called in the `U256` fallback does).

Hypotheses. Every function reached is covered by a proof of its body, including the
`src/wide_math.rs` callees `floor_div_signed_conservative_i128`, `mul_div_floor_u256_with_rem`
(with `U512::mul_u256`, `U512::div_rem_by_u256`, `U512::checked_div_rem_by_u256`,
`div_rem_u256` and their helpers), `U256::from_u128` and `U256::try_into_u128` (the earlier
results E3, D5, A1, A3, reused for this build), except the core-library functions whose
documented behaviour the earlier result D5 already assumed: `PartialOrd::gt`/`ge` on `U256`,
`Option::<U256>::expect` and `next`/`next_back` of `Range<usize>` (`Spec.WideMath.U512DivLib`),
which is a hypothesis of the `f_snap_after_settlement` properties. `kfDivGlobals S` (the
read-only literals of those `wide_math` functions, placed at start-up) is a precondition, as in
the earlier results.
-/

@[expose] public section

namespace Spec.KF
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (okValue errValue v16ErrorValue)
open Spec.WideMath (someVal noneVal U512DivLib)

/-! ## Values and calls -/

/-- The Rust value of an `Option<i128>`. -/
def optionI128Value {PROP : Type} [BI PROP] (S : Vocab PROP) : Option Int → S.Value
  | none => noneVal S
  | some x => someVal S (S.intVal x)

/-- The call `scaled_adl_delta_fast(abs_basis_q, a_basis, then, now)` with outcome
predicate `Φ`. -/
def callScaledAdlDeltaFast {PROP : Type} [BI PROP] (S : Vocab PROP)
    (abs_basis_q a_basis : Nat) («then» now : Int) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KF.Fn.«percolator::v16::scaled_adl_delta_fast»
    [(S.intVal abs_basis_q, S.argTy Mir.KF.Fn.«percolator::v16::scaled_adl_delta_fast» 0),
     (S.intVal a_basis, S.argTy Mir.KF.Fn.«percolator::v16::scaled_adl_delta_fast» 1),
     (S.intVal «then», S.argTy Mir.KF.Fn.«percolator::v16::scaled_adl_delta_fast» 2),
     (S.intVal now, S.argTy Mir.KF.Fn.«percolator::v16::scaled_adl_delta_fast» 3)] Φ

/-- The call `f_snap_after_settlement(abs_basis, a_basis, f_snap, f_now, f_delta)` with outcome
predicate `Φ`. -/
def callFSnapAfterSettlement {PROP : Type} [BI PROP] (S : Vocab PROP)
    (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.KF.Fn.«percolator::v16::f_snap_after_settlement»
    [(S.intVal abs_basis, S.argTy Mir.KF.Fn.«percolator::v16::f_snap_after_settlement» 0),
     (S.intVal a_basis, S.argTy Mir.KF.Fn.«percolator::v16::f_snap_after_settlement» 1),
     (S.intVal f_snap, S.argTy Mir.KF.Fn.«percolator::v16::f_snap_after_settlement» 2),
     (S.intVal f_now, S.argTy Mir.KF.Fn.«percolator::v16::f_snap_after_settlement» 3),
     (S.intVal f_delta, S.argTy Mir.KF.Fn.«percolator::v16::f_snap_after_settlement» 4)] Φ

/-- The exact error condition of `f_snap_after_settlement` when `abs_basis > 0`,
`den <= u128::MAX` and `f_delta = fl(f_snap, f_now)`: the snapshot moves (`f_snap != f_now`),
and either `a_basis = 0` (then `den = 0`) or one of the overflow conditions `kf_err_cond` holds
(the signed ceiling `step = ceil(f_delta * den / abs_basis)` is outside
`[-(2^127 - 1), 2^127 - 1]`, or `f_snap + step < i128::MIN`, or
`f_now - (f_snap + step) > i128::MAX`). -/
def kf_err_exact (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int) : Prop :=
  f_snap ≠ f_now ∧ (a_basis = 0 ∨ kf_err_cond abs_basis a_basis f_snap f_now f_delta)

/-! ## KF.delta -/

/-- **KF.delta.** `scaled_adl_delta_fast(abs_basis_q, a_basis, then, now)` never panics, and if
it returns `Some(x)` then `x = fl(then, now) = floor(abs_basis_q * (now - then) / den)` with
`den = a_basis * POS_SCALE`. -/
def scaled_adl_delta_fast_delta : Property := fun S =>
  -- For any basis, ADL basis and pair of index values,
  ∀ (abs_basis_q a_basis : Nat) («then» now : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis_q → U128 a_basis → I128 «then» → I128 now →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not panic and, if it returns,
      callScaledAdlDeltaFast S abs_basis_q a_basis «then» now (S.returns fun v => iprop(
        -- POSTCONDITIONS
        -- the result is an `Option<i128>` whose `Some` value is `fl(then, now)`.
        ⌊∃ r : Option Int, v = optionI128Value S r ∧
          ∀ x, r = some x → x = fl abs_basis_q a_basis «then» now⌋))

/-! ## KF.nopanic -/

/-- **KF.nopanic.** No execution of `f_snap_after_settlement(abs_basis, a_basis, f_snap, f_now,
f_delta)` panics. -/
def f_snap_after_settlement_nopanic : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any inputs,
  ∀ (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 f_snap → I128 f_now → I128 f_delta →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not unwind (panic).
      callFSnapAfterSettlement S abs_basis a_basis f_snap f_now f_delta
        (S.returns fun _ => iprop(True))

/-! ## KF.carry -/

/-- **KF.carry, as requested.** With `abs_basis > 0`, `den <= u128::MAX` and
`f_delta = fl(f_snap, f_now)`: if the call returns `Ok(s)`, then (i) `s <= f_now` and
`abs_basis * (f_now - s) < den`, and (ii)
`f_delta * den <= abs_basis * (s - f_snap) < f_delta * den + abs_basis`.

This literal statement fails when `a_basis = 0` (so `den = 0`, and `fl` is read as `0`):
`abs_basis = 1`, `a_basis = 0`, `f_snap = f_now = 0`, `f_delta = 0` returns `Ok(0)`, and (i)
would require `0 < 0`. See `f_snap_after_settlement_carry_of_pos` for the corrected property. -/
def f_snap_after_settlement_carry : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any inputs,
  ∀ (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 f_snap → I128 f_now → I128 f_delta →
    -- the basis is positive,
    0 < abs_basis →
    -- `den` fits a `u128`,
    U128 (den a_basis) →
    -- and `f_delta` is the exact floor `fl(f_snap, f_now)`,
    f_delta = fl abs_basis a_basis f_snap f_now →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not panic and, if it returns `Ok(s)`,
      callFSnapAfterSettlement S abs_basis a_basis f_snap f_now f_delta (S.returns fun v => iprop(
        -- POSTCONDITION
        -- (i) and (ii) hold for `s`.
        ⌊∀ s : Int, v = okValue S (S.intVal s) → carry_post abs_basis a_basis f_snap f_now f_delta s⌋))

/-- **KF.carry with an added precondition `a_basis > 0`** (equivalently `den > 0`): the
statement of `f_snap_after_settlement_carry` under this extra precondition. -/
def f_snap_after_settlement_carry_of_pos : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any inputs,
  ∀ (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 f_snap → I128 f_now → I128 f_delta →
    -- the basis is positive,
    0 < abs_basis →
    -- the ADL basis is positive (added precondition),
    0 < a_basis →
    -- `den` fits a `u128`,
    U128 (den a_basis) →
    -- and `f_delta` is the exact floor `fl(f_snap, f_now)`,
    f_delta = fl abs_basis a_basis f_snap f_now →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not panic and, if it returns `Ok(s)`,
      callFSnapAfterSettlement S abs_basis a_basis f_snap f_now f_delta (S.returns fun v => iprop(
        -- POSTCONDITION
        -- (i) and (ii) hold for `s`.
        ⌊∀ s : Int, v = okValue S (S.intVal s) → carry_post abs_basis a_basis f_snap f_now f_delta s⌋))

/-! ## KF.ok -/

/-- **KF.ok, as requested.** Under the hypotheses of KF.carry (as requested), the call returns
`Ok` whenever in addition `|f_now - f_snap| + den <= i128::MAX` and `f_now - den >= i128::MIN`.

This literal statement fails when `a_basis = 0`: `abs_basis = 1`, `a_basis = 0`, `f_snap = 0`,
`f_now = 1`, `f_delta = 0` meets every premise and returns `Err(ArithmeticOverflow)`. See
`f_snap_after_settlement_ok_exact` for the exact error condition and the corrected property. -/
def f_snap_after_settlement_ok : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any inputs,
  ∀ (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 f_snap → I128 f_now → I128 f_delta →
    -- the basis is positive,
    0 < abs_basis →
    -- `den` fits a `u128`,
    U128 (den a_basis) →
    -- `f_delta` is the exact floor `fl(f_snap, f_now)`,
    f_delta = fl abs_basis a_basis f_snap f_now →
    -- and the index values leave room for one `den` in `i128`,
    kf_ok_sufficient abs_basis a_basis f_snap f_now →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not panic and, if it returns, it returns `Ok(_)`.
      callFSnapAfterSettlement S abs_basis a_basis f_snap f_now f_delta (S.returns fun v => iprop(
        -- POSTCONDITION
        ⌊∃ s : Int, v = okValue S (S.intVal s)⌋))

/-- **KF.ok, exact error condition and corrected statement.** With `abs_basis > 0`,
`den <= u128::MAX` and `f_delta = fl(f_snap, f_now)` (any `a_basis`), the call does not panic;
if it returns, it returns `Err(ArithmeticOverflow)` exactly when `kf_err_exact` holds, and
`Ok(s)` (for an `i128` `s`) otherwise. Moreover, if `a_basis > 0` (added precondition),
`|f_now - f_snap| + den <= i128::MAX` and `f_now - den >= i128::MIN`, it returns `Ok`. -/
def f_snap_after_settlement_ok_exact : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any inputs,
  ∀ (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 f_snap → I128 f_now → I128 f_delta →
    -- the basis is positive,
    0 < abs_basis →
    -- `den` fits a `u128`,
    U128 (den a_basis) →
    -- and `f_delta` is the exact floor `fl(f_snap, f_now)`,
    f_delta = fl abs_basis a_basis f_snap f_now →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- the call does not panic and, if it returns,
      callFSnapAfterSettlement S abs_basis a_basis f_snap f_now f_delta (S.returns fun v => iprop(
        -- POSTCONDITIONS
        -- it returns `Err(ArithmeticOverflow)` exactly when the error condition holds,
        -- and `Ok(s)` otherwise,
        ⌊(v = errValue S (v16ErrorValue S ArithmeticOverflow) ∧
            kf_err_exact abs_basis a_basis f_snap f_now f_delta) ∨
          (∃ s : Int, I128 s ∧ v = okValue S (S.intVal s) ∧
            ¬ kf_err_exact abs_basis a_basis f_snap f_now f_delta)⌋ ∗
        -- and, for `a_basis > 0` with room for one `den`, it returns `Ok(_)`.
        ⌊0 < a_basis → kf_ok_sufficient abs_basis a_basis f_snap f_now →
          ∃ s : Int, v = okValue S (S.intVal s)⌋))

/-! ## KF.cadence -/

/-- What follows one settlement of a chain: if the call returns `Ok(s)`, continue with `k s`; if
it returns `Err(_)` (or unwinds), the chain stops and nothing more is claimed. -/
def onOkI128 {PROP : Type} [BI PROP] (S : Vocab PROP) (k : Int → PROP) :
    CallOutcome S.Value → PROP
  | .returned v => iprop(∀ s : Int, ⌜v = okValue S (S.intVal s)⌝ -∗ k s)
  | .unwound => iprop(True)

/-- `settleChain S abs_basis a_basis bs s k`: settle the leg successively against the index
values `bs = [b_1, ..., b_n]` starting from the snapshot `s = s_0`: the `i`-th call is
`f_snap_after_settlement(abs_basis, a_basis, s_(i-1), b_i, d_i)` with `d_i = fl(s_(i-1), b_i)`,
and `s_i` is its `Ok` result. If all calls return `Ok`, `k s_n (d_1 + ... + d_n)` holds. A step
whose `d_i` is not an `i128` cannot be called (the argument has type `i128`); the chain then
stops and nothing more is claimed, as when a call returns `Err`. -/
def settleChain {PROP : Type} [BI PROP] (S : Vocab PROP) (abs_basis a_basis : Nat) :
    List Int → Int → (Int → Int → PROP) → PROP
  | [], s, k => k s 0
  | b :: bs, s, k =>
    if I128 (fl abs_basis a_basis s b) then
      callFSnapAfterSettlement S abs_basis a_basis s b (fl abs_basis a_basis s b)
        (onOkI128 S fun s' =>
          settleChain S abs_basis a_basis bs s' (fun sn sumD => k sn (fl abs_basis a_basis s b + sumD)))
    else iprop(True)

/-- **KF.cadence.** Fix `abs_basis > 0` and `a_basis` with `den <= u128::MAX`, a snapshot `s_0`
and `n >= 1` index values `b_1, ..., b_n` (in any order). If all `n` chained settlements return
`Ok`, then with `T = fl(s_0, b_n)` and `D = d_1 + ... + d_n`: `D <= T`,
`(T - D) * den < den + n * abs_basis`, and `D = T` when `den` is a multiple of `abs_basis`. The
chain is the function's own call applied repeatedly (`settleChain`), not a model of it. -/
def f_snap_after_settlement_cadence : Property := fun S =>
  -- ASSUMPTION: the documented behaviour of the core-library functions reached through
  -- `mul_div_floor_u256_with_rem` (as assumed by the earlier result D5).
  U512DivLib S →
  -- For any basis, ADL basis, starting snapshot and nonempty list of index values,
  ∀ (abs_basis a_basis : Nat) (s₀ : Int) (bs : List Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U128 abs_basis → U128 a_basis → I128 s₀ → (∀ b ∈ bs, I128 b) →
    -- the basis is positive,
    0 < abs_basis →
    -- `den` fits a `u128`,
    U128 (den a_basis) →
    -- and there is at least one settlement,
    bs ≠ [] →
    -- then, given the read-only literals of the `wide_math` callees,
    kfDivGlobals S ⊢
      -- if all settlements return `Ok`, the sum `sumD` of the settled amounts
      settleChain S abs_basis a_basis bs s₀ (fun _ sumD => iprop(
        -- POSTCONDITION
        -- is within the stated bounds of `T = fl(s_0, b_n)`.
        ⌊cadence_post abs_basis a_basis bs.length sumD
          (fl abs_basis a_basis s₀ (bs.getLast?.getD s₀))⌋))

end Spec.KF
