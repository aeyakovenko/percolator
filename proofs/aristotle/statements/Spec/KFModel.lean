module

/-!
# Independent mathematical model for the KF / KR properties (engine revision `bfd4d0c4`)

A mathematical reading of the free functions `scaled_adl_delta_fast`, `f_snap_after_settlement`,
`f_snap_residual_is_sub_atom` and of the associated function
`V16Core::kernel_bankruptcy_residual_step_capacity` of `src/v16.rs`, and of the properties
requested about them.

Conventions:

* Rust `u128` values are modelled by `Nat` (range predicate `U128`), `i128` values by `Int`
  (range predicate `I128`). Every arithmetic operation in this file is exact.
* A checked Rust operation (`checked_add`, `checked_mul`, `try_from`, ...) is modelled by the exact
  result together with an explicit range test; `U256` arithmetic is modelled exactly (with the
  checks of the source mirrored against `U256_MAX`).
* `floor_div_signed_conservative_i128(n, d)` is `Int.fdiv n d` (floor toward minus infinity), and
  `mul_div_floor_u256_with_rem(a, b, d)` is `(a * b / d, a * b % d)`.
* A `V16Result<T>` is modelled by `Except Nat T`: `.error d` is `Err(e)` for the `V16Error`
  with discriminant `d` (declaration order: `InvalidConfig = 0`, `ArithmeticOverflow = 1`, ...).
* Division by zero: `Int.fdiv x 0 = 0` and `n / 0 = 0` (Lean's convention). In particular
  `fl abs_basis 0 x y = 0`, since `den 0 = 0`.
-/

@[expose] public section

namespace Spec.KF

/-! ## Constants and ranges -/

/-- `POS_SCALE` of `src/lib.rs`: `1_000_000`. -/
def POS_SCALE : Nat := 10 ^ 6
/-- `ADL_ONE` of `src/lib.rs`: `1_000_000_000_000_000`. -/
def ADL_ONE : Nat := 10 ^ 15
/-- `SOCIAL_LOSS_DEN` of `src/lib.rs`: `1_000_000_000_000_000_000_000`. -/
def SOCIAL_LOSS_DEN : Nat := 10 ^ 21
/-- `MAX_VAULT_TVL` of `src/lib.rs`: `10_000_000_000_000_000`. -/
def MAX_VAULT_TVL : Nat := 10 ^ 16

/-- `i128::MIN`. -/
def I128_MIN : Int := -2 ^ 127
/-- `i128::MAX`. -/
def I128_MAX : Int := 2 ^ 127 - 1
/-- `u128::MAX`. -/
def U128_MAX : Nat := 2 ^ 128 - 1
/-- `U256::MAX`. -/
def U256_MAX : Nat := 2 ^ 256 - 1

/-- `x` is representable as an `i128`. -/
def I128 (x : Int) : Prop := I128_MIN ≤ x ∧ x ≤ I128_MAX
/-- `x` is representable as a `u128`. -/
def U128 (x : Nat) : Prop := x ≤ U128_MAX

instance (x : Int) : Decidable (I128 x) := by unfold I128; infer_instance
instance (x : Nat) : Decidable (U128 x) := by unfold U128; infer_instance

/-- The discriminant of `V16Error::ArithmeticOverflow`. -/
def ArithmeticOverflow : Nat := 1

/-! ## The settlement quantities -/

/-- `den = a_basis * POS_SCALE`, the mathematical product. -/
def den (a_basis : Nat) : Nat := a_basis * POS_SCALE

/-- `fl(x, y) = floor(abs_basis * (y - x) / den)`, rounded toward minus infinity
(`0` when `den = 0`). -/
def fl (abs_basis a_basis : Nat) (x y : Int) : Int :=
  Int.fdiv (abs_basis * (y - x)) (den a_basis)

/-- The signed ceiling `ceil(f_delta * den / abs_basis)` (`0` when `abs_basis = 0`). -/
def ceilStep (abs_basis a_basis : Nat) (f_delta : Int) : Int :=
  -Int.fdiv (-(f_delta * den a_basis)) abs_basis

/-! ## Property predicates -/

/-- KF.carry (i): the carried remainder is non-negative and worth less than one atom:
`s <= f_now` and `abs_basis * (f_now - s) < den`. -/
def carry_i (A a : Nat) (f_now s : Int) : Prop :=
  s ≤ f_now ∧ A * (f_now - s) < den a

/-- KF.carry (ii): the snapshot advance covers exactly the settled amount:
`f_delta * den <= abs_basis * (s - f_snap) < f_delta * den + abs_basis`. -/
def carry_ii (A a : Nat) (f_snap f_delta s : Int) : Prop :=
  f_delta * den a ≤ A * (s - f_snap) ∧ A * (s - f_snap) < f_delta * den a + A

/-- KF.carry: both (i) and (ii) for the result `Ok(s)`. -/
def carry_post (A a : Nat) (f_snap f_now f_delta s : Int) : Prop :=
  carry_i A a f_now s ∧ carry_ii A a f_snap f_delta s

/-- KF.ok: the exact error condition of `f_snap_after_settlement` under the KF.carry hypotheses
(with `a_basis > 0`): the snapshot moves, and the step `ceil(f_delta * den / abs_basis)` is out
of `[-(2^127 - 1), 2^127 - 1]`, or `f_snap + step` underflows `i128`, or the remaining gap
`f_now - (f_snap + step)` exceeds `i128::MAX`. -/
def kf_err_cond (A a : Nat) (f_snap f_now f_delta : Int) : Prop :=
  f_snap ≠ f_now ∧
    (¬(-I128_MAX ≤ ceilStep A a f_delta ∧ ceilStep A a f_delta ≤ I128_MAX) ∨
      f_snap + ceilStep A a f_delta < I128_MIN ∨
      f_now - (f_snap + ceilStep A a f_delta) > I128_MAX)

/-- KF.ok: the sufficient condition for `Ok`:
`|f_now - f_snap| + den <= i128::MAX` and `f_now - den >= i128::MIN`. -/
def kf_ok_sufficient (_A a : Nat) (f_snap f_now : Int) : Prop :=
  ((f_now - f_snap).natAbs : Int) + den a ≤ I128_MAX ∧ f_now - den a ≥ I128_MIN

/-- KF.cadence: after `n` settlements whose settled amounts sum to `sumD`, against the total
`T = fl(f_0, f_n)` of one settlement over the whole interval:
`sumD <= T`, `(T - sumD) * den < den + n * abs_basis`, and `sumD = T` when `abs_basis | den`. -/
def cadence_post (A a n : Nat) (sumD T : Int) : Prop :=
  sumD ≤ T ∧ (T - sumD) * den a < den a + n * A ∧ (A ∣ den a → sumD = T)

instance (A a : Nat) (f_now s : Int) : Decidable (carry_i A a f_now s) := by
  unfold carry_i; infer_instance
instance (A a : Nat) (f_snap f_delta s : Int) : Decidable (carry_ii A a f_snap f_delta s) := by
  unfold carry_ii; infer_instance
instance (A a : Nat) (f_snap f_now f_delta s : Int) :
    Decidable (carry_post A a f_snap f_now f_delta s) := by
  unfold carry_post; infer_instance
instance (A a : Nat) (f_snap f_now f_delta : Int) :
    Decidable (kf_err_cond A a f_snap f_now f_delta) := by
  unfold kf_err_cond; infer_instance
instance (A a : Nat) (f_snap f_now : Int) : Decidable (kf_ok_sufficient A a f_snap f_now) := by
  unfold kf_ok_sufficient; infer_instance
instance (A a n : Nat) (sumD T : Int) : Decidable (cadence_post A a n sumD T) := by
  unfold cadence_post; infer_instance

/-! ## Executable references of the Rust bodies -/

/-- `scaled_adl_delta_fast(abs_basis_q, a_basis, then, now) -> Option<i128>`:

```
if abs_basis_q == 0 { return Some(0); }
if a_basis != ADL_ONE { return None; }
let adl_one_i = i128::try_from(ADL_ONE).ok()?;
let delta = now.checked_sub(then)?;
if delta % adl_one_i != 0 {
    let basis_i = i128::try_from(abs_basis_q).ok()?;
    let numerator = delta.checked_mul(basis_i)?;
    let den = ADL_ONE.checked_mul(POS_SCALE)?;
    return Some(floor_div_signed_conservative_i128(numerator, den));
}
let scaled_delta = delta / adl_one_i;
let basis_i = i128::try_from(abs_basis_q).ok()?;
let numerator = scaled_delta.checked_mul(basis_i)?;
Some(floor_div_signed_conservative_i128(numerator, POS_SCALE))
``` -/
def sadRes (abs_basis_q a_basis : Nat) («then» now : Int) : Option Int :=
  if abs_basis_q = 0 then some 0
  else if a_basis ≠ ADL_ONE then none
  else
    let delta := now - «then»
    if ¬I128 delta then none
    else if Int.tmod delta ADL_ONE ≠ 0 then
      if ¬I128 abs_basis_q then none
      else
        let numerator := delta * abs_basis_q
        if ¬I128 numerator then none
        else if ¬U128 (ADL_ONE * POS_SCALE) then none
        else some (Int.fdiv numerator (ADL_ONE * POS_SCALE))
    else
      let scaled_delta := Int.tdiv delta ADL_ONE
      if ¬I128 abs_basis_q then none
      else
        let numerator := scaled_delta * abs_basis_q
        if ¬I128 numerator then none
        else some (Int.fdiv numerator POS_SCALE)

/-- `f_snap_residual_is_sub_atom(abs_basis, a_basis, f_snap, f_target) -> V16Result<bool>`:

```
if f_snap == f_target { return Ok(true); }
if abs_basis == 0 || f_snap > f_target { return Ok(false); }
let gap = f_target.checked_sub(f_snap).ok_or(V16Error::ArithmeticOverflow)?.unsigned_abs();
let den = a_basis.checked_mul(POS_SCALE).ok_or(V16Error::ArithmeticOverflow)?;
Ok(match gap.checked_mul(abs_basis) { Some(value) => value < den, None => false })
``` -/
def subAtomRes (abs_basis a_basis : Nat) (f_snap f_target : Int) : Except Nat Bool :=
  if f_snap = f_target then .ok true
  else if abs_basis = 0 ∨ f_snap > f_target then .ok false
  else if ¬I128 (f_target - f_snap) then .error ArithmeticOverflow
  else
    let gap := (f_target - f_snap).natAbs
    if ¬U128 (den a_basis) then .error ArithmeticOverflow
    else if U128 (gap * abs_basis) then .ok (decide (gap * abs_basis < den a_basis))
    else .ok false

/-- `f_snap_after_settlement(abs_basis, a_basis, f_snap, f_now, f_delta) -> V16Result<i128>`:

```
if abs_basis == 0 || f_snap == f_now { return Ok(f_now); }
let den = a_basis.checked_mul(POS_SCALE).ok_or(V16Error::ArithmeticOverflow)?;
let abs_delta = f_delta.unsigned_abs();
let (q, r) = match abs_delta.checked_mul(den) {
    Some(p) => (p / abs_basis, p % abs_basis),
    None => {
        let (q, r) = mul_div_floor_u256_with_rem(
            U256::from_u128(abs_delta), U256::from_u128(den), U256::from_u128(abs_basis));
        (q.try_into_u128().ok_or(V16Error::ArithmeticOverflow)?,
         r.try_into_u128().ok_or(V16Error::ArithmeticOverflow)?)
    }
};
let step = if f_delta >= 0 {
    let mag = if r != 0 { q.checked_add(1).ok_or(V16Error::ArithmeticOverflow)? } else { q };
    i128::try_from(mag).map_err(|_| V16Error::ArithmeticOverflow)?
} else {
    i128::try_from(q).map_err(|_| V16Error::ArithmeticOverflow)?
        .checked_neg().ok_or(V16Error::ArithmeticOverflow)?
};
let next = f_snap.checked_add(step).ok_or(V16Error::ArithmeticOverflow)?;
if next > f_now || !f_snap_residual_is_sub_atom(abs_basis, a_basis, next, f_now)? {
    return Err(V16Error::ArithmeticOverflow);
}
Ok(next)
``` -/
def fsnapRes (abs_basis a_basis : Nat) (f_snap f_now f_delta : Int) : Except Nat Int :=
  if abs_basis = 0 ∨ f_snap = f_now then .ok f_now
  else if ¬U128 (den a_basis) then .error ArithmeticOverflow
  else
    let p := f_delta.natAbs * den a_basis
    -- both arms of the `match` compute `(p / abs_basis, p % abs_basis)`; the wide arm checks
    -- that both fit in `u128`
    let q := p / abs_basis
    let r := p % abs_basis
    if ¬U128 p ∧ ¬(U128 q ∧ U128 r) then .error ArithmeticOverflow
    else
      let stepRes : Except Nat Int :=
        if f_delta ≥ 0 then
          if r ≠ 0 ∧ ¬U128 (q + 1) then .error ArithmeticOverflow
          else
            let mag := if r ≠ 0 then q + 1 else q
            if (mag : Int) > I128_MAX then .error ArithmeticOverflow else .ok (mag : Int)
        else if (q : Int) > I128_MAX then .error ArithmeticOverflow
        else if ¬I128 (-(q : Int)) then .error ArithmeticOverflow
        else .ok (-(q : Int))
      match stepRes with
      | .error e => .error e
      | .ok step =>
        let next := f_snap + step
        if ¬I128 next then .error ArithmeticOverflow
        else if next > f_now then .error ArithmeticOverflow
        else match subAtomRes abs_basis a_basis next f_now with
          | .error e => .error e
          | .ok false => .error ArithmeticOverflow
          | .ok true => .ok next

/-- A chain of settlements: starting from the snapshot `s`, settle against each successive
`f_now` in `fs` with `f_delta = fl(s, f_now)`; returns the final snapshot and the sum of the
settled amounts `f_delta`, or the first error. -/
def chainRes (A a : Nat) (s : Int) : List Int → Except Nat (Int × Int)
  | [] => .ok (s, 0)
  | f :: fs =>
    match fsnapRes A a s f (fl A a s f) with
    | .error e => .error e
    | .ok s' =>
      match chainRes A a s' fs with
      | .error e => .error e
      | .ok (sf, sumD) => .ok (sf, fl A a s f + sumD)

/-! ## Part 2: the bankruptcy-residual booking step -/

/-- The B-index increment of booking `c` atoms against remainder `rem` and weight sum
`weight_sum`: `(c * SOCIAL_LOSS_DEN + rem) / weight_sum`. -/
def bookDelta (c rem weight_sum : Nat) : Nat := (c * SOCIAL_LOSS_DEN + rem) / weight_sum

/-- The `Ok(chunk)` arm of the `kani::ensures` clause of
`kernel_bankruptcy_residual_step_capacity` for the result `Ok(c)`:

```
Ok(chunk) => *chunk <= residual_remaining
    && *chunk <= chunk_atoms
    && (residual_remaining != 0 && weight_sum != 0 || *chunk == 0),
Err(e) => *e == V16Error::ArithmeticOverflow,
``` -/
def capacity_ensures (_b_now weight_sum _rem residual_remaining chunk_atoms c : Nat) : Prop :=
  c ≤ residual_remaining ∧ c ≤ chunk_atoms ∧
    (residual_remaining ≠ 0 ∧ weight_sum ≠ 0 ∨ c = 0)

instance (b w r rr ca c : Nat) : Decidable (capacity_ensures b w r rr ca c) := by
  unfold capacity_ensures; infer_instance

/-- `V16Core::kernel_bankruptcy_residual_step_capacity(b_now, weight_sum, rem,
residual_remaining, chunk_atoms) -> V16Result<u128>` (`kernel_social_loss_chunk_cap(rr, ca)` is
`rr.min(ca)`):

```
if residual_remaining == 0 || weight_sum == 0 { return Ok(0); }
let candidate = Self::kernel_social_loss_chunk_cap(residual_remaining, chunk_atoms);
if candidate != 0 {
    if let Some(delta_b) = candidate.checked_mul(SOCIAL_LOSS_DEN)
        .and_then(|v| v.checked_add(rem)).map(|v| v / weight_sum)
    {
        if delta_b != 0 && b_now.checked_add(delta_b).is_some() { return Ok(candidate); }
    }
}
let headroom_plus_one = U256::from_u128(u128::MAX - b_now).checked_add(U256::ONE)
    .ok_or(V16Error::ArithmeticOverflow)?;
let max_scaled = headroom_plus_one.checked_mul(U256::from_u128(weight_sum))
    .and_then(|v| v.checked_sub(U256::ONE)).ok_or(V16Error::ArithmeticOverflow)?;
if U256::from_u128(rem) > max_scaled { return Ok(0); }
let max_chunk_by_b_wide = max_scaled.checked_sub(U256::from_u128(rem))
    .and_then(|v| v.checked_div(U256::from_u128(SOCIAL_LOSS_DEN)))
    .ok_or(V16Error::ArithmeticOverflow)?;
let max_chunk_by_b = max_chunk_by_b_wide.try_into_u128().unwrap_or(residual_remaining);
Ok(residual_remaining.min(max_chunk_by_b).min(chunk_atoms))
``` -/
def capacityRes (b_now weight_sum rem rr ca : Nat) : Except Nat Nat :=
  if rr = 0 ∨ weight_sum = 0 then .ok 0
  else
    let candidate := min rr ca
    if candidate ≠ 0 ∧ U128 (candidate * SOCIAL_LOSS_DEN) ∧
        U128 (candidate * SOCIAL_LOSS_DEN + rem) ∧
        bookDelta candidate rem weight_sum ≠ 0 ∧
        U128 (b_now + bookDelta candidate rem weight_sum) then .ok candidate
    else
      let headroom_plus_one := (U128_MAX - b_now) + 1
      if headroom_plus_one > U256_MAX then .error ArithmeticOverflow
      else if headroom_plus_one * weight_sum > U256_MAX ∨ headroom_plus_one * weight_sum < 1 then
        .error ArithmeticOverflow
      else
        let max_scaled := headroom_plus_one * weight_sum - 1
        if rem > max_scaled then .ok 0
        else
          let wide := (max_scaled - rem) / SOCIAL_LOSS_DEN
          let max_chunk_by_b := if U128 wide then wide else rr
          .ok (min (min rr max_chunk_by_b) ca)

end Spec.KF
