module

public import Spec.KernelModel

/-!
# Independent mathematical reading of the accrual-segment kernels (engine revision `80f8f0b4`)

A mathematical reading of the `kani::ensures` clauses above three associated functions of
`V16Core` in `src/v16.rs`:

* `kernel_accrual_price_move_within_cap(old_price, new_price, max_price_move_bps_per_slot,
  segment_dt) -> V16Result<()>` (`ka_ensures`),
* `kernel_accrual_funding_num(funding_rate_e9, segment_dt, effective_price, funding_active)
  -> V16Result<i128>` (`kn_ensures`),
* `kernel_accrual_index_update(k_long, k_short, f_long, f_short, a_long, a_short, price_delta,
  funding_num) -> V16Result<(i128, i128, i128, i128)>` (`ki_ensures`, and the corrected reading
  `ki_ensures_exact`).

Conventions:

* Rust `u64` and `u128` values are modelled by `Nat` with the range predicates `U64`, `U128`;
  `i128` values by `Int` with `I128` (`Spec.Kernel`). Every arithmetic operation written in this
  file is exact (no wrapping, no overflow).
* A checked Rust operation (`checked_mul`, `checked_add`, `checked_sub`, `checked_neg`) is the
  exact result when it is in the range of its type and `none` otherwise (`checkedU128`,
  `checkedI128`). `Option::and_then` is `Option.bind`.
* `x as i128` of a `u128` is `asI128` (two's-complement reinterpretation); `/` and `%` on `i128`
  round toward zero (`Int.tdiv`, `Int.tmod`), as in Rust.
* A `V16Result<T>` is modelled by `Except Nat T`: `.ok x` is `Ok(x)` and `.error d` is `Err(e)` for
  the `V16Error` with discriminant `d` (declaration order: `InvalidConfig = 0`,
  `ArithmeticOverflow = 1`, ..., `RecoveryRequired = 9`, ...).
* `FUNDING_DEN` and `MAX_MARGIN_BPS` are the constants of `src/lib.rs`:
  `pub const FUNDING_DEN: u128 = 1_000_000_000;` and `pub const MAX_MARGIN_BPS: u64 = 10_000;`.
-/

@[expose] public section

namespace Spec.Seg
open Spec.Kernel (U64 U128 I128 two128)

/-! ## Constants -/

/-- `FUNDING_DEN` of `src/lib.rs` (`1_000_000_000`), as the `i128` value `FUNDING_DEN as i128`. -/
def FUNDING_DEN : Int := 10 ^ 9

/-- `MAX_MARGIN_BPS` of `src/lib.rs` (`10_000`). -/
def MAX_MARGIN_BPS : Nat := 10_000

/-- `i128::MIN`. -/
def I128_MIN : Int := -2 ^ 127

/-- `i128::MAX`. -/
def I128_MAX : Int := 2 ^ 127 - 1

/-- The discriminant of `V16Error::ArithmeticOverflow`. -/
def ArithmeticOverflow : Nat := 1

/-- The discriminant of `V16Error::RecoveryRequired`. -/
def RecoveryRequired : Nat := 9

/-! ## Checked operations -/

instance (x : Nat) : Decidable (U128 x) := by unfold U128; infer_instance
instance (x : Int) : Decidable (I128 x) := by unfold I128; infer_instance

/-- A `u128` checked operation with exact result `x`: `some x` if it fits in `u128`. -/
def checkedU128 (x : Nat) : Option Nat := if U128 x then some x else none

/-- An `i128` checked operation with exact result `x`: `some x` if it fits in `i128`. -/
def checkedI128 (x : Int) : Option Int := if I128 x then some x else none

/-- `x as i128` for a `u128` value `x` (two's-complement reinterpretation). -/
def asI128 (x : Nat) : Int := if (x : Int) ≤ I128_MAX then x else (x : Int) - two128

/-- `a.abs_diff(b)` on `u64`: `|a - b|`. -/
def absDiff (a b : Nat) : Nat := if a ≤ b then b - a else a - b

/-! ## KA: `kernel_accrual_price_move_within_cap` -/

/-- The `ensures` clause of `kernel_accrual_price_move_within_cap(old_price, new_price,
max_price_move_bps_per_slot, segment_dt)`:
```text
let lhs = (new_price.abs_diff(old_price) as u128) * (MAX_MARGIN_BPS as u128);
match (max_price_move_bps_per_slot as u128)
    .checked_mul(segment_dt as u128)
    .and_then(|v| v.checked_mul(old_price as u128))
{
    None => *result == Err(V16Error::ArithmeticOverflow),
    Some(rhs) => *result == if lhs > rhs { Err(V16Error::RecoveryRequired) } else { Ok(()) },
}
```
(`lhs` is computed exactly; it is below `2^78`, so the clause's `*` cannot overflow.) -/
def ka_ensures (old_price new_price max_price_move_bps_per_slot segment_dt : Nat)
    (result : Except Nat Unit) : Prop :=
  let lhs := absDiff new_price old_price * MAX_MARGIN_BPS
  match (checkedU128 (max_price_move_bps_per_slot * segment_dt)).bind
      (fun v => checkedU128 (v * old_price)) with
  | none => result = .error ArithmeticOverflow
  | some rhs => result = if lhs > rhs then .error RecoveryRequired else .ok ()

/-! ## KN: `kernel_accrual_funding_num` -/

/-- The `ensures` clause of `kernel_accrual_funding_num(funding_rate_e9, segment_dt,
effective_price, funding_active)`:
```text
if !funding_active {
    *result == Ok(0)
} else {
    match funding_rate_e9
        .checked_mul(segment_dt as i128)
        .and_then(|v| v.checked_mul(effective_price as i128))
    {
        Some(v) => *result == Ok(v),
        None => *result == Err(V16Error::ArithmeticOverflow),
    }
}
```
(`segment_dt as i128` and `effective_price as i128` of `u64` values are exact.) -/
def kn_ensures (funding_rate_e9 : Int) (segment_dt effective_price : Nat) (funding_active : Bool)
    (result : Except Nat Int) : Prop :=
  if !funding_active then result = .ok 0
  else
    match (checkedI128 (funding_rate_e9 * segment_dt)).bind
        (fun v => checkedI128 (v * effective_price)) with
    | some v => result = .ok v
    | none => result = .error ArithmeticOverflow

/-! ## KI: `kernel_accrual_index_update` -/

/-- The common part of the `ensures` clause of `kernel_accrual_index_update` for a result
`Ok((kl, ks, fl, fs))`: the range facts, the exact K updates and the `checked_sub` of the F
indices, with `funding` the reading of the funding conjunct given `dfl = fl - f_long`,
`dfs = fs - f_short`, `al = a_long as i128`, `as_ = a_short as i128`. -/
def ki_ok_common (k_long k_short f_long f_short : Int) (a_long a_short : Nat)
    (price_delta : Int) (kl ks fl fs : Int)
    (funding : Int → Int → Int → Int → Prop) : Prop :=
  let al := asI128 a_long
  let as_ := asI128 a_short
  (a_long : Int) ≤ I128_MAX ∧ (a_short : Int) ≤ I128_MAX ∧
  kl ≠ I128_MIN ∧ ks ≠ I128_MIN ∧ fl ≠ I128_MIN ∧ fs ≠ I128_MIN ∧
  (checkedI128 (price_delta * al)).bind (fun d => checkedI128 (k_long + d)) = some kl ∧
  ((checkedI128 (price_delta * as_)).bind (fun d => checkedI128 (-d))).bind
      (fun d => checkedI128 (k_short + d)) = some ks ∧
  match checkedI128 (fl - f_long), checkedI128 (fs - f_short) with
  | some dfl, some dfs => funding al as_ dfl dfs
  | _, _ => False

/-- The funding conjunct of the `ensures` clause of `kernel_accrual_index_update`, literally:
```text
if funding_num % fd == 0 {
    Some(dfl) == (funding_num / fd).checked_mul(al).and_then(|v| v.checked_neg())
        && Some(dfs) == (funding_num / fd).checked_mul(as_)
} else {
    match (funding_num.checked_mul(al).and_then(|v| v.checked_neg()), funding_num.checked_mul(as_)) {
        (Some(nl), Some(ns)) => dfl.checked_mul(fd).map_or(false, |lo| lo <= nl && nl - lo < fd)
            && dfs.checked_mul(fd).map_or(false, |lo| lo <= ns && ns - lo < fd),
        _ => false,
    }
}
```
(`nl - lo` and `ns - lo` are computed exactly.) -/
def ki_funding (funding_num : Int) (al as_ dfl dfs : Int) : Prop :=
  if Int.tmod funding_num FUNDING_DEN = 0 then
    some dfl = (checkedI128 (Int.tdiv funding_num FUNDING_DEN * al)).bind
        (fun v => checkedI128 (-v)) ∧
    some dfs = checkedI128 (Int.tdiv funding_num FUNDING_DEN * as_)
  else
    match (checkedI128 (funding_num * al)).bind (fun v => checkedI128 (-v)),
        checkedI128 (funding_num * as_) with
    | some nl, some ns =>
      (match checkedI128 (dfl * FUNDING_DEN) with
        | some lo => lo ≤ nl ∧ nl - lo < FUNDING_DEN
        | none => False) ∧
      (match checkedI128 (dfs * FUNDING_DEN) with
        | some lo => lo ≤ ns ∧ ns - lo < FUNDING_DEN
        | none => False)
    | _, _ => False

/-- The funding conjunct with the two `checked_mul(fd)` products read as exact integers (the
correction of `ki_funding`): in the non-multiple branch, `dfl * fd <= nl < dfl * fd + fd` and
`dfs * fd <= ns < dfs * fd + fd`. Everything else is as in `ki_funding`. -/
def ki_funding_exact (funding_num : Int) (al as_ dfl dfs : Int) : Prop :=
  if Int.tmod funding_num FUNDING_DEN = 0 then
    some dfl = (checkedI128 (Int.tdiv funding_num FUNDING_DEN * al)).bind
        (fun v => checkedI128 (-v)) ∧
    some dfs = checkedI128 (Int.tdiv funding_num FUNDING_DEN * as_)
  else
    match (checkedI128 (funding_num * al)).bind (fun v => checkedI128 (-v)),
        checkedI128 (funding_num * as_) with
    | some nl, some ns =>
      (dfl * FUNDING_DEN ≤ nl ∧ nl - dfl * FUNDING_DEN < FUNDING_DEN) ∧
      (dfs * FUNDING_DEN ≤ ns ∧ ns - dfs * FUNDING_DEN < FUNDING_DEN)
    | _, _ => False

/-- The `ensures` clause of `kernel_accrual_index_update(k_long, k_short, f_long, f_short,
a_long, a_short, price_delta, funding_num)`, literally (`al = a_long as i128`,
`as_ = a_short as i128`, `fd = FUNDING_DEN as i128`):
```text
Ok((kl, ks, fl, fs)) =>
    a_long <= i128::MAX as u128 && a_short <= i128::MAX as u128
    && *kl != i128::MIN && *ks != i128::MIN && *fl != i128::MIN && *fs != i128::MIN
    && price_delta.checked_mul(al).and_then(|d| k_long.checked_add(d)) == Some(*kl)
    && price_delta.checked_mul(as_).and_then(|d| d.checked_neg())
        .and_then(|d| k_short.checked_add(d)) == Some(*ks)
    && match (fl.checked_sub(f_long), fs.checked_sub(f_short)) {
        (Some(dfl), Some(dfs)) => <ki_funding>,
        _ => false,
    }
Err(e) => *e == V16Error::ArithmeticOverflow,
``` -/
def ki_ensures (k_long k_short f_long f_short : Int) (a_long a_short : Nat)
    (price_delta funding_num : Int) : Except Nat (Int × Int × Int × Int) → Prop
  | .ok (kl, ks, fl, fs) =>
    ki_ok_common k_long k_short f_long f_short a_long a_short price_delta kl ks fl fs
      (ki_funding funding_num)
  | .error e => e = ArithmeticOverflow

/-- The corrected `ensures` clause: `ki_ensures` with `ki_funding_exact` in place of
`ki_funding` (the products `dfl * fd`, `dfs * fd` are exact integers instead of
`checked_mul`s that must fit in `i128`). -/
def ki_ensures_exact (k_long k_short f_long f_short : Int) (a_long a_short : Nat)
    (price_delta funding_num : Int) : Except Nat (Int × Int × Int × Int) → Prop
  | .ok (kl, ks, fl, fs) =>
    ki_ok_common k_long k_short f_long f_short a_long a_short price_delta kl ks fl fs
      (ki_funding_exact funding_num)
  | .error e => e = ArithmeticOverflow

end Spec.Seg
