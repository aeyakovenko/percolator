# Specification: `wide_math` (deployed build)

Scope: the module `src/wide_math.rs` of this crate as compiled without `cfg(kani)`, that is,
`U256` and `I256` stored as `[u64; 4]`, the private `U512`, and the free functions below. These are
the arithmetic helpers every accounting formula of the risk engine goes through.

Please prove **functional correctness against mathematical integers**, not only absence of
undefined behavior. A property that fails should be reported as refuted with the failing input.

## Notation

- For `x: U256` with limbs `x.0 = [l0, l1, l2, l3]`, `val(x) = l0 + 2^64*l1 + 2^128*l2 + 2^192*l3`,
  a natural number below `2^256`.
- For `x: I256`, `sval(x)` is the two's-complement value of the same four limbs, an integer in
  `[-2^255, 2^255)`.
- For the private `U512` with `x.0 = [w0, w1, w2, w3]` (`u128` words),
  `val(x) = w0 + 2^128*w1 + 2^256*w2 + 2^384*w3`.
- `floor(p/q)` and `ceil(p/q)` are the mathematical floor and ceiling of the rational `p/q`, also
  for negative `p` (floor rounds toward minus infinity).
- "Returns" means the call returns normally. "Panics" means it does not return. Unless a property
  says otherwise, a property about a returned value says nothing about inputs on which the function
  panics; the panic conditions are stated as separate properties.

## A. Representation

- A1. `U256::new(lo, hi)` has `val = lo + 2^128*hi`; `x.lo()` and `x.hi()` return the low and high
  128 bits of `val(x)`; `U256::from_u128(v)` and `U256::from_u64(v)` have `val = v`.
- A2. `U256::ZERO`, `U256::ONE`, `U256::MAX` have values `0`, `1`, `2^256 - 1`. `x.is_zero()` is
  true exactly when `val(x) = 0`.
- A3. `x.try_into_u128()` is `Some(val(x))` exactly when `val(x) < 2^128`, otherwise `None`.
- A4. Derived equality on `U256` holds exactly when the values are equal, and `Ord`/`PartialOrd` on
  `U256` agree with the order of `val`.
- A5. `I256::from_i128(v)` has `sval = v`; `I256::from_u128(v)` has `sval = v`;
  `x.try_into_i128()` is `Some(sval(x))` exactly when `sval(x)` fits in `i128`, otherwise `None`.
- A6. `I256::ZERO`, `ONE`, `MINUS_ONE`, `MAX`, `MIN` have `sval` `0`, `1`, `-1`, `2^255 - 1`,
  `-2^255`. `is_zero`, `is_negative`, `is_positive` and `signum` agree with the sign of `sval`.
- A7. `x.abs_u256()` has `val = |sval(x)|` for every `x` other than `I256::MIN`, and no execution
  panics there. At `I256::MIN` the function asserts `self != Self::MIN`, so every execution panics.
  (Corrected on 2026-11-20: the first version of this label claimed the value `2^255` at
  `I256::MIN`; the proof attempt showed the assert.)
- A8. `I256::from_raw_u256(v)` reinterprets the limbs: `sval = val(v)` if `val(v) < 2^255`, else
  `val(v) - 2^256`. `I256::from_u256_or_overflow(v)` is `Some` of that exactly when
  `val(v) < 2^255`, otherwise `None`.

## B. `U256` arithmetic

- B1. `a.checked_add(b)` is `Some(c)` with `val(c) = val(a) + val(b)` exactly when that sum is below
  `2^256`, otherwise `None`.
- B2. `a.checked_sub(b)` is `Some(c)` with `val(c) = val(a) - val(b)` exactly when
  `val(a) >= val(b)`, otherwise `None`.
- B3. `a.checked_mul(b)` is `Some(c)` with `val(c) = val(a) * val(b)` exactly when that product is
  below `2^256`, otherwise `None`.
- B4. `a.checked_div(b)` and `a.checked_rem(b)` are `None` exactly when `val(b) = 0`; otherwise
  they are `Some(floor(val(a)/val(b)))` and `Some(val(a) mod val(b))`.
- B5. `a.overflowing_add(b)` returns `((val(a) + val(b)) mod 2^256, carry)` with `carry` true
  exactly when the sum is at least `2^256`. `a.overflowing_sub(b)` returns
  `((val(a) - val(b)) mod 2^256, borrow)` with `borrow` true exactly when `val(a) < val(b)`.
- B6. `a.saturating_add(b)` has value `min(val(a) + val(b), 2^256 - 1)`; `a.saturating_sub(b)` has
  value `max(val(a) - val(b), 0)`.
- B7. `a.shl(n)` has value `(val(a) * 2^n) mod 2^256` for `n < 256` and `0` for `n >= 256`.
  `a.shr(n)` has value `floor(val(a) / 2^n)` for `n < 256` and `0` for `n >= 256`. Neither panics.
- B8. `bitand` and `bitor` are the bitwise operations on the 256-bit values.
- B9. The operator impls (`+ - * / % << >> & | += -=`) return the same values as B1 to B8 when the
  checked form is `Some`, and panic exactly when the checked form is `None`.

## C. Division

- C1. `div_rem_u256(num, den)` panics exactly when `val(den) = 0`.
- C2. For `val(den) != 0`, `div_rem_u256(num, den)` returns `(q, r)` with
  `val(num) = val(q) * val(den) + val(r)` and `val(r) < val(den)`.
- C3. `ceil_div_positive_checked(n, d)` panics exactly when `val(d) = 0`, and otherwise returns a
  value equal to `ceil(val(n)/val(d))`.

## D. Wide product and multiply-divide

- D1. The private `widening_mul_u128(a, b)` returns `(lo, hi)` with `lo + 2^128*hi = a*b` for all
  inputs, and never panics.
- D2. The private `U512::mul_u256(a, b)` has `val = val(a) * val(b)` for all inputs, and never
  panics.
- D3. The private `U512::checked_div_rem_by_u256(self, den)` panics exactly when `val(den) = 0`.
  Otherwise, with `Q = floor(val(self)/val(den))` and `R = val(self) mod val(den)`, it returns
  `Some((q, r))` with `val(q) = Q` and `val(r) = R` exactly when `Q < 2^256`, and `None` exactly
  when `Q >= 2^256`.
- D4. `mul_div_floor_u256(a, b, d)` panics exactly when `val(d) = 0` or
  `floor(val(a)*val(b)/val(d)) >= 2^256`; otherwise it returns that floor.
- D5. `mul_div_floor_u256_with_rem(a, b, d)` has the panic condition of D4 and otherwise returns
  `(q, r)` with `val(a)*val(b) = val(q)*val(d) + val(r)` and `val(r) < val(d)`.
- D6. `mul_div_ceil_u256(a, b, d)` panics exactly when `val(d) = 0` or
  `ceil(val(a)*val(b)/val(d)) >= 2^256`; otherwise it returns that ceiling.
- D7. `checked_mul_div_ceil_u256(a, b, d)` never panics. It is `Some(c)` with
  `val(c) = ceil(val(a)*val(b)/val(d))` exactly when `val(d) != 0` and that ceiling is below
  `2^256`; otherwise it is `None`.
- D8. `saturating_mul_u256_u64(a, b)` has value `min(val(a)*b, 2^256 - 1)`;
  `saturating_mul_u128_u64(a, b)` returns `min(a*b, 2^128 - 1)`. Neither panics.

## E. Signed division

- E1. `floor_div_signed_conservative(n, d)` panics when `val(d) = 0`. Whenever it returns, the
  result `x` satisfies `sval(x) = floor(sval(n)/val(d))`.
- E2. For `val(d) != 0`, `floor_div_signed_conservative(n, d)` returns unless `sval(n) = -2^255` and
  `val(d) = 1`. (That one input is refused although its floor, `-2^255`, is representable. Please
  report whether this exception is exact.)
- E3. `floor_div_signed_conservative_i128(n, d)` panics when `d = 0`. Whenever it returns, the
  result equals `floor(n/d)`. For `d != 0` it returns unless `n = i128::MIN` and `d = 1`.
- E4. Let `F = floor(val(abs_basis) * sval(k_diff) / val(denominator))`.
  `wide_signed_mul_div_floor(abs_basis, k_diff, denominator)` panics when `val(denominator) = 0`.
  Whenever it returns and `F` lies in `[-2^255, 2^255)`, the result `x` satisfies `sval(x) = F`.
- E4b. (I expect this one to be false; please confirm with a counterexample or prove it.)
  `wide_signed_mul_div_floor` does not return when `F >= 2^255`. My own test:
  `wide_signed_mul_div_floor(2^255, 1, 1)` returns `I256::MIN`. The engine does not call this
  function; only the module's unit tests do.
- E5. `wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then, k_now, den)` panics when `den = 0`.
  Whenever it returns, the result equals `floor(abs_basis * (k_now - k_then) / den)`, where
  `k_now - k_then` is the exact integer difference (it may not fit in `i128`).
- E6. `I256::checked_add`, `checked_sub` and `checked_neg` return `Some` of the exact integer
  result exactly when it lies in `[-2^255, 2^255)`, otherwise `None`. `I256::checked_mul_i256`
  returns `Some` of the exact product exactly when it lies in that range, otherwise `None`.
  `I256::saturating_add` clamps the exact sum to that range.

## F. Native helpers

- F1. `mul_div_floor_u128(a, b, d)` panics exactly when `d = 0` or `a*b >= 2^128`; otherwise it
  returns `floor(a*b/d)`.
- F2. `mul_div_ceil_u128(a, b, d)` panics exactly when `d = 0` or `a*b >= 2^128`; otherwise it
  returns `ceil(a*b/d)`.
- F3. `wide_mul_div_floor_u128(a, b, d)` panics exactly when `d = 0` or `floor(a*b/d) >= 2^128`;
  otherwise it returns `floor(a*b/d)` (the product is exact, it does not have to fit in `u128`).
- F4. `wide_mul_div_ceil_u128_or_over_i128max(a, b, d)` panics exactly when `d = 0`. Otherwise it is
  `Ok(c)` with `c = ceil(a*b/d)` exactly when `c <= i128::MAX`, and `Err(OverI128Magnitude)`
  otherwise.
- F5. `fee_debt_u128_checked(x)` returns `-x` for `x < 0` (including `2^127` for `i128::MIN`) and
  `0` otherwise, and never panics.

## G. Rounding conservation (consequences the engine relies on)

For `val(d) != 0` and inputs on which the functions return:

- G1. `mul_div_ceil_u256(a, b, d) - mul_div_floor_u256(a, b, d)` is `0` when `d` divides `a*b`, and
  `1` otherwise.
- G2. `val(mul_div_floor_u256(a, b, d)) * val(d) <= val(a)*val(b) <= val(mul_div_ceil_u256(a, b, d)) * val(d)`.
- G3. Monotonicity: if `val(a) <= val(a')` then
  `val(mul_div_floor_u256(a, b, d)) <= val(mul_div_floor_u256(a', b, d))`, and the same for the
  ceiling form.
- G4. A share never exceeds its whole: if `val(b) <= val(d)` then
  `val(mul_div_floor_u256(a, b, d)) <= val(a)`.
- G5. Splitting loses less than one atom per part: for weights `w1 + w2 = W` with `W != 0`,
  `floor(X*w1/W) + floor(X*w2/W)` is `X` or `X - 1`, computed with `mul_div_floor_u256`.
