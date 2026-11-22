module

public import Spec.Kernel
public import Spec.SegModel
public import Spec.Rust.Seg

/-!
# Public contracts for the per-segment accrual kernels (engine revision `80f8f0b4`)

Associated functions of `V16Core` in `src/v16.rs`, as compiled in the program `Mir.Seg`
(handles in `Spec.Rust.Seg`): default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`.

* Part 1, `kernel_accrual_price_move_within_cap(old_price, new_price,
  max_price_move_bps_per_slot, segment_dt) -> V16Result<()>`: KA.contract, KA.nopanic.
* Part 2, `kernel_accrual_funding_num(funding_rate_e9, segment_dt, effective_price,
  funding_active) -> V16Result<i128>`: KN.contract, KN.nopanic.
* Part 3, `kernel_accrual_index_update(k_long, k_short, f_long, f_short, a_long, a_short,
  price_delta, funding_num) -> V16Result<(i128, i128, i128, i128)>` and
  `kernel_adl_scaled_accrual_index_deltas(price_delta, funding_num, a_long, a_short)`:
  KI.contract (as written, and corrected), KI.nopanic, KI.floors, KI.funding_conserves,
  KI.mark_to_market, KI.path (one call, and two successive calls).

Inputs range over the whole of their Rust types. Exact integers throughout; `FD = FUNDING_DEN =
10^9`, and `Int.fdiv x FD` is `floor(x / FD)`, rounded toward minus infinity.

Every property is a partial-correctness statement: `ifReturns S P` says that `P` holds of the
result if the call returns; `S.returns` additionally says that no execution panics (unwinds).
Nothing is claimed about termination (the functions have no loops).
-/

@[expose] public section

namespace Spec.Seg
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (U64 U128 I128 okValue errValue v16ErrorValue ifReturns)

/-! ## Values and calls -/

/-- The Rust value of a `V16Result<()>`: `Ok(())` or `Err(e)` (by discriminant). -/
def unitResultValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Except Nat Unit → S.Value
  | .ok () => okValue S S.unitVal
  | .error d => errValue S (v16ErrorValue S d)

/-- The Rust value of a `V16Result<i128>`. -/
def i128ResultValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Except Nat Int → S.Value
  | .ok x => okValue S (S.intVal x)
  | .error d => errValue S (v16ErrorValue S d)

/-- The Rust value of an `(i128, i128, i128, i128)` tuple. -/
def quadValue {PROP : Type} [BI PROP] (S : Vocab PROP) (a b c d : Int) : S.Value :=
  S.tupleVal [S.intVal a, S.intVal b, S.intVal c, S.intVal d]

/-- The Rust value of a `V16Result<(i128, i128, i128, i128)>`. -/
def quadResultValue {PROP : Type} [BI PROP] (S : Vocab PROP) :
    Except Nat (Int × Int × Int × Int) → S.Value
  | .ok (a, b, c, d) => okValue S (quadValue S a b c d)
  | .error e => errValue S (v16ErrorValue S e)

/-- A `V16Result<T>` is well formed: `Err(e)` for one of the twelve `V16Error` variants, or
`Ok(x)` with `x` satisfying the range predicate `wf` of `T`. -/
def ResultWF {T : Type} (wf : T → Prop) : Except Nat T → Prop
  | .ok x => wf x
  | .error d => d < 12

/-- The four components are `i128` values. -/
def QuadWF : Int × Int × Int × Int → Prop
  | (a, b, c, d) => I128 a ∧ I128 b ∧ I128 c ∧ I128 d

/-- The call `V16Core::kernel_accrual_price_move_within_cap(old_price, new_price,
max_price_move_bps_per_slot, segment_dt)` with outcome predicate `Φ`. -/
def callPriceMoveWithinCap {PROP : Type} [BI PROP] (S : Vocab PROP)
    (old_price new_price max_price_move_bps_per_slot segment_dt : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap»
    [(S.intVal old_price,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap» 0),
     (S.intVal new_price,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap» 1),
     (S.intVal max_price_move_bps_per_slot,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap» 2),
     (S.intVal segment_dt,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_price_move_within_cap» 3)] Φ

/-- The call `V16Core::kernel_accrual_funding_num(funding_rate_e9, segment_dt, effective_price,
funding_active)` with outcome predicate `Φ`. -/
def callFundingNum {PROP : Type} [BI PROP] (S : Vocab PROP)
    (funding_rate_e9 : Int) (segment_dt effective_price : Nat) (funding_active : Bool)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_funding_num»
    [(S.intVal funding_rate_e9,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_funding_num» 0),
     (S.intVal segment_dt,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_funding_num» 1),
     (S.intVal effective_price,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_funding_num» 2),
     (S.boolVal funding_active,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_funding_num» 3)] Φ

/-- The call `V16Core::kernel_accrual_index_update(k_long, k_short, f_long, f_short, a_long,
a_short, price_delta, funding_num)` with outcome predicate `Φ`. -/
def callIndexUpdate {PROP : Type} [BI PROP] (S : Vocab PROP)
    (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update»
    [(S.intVal k_long, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 0),
     (S.intVal k_short, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 1),
     (S.intVal f_long, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 2),
     (S.intVal f_short, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 3),
     (S.intVal a_long, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 4),
     (S.intVal a_short, S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 5),
     (S.intVal price_delta,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 6),
     (S.intVal funding_num,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_accrual_index_update» 7)] Φ

/-- The call `V16Core::kernel_adl_scaled_accrual_index_deltas(price_delta, funding_num, a_long,
a_short)` with outcome predicate `Φ`. -/
def callIndexDeltas {PROP : Type} [BI PROP] (S : Vocab PROP)
    (price_delta funding_num : Int) (a_long a_short : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Seg.Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas»
    [(S.intVal price_delta,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas» 0),
     (S.intVal funding_num,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas» 1),
     (S.intVal a_long,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas» 2),
     (S.intVal a_short,
        S.argTy Mir.Seg.Fn.«percolator::v16::V16Core::kernel_adl_scaled_accrual_index_deltas» 3)] Φ

/-- A postcondition about the `Ok` result of `kernel_accrual_index_update`: if the call returns
`Ok((kl, ks, fl, fs))`, then `P kl ks fl fs`. An `Err` return or an unwind is not constrained. -/
def ifOkQuad {PROP : Type} [BI PROP] (S : Vocab PROP) (P : Int → Int → Int → Int → Prop) :
    CallOutcome S.Value → PROP :=
  ifReturns S fun v => ∀ kl ks fl fs : Int, v = okValue S (quadValue S kl ks fl fs) →
    P kl ks fl fs

/-- What follows the first of two successive calls of `kernel_accrual_index_update`: if it
returns `Ok((kl, ks, fl, fs))`, continue with `k kl ks fl fs`; if it returns
`Err(_)` or unwinds, nothing more is claimed. -/
def onOkQuad {PROP : Type} [BI PROP] (S : Vocab PROP) (k : Int → Int → Int → Int → PROP) :
    CallOutcome S.Value → PROP
  | .returned v => iprop(∀ kl : Int, ∀ ks : Int, ∀ fl : Int, ∀ fs : Int,
      ⌜v = okValue S (quadValue S kl ks fl fs)⌝ -∗ k kl ks fl fs)
  | .unwound => iprop(True)

/-! ## Part 1: KA -/

/-- **KA.contract.** For all `u64` inputs, if
`kernel_accrual_price_move_within_cap(old_price, new_price, max_price_move_bps_per_slot,
segment_dt)` returns, its result satisfies the `ensures` clause above the function
(`ka_ensures`). -/
def kernel_accrual_price_move_within_cap_contract : Property := fun S =>
  -- For any old and new price, per-slot cap and segment length,
  ∀ (old_price new_price max_price_move_bps_per_slot segment_dt : Nat),
    -- PRECONDITIONS
    -- if they are `u64` values (there is no `requires`),
    U64 old_price → U64 new_price → U64 max_price_move_bps_per_slot → U64 segment_dt →
    ⊢ -- then, if the call returns,
      callPriceMoveWithinCap S old_price new_price max_price_move_bps_per_slot segment_dt
        (ifReturns S fun v =>
        -- POSTCONDITION
        -- the result is a `V16Result<()>` satisfying the clause.
        ∃ res, ResultWF (fun _ => True) res ∧ v = unitResultValue S res ∧
          ka_ensures old_price new_price max_price_move_bps_per_slot segment_dt res)

/-- **KA.nopanic.** No execution of `kernel_accrual_price_move_within_cap(..)` panics. -/
def kernel_accrual_price_move_within_cap_nopanic : Property := fun S =>
  -- For any old and new price, per-slot cap and segment length,
  ∀ (old_price new_price max_price_move_bps_per_slot segment_dt : Nat),
    -- PRECONDITIONS
    -- if they are `u64` values,
    U64 old_price → U64 new_price → U64 max_price_move_bps_per_slot → U64 segment_dt →
    ⊢ -- then the call does not unwind (panic).
      callPriceMoveWithinCap S old_price new_price max_price_move_bps_per_slot segment_dt
        (S.returns fun _ => iprop(True))

/-! ## Part 2: KN -/

/-- **KN.contract.** For all inputs, if `kernel_accrual_funding_num(funding_rate_e9, segment_dt,
effective_price, funding_active)` returns, its result satisfies the `ensures` clause above the
function (`kn_ensures`). -/
def kernel_accrual_funding_num_contract : Property := fun S =>
  -- For any funding rate, segment length, price and activity flag,
  ∀ (funding_rate_e9 : Int) (segment_dt effective_price : Nat) (funding_active : Bool),
    -- PRECONDITIONS
    -- if they are values of their Rust types (there is no `requires`),
    I128 funding_rate_e9 → U64 segment_dt → U64 effective_price →
    ⊢ -- then, if the call returns,
      callFundingNum S funding_rate_e9 segment_dt effective_price funding_active
        (ifReturns S fun v =>
        -- POSTCONDITION
        -- the result is a `V16Result<i128>` satisfying the clause.
        ∃ res, ResultWF I128 res ∧ v = i128ResultValue S res ∧
          kn_ensures funding_rate_e9 segment_dt effective_price funding_active res)

/-- **KN.nopanic.** No execution of `kernel_accrual_funding_num(..)` panics. -/
def kernel_accrual_funding_num_nopanic : Property := fun S =>
  -- For any funding rate, segment length, price and activity flag,
  ∀ (funding_rate_e9 : Int) (segment_dt effective_price : Nat) (funding_active : Bool),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 funding_rate_e9 → U64 segment_dt → U64 effective_price →
    ⊢ -- then the call does not unwind (panic).
      callFundingNum S funding_rate_e9 segment_dt effective_price funding_active
        (S.returns fun _ => iprop(True))

/-! ## Part 3: KI -/

/-- **KI.contract, as written.** For all inputs, if `kernel_accrual_index_update(..)` returns,
its result satisfies the `ensures` clause above the function (`ki_ensures`).

This literal statement is false: for `k_long = k_short = f_long = f_short = 0`,
`a_long = a_short = 1`, `price_delta = 0`, `funding_num = i128::MAX`, the call returns
`Ok((0, 0, fl, fs))` with `fl = floor(-i128::MAX / FD) = -170141183460469231731687303716`, and
the clause's `dfl.checked_mul(fd)` (`-170141183460469231731687303716000000000 < i128::MIN`)
overflows, so `map_or(false, ..)` makes the clause false. See
`kernel_accrual_index_update_contract_exact` for the corrected property. -/
def kernel_accrual_index_update_contract : Property := fun S =>
  -- For any indices, effective positions, price delta and funding amount,
  ∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types (there is no `requires`),
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    ⊢ -- then, if the call returns,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifReturns S fun v =>
        -- POSTCONDITION
        -- the result is a `V16Result<(i128, i128, i128, i128)>` satisfying the clause.
        ∃ res, ResultWF QuadWF res ∧ v = quadResultValue S res ∧
          ki_ensures k_long k_short f_long f_short a_long a_short price_delta funding_num res)

/-- **KI.contract, corrected.** For all inputs, if `kernel_accrual_index_update(..)` returns, its
result satisfies the `ensures` clause with the two products `dfl * fd` and `dfs * fd` read as
exact integers instead of `checked_mul`s (`ki_ensures_exact`); every other conjunct is literal. -/
def kernel_accrual_index_update_contract_exact : Property := fun S =>
  -- For any indices, effective positions, price delta and funding amount,
  ∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    ⊢ -- then, if the call returns,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifReturns S fun v =>
        -- POSTCONDITION
        -- the result is a `V16Result<(i128, i128, i128, i128)>` satisfying the corrected clause.
        ∃ res, ResultWF QuadWF res ∧ v = quadResultValue S res ∧
          ki_ensures_exact k_long k_short f_long f_short a_long a_short price_delta funding_num
            res)

/-- **KI.nopanic.** Neither `kernel_accrual_index_update(..)` nor
`kernel_adl_scaled_accrual_index_deltas(price_delta, funding_num, a_long, a_short)` panics, on
any input. -/
def kernel_accrual_index_update_nopanic : Property := fun S =>
  -- `kernel_accrual_index_update`: for any inputs of their Rust types,
  (∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    ⊢ -- the call does not unwind (panic);
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (S.returns fun _ => iprop(True))) ∧
  -- `kernel_adl_scaled_accrual_index_deltas`: for any inputs of their Rust types,
  (∀ (price_delta funding_num : Int) (a_long a_short : Nat),
    I128 price_delta → I128 funding_num → U128 a_long → U128 a_short →
    ⊢ -- the call does not unwind (panic).
      callIndexDeltas S price_delta funding_num a_long a_short (S.returns fun _ => iprop(True)))

/-- **KI.floors.** If `kernel_accrual_index_update(..)` returns `Ok((kl, ks, fl, fs))`, then
`fl - f_long = floor(-funding_num * a_long / FD)` and `fs - f_short = floor(funding_num * a_short
/ FD)` (exact integers, rounded toward minus infinity), whether or not `funding_num` is a multiple
of `FD`. -/
def kernel_accrual_index_update_floors : Property := fun S =>
  -- For any indices, effective positions, price delta and funding amount,
  ∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    ⊢ -- then, if the call returns `Ok((kl, ks, fl, fs))`,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifOkQuad S fun _ _ fl fs =>
        -- POSTCONDITIONS
        -- the F indices moved by the floored funding deltas.
        fl - f_long = Int.fdiv (-funding_num * a_long) FUNDING_DEN ∧
        fs - f_short = Int.fdiv (funding_num * a_short) FUNDING_DEN)

/-- **KI.funding_conserves.** If `kernel_accrual_index_update(..)` returns
`Ok((kl, ks, fl, fs))` with `a_long >= 1` and `a_short >= 1`, then with `dl = fl - f_long` and
`ds = fs - f_short`: `dl * a_short + ds * a_long <= 0` and
`dl * a_short + ds * a_long > -(a_long + a_short)`; and if `funding_num` is a multiple of `FD`,
`dl * a_short + ds * a_long = 0`. -/
def kernel_accrual_index_update_funding_conserves : Property := fun S =>
  -- For any indices, effective positions, price delta and funding amount,
  ∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    -- and both effective positions are at least 1,
    1 ≤ a_long → 1 ≤ a_short →
    ⊢ -- then, if the call returns `Ok((kl, ks, fl, fs))`,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifOkQuad S fun _ _ fl fs =>
        -- POSTCONDITIONS
        -- funding creates no value,
        (fl - f_long) * a_short + (fs - f_short) * a_long ≤ 0 ∧
        -- the cross-side dust is less than one index unit per side,
        -(((a_long : Int) + a_short)) < (fl - f_long) * a_short + (fs - f_short) * a_long ∧
        -- and a whole multiple of `FD` is transferred exactly.
        (FUNDING_DEN ∣ funding_num →
          (fl - f_long) * a_short + (fs - f_short) * a_long = 0))

/-- **KI.mark_to_market.** If `kernel_accrual_index_update(..)` returns `Ok((kl, ks, fl, fs))`
and `a_long == a_short`, then `(kl - k_long) + (ks - k_short) = 0`. -/
def kernel_accrual_index_update_mark_to_market : Property := fun S =>
  -- For any indices, effective positions, price delta and funding amount,
  ∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    -- and the two effective positions are equal,
    a_long = a_short →
    ⊢ -- then, if the call returns `Ok((kl, ks, fl, fs))`,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifOkQuad S fun kl ks _ _ =>
        -- POSTCONDITION
        -- mark to market creates no value.
        (kl - k_long) + (ks - k_short) = 0)

/-- **KI.path.** (1) If `kernel_accrual_index_update(..)` returns `Ok((kl, ks, fl, fs))`, then
`kl - k_long = price_delta * a_long` and `ks - k_short = -(price_delta * a_short)` (exact
integers). (2) For two successive calls with the same `a_long` and `a_short`, the second starting
from the four indices returned by the first, with price deltas `d1` then `d2` and any funding
amounts `n1`, `n2`: if both return `Ok`, the K indices after the second are
`k_long + (d1 + d2) * a_long` and `k_short - (d1 + d2) * a_short`. -/
def kernel_accrual_index_update_path : Property := fun S =>
  -- (1) One call: for any indices, effective positions, price delta and funding amount,
  (∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (price_delta funding_num : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 price_delta → I128 funding_num →
    ⊢ -- then, if the call returns `Ok((kl, ks, fl, fs))`,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short price_delta funding_num
        (ifOkQuad S fun kl ks _ _ =>
        -- POSTCONDITION
        -- the K indices moved by exactly `price_delta * a_long` and `-(price_delta * a_short)`.
        kl - k_long = price_delta * a_long ∧ ks - k_short = -(price_delta * a_short))) ∧
  -- (2) Two successive calls: for any starting indices, effective positions, price deltas
  -- `d1`, `d2` and funding amounts `n1`, `n2`,
  (∀ (k_long k_short f_long f_short : Int) (a_long a_short : Nat) (d1 n1 d2 n2 : Int),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    I128 k_long → I128 k_short → I128 f_long → I128 f_short → U128 a_long → U128 a_short →
    I128 d1 → I128 n1 → I128 d2 → I128 n2 →
    ⊢ -- then, if the first call returns `Ok((kl1, ks1, fl1, fs1))` and the second call, made
      -- from those indices, returns `Ok((kl2, ks2, fl2, fs2))`,
      callIndexUpdate S k_long k_short f_long f_short a_long a_short d1 n1
        (onOkQuad S fun kl1 ks1 fl1 fs1 =>
          callIndexUpdate S kl1 ks1 fl1 fs1 a_long a_short d2 n2
            (ifOkQuad S fun kl2 ks2 _ _ =>
            -- POSTCONDITION
            -- the K indices moved by the summed price delta.
            kl2 = k_long + (d1 + d2) * a_long ∧ ks2 = k_short - (d1 + d2) * a_short)))

end Spec.Seg
