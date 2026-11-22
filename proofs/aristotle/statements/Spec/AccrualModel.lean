module

public import Spec.KernelModel

/-!
# Independent mathematical reading of `canonical_accrual_price_step_v16`

A mathematical reading of the `kani::ensures` clause above the free function
`canonical_accrual_price_step_v16(current, target, cap_anchor, max_change_bps, exposed,
remainder_before_bps_num) -> V16Result<(u64, u16)>` of `src/v16.rs` (engine revision `f24a561c`),
and of the quantities used by the multi-step properties.

Conventions:

* Rust `u64` and `u16` values are modelled by `Nat` with the range predicates `U64` (`x < 2^64`)
  and `U16` (`x < 2^16`).
* `+`, `-`, `*` inside the model are exact arithmetic on `Nat`; a subtraction is only used where
  its operands are ordered (as in the source), and `absDiff a b` is `|a - b|`.
* `MAX_ORACLE_PRICE` and `MAX_MARGIN_BPS` are the constants of `src/lib.rs`:
  `pub const MAX_ORACLE_PRICE: u64 = 1_000_000_000_000;` and
  `pub const MAX_MARGIN_BPS: u64 = 10_000;`.
* A `V16Result<(u64, u16)>` is modelled by `Except Nat (Nat × Nat)`: `.ok (p, r)` is
  `Ok((p, r))` and `.error d` is `Err(e)` for the `V16Error` with discriminant `d`
  (declaration order: `InvalidConfig = 0`, `ArithmeticOverflow = 1`, ...).
-/

@[expose] public section

namespace Spec.Accrual
open Spec.Kernel (U64)

/-- A `u16` value. -/
def U16 (x : Nat) : Prop := x < 2 ^ 16

/-- `MAX_ORACLE_PRICE` of `src/lib.rs`: `1_000_000_000_000`. -/
def MAX_ORACLE_PRICE : Nat := 1000000000000

/-- `MAX_MARGIN_BPS` of `src/lib.rs`: `10_000`. -/
def MAX_MARGIN_BPS : Nat := 10000

/-- The Rust discriminant of `V16Error::InvalidConfig`. -/
def errInvalidConfig : Nat := 0

/-- `a.abs_diff(b)`, i.e. `|a - b|` in exact arithmetic. -/
def absDiff (a b : Nat) : Nat := if a ≤ b then b - a else a - b

/-- `x` lies between `a` and `b` inclusive (in either order). -/
def Between (a b x : Nat) : Prop := min a b ≤ x ∧ x ≤ max a b

/-- The `valid` condition of the `ensures` clause:

```
current != 0 && target != 0 && cap_anchor != 0
    && current <= MAX_ORACLE_PRICE && target <= MAX_ORACLE_PRICE && cap_anchor <= MAX_ORACLE_PRICE
    && (remainder_before_bps_num as u64) < MAX_MARGIN_BPS
``` -/
def accrualValid (current target cap_anchor remainder_before_bps_num : Nat) : Prop :=
  current ≠ 0 ∧ target ≠ 0 ∧ cap_anchor ≠ 0 ∧
  current ≤ MAX_ORACLE_PRICE ∧ target ≤ MAX_ORACLE_PRICE ∧ cap_anchor ≤ MAX_ORACLE_PRICE ∧
  remainder_before_bps_num < MAX_MARGIN_BPS

instance (current target cap_anchor remainder_before_bps_num : Nat) :
    Decidable (accrualValid current target cap_anchor remainder_before_bps_num) := by
  unfold accrualValid; infer_instance

/-- The `ensures` clause of `canonical_accrual_price_step_v16`, literally, about the result
`result` of the call (`.ok (next, rem)` for `Ok((next, rem))`, `.error d` for `Err(e)` with
discriminant `d`):

```
if !valid {
    *result == Err(V16Error::InvalidConfig)
} else if !exposed {
    *result == Ok((target, 0))
} else if current == target || max_change_bps == 0 {
    *result == Ok((current, 0))
} else {
    let allowance = (cap_anchor as u128) * (max_change_bps as u128) + remainder_before_bps_num as u128;
    let distance = if current > target { current - target } else { target - current } as u128;
    match result {
        Ok((next, 0)) if *next == target => distance * (MAX_MARGIN_BPS as u128) <= allowance,
        Ok((next, rem)) => {
            let moved = if *next > current { *next - current } else { current - *next } as u128;
            (moved == 0 || (*next > current) == (target > current))
                && moved < distance
                && (*rem as u64) < MAX_MARGIN_BPS
                && moved * (MAX_MARGIN_BPS as u128) + *rem as u128 == allowance
        }
        Err(_) => false,
    }
}
```

The arithmetic of the clause is read in exact integers (`allowance`, `distance * MAX_MARGIN_BPS`
and `moved * MAX_MARGIN_BPS + rem` all fit in a `u128` for valid inputs). -/
def accrual_ensures (current target cap_anchor max_change_bps : Nat) (exposed : Bool)
    (remainder_before_bps_num : Nat) (result : Except Nat (Nat × Nat)) : Prop :=
  if ¬ accrualValid current target cap_anchor remainder_before_bps_num then
    result = .error errInvalidConfig
  else if exposed = false then
    result = .ok (target, 0)
  else if current = target ∨ max_change_bps = 0 then
    result = .ok (current, 0)
  else
    let allowance := cap_anchor * max_change_bps + remainder_before_bps_num
    let distance := absDiff current target
    match result with
    | .ok (next, rem) =>
      if rem = 0 ∧ next = target then
        distance * MAX_MARGIN_BPS ≤ allowance
      else
        let moved := absDiff next current
        (moved = 0 ∨ (decide (current < next) = decide (current < target))) ∧
        moved < distance ∧
        rem < MAX_MARGIN_BPS ∧
        moved * MAX_MARGIN_BPS + rem = allowance
    | .error _ => False

end Spec.Accrual
