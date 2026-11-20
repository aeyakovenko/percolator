module

public import Spec.WideMathDiv

/-!
# Public contracts for the functions built on the long divisions (steps 2–4)

Properties B4, B9 (`/`, `%`), C3, D4–D7, F1–F4, E1–E5 and G1–G5 of
`aristotle/wide_math_spec.md`, about `src/wide_math.rs` compiled without `cfg(kani)`, default
features, release profile with `overflow-checks = true`. Values as in `Spec.WideMathLoopFree`
(`u256Of`, `i256Of`) and `Spec.WideMathDiv` (`u512Of`).

Every property is a partial-correctness statement:

* `S.returns P`: **no execution panics**, and `P` holds of the result if the call returns;
* `neverReturns S`: **every execution panics** (the call never returns normally);
* `ifReturns S P`: **if the call returns**, `P` holds of the result (a panic is allowed).

None of them claims termination. Each property lists, as hypotheses, the assumed documented
behaviour of the core-library functions it reaches that are not covered by proof (see
`Spec.WideMathDiv`), and takes `divGlobals S` (the program's read-only literals and constants,
placed at start-up) as precondition.

Notation. `ceilDiv p q = (p + q - 1) / q` is the mathematical ceiling of `p / q` for `q > 0`.
`Int.fdiv p q` is the floor of `p / q` (rounding toward minus infinity).
-/

@[expose] public section

namespace Spec.WideMath
open Iris Iris.BI Rust2Lean.Public

/-! ## Vocabulary -/

/-- `ceil(p / q)` for natural numbers, `q > 0`. -/
def ceilDiv (p q : Nat) : Nat := (p + q - 1) / q

/-- Call the three-argument function `f` on `a`, `b`, `c`, at the types `f` declares. -/
def rcall3 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b c : S.Value)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call f [(a, S.argTy f 0), (b, S.argTy f 1), (c, S.argTy f 2)] Φ

/-- `rcall3`, observing the panic payload stack (`before` on entry). -/
def rcallUnwind3 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b c : S.Value)
    (before : List S.Ptr) (Φ : CallOutcome S.Value → List S.Ptr → PROP) : PROP :=
  S.callUnwind f [(a, S.argTy f 0), (b, S.argTy f 1), (c, S.argTy f 2)] before Φ

/-- Call the four-argument function `f` on `a`, `b`, `c`, `d`, at the types `f` declares. -/
def rcall4 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b c d : S.Value)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call f [(a, S.argTy f 0), (b, S.argTy f 1), (c, S.argTy f 2), (d, S.argTy f 3)] Φ

/-- `rcall4`, observing the panic payload stack (`before` on entry). -/
def rcallUnwind4 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b c d : S.Value)
    (before : List S.Ptr) (Φ : CallOutcome S.Value → List S.Ptr → PROP) : PROP :=
  S.callUnwind f [(a, S.argTy f 0), (b, S.argTy f 1), (c, S.argTy f 2), (d, S.argTy f 3)]
    before Φ

/-- A postcondition for a first call followed by a second one: if the call returns `v` with
final payload stack `after`, then `K v after` (typically the second call); a panic is allowed and
constrains nothing. -/
def thenIfReturns {PROP : Type} [BI PROP] (S : Vocab PROP) (K : S.Value → List S.Ptr → PROP) :
    CallOutcome S.Value → List S.Ptr → PROP
  | .returned v, after => K v after
  | .unwound, _ => iprop(True)

/-- The assumed library behaviour reached by `div_rem_u256`: `gt` and `ge` on `U256`. -/
def DivRemLib {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  GtU256Contract S ∧ GeU256Contract S

/-- The assumed library behaviour reached by `U512::checked_div_rem_by_u256`: `gt` and `ge` on
`U256`, `Option::<U256>::expect`, and `next`/`next_back` of `Range<usize>`. -/
def U512DivLib {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  GtU256Contract S ∧ GeU256Contract S ∧ ExpectU256Contract S ∧ RangeNextContract S ∧
    RangeNextBackContract S

/-- `Ok(c)` of a `Result<u128, OverI128Magnitude>`. -/
def okU128Val {PROP : Type} [BI PROP] (S : Vocab PROP) (c : Nat) : S.Value :=
  S.variantVal 0 (S.tupleVal [S.intVal c])

/-- `Err(OverI128Magnitude)` of a `Result<u128, OverI128Magnitude>` (`OverI128Magnitude` is a
unit struct). -/
def errOverI128Val {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Value :=
  S.variantVal 1 (S.tupleVal [S.tupleVal []])

/-! ## B4 and B9 (`/`, `%`) -/

/--
B4. Assuming `DivRemLib`: `a.checked_div(b)` and `a.checked_rem(b)` do not panic; they return
`None` exactly when `val(b) = 0`, and otherwise `Some(floor(val(a)/val(b)))` and
`Some(val(a) mod val(b))`.
-/
def B4_checked_div_rem : Property := fun S =>
  -- ASSUMPTION: `gt` and `ge` on `U256` behave as documented.
  DivRemLib S →
  -- For any `U256` values x and y,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- `x.checked_div(y)` does not panic and returns `None` if y = 0, else `Some(x / y)`;
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::checked_div» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(⌊v = if y = 0 then noneVal S else someVal S (u256Of S (x / y))⌋))) ∧
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- `x.checked_rem(y)` does not panic and returns `None` if y = 0, else `Some(x % y)`.
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::checked_rem» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(⌊v = if y = 0 then noneVal S else someVal S (u256Of S (x % y))⌋)))

/--
B9 (`/` and `%`). Assuming `DivRemLib` and the documented behaviour of `Option::<U256>::expect`:
`a / b` and `a % b` return the values of B4 and do not panic when `val(b) != 0`, and panic on
every execution when `val(b) = 0` (exactly when the checked form is `None`).
-/
def B9_div_rem_operators : Property := fun S =>
  -- ASSUMPTIONS: `gt`, `ge` on `U256` and `Option::<U256>::expect` behave as documented.
  DivRemLib S → ExpectU256Contract S →
  -- `Div::div`: for any `U256` values x and y,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if y ≠ 0, `x / y` does not panic and returns x / y;
    (y ≠ 0 → divGlobals S ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Div>::div»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x / y)⌋))) ∧
    -- if y = 0, every execution of `x / y` panics.
    (y = 0 → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Div>::div»
        (u256Of S x) (u256Of S y) before (neverReturns S))) ∧
  -- `Rem::rem`: for any `U256` values x and y,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if y ≠ 0, `x % y` does not panic and returns x % y;
    (y ≠ 0 → divGlobals S ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Rem>::rem»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x % y)⌋))) ∧
    -- if y = 0, every execution of `x % y` panics.
    (y = 0 → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Rem>::rem»
        (u256Of S x) (u256Of S y) before (neverReturns S)))

/-! ## C3 -/

/--
C3. Assuming `DivRemLib` and the documented behaviour of `Option::<U256>::expect`:
`ceil_div_positive_checked(n, d)` panics on every execution when `val(d) = 0`; when
`val(d) != 0`, no execution panics and, if it returns, the result is `ceil(val(n)/val(d))`.
-/
def C3_ceil_div_positive_checked : Property := fun S =>
  -- ASSUMPTIONS: `gt`, `ge` on `U256` and `Option::<U256>::expect` behave as documented.
  DivRemLib S → ExpectU256Contract S →
  -- For any `U256` values n and d with d ≠ 0, no execution panics and the
  -- result is ceil(n / d);
  (∀ n d : Nat, n < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 → divGlobals S ⊢
    rcall2 S Mir.Fn.«percolator::wide_math::ceil_div_positive_checked» (u256Of S n) (u256Of S d)
      (S.returns fun v => iprop(⌊v = u256Of S (ceilDiv n d)⌋))) ∧
  -- with d = 0, every execution panics.
  (∀ (n : Nat) (before : List S.Ptr), n < 2 ^ 256 → divGlobals S ⊢
    rcallUnwind2 S Mir.Fn.«percolator::wide_math::ceil_div_positive_checked» (u256Of S n)
      (u256Of S 0) before (neverReturns S))

/-! ## D4–D7 -/

/--
D4. Assuming `U512DivLib`: `mul_div_floor_u256(a, b, d)` panics on every execution exactly when
`val(d) = 0` or `floor(val(a)*val(b)/val(d)) >= 2^256`; otherwise no execution panics and, if it
returns, the result is that floor.
-/
def D4_mul_div_floor_u256 : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 →
    -- if d ≠ 0 and a * b / d < 2^256, no execution panics and the result is a * b / d;
    (d ≠ 0 → a * b / d < 2 ^ 256 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d)
        (S.returns fun v => iprop(⌊v = u256Of S (a * b / d)⌋))) ∧
    -- if d = 0 or a * b / d ≥ 2^256, every execution panics.
    ((d = 0 ∨ 2 ^ 256 ≤ a * b / d) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (neverReturns S))

/--
D5. Assuming `U512DivLib`: `mul_div_floor_u256_with_rem(a, b, d)` panics on every execution
exactly when `val(d) = 0` or `floor(val(a)*val(b)/val(d)) >= 2^256`; otherwise no execution
panics and, if it returns `(q, r)`, then `val(a)*val(b) = val(q)*val(d) + val(r)` and
`val(r) < val(d)`.
-/
def D5_mul_div_floor_u256_with_rem : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 →
    -- if d ≠ 0 and a * b / d < 2^256, no execution panics and the result is
    -- a pair (q, r) with a * b = q * d + r and r < d;
    (d ≠ 0 → a * b / d < 2 ^ 256 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256_with_rem»
        (u256Of S a) (u256Of S b) (u256Of S d)
        (S.returns fun v => iprop(∃ q r : Nat, ⌊q < 2 ^ 256 ∧ r < 2 ^ 256⌋ ∗
          ⌊v = pairVal S (u256Of S q) (u256Of S r)⌋ ∗ ⌊a * b = q * d + r ∧ r < d⌋))) ∧
    -- if d = 0 or a * b / d ≥ 2^256, every execution panics.
    ((d = 0 ∨ 2 ^ 256 ≤ a * b / d) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256_with_rem»
        (u256Of S a) (u256Of S b) (u256Of S d) before (neverReturns S))

/--
D6. Assuming `U512DivLib`: `mul_div_ceil_u256(a, b, d)` panics on every execution exactly when
`val(d) = 0` or `ceil(val(a)*val(b)/val(d)) >= 2^256`; otherwise no execution panics and, if it
returns, the result is that ceiling.
-/
def D6_mul_div_ceil_u256 : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 →
    -- if d ≠ 0 and ceil(a * b / d) < 2^256, no execution panics and the result is
    -- ceil(a * b / d);
    (d ≠ 0 → ceilDiv (a * b) d < 2 ^ 256 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
        (u256Of S a) (u256Of S b) (u256Of S d)
        (S.returns fun v => iprop(⌊v = u256Of S (ceilDiv (a * b) d)⌋))) ∧
    -- if d = 0 or ceil(a * b / d) ≥ 2^256, every execution panics.
    ((d = 0 ∨ 2 ^ 256 ≤ ceilDiv (a * b) d) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (neverReturns S))

/--
D7. Assuming `U512DivLib`: `checked_mul_div_ceil_u256(a, b, d)` never panics; it returns
`Some(c)` with `val(c) = ceil(val(a)*val(b)/val(d))` exactly when `val(d) != 0` and that ceiling
is below `2^256`, and `None` otherwise.
-/
def D7_checked_mul_div_ceil_u256 : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d, no execution panics and the result is
  -- `Some(ceil(a * b / d))` if d ≠ 0 and that ceiling is below 2^256, else `None`.
  ∀ a b d : Nat, a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 → divGlobals S ⊢
    rcall3 S Mir.Fn.«percolator::wide_math::checked_mul_div_ceil_u256»
      (u256Of S a) (u256Of S b) (u256Of S d)
      (S.returns fun v => iprop(⌊v = if d ≠ 0 ∧ ceilDiv (a * b) d < 2 ^ 256 then
        someVal S (u256Of S (ceilDiv (a * b) d)) else noneVal S⌋))

/-! ## F1–F4 -/

/--
F1. Assuming the documented behaviour of `Option::<u128>::expect`: `mul_div_floor_u128(a, b, d)`
panics on every execution exactly when `d = 0` or `a*b >= 2^128`; otherwise no execution panics
and, if it returns, the result is `floor(a*b/d)`.
-/
def F1_mul_div_floor_u128 : Property := fun S =>
  -- ASSUMPTION: `Option::<u128>::expect` behaves as documented.
  ExpectU128Contract S →
  -- For any `u128` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 128 → b < 2 ^ 128 → d < 2 ^ 128 →
    -- if d ≠ 0 and a * b < 2^128, no execution panics and the result is a * b / d;
    (d ≠ 0 → a * b < 2 ^ 128 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u128»
        (S.intVal a) (S.intVal b) (S.intVal d)
        (S.returns fun v => iprop(⌊v = S.intVal (a * b / d)⌋))) ∧
    -- if d = 0 or a * b ≥ 2^128, every execution panics.
    ((d = 0 ∨ 2 ^ 128 ≤ a * b) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u128»
        (S.intVal a) (S.intVal b) (S.intVal d) before (neverReturns S))

/--
F2. Assuming the documented behaviour of `Option::<u128>::expect`: `mul_div_ceil_u128(a, b, d)`
panics on every execution exactly when `d = 0` or `a*b >= 2^128`; otherwise no execution panics
and, if it returns, the result is `ceil(a*b/d)`.
-/
def F2_mul_div_ceil_u128 : Property := fun S =>
  -- ASSUMPTION: `Option::<u128>::expect` behaves as documented.
  ExpectU128Contract S →
  -- For any `u128` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 128 → b < 2 ^ 128 → d < 2 ^ 128 →
    -- if d ≠ 0 and a * b < 2^128, no execution panics and the result is ceil(a * b / d);
    (d ≠ 0 → a * b < 2 ^ 128 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u128»
        (S.intVal a) (S.intVal b) (S.intVal d)
        (S.returns fun v => iprop(⌊v = S.intVal (ceilDiv (a * b) d)⌋))) ∧
    -- if d = 0 or a * b ≥ 2^128, every execution panics.
    ((d = 0 ∨ 2 ^ 128 ≤ a * b) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u128»
        (S.intVal a) (S.intVal b) (S.intVal d) before (neverReturns S))

/--
F3. Assuming `U512DivLib` and the documented behaviour of `Option::<u128>::expect`:
`wide_mul_div_floor_u128(a, b, d)` panics on every execution exactly when `d = 0` or
`floor(a*b/d) >= 2^128` (the product is exact); otherwise no execution panics and, if it
returns, the result is `floor(a*b/d)`.
-/
def F3_wide_mul_div_floor_u128 : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  U512DivLib S → ExpectU128Contract S →
  -- For any `u128` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 128 → b < 2 ^ 128 → d < 2 ^ 128 →
    -- if d ≠ 0 and a * b / d < 2^128, no execution panics and the result is a * b / d;
    (d ≠ 0 → a * b / d < 2 ^ 128 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::wide_mul_div_floor_u128»
        (S.intVal a) (S.intVal b) (S.intVal d)
        (S.returns fun v => iprop(⌊v = S.intVal (a * b / d)⌋))) ∧
    -- if d = 0 or a * b / d ≥ 2^128, every execution panics.
    ((d = 0 ∨ 2 ^ 128 ≤ a * b / d) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::wide_mul_div_floor_u128»
        (S.intVal a) (S.intVal b) (S.intVal d) before (neverReturns S))

/--
F4. Assuming `U512DivLib`: `wide_mul_div_ceil_u128_or_over_i128max(a, b, d)` panics on every
execution exactly when `d = 0`. Otherwise no execution panics and, if it returns, the result is
`Ok(c)` with `c = ceil(a*b/d)` when `c <= i128::MAX`, and `Err(OverI128Magnitude)` otherwise.
-/
def F4_wide_mul_div_ceil_u128_or_over_i128max : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `u128` values a, b, d,
  ∀ a b d : Nat, a < 2 ^ 128 → b < 2 ^ 128 → d < 2 ^ 128 →
    -- if d ≠ 0, no execution panics and the result is `Ok(ceil(a * b / d))` when that
    -- ceiling is at most i128::MAX = 2^127 - 1, else `Err(OverI128Magnitude)`;
    (d ≠ 0 → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::wide_mul_div_ceil_u128_or_over_i128max»
        (S.intVal a) (S.intVal b) (S.intVal d)
        (S.returns fun v => iprop(⌊v = if ceilDiv (a * b) d ≤ 2 ^ 127 - 1 then
          okU128Val S (ceilDiv (a * b) d) else errOverI128Val S⌋))) ∧
    -- if d = 0, every execution panics.
    (d = 0 → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::wide_mul_div_ceil_u128_or_over_i128max»
        (S.intVal a) (S.intVal b) (S.intVal d) before (neverReturns S))

/-! ## E1–E5 -/

/--
E1. Assuming `DivRemLib` and the documented behaviour of `Option::<U256>::expect` and
`Option::<I256>::expect`: `floor_div_signed_conservative(n, d)` panics on every execution when
`val(d) = 0`; and for `val(d) != 0`, if it returns `x`, then `sval(x) = floor(sval(n)/val(d))`.
-/
def E1_floor_div_signed_conservative : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  DivRemLib S → ExpectU256Contract S → ExpectI256Contract S →
  -- with d = 0, every execution panics;
  (∀ (s : Int) (before : List S.Ptr), InI256 s → divGlobals S ⊢
    rcallUnwind2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative»
      (i256Of S s) (u256Of S 0) before (neverReturns S)) ∧
  -- for d ≠ 0, if the call returns x, then sval(x) = floor(s / d).
  (∀ (s : Int) (d : Nat) (before : List S.Ptr), InI256 s → d < 2 ^ 256 → d ≠ 0 →
    divGlobals S ⊢
      rcallUnwind2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative»
        (i256Of S s) (u256Of S d) before
        (ifReturns S fun v => iprop(⌊v = i256Of S (Int.fdiv s d)⌋)))

/--
E2. Assuming `DivRemLib` and the documented behaviour of `Option::<U256>::expect` and
`Option::<I256>::expect`: for `val(d) != 0`, no execution of
`floor_div_signed_conservative(n, d)` panics unless `sval(n) = -2^255` and `val(d) = 1` (and the
result is `floor(sval(n)/val(d))`); for `sval(n) = -2^255` and `val(d) = 1` every execution
panics. So the exception is exact. The function calls `abs_u256` only for `sval(n) >= 0`, so the
assertion of `abs_u256` is never reached.
-/
def E2_floor_div_signed_conservative_panics : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  DivRemLib S → ExpectU256Contract S → ExpectI256Contract S →
  -- for d ≠ 0 and (s, d) ≠ (-2^255, 1), no execution panics and the result is
  -- floor(s / d);
  (∀ (s : Int) (d : Nat), InI256 s → d < 2 ^ 256 → d ≠ 0 → ¬ (s = -2 ^ 255 ∧ d = 1) →
    divGlobals S ⊢
      rcall2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative»
        (i256Of S s) (u256Of S d) (S.returns fun v => iprop(⌊v = i256Of S (Int.fdiv s d)⌋))) ∧
  -- for s = -2^255 and d = 1, every execution panics.
  (∀ before : List S.Ptr, divGlobals S ⊢
    rcallUnwind2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative»
      (i256Of S (-2 ^ 255)) (u256Of S 1) before (neverReturns S))

/--
E3. `floor_div_signed_conservative_i128(n, d)` panics on every execution when `d = 0`; for
`d != 0` no execution panics unless `n = i128::MIN` and `d = 1`, and the result is
`floor(n/d)`; for `n = i128::MIN` and `d = 1` every execution panics.
-/
def E3_floor_div_signed_conservative_i128 : Property := fun S =>
  -- For any `i128` value n and `u128` value d,
  ∀ (n : Int) (d : Nat), -2 ^ 127 ≤ n ∧ n < 2 ^ 127 → d < 2 ^ 128 →
    -- if d ≠ 0 and (n, d) ≠ (i128::MIN, 1), no execution panics and the result is
    -- floor(n / d);
    (d ≠ 0 → ¬ (n = -2 ^ 127 ∧ d = 1) → divGlobals S ⊢
      rcall2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative_i128»
        (S.intVal n) (S.intVal d) (S.returns fun v => iprop(⌊v = S.intVal (Int.fdiv n d)⌋))) ∧
    -- if d = 0, or n = i128::MIN and d = 1, every execution panics.
    ((d = 0 ∨ (n = -2 ^ 127 ∧ d = 1)) → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind2 S Mir.Fn.«percolator::wide_math::floor_div_signed_conservative_i128»
        (S.intVal n) (S.intVal d) before (neverReturns S))

/-- The exact panic condition of `wide_signed_mul_div_floor(abs_basis, k_diff, denominator)`
for `A = val(abs_basis)`, `k = sval(k_diff)`, `D = val(denominator)`: `D = 0`, or `A != 0`,
`k != 0` and one of: `k = -2^255` (the function's own `assert!(k_diff != I256::MIN)`);
`k > 0` and `floor(A*k/D) >= 2^256`; `k < 0` and, with `C = ceil(A*|k|/D)`, `C >= 2^256` or
`C = 2^255`. -/
def WsmdPanics (A : Nat) (k : Int) (D : Nat) : Prop :=
  D = 0 ∨ (A ≠ 0 ∧ k ≠ 0 ∧
    (k = -2 ^ 255 ∨
     (0 < k ∧ 2 ^ 256 ≤ A * k.natAbs / D) ∨
     (k < 0 ∧ (2 ^ 256 ≤ ceilDiv (A * k.natAbs) D ∨ ceilDiv (A * k.natAbs) D = 2 ^ 255))))

/--
E4. Assuming `U512DivLib` and the documented behaviour of `Option::<I256>::expect`: with
`F = floor(val(abs_basis) * sval(k_diff) / val(denominator))`,
`wide_signed_mul_div_floor(abs_basis, k_diff, denominator)` panics on every execution when
`val(denominator) = 0`; and for `val(denominator) != 0`, if it returns `x` and `F` lies in
`[-2^255, 2^255)`, then `sval(x) = F`.
-/
def E4_wide_signed_mul_div_floor : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  U512DivLib S → ExpectI256Contract S →
  -- with denominator = 0, every execution panics;
  (∀ (A : Nat) (k : Int) (before : List S.Ptr), A < 2 ^ 256 → InI256 k → divGlobals S ⊢
    rcallUnwind3 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor»
      (u256Of S A) (i256Of S k) (u256Of S 0) before (neverReturns S)) ∧
  -- for D ≠ 0, if the call returns x and F = floor(A * k / D) is in range, sval(x) = F.
  (∀ (A : Nat) (k : Int) (D : Nat) (before : List S.Ptr), A < 2 ^ 256 → InI256 k →
    D < 2 ^ 256 → D ≠ 0 → divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor»
        (u256Of S A) (i256Of S k) (u256Of S D) before
        (ifReturns S fun v => iprop(⌊InI256 (Int.fdiv (A * k) D) →
          v = i256Of S (Int.fdiv (A * k) D)⌋)))

/--
E4 (exact panic conditions, which the request asks for every caller of `abs_u256`). Assuming
`U512DivLib` and the documented behaviour of `Option::<I256>::expect`: every execution of
`wide_signed_mul_div_floor(abs_basis, k_diff, denominator)` panics exactly when
`WsmdPanics A k D`; otherwise no execution panics and, if it returns, the result is the
two's-complement wrap of `F = floor(A*k/D)` into `I256` (`i256Of` takes `F mod 2^256`), which
is `F` itself when `F` is in range. The function's own `assert!(k_diff != I256::MIN)` fires
before `abs_u256` is called, so the assertion of `abs_u256` is never reached.
-/
def E4_wide_signed_mul_div_floor_panics : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  U512DivLib S → ExpectI256Contract S →
  -- For any `U256` values A, D and `I256` value k,
  ∀ (A : Nat) (k : Int) (D : Nat), A < 2 ^ 256 → InI256 k → D < 2 ^ 256 →
    -- if the panic condition fails, no execution panics and the result is F (wrapped);
    (¬ WsmdPanics A k D → divGlobals S ⊢
      rcall3 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor»
        (u256Of S A) (i256Of S k) (u256Of S D)
        (S.returns fun v => iprop(⌊v = i256Of S (Int.fdiv (A * k) D)⌋))) ∧
    -- if it holds, every execution panics.
    (WsmdPanics A k D → ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor»
        (u256Of S A) (i256Of S k) (u256Of S D) before (neverReturns S))

/--
E5. Assuming `DivRemLib` and the documented behaviour of `Option::<U256>::expect`,
`Option::<u128>::expect` and `Option::<I256>::expect`:
`wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then, k_now, den)` panics on every execution
when `den = 0`; and for `den != 0`, if it returns `x`, then
`x = floor(abs_basis * (k_now - k_then) / den)` with the exact integer difference.
-/
def E5_wide_signed_mul_div_floor_from_k_pair : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  DivRemLib S → ExpectU256Contract S → ExpectU128Contract S → ExpectI256Contract S →
  -- with den = 0, every execution panics;
  (∀ (A : Nat) (k0 k1 : Int) (before : List S.Ptr), A < 2 ^ 128 →
    -2 ^ 127 ≤ k0 ∧ k0 < 2 ^ 127 → -2 ^ 127 ≤ k1 ∧ k1 < 2 ^ 127 → divGlobals S ⊢
      rcallUnwind4 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor_from_k_pair»
        (S.intVal A) (S.intVal k0) (S.intVal k1) (S.intVal 0) before (neverReturns S)) ∧
  -- for den ≠ 0, if the call returns x, then x = floor(A * (k_now - k_then) / den).
  (∀ (A : Nat) (k0 k1 : Int) (den : Nat) (before : List S.Ptr), A < 2 ^ 128 →
    -2 ^ 127 ≤ k0 ∧ k0 < 2 ^ 127 → -2 ^ 127 ≤ k1 ∧ k1 < 2 ^ 127 → den < 2 ^ 128 → den ≠ 0 →
    divGlobals S ⊢
      rcallUnwind4 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor_from_k_pair»
        (S.intVal A) (S.intVal k0) (S.intVal k1) (S.intVal den) before
        (ifReturns S fun v => iprop(⌊v = S.intVal (Int.fdiv (A * (k1 - k0)) den)⌋)))

/--
E5 (exact panic conditions, which the request asks for every caller of `abs_u256`). Assuming the
same library behaviour as E5: with `F = floor(abs_basis * (k_now - k_then) / den)`, every
execution of `wide_signed_mul_div_floor_from_k_pair(abs_basis, k_then, k_now, den)` panics
exactly when `den = 0` or `|F| >= 2^127` (so the representable result `i128::MIN` is refused);
otherwise no execution panics and, if it returns, the result is `F`. The difference
`k_now - k_then` is never `I256::MIN`, so the assertion of `abs_u256` is never reached.
-/
def E5_wide_signed_mul_div_floor_from_k_pair_panics : Property := fun S =>
  -- ASSUMPTIONS: the library functions reached behave as documented.
  DivRemLib S → ExpectU256Contract S → ExpectU128Contract S → ExpectI256Contract S →
  -- For any `u128` values A, den and `i128` values k_then = k0, k_now = k1,
  ∀ (A : Nat) (k0 k1 : Int) (den : Nat), A < 2 ^ 128 →
    -2 ^ 127 ≤ k0 ∧ k0 < 2 ^ 127 → -2 ^ 127 ≤ k1 ∧ k1 < 2 ^ 127 → den < 2 ^ 128 →
    -- if den ≠ 0 and |F| < 2^127, no execution panics and the result is F;
    (den ≠ 0 → (Int.fdiv (A * (k1 - k0)) den).natAbs < 2 ^ 127 → divGlobals S ⊢
      rcall4 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor_from_k_pair»
        (S.intVal A) (S.intVal k0) (S.intVal k1) (S.intVal den)
        (S.returns fun v => iprop(⌊v = S.intVal (Int.fdiv (A * (k1 - k0)) den)⌋))) ∧
    -- if den = 0 or |F| ≥ 2^127, every execution panics.
    ((den = 0 ∨ 2 ^ 127 ≤ (Int.fdiv (A * (k1 - k0)) den).natAbs) →
      ∀ before : List S.Ptr, divGlobals S ⊢
      rcallUnwind4 S Mir.Fn.«percolator::wide_math::wide_signed_mul_div_floor_from_k_pair»
        (S.intVal A) (S.intVal k0) (S.intVal k1) (S.intVal den) before (neverReturns S))

/-! ## G1–G5 (consequences, for `val(d) != 0` and inputs on which the calls return) -/

/--
G1. Assuming `U512DivLib`: for `val(d) != 0`, if `mul_div_floor_u256(a, b, d)` returns `x` and
then `mul_div_ceil_u256(a, b, d)` returns `y`, then `val(y) - val(x)` is `0` when `val(d)`
divides `val(a)*val(b)` and `1` otherwise.
-/
def G1_ceil_minus_floor : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d with d ≠ 0 and any panic payload stack,
  ∀ (a b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    divGlobals S ⊢
      -- if `mul_div_floor_u256(a, b, d)` returns x,
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (thenIfReturns S fun x after =>
        -- and then `mul_div_ceil_u256(a, b, d)` returns y,
        rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
          (u256Of S a) (u256Of S b) (u256Of S d) after (ifReturns S fun y => iprop(
          -- then val(y) - val(x) is 0 if d divides a * b, and 1 otherwise.
          ⌊∃ X Y : Nat, X < 2 ^ 256 ∧ Y < 2 ^ 256 ∧ x = u256Of S X ∧ y = u256Of S Y ∧
            (Y : Int) - X = if d ∣ a * b then 0 else 1⌋)))

/--
G2. Assuming `U512DivLib`: for `val(d) != 0`, if `mul_div_floor_u256(a, b, d)` returns `x` then
`val(x) * val(d) <= val(a)*val(b)`, and if `mul_div_ceil_u256(a, b, d)` returns `y` then
`val(a)*val(b) <= val(y) * val(d)`.
-/
def G2_floor_ceil_bracket : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b, d with d ≠ 0 and any panic payload stack,
  (∀ (a b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    divGlobals S ⊢
      -- if `mul_div_floor_u256(a, b, d)` returns x, then val(x) * d ≤ a * b;
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (ifReturns S fun x => iprop(
          ⌊∃ X : Nat, X < 2 ^ 256 ∧ x = u256Of S X ∧ X * d ≤ a * b⌋))) ∧
  (∀ (a b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    divGlobals S ⊢
      -- if `mul_div_ceil_u256(a, b, d)` returns y, then a * b ≤ val(y) * d.
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (ifReturns S fun y => iprop(
          ⌊∃ Y : Nat, Y < 2 ^ 256 ∧ y = u256Of S Y ∧ a * b ≤ Y * d⌋)))

/--
G3. Assuming `U512DivLib`: for `val(d) != 0` and `val(a) <= val(a')`, if
`mul_div_floor_u256(a, b, d)` returns `x` and then `mul_div_floor_u256(a', b, d)` returns `x'`,
then `val(x) <= val(x')`; and the same for `mul_div_ceil_u256`.
-/
def G3_monotone : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- floor form: for any `U256` values a ≤ a', b, d with d ≠ 0,
  (∀ (a a' b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → a' < 2 ^ 256 → b < 2 ^ 256 →
    d < 2 ^ 256 → d ≠ 0 → a ≤ a' → divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (thenIfReturns S fun x after =>
        rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
          (u256Of S a') (u256Of S b) (u256Of S d) after (ifReturns S fun x' => iprop(
          -- if both calls return, val(x) ≤ val(x').
          ⌊∃ X X' : Nat, X < 2 ^ 256 ∧ X' < 2 ^ 256 ∧ x = u256Of S X ∧ x' = u256Of S X' ∧
            X ≤ X'⌋)))) ∧
  -- ceiling form: likewise.
  (∀ (a a' b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → a' < 2 ^ 256 → b < 2 ^ 256 →
    d < 2 ^ 256 → d ≠ 0 → a ≤ a' → divGlobals S ⊢
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (thenIfReturns S fun y after =>
        rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_ceil_u256»
          (u256Of S a') (u256Of S b) (u256Of S d) after (ifReturns S fun y' => iprop(
          ⌊∃ Y Y' : Nat, Y < 2 ^ 256 ∧ Y' < 2 ^ 256 ∧ y = u256Of S Y ∧ y' = u256Of S Y' ∧
            Y ≤ Y'⌋))))

/--
G4. Assuming `U512DivLib`: for `val(d) != 0` and `val(b) <= val(d)`, if
`mul_div_floor_u256(a, b, d)` returns `x`, then `val(x) <= val(a)`.
-/
def G4_share_le_whole : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values a, b ≤ d with d ≠ 0 and any panic payload stack,
  ∀ (a b d : Nat) (before : List S.Ptr), a < 2 ^ 256 → b < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    b ≤ d → divGlobals S ⊢
      -- if `mul_div_floor_u256(a, b, d)` returns x, then val(x) ≤ a.
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S a) (u256Of S b) (u256Of S d) before (ifReturns S fun x => iprop(
          ⌊∃ X : Nat, X < 2 ^ 256 ∧ x = u256Of S X ∧ X ≤ a⌋))

/--
G5. Assuming `U512DivLib`: for weights `w1 + w2 = W` with `W != 0` and any `X`, if
`mul_div_floor_u256(X, w1, W)` returns `r1` and then `mul_div_floor_u256(X, w2, W)` returns `r2`,
then `val(r1) + val(r2)` is `X` or `X - 1`.
-/
def G5_split_loses_lt_one : Property := fun S =>
  -- ASSUMPTION: the library functions reached behave as documented.
  U512DivLib S →
  -- For any `U256` values x, w1, w2, W with w1 + w2 = W ≠ 0 and any panic payload stack,
  ∀ (x w1 w2 W : Nat) (before : List S.Ptr), x < 2 ^ 256 → w1 < 2 ^ 256 → w2 < 2 ^ 256 →
    W < 2 ^ 256 → w1 + w2 = W → W ≠ 0 → divGlobals S ⊢
      -- if `mul_div_floor_u256(x, w1, W)` returns r1,
      rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
        (u256Of S x) (u256Of S w1) (u256Of S W) before (thenIfReturns S fun r1 after =>
        -- and then `mul_div_floor_u256(x, w2, W)` returns r2,
        rcallUnwind3 S Mir.Fn.«percolator::wide_math::mul_div_floor_u256»
          (u256Of S x) (u256Of S w2) (u256Of S W) after (ifReturns S fun r2 => iprop(
          -- then val(r1) + val(r2) is x or x - 1.
          ⌊∃ R1 R2 : Nat, R1 < 2 ^ 256 ∧ R2 < 2 ^ 256 ∧ r1 = u256Of S R1 ∧ r2 = u256Of S R2 ∧
            (R1 + R2 = x ∨ R1 + R2 + 1 = x)⌋)))

end Spec.WideMath

end
