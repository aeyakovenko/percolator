module

public import Spec.Rust
public import Spec.WideMath
public import SepLogic.Public

/-!
# Public contracts for the loop-free functions of `percolator::wide_math`

Properties A1–A8, B1–B3, B5–B9 (without `/` and `%`), E6, D8 and F5 of
`aristotle/wide_math_spec.md`, about `src/wide_math.rs` compiled without `cfg(kani)`
(`U256` and `I256` stored as `[u64; 4]`), default features, release profile with
`overflow-checks = true`.

Every value is given through the mathematical value it represents:

* `u256Of S x` is the `U256` with `val = x` (for `x < 2^256`): its limbs are the base-`2^64`
  digits of `x`. Every `U256` is `u256Of S x` for exactly one such `x`.
* `i256Of S s` is the `I256` with `sval = s` (for `-2^255 ≤ s < 2^255`): the limbs of the
  `U256` of value `s mod 2^256` (two's complement). Every `I256` is `i256Of S s` for exactly
  one such `s`.

All contracts are partial-correctness statements. `S.returns` excludes unwinding: a contract
ending in it says that **no execution panics**, and what holds if the call returns normally.
`neverReturns S` excludes normal return: a contract ending in it says that **every execution
panics** (it never returns normally). Neither form claims termination.
-/

@[expose] public section

namespace Spec.WideMath
open Iris Iris.BI Rust2Lean.Public

/-! ## Vocabulary -/

/-- Call the one-argument function `f` on `a`, at the type `f` declares. -/
def rcall1 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a : S.Value)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call f [(a, S.argTy f 0)] Φ

/-- Call the two-argument function `f` on `a`, `b`, at the types `f` declares. -/
def rcall2 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b : S.Value)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call f [(a, S.argTy f 0), (b, S.argTy f 1)] Φ

/-- `rcall1`, observing the panic payload stack (`before` on entry). -/
def rcallUnwind1 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a : S.Value)
    (before : List S.Ptr) (Φ : CallOutcome S.Value → List S.Ptr → PROP) : PROP :=
  S.callUnwind f [(a, S.argTy f 0)] before Φ

/-- `rcall2`, observing the panic payload stack (`before` on entry). -/
def rcallUnwind2 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (a b : S.Value)
    (before : List S.Ptr) (Φ : CallOutcome S.Value → List S.Ptr → PROP) : PROP :=
  S.callUnwind f [(a, S.argTy f 0), (b, S.argTy f 1)] before Φ

/-- The `U256` with value `x` (`x < 2^256`): `U256([l0, l1, l2, l3])` with `l0 .. l3` the
base-`2^64` digits of `x`, least significant first. -/
def u256Of {PROP : Type} [BI PROP] (S : Vocab PROP) (x : Nat) : S.Value :=
  u256Value S (x % 2 ^ 64) (x / 2 ^ 64 % 2 ^ 64) (x / 2 ^ 128 % 2 ^ 64) (x / 2 ^ 192 % 2 ^ 64)

/-- The `I256` with two's-complement value `s` (`-2^255 ≤ s < 2^255`): the limbs of the
`U256` of value `s mod 2^256`. -/
def i256Of {PROP : Type} [BI PROP] (S : Vocab PROP) (s : Int) : S.Value :=
  u256Of S (s % 2 ^ 256).toNat

/-- `s` is in the range of `I256`, `[-2^255, 2^255)`. -/
abbrev InI256 (s : Int) : Prop := -2 ^ 255 ≤ s ∧ s < 2 ^ 255

/-- The type `U256` (the return type of `U256::new`). -/
def u256Ty {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Fn.«percolator::wide_math::U256::new»

/-- The type `I256` (the return type of `I256::from_u128`). -/
def i256Ty {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Fn.«percolator::wide_math::I256::from_u128»

/-- `Some(v)`. -/
def someVal {PROP : Type} [BI PROP] (S : Vocab PROP) (v : S.Value) : S.Value :=
  S.variantVal 1 (S.tupleVal [v])

/-- `None`. -/
def noneVal {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Value :=
  S.variantVal 0 (S.tupleVal [])

/-- The tuple `(a, b)`. -/
def pairVal {PROP : Type} [BI PROP] (S : Vocab PROP) (a b : S.Value) : S.Value :=
  S.tupleVal [a, b]

/-- The `core::cmp::Ordering` value `c` (`Less = -1`, `Equal = 0`, `Greater = 1`). -/
def orderingVal {PROP : Type} [BI PROP] (S : Vocab PROP) (c : Ordering) : S.Value :=
  S.variantVal (match c with | .lt => -1 | .eq => 0 | .gt => 1) (S.tupleVal [])

/-- A share `q` of the `U256` of value `x` at `p` (what `&U256` lends). -/
def u256At {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (q : Qp) (x : Nat) : PROP :=
  S.pointsToF p q (u256Ty S) (u256Of S x)

/-- A share `q` of the `I256` of value `s` at `p` (what `&I256` lends). -/
def i256At {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (q : Qp) (s : Int) : PROP :=
  S.pointsToF p q (i256Ty S) (i256Of S s)

/-- Bytes of a read-only global: `n` copies of `b`. -/
def bytesRep (n : Nat) (b : UInt8) : List UInt8 := List.replicate n b

/-- The read-only constant `I256::MIN` (`-2^255`), which the code reads by reference: 31 zero
bytes and `0x80` (little-endian). -/
def i256MinConst {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  S.global Mir.Global.«const [32 bytes] fa48e7fb4f2bbad8» 8 (bytesRep 31 0 ++ [128])

/-- The read-only constants `I256::ONE` (`1`) and `I256::MINUS_ONE` (`-1`), which
`checked_mul_i256` reads by reference. -/
def i256OneConsts {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(S.global Mir.Global.«const [32 bytes] 0c90692f6c7a0ddf» 8 ([1] ++ bytesRep 31 0) ∗
    S.global Mir.Global.«const [32 bytes] 52f06c581ecebcd8» 8 (bytesRep 32 255))

/-- The bytes of a string literal. -/
def strBytes (s : String) : List UInt8 := s.toUTF8.toList

/-! ## A1 `U256::new`, `lo`, `hi`, `from_u128`, `from_u64` -/

/--
A1. `U256::new(lo, hi)` has `val = lo + 2^128 * hi`; `x.lo()` and `x.hi()` return the low and
high 128 bits of `val(x)`; `U256::from_u128(v)` and `U256::from_u64(v)` have `val = v`.
No call panics.
-/
def A1_u256_representation : Property := fun S =>
  -- `U256::new`: for any `u128`s `lo` and `hi`,
  (∀ lo hi : Nat, lo < 2 ^ 128 → hi < 2 ^ 128 →
    ⊢ -- `U256::new(lo, hi)` does not panic and, if it returns,
      rcall2 S Mir.Fn.«percolator::wide_math::U256::new» (S.intVal lo) (S.intVal hi)
        (S.returns fun v =>
          -- POSTCONDITION: the result has value lo + 2^128 * hi.
          iprop(⌊v = u256Of S (lo + 2 ^ 128 * hi)⌋))) ∧
  -- `U256::lo`: for any reference `p` to a `U256` of value `x`
  -- of which the caller lends a share `q`,
  (∀ (p : S.Ptr) (q : Qp) (x : Nat), x < 2 ^ 256 →
    u256At S p q x ⊢
      -- `x.lo()` does not panic and, if it returns,
      rcall1 S Mir.Fn.«percolator::wide_math::U256::lo» (S.ptrVal p)
        (S.returns fun v => iprop(
          -- POSTCONDITIONS: the result is the low 128 bits,
          ⌊v = S.intVal ((x % 2 ^ 128 : Nat) : Int)⌋ ∗
          -- and the share comes back.
          u256At S p q x))) ∧
  -- `U256::hi`: likewise,
  (∀ (p : S.Ptr) (q : Qp) (x : Nat), x < 2 ^ 256 →
    u256At S p q x ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::U256::hi» (S.ptrVal p)
        (S.returns fun v => iprop(
          -- the result is the high 128 bits, and the share comes back.
          ⌊v = S.intVal ((x / 2 ^ 128 : Nat) : Int)⌋ ∗ u256At S p q x))) ∧
  -- `U256::from_u128`: for any `u128` `v`,
  (∀ v : Nat, v < 2 ^ 128 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::U256::from_u128» (S.intVal v)
        -- the result has value v.
        (S.returns fun r => iprop(⌊r = u256Of S v⌋))) ∧
  -- `U256::from_u64`: for any `u64` `v`,
  (∀ v : Nat, v < 2 ^ 64 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::U256::from_u64» (S.intVal v)
        -- the result has value v.
        (S.returns fun r => iprop(⌊r = u256Of S v⌋)))

/-! ## A2 `U256::is_zero` -/

/--
A2 (function part). `x.is_zero()` returns `true` exactly when `val(x) = 0`, and does not panic.
-/
def A2_u256_is_zero : Property := fun S =>
  -- For any reference `p` to a `U256` of value `x`, lent at share `q`,
  ∀ (p : S.Ptr) (q : Qp) (x : Nat), x < 2 ^ 256 →
    u256At S p q x ⊢
      -- `x.is_zero()` does not panic and, if it returns,
      rcall1 S Mir.Fn.«percolator::wide_math::U256::is_zero» (S.ptrVal p)
        (S.returns fun v => iprop(
          -- POSTCONDITIONS: the result is `val(x) == 0`,
          ⌊v = S.boolVal (decide (x = 0))⌋ ∗
          -- and the share comes back.
          u256At S p q x))

/-! ## A3 `U256::try_into_u128` -/

/--
A3. `x.try_into_u128()` is `Some(val(x))` exactly when `val(x) < 2^128`, otherwise `None`. It
does not panic.
-/
def A3_u256_try_into_u128 : Property := fun S =>
  -- For any reference `p` to a `U256` of value `x`, lent at share `q`,
  ∀ (p : S.Ptr) (q : Qp) (x : Nat), x < 2 ^ 256 →
    u256At S p q x ⊢
      -- `x.try_into_u128()` does not panic and, if it returns,
      rcall1 S Mir.Fn.«percolator::wide_math::U256::try_into_u128» (S.ptrVal p)
        (S.returns fun v => iprop(
          -- POSTCONDITIONS: the result is `Some(x)` if x < 2^128, else `None`,
          ⌊v = if x < 2 ^ 128 then someVal S (S.intVal x) else noneVal S⌋ ∗
          -- and the share comes back.
          u256At S p q x))

/-! ## A4 `PartialEq`, `Ord`, `PartialOrd` on `U256` -/

/--
A4. The derived `<U256 as PartialEq>::eq` returns `true` exactly when the values are equal;
`<U256 as Ord>::cmp` returns the order of the values and `<U256 as PartialOrd>::partial_cmp`
returns `Some` of it. No call panics.
-/
def A4_u256_eq_ord : Property := fun S =>
  -- `eq`: for any references `p`, `p'` to `U256`s of values `x`, `y`
  -- (lent at shares `q`, `q'`),
  (∀ (p p' : S.Ptr) (q q' : Qp) (x y : Nat), x < 2 ^ 256 → y < 2 ^ 256 →
    u256At S p q x ∗ u256At S p' q' y ⊢
      -- `x == y` does not panic and, if it returns,
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialEq>::eq»
        (S.ptrVal p) (S.ptrVal p')
        (S.returns fun v => iprop(
          -- the result is `x = y`, and both shares come back.
          ⌊v = S.boolVal (decide (x = y))⌋ ∗ u256At S p q x ∗ u256At S p' q' y))) ∧
  -- `cmp`: likewise,
  (∀ (p p' : S.Ptr) (q q' : Qp) (x y : Nat), x < 2 ^ 256 → y < 2 ^ 256 →
    u256At S p q x ∗ u256At S p' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::Ord>::cmp»
        (S.ptrVal p) (S.ptrVal p')
        (S.returns fun v => iprop(
          -- the result is the ordering of x and y, and both shares come back.
          ⌊v = orderingVal S (compare x y)⌋ ∗ u256At S p q x ∗ u256At S p' q' y))) ∧
  -- `partial_cmp`: likewise,
  (∀ (p p' : S.Ptr) (q q' : Qp) (x y : Nat), x < 2 ^ 256 → y < 2 ^ 256 →
    u256At S p q x ∗ u256At S p' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialOrd>::partial_cmp»
        (S.ptrVal p) (S.ptrVal p')
        (S.returns fun v => iprop(
          -- the result is `Some` of the ordering, and both shares come back.
          ⌊v = someVal S (orderingVal S (compare x y))⌋ ∗
            u256At S p q x ∗ u256At S p' q' y)))

/-! ## A5 `I256::from_i128`, `from_u128`, `try_into_i128` -/

/--
A5. `I256::from_i128(v)` and `I256::from_u128(v)` have `sval = v`; `x.try_into_i128()` is
`Some(sval(x))` exactly when `sval(x)` fits in `i128`, otherwise `None`. No call panics.
-/
def A5_i256_conversions : Property := fun S =>
  -- `from_i128`: for any `i128` `v`,
  (∀ v : Int, -2 ^ 127 ≤ v → v < 2 ^ 127 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::I256::from_i128» (S.intVal v)
        -- the result has sval = v.
        (S.returns fun r => iprop(⌊r = i256Of S v⌋))) ∧
  -- `from_u128`: for any `u128` `v`,
  (∀ v : Nat, v < 2 ^ 128 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::I256::from_u128» (S.intVal v)
        -- the result has sval = v.
        (S.returns fun r => iprop(⌊r = i256Of S v⌋))) ∧
  -- `try_into_i128`: for any reference `p` to an `I256` of value `s`, lent at share `q`,
  (∀ (p : S.Ptr) (q : Qp) (s : Int), InI256 s →
    i256At S p q s ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::try_into_i128» (S.ptrVal p)
        (S.returns fun v => iprop(
          -- the result is `Some(s)` if s fits in `i128`, else `None`,
          ⌊v = if -2 ^ 127 ≤ s ∧ s < 2 ^ 127 then someVal S (S.intVal s) else noneVal S⌋ ∗
          -- and the share comes back.
          i256At S p q s)))

/-! ## A6 `I256::is_zero`, `is_negative`, `is_positive`, `signum` -/

/--
A6 (function part). `is_zero`, `is_negative`, `is_positive` and `signum` agree with the sign of
`sval`. No call panics.
-/
def A6_i256_sign : Property := fun S =>
  -- For any reference `p` to an `I256` of value `s`, lent at share `q`:
  -- `is_zero` returns `s == 0`,
  (∀ (p : S.Ptr) (q : Qp) (s : Int), InI256 s →
    i256At S p q s ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::is_zero» (S.ptrVal p)
        (S.returns fun v => iprop(⌊v = S.boolVal (decide (s = 0))⌋ ∗ i256At S p q s))) ∧
  -- `is_negative` returns `s < 0`,
  (∀ (p : S.Ptr) (q : Qp) (s : Int), InI256 s →
    i256At S p q s ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::is_negative» (S.ptrVal p)
        (S.returns fun v => iprop(⌊v = S.boolVal (decide (s < 0))⌋ ∗ i256At S p q s))) ∧
  -- `is_positive` returns `s > 0`,
  (∀ (p : S.Ptr) (q : Qp) (s : Int), InI256 s →
    i256At S p q s ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::is_positive» (S.ptrVal p)
        (S.returns fun v => iprop(⌊v = S.boolVal (decide (0 < s))⌋ ∗ i256At S p q s))) ∧
  -- and `signum` returns the sign of s (an `i8`: -1, 0 or 1); each call
  -- does not panic and gives the share back.
  (∀ (p : S.Ptr) (q : Qp) (s : Int), InI256 s →
    i256At S p q s ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::signum» (S.ptrVal p)
        (S.returns fun v => iprop(⌊v = S.intVal (Int.sign s)⌋ ∗ i256At S p q s)))

/-! ## A7 `I256::abs_u256` -/

/-- A postcondition about returned values only: if the call returns normally with `v`, then
`Q v`; a panic (unwinding, with any final payload stack) is allowed and constrains nothing. -/
def ifReturns {PROP : Type} [BI PROP] (S : Vocab PROP) (Q : S.Value → PROP) :
    CallOutcome S.Value → List S.Ptr → PROP
  | .returned v, _ => Q v
  | .unwound, _ => iprop(True)

/--
A7 as stated in the specification, read as a property about returned values (it says nothing
about inputs on which the call panics): for **every** `x`, including `I256::MIN`, if
`x.abs_u256()` returns, the result has value `|sval(x)|`. On `I256::MIN` the call never returns
(see `A7_abs_u256_min_panics`), so for that input the property holds only vacuously; for every
other input the call does not panic (see `A7_abs_u256_not_min`).
-/
def A7_i256_abs_u256 : Property := fun S =>
  -- For any `I256` value `s` and any panic payload stack,
  ∀ s : Int, InI256 s → ∀ before : List S.Ptr,
    -- given the read-only constants the function reads,
    i256MinConst S ∗
    S.global Mir.Global.«"abs_u256 called on I256::MIN"» 1
      (strBytes "abs_u256 called on I256::MIN") ⊢
      -- if `x.abs_u256()` returns (a panic is allowed),
      rcallUnwind1 S Mir.Fn.«percolator::wide_math::I256::abs_u256» (i256Of S s) before
        -- the result has value |s|.
        (ifReturns S fun v => iprop(⌊v = u256Of S s.natAbs⌋))

/--
A7 for every `x` other than `I256::MIN`: `x.abs_u256()` does not panic, and returns the `U256`
of value `|sval(x)|`.
-/
def A7_abs_u256_not_min : Property := fun S =>
  -- For any `I256` value `s` other than `-2^255`,
  ∀ s : Int, InI256 s → s ≠ -2 ^ 255 →
    -- given the read-only constant `I256::MIN` (compared against by reference),
    i256MinConst S ⊢
      -- `x.abs_u256()` does not panic and, if it returns,
      rcall1 S Mir.Fn.«percolator::wide_math::I256::abs_u256» (i256Of S s)
        -- POSTCONDITION: the result has value |s|.
        (S.returns fun v => iprop(⌊v = u256Of S s.natAbs⌋))

/--
`I256::MIN.abs_u256()` panics on every execution (the `assert!` in its body fails): it never
returns normally, so in particular it never returns `2^255`.
-/
def A7_abs_u256_min_panics : Property := fun S =>
  -- For any panic payload stack,
  ∀ before : List S.Ptr,
    -- given the read-only constants the function reads (`I256::MIN` and
    -- the assertion message),
    i256MinConst S ∗
    S.global Mir.Global.«"abs_u256 called on I256::MIN"» 1
      (strBytes "abs_u256 called on I256::MIN") ⊢
      -- `I256::MIN.abs_u256()` never returns normally.
      rcallUnwind1 S Mir.Fn.«percolator::wide_math::I256::abs_u256» (i256Of S (-2 ^ 255))
        before (neverReturns S)

/-! ## A8 `I256::from_raw_u256`, `from_u256_or_overflow` -/

/--
A8. `I256::from_raw_u256(v)` reinterprets the limbs: its `sval` is `val(v)` if
`val(v) < 2^255`, else `val(v) - 2^256`. `I256::from_u256_or_overflow(v)` is `Some` of that
exactly when `val(v) < 2^255`, otherwise `None`. No call panics.
-/
def A8_i256_from_raw : Property := fun S =>
  -- `from_raw_u256`: for any `U256` value `x`,
  (∀ x : Nat, x < 2 ^ 256 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::I256::from_raw_u256» (u256Of S x)
        (S.returns fun v => iprop(
          -- the result has sval = x if x < 2^255, else x - 2^256.
          ⌊v = i256Of S (if x < 2 ^ 255 then (x : Int) else (x : Int) - 2 ^ 256)⌋))) ∧
  -- `from_u256_or_overflow`: for any `U256` value `x`,
  (∀ x : Nat, x < 2 ^ 256 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::I256::from_u256_or_overflow» (u256Of S x)
        (S.returns fun v => iprop(
          -- the result is `Some` of the `I256` with sval = x if x < 2^255, else `None`.
          ⌊v = if x < 2 ^ 255 then someVal S (i256Of S x) else noneVal S⌋)))

/-! ## B1–B3 checked arithmetic -/

/--
B1. `a.checked_add(b)` is `Some(c)` with `val(c) = val(a) + val(b)` exactly when that sum is
below `2^256`, otherwise `None`. It does not panic.
-/
def B1_checked_add : Property := fun S =>
  -- For any `U256` values `x` and `y`,
  ∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ -- `a.checked_add(b)` does not panic and, if it returns,
      rcall2 S Mir.Fn.«percolator::wide_math::U256::checked_add» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(
          -- the result is `Some(x + y)` if x + y < 2^256, else `None`.
          ⌊v = if x + y < 2 ^ 256 then someVal S (u256Of S (x + y)) else noneVal S⌋))

/--
B2. `a.checked_sub(b)` is `Some(c)` with `val(c) = val(a) - val(b)` exactly when
`val(a) >= val(b)`, otherwise `None`. It does not panic.
-/
def B2_checked_sub : Property := fun S =>
  -- For any `U256` values `x` and `y`,
  ∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ -- `a.checked_sub(b)` does not panic and, if it returns,
      rcall2 S Mir.Fn.«percolator::wide_math::U256::checked_sub» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(
          -- the result is `Some(x - y)` if y ≤ x, else `None`.
          ⌊v = if y ≤ x then someVal S (u256Of S (x - y)) else noneVal S⌋))

/--
B3. `a.checked_mul(b)` is `Some(c)` with `val(c) = val(a) * val(b)` exactly when that product is
below `2^256`, otherwise `None`. It does not panic.
-/
def B3_checked_mul : Property := fun S =>
  -- For any `U256` values `x` and `y`,
  ∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ -- `a.checked_mul(b)` does not panic and, if it returns,
      rcall2 S Mir.Fn.«percolator::wide_math::U256::checked_mul» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(
          -- the result is `Some(x * y)` if x * y < 2^256, else `None`.
          ⌊v = if x * y < 2 ^ 256 then someVal S (u256Of S (x * y)) else noneVal S⌋))

/-! ## B5 overflowing arithmetic -/

/--
B5. `a.overflowing_add(b)` returns `((val(a) + val(b)) mod 2^256, carry)` with `carry` true
exactly when the sum is at least `2^256`. `a.overflowing_sub(b)` returns
`((val(a) - val(b)) mod 2^256, borrow)` with `borrow` true exactly when `val(a) < val(b)`.
Neither panics.
-/
def B5_overflowing : Property := fun S =>
  -- `overflowing_add`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::overflowing_add» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(
          -- the result is ((x + y) mod 2^256, x + y ≥ 2^256).
          ⌊v = pairVal S (u256Of S ((x + y) % 2 ^ 256)) (S.boolVal (decide (2 ^ 256 ≤ x + y)))⌋))) ∧
  -- `overflowing_sub`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::overflowing_sub» (u256Of S x) (u256Of S y)
        (S.returns fun v => iprop(
          -- the result is ((x - y) mod 2^256, x < y), with x - y the integer difference.
          ⌊v = pairVal S (u256Of S (((x : Int) - y) % 2 ^ 256).toNat)
            (S.boolVal (decide (x < y)))⌋)))

/-! ## B6 saturating arithmetic -/

/--
B6. `a.saturating_add(b)` has value `min(val(a) + val(b), 2^256 - 1)`; `a.saturating_sub(b)` has
value `max(val(a) - val(b), 0)`. Neither panics.
-/
def B6_saturating : Property := fun S =>
  -- `saturating_add`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::saturating_add» (u256Of S x) (u256Of S y)
        -- the result has value min(x + y, 2^256 - 1).
        (S.returns fun v => iprop(⌊v = u256Of S (min (x + y) (2 ^ 256 - 1))⌋))) ∧
  -- `saturating_sub`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::saturating_sub» (u256Of S x) (u256Of S y)
        -- the result has value max(x - y, 0) (`Nat` subtraction truncates at 0).
        (S.returns fun v => iprop(⌊v = u256Of S (x - y)⌋)))

/-! ## B7 shifts -/

/--
B7. `a.shl(n)` has value `(val(a) * 2^n) mod 2^256` for `n < 256` and `0` for `n >= 256`;
`a.shr(n)` has value `floor(val(a) / 2^n)` for `n < 256` and `0` for `n >= 256`. Neither panics,
for every `u32` shift amount `n`.
-/
def B7_shifts : Property := fun S =>
  -- `shl`: for any `U256` value `x` and any `u32` `n`,
  (∀ (x : Nat) (n : Nat), x < 2 ^ 256 → n < 2 ^ 32 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::shl» (u256Of S x) (S.intVal n)
        (S.returns fun v => iprop(
          -- the result has value (x * 2^n) mod 2^256 if n < 256, else 0.
          ⌊v = u256Of S (if n < 256 then x * 2 ^ n % 2 ^ 256 else 0)⌋))) ∧
  -- `shr`: for any `U256` value `x` and any `u32` `n`,
  (∀ (x : Nat) (n : Nat), x < 2 ^ 256 → n < 2 ^ 32 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::shr» (u256Of S x) (S.intVal n)
        (S.returns fun v => iprop(
          -- the result has value floor(x / 2^n) if n < 256, else 0.
          ⌊v = u256Of S (if n < 256 then x / 2 ^ n else 0)⌋)))

/-! ## B8 bitwise operations -/

/--
B8. `a.bitand(b)` and `a.bitor(b)` are the bitwise and/or of the 256-bit values. Neither
panics.
-/
def B8_bitwise : Property := fun S =>
  -- `bitand`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::bitand» (u256Of S x) (u256Of S y)
        -- the result has value x AND y (bitwise).
        (S.returns fun v => iprop(⌊v = u256Of S (x &&& y)⌋))) ∧
  -- `bitor`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::U256::bitor» (u256Of S x) (u256Of S y)
        -- the result has value x OR y (bitwise).
        (S.returns fun v => iprop(⌊v = u256Of S (x ||| y)⌋)))

/-! ## B9 operator impls (`+ - * << >> & | += -=`) -/

/-- A share `q` of the bytes `bs` of a string at `m` (what a `&str` lends): `bs.length`
consecutive `u8` cells. -/
def strAt {PROP : Type} [BI PROP] (S : Vocab PROP) (m : S.Ptr) (q : Qp) (bs : List UInt8) :
    PROP :=
  S.pointsToF m q (S.arrayTy (S.intTy false 1) bs.length)
    (S.tupleVal (bs.map fun b => S.intVal b.toNat))

/--
**Assumed library behaviour** of `core::option::Option::<U256>::expect` (its documented
contract): on `Some(v)` it returns `v` without panicking; on `None` it panics (never returns
normally). The message is an ASCII `&str` of which the caller lends a share.
-/
def ExpectU256Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  -- On `Some(x)` with an ASCII message `bs` at `m` (share `q`),
  (∀ (x : Nat) (m : S.Ptr) (q : Qp) (bs : List UInt8), x < 2 ^ 256 →
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcall2 S Mir.Fn.«core::option::Option::<percolator::wide_math::U256>::expect»
        (someVal S (u256Of S x)) (S.slicePtrVal m bs.length)
        -- `expect` does not panic, returns x and gives the share back;
        (S.returns fun v => iprop(⌊v = u256Of S x⌋ ∗ strAt S m q bs))) ∧
  -- on `None`, it never returns normally.
  (∀ (m : S.Ptr) (q : Qp) (bs : List UInt8) (before : List S.Ptr),
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcallUnwind2 S Mir.Fn.«core::option::Option::<percolator::wide_math::U256>::expect»
        (noneVal S) (S.slicePtrVal m bs.length) before (neverReturns S))

/-- The panic-message literals of the operator impls `+`, `-` and `*` of `U256`, in read-only
memory. -/
def opMessages {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(S.global Mir.Global.«"U256 add overflow"» 1 (strBytes "U256 add overflow") ∗
    S.global Mir.Global.«"U256 sub underflow"» 1 (strBytes "U256 sub underflow") ∗
    S.global Mir.Global.«"U256 mul overflow"» 1 (strBytes "U256 mul overflow"))

/--
B9 (`+ - * << >> & | += -=`). Assuming the documented behaviour of `Option::<U256>::expect`
(`ExpectU256Contract`): `a + b`, `a - b` and `a * b` return the value of B1/B2/B3 and do not
panic when the checked form is `Some`, and panic on every execution when it is `None`;
`a << n`, `a >> n`, `a & b` and `a | b` return the values of B7/B8 and do not panic;
`a += b` and `a -= b` store the value of B1/B2 into `a` and do not panic when the checked form
is `Some`, and panic on every execution when it is `None`.
-/
def B9_operators : Property := fun S =>
  -- ASSUMPTION: `Option::<U256>::expect` behaves as documented.
  ExpectU256Contract S →
  -- `Add::add`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if x + y < 2^256, `a + b` does not panic and returns x + y;
    (x + y < 2 ^ 256 → opMessages S ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Add>::add»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x + y)⌋))) ∧
    -- otherwise every execution of `a + b` panics.
    (2 ^ 256 ≤ x + y → ∀ before : List S.Ptr, opMessages S ⊢
      rcallUnwind2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Add>::add»
        (u256Of S x) (u256Of S y) before (neverReturns S))) ∧
  -- `Sub::sub`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if y ≤ x, `a - b` does not panic and returns x - y;
    (y ≤ x → opMessages S ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Sub>::sub»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x - y)⌋))) ∧
    -- otherwise every execution of `a - b` panics.
    (x < y → ∀ before : List S.Ptr, opMessages S ⊢
      rcallUnwind2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Sub>::sub»
        (u256Of S x) (u256Of S y) before (neverReturns S))) ∧
  -- `Mul::mul`: for any `U256` values `x` and `y`,
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if x * y < 2^256, `a * b` does not panic and returns x * y;
    (x * y < 2 ^ 256 → opMessages S ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Mul>::mul»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x * y)⌋))) ∧
    -- otherwise every execution of `a * b` panics.
    (2 ^ 256 ≤ x * y → ∀ before : List S.Ptr, opMessages S ⊢
      rcallUnwind2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::Mul>::mul»
        (u256Of S x) (u256Of S y) before (neverReturns S))) ∧
  -- `Shl<u32>::shl`: for any `U256` value `x` and `u32` `n`, `a << n` does
  -- not panic and returns the value of B7;
  (∀ (x n : Nat), x < 2 ^ 256 → n < 2 ^ 32 →
    ⊢ rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::bit::Shl<u32>>::shl»
        (u256Of S x) (S.intVal n)
        (S.returns fun v => iprop(⌊v = u256Of S (if n < 256 then x * 2 ^ n % 2 ^ 256 else 0)⌋))) ∧
  -- `Shr<u32>::shr`: likewise `a >> n`;
  (∀ (x n : Nat), x < 2 ^ 256 → n < 2 ^ 32 →
    ⊢ rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::bit::Shr<u32>>::shr»
        (u256Of S x) (S.intVal n)
        (S.returns fun v => iprop(⌊v = u256Of S (if n < 256 then x / 2 ^ n else 0)⌋))) ∧
  -- `BitAnd::bitand`: `a & b` does not panic and returns x AND y;
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::bit::BitAnd>::bitand»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x &&& y)⌋))) ∧
  -- `BitOr::bitor`: `a | b` does not panic and returns x OR y;
  (∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    ⊢ rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::bit::BitOr>::bitor»
        (u256Of S x) (u256Of S y) (S.returns fun v => iprop(⌊v = u256Of S (x ||| y)⌋))) ∧
  -- `AddAssign::add_assign`: for any reference `p` to a `U256` of value `x`
  -- that the caller owns, and any `U256` value `y`,
  (∀ (p : S.Ptr) (x y : Nat), x < 2 ^ 256 → y < 2 ^ 256 →
    -- if x + y < 2^256, `a += b` does not panic and, if it returns, the caller
    -- owns `a` again, now of value x + y;
    (x + y < 2 ^ 256 → opMessages S ∗ S.pointsTo p (u256Ty S) (u256Of S x) ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::AddAssign>::add_assign»
        (S.ptrVal p) (u256Of S y)
        (S.returns fun _ => iprop(S.pointsTo p (u256Ty S) (u256Of S (x + y))))) ∧
    -- otherwise every execution of `a += b` panics.
    (2 ^ 256 ≤ x + y → ∀ before : List S.Ptr,
      opMessages S ∗ S.pointsTo p (u256Ty S) (u256Of S x) ⊢
      rcallUnwind2 S
        Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::AddAssign>::add_assign»
        (S.ptrVal p) (u256Of S y) before (neverReturns S))) ∧
  -- `SubAssign::sub_assign`: likewise,
  (∀ (p : S.Ptr) (x y : Nat), x < 2 ^ 256 → y < 2 ^ 256 →
    -- if y ≤ x, `a -= b` does not panic and leaves `a` of value x - y;
    (y ≤ x → opMessages S ∗ S.pointsTo p (u256Ty S) (u256Of S x) ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::SubAssign>::sub_assign»
        (S.ptrVal p) (u256Of S y)
        (S.returns fun _ => iprop(S.pointsTo p (u256Ty S) (u256Of S (x - y))))) ∧
    -- otherwise every execution of `a -= b` panics.
    (x < y → ∀ before : List S.Ptr,
      opMessages S ∗ S.pointsTo p (u256Ty S) (u256Of S x) ⊢
      rcallUnwind2 S
        Mir.Fn.«<percolator::wide_math::U256 as core::ops::arith::SubAssign>::sub_assign»
        (S.ptrVal p) (u256Of S y) before (neverReturns S)))

/-! ## E6 `I256::checked_add`, `checked_sub`, `checked_neg`, `checked_mul_i256`,
`saturating_add` -/

/-- `Some` of the `I256` of value `r` when `r` is in range, else `None`. -/
def checkedI256 {PROP : Type} [BI PROP] (S : Vocab PROP) (r : Int) : S.Value :=
  if InI256 r then someVal S (i256Of S r) else noneVal S

/-- `r` clamped to `[-2^255, 2^255 - 1]`. -/
def clampI256 (r : Int) : Int := max (-2 ^ 255) (min r (2 ^ 255 - 1))

/--
**Assumed library behaviour** of core's provided method `PartialOrd::gt`, instantiated for
`U256` (its documented contract): `a > b` returns whether `a.partial_cmp(b)` is
`Some(Greater)`. Formally: for `U256` values `x`, `y`, whenever `partial_cmp` on references to
them returns `Some(compare x y)` (giving the shares back), `gt` on references to them returns
`compare x y == Greater` without panicking and gives the shares back.
-/
def GtU256Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if `partial_cmp` returns `Some(compare x y)` on any references to x and y,
    (∀ (r r' : S.Ptr) (q q' : Qp), u256At S r q x ∗ u256At S r' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialOrd>::partial_cmp»
        (S.ptrVal r) (S.ptrVal r')
        (S.returns fun v => iprop(⌊v = someVal S (orderingVal S (compare x y))⌋ ∗
          u256At S r q x ∗ u256At S r' q' y))) →
    -- then `gt` on references to x and y returns `compare x y == Greater`.
    (∀ (p p' : S.Ptr) (q q' : Qp), u256At S p q x ∗ u256At S p' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialOrd>::gt»
        (S.ptrVal p) (S.ptrVal p')
        (S.returns fun v => iprop(⌊v = S.boolVal (compare x y == .gt)⌋ ∗
          u256At S p q x ∗ u256At S p' q' y)))

/--
E6. `I256::checked_add`, `checked_sub` and `checked_neg` return `Some` of the exact integer
result exactly when it lies in `[-2^255, 2^255)`, otherwise `None`; `I256::checked_mul_i256`
returns `Some` of the exact product exactly when it lies in that range, otherwise `None`;
`I256::saturating_add` clamps the exact sum to that range. No call panics. The result for
`checked_mul_i256` assumes the documented behaviour of `PartialOrd::gt` on `U256`
(`GtU256Contract`).
-/
def E6_i256_checked : Property := fun S =>
  -- `checked_add`: for any `I256` values `s`, `t`,
  (∀ s t : Int, InI256 s → InI256 t →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::I256::checked_add» (i256Of S s) (i256Of S t)
        -- the result is `Some(s + t)` if in range, else `None`.
        (S.returns fun v => iprop(⌊v = checkedI256 S (s + t)⌋))) ∧
  -- `checked_sub`: for any `I256` values `s`, `t`, given the read-only
  -- constant `I256::MIN`,
  (∀ s t : Int, InI256 s → InI256 t →
    i256MinConst S ⊢
      rcall2 S Mir.Fn.«percolator::wide_math::I256::checked_sub» (i256Of S s) (i256Of S t)
        -- the result is `Some(s - t)` if in range, else `None`.
        (S.returns fun v => iprop(⌊v = checkedI256 S (s - t)⌋))) ∧
  -- `checked_neg`: for any `I256` value `s`, given `I256::MIN`,
  (∀ s : Int, InI256 s →
    i256MinConst S ⊢
      rcall1 S Mir.Fn.«percolator::wide_math::I256::checked_neg» (i256Of S s)
        -- the result is `Some(-s)` if in range, else `None`.
        (S.returns fun v => iprop(⌊v = checkedI256 S (-s)⌋))) ∧
  -- `checked_mul_i256`: ASSUMING `PartialOrd::gt` on `U256` behaves as
  -- documented, for any `I256` values `s`, `t`, given the read-only constants
  -- `I256::MIN`, `I256::ONE`, `I256::MINUS_ONE`,
  (GtU256Contract S → ∀ s t : Int, InI256 s → InI256 t →
    i256MinConst S ∗ i256OneConsts S ⊢
      rcall2 S Mir.Fn.«percolator::wide_math::I256::checked_mul_i256» (i256Of S s) (i256Of S t)
        -- the result is `Some(s * t)` if in range, else `None`.
        (S.returns fun v => iprop(⌊v = checkedI256 S (s * t)⌋))) ∧
  -- `saturating_add`: for any `I256` values `s`, `t`,
  (∀ s t : Int, InI256 s → InI256 t →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::I256::saturating_add» (i256Of S s) (i256Of S t)
        -- the result is s + t clamped to [-2^255, 2^255 - 1].
        (S.returns fun v => iprop(⌊v = i256Of S (clampI256 (s + t))⌋)))

/-! ## D8 saturating multiplications -/

/--
D8. `saturating_mul_u256_u64(a, b)` has value `min(val(a) * b, 2^256 - 1)`;
`saturating_mul_u128_u64(a, b)` returns `min(a * b, 2^128 - 1)`. Neither panics.
-/
def D8_saturating_mul : Property := fun S =>
  -- `saturating_mul_u256_u64`: for any `U256` value `x` and `u64` `b`,
  (∀ x b : Nat, x < 2 ^ 256 → b < 2 ^ 64 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::saturating_mul_u256_u64» (u256Of S x) (S.intVal b)
        -- the result has value min(x * b, 2^256 - 1).
        (S.returns fun v => iprop(⌊v = u256Of S (min (x * b) (2 ^ 256 - 1))⌋))) ∧
  -- `saturating_mul_u128_u64`: for any `u128` `a` and `u64` `b`,
  (∀ a b : Nat, a < 2 ^ 128 → b < 2 ^ 64 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::saturating_mul_u128_u64» (S.intVal a) (S.intVal b)
        -- the result is min(a * b, 2^128 - 1).
        (S.returns fun v => iprop(⌊v = S.intVal ((min (a * b) (2 ^ 128 - 1) : Nat) : Int)⌋)))

/-! ## F5 `fee_debt_u128_checked` -/

/--
F5. `fee_debt_u128_checked(x)` returns `-x` for `x < 0` (including `2^127` for `i128::MIN`)
and `0` otherwise, and does not panic.
-/
def F5_fee_debt : Property := fun S =>
  -- For any `i128` `x`,
  ∀ x : Int, -2 ^ 127 ≤ x → x < 2 ^ 127 →
    ⊢ rcall1 S Mir.Fn.«percolator::wide_math::fee_debt_u128_checked» (S.intVal x)
        -- the result is -x if x < 0, else 0.
        (S.returns fun v => iprop(⌊v = S.intVal (if x < 0 then -x else 0)⌋))

end Spec.WideMath

end
