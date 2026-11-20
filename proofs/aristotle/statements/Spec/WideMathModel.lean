module

public import Mathlib.Data.Nat.Init

/-!
# Independent mathematical model for `percolator::wide_math`

Pure definitions describing the values of the wide-integer types of
`src/wide_math.rs` (non-`kani` build) and the expected outcomes of
`div_rem_u256`, `widening_mul_u128`, `U512::mul_u256` and
`U512::checked_div_rem_by_u256`. This file contains no proofs.
-/

@[expose] public section

namespace Spec.WideMath

/-- The unsigned value of a `U256([l0, l1, l2, l3])`, whose four `u64` limbs are stored
little-endian (`l0` least significant). -/
def u256Val (l0 l1 l2 l3 : Nat) : Nat :=
  l0 + 2 ^ 64 * l1 + 2 ^ 128 * l2 + 2 ^ 192 * l3

/-- The unsigned value of the private `U512([w0, w1, w2, w3])`, whose four `u128` words are
stored little-endian (`w0` least significant). -/
def u512Val (w0 w1 w2 w3 : Nat) : Nat :=
  w0 + 2 ^ 128 * w1 + 2 ^ 256 * w2 + 2 ^ 384 * w3

/-- Expected result of `div_rem_u256(num, den)` when `den` is nonzero: the returned pair
`(q, r)` (as values) is the Euclidean quotient and remainder, i.e.
`num = q * den + r` with `r < den`. -/
def DivRemResult (num den q r : Nat) : Prop :=
  num = q * den + r ∧ r < den

/-- Expected result of the private `widening_mul_u128(a, b)`: the returned pair `(lo, hi)` of
`u128` values is the full 256-bit product, `lo + 2^128 * hi = a * b`. -/
def WideningMulResult (a b lo hi : Nat) : Prop :=
  lo + 2 ^ 128 * hi = a * b

/-- Expected result of the private `U512::mul_u256(a, b)`: the value of the returned `U512` is
the exact product of the values of the two `U256` arguments. -/
def MulU256Result (a b p : Nat) : Prop :=
  p = a * b

/-- Expected `Some((q, r))` outcome of the private `U512::checked_div_rem_by_u256(self, den)`
for a nonzero `den`, with `n` the value of `self` and `d` the value of `den`: the quotient
`n / d` fits in a `U256` (is below `2^256`), and the returned values are exactly
`q = n / d` and `r = n % d`. -/
def CheckedDivRemSome (n d q r : Nat) : Prop :=
  n / d < 2 ^ 256 ∧ q = n / d ∧ r = n % d

/-- Condition for the `None` outcome of the private `U512::checked_div_rem_by_u256(self, den)`
for a nonzero `den`, with `n` the value of `self` and `d` the value of `den`: the quotient
`n / d` does not fit in a `U256`, i.e. `2^256 ≤ n / d`. -/
def CheckedDivRemNone (n d : Nat) : Prop :=
  2 ^ 256 ≤ n / d

end Spec.WideMath

end
