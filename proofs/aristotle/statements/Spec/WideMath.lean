module

public import Spec.Rust
public import Spec.WideMathModel
public import SepLogic.Public

/-!
# Public contracts for `percolator::wide_math` (non-`kani` build)

Contracts for the properties C1, C2, D1, D2 and D3 of `aristotle/wide_math_spec.md`, about
`div_rem_u256`, the private `widening_mul_u128`, and the private `U512::mul_u256` and
`U512::checked_div_rem_by_u256`, stated over the public vocabulary and the value functions of
`Spec.WideMathModel`.

All contracts describe partial correctness: they say what holds if a call returns normally and
whether it may unwind (panic). They do not say that a call terminates.
-/

@[expose] public section

namespace Spec.WideMath
open Iris Iris.BI Rust2Lean.Public

/-- The value of a `U256` whose limbs, least significant first, are `l0 l1 l2 l3`: the struct
`U256([l0, l1, l2, l3])`. -/
def u256Value {PROP : Type} [BI PROP] (S : Vocab PROP) (l0 l1 l2 l3 : Nat) : S.Value :=
  S.tupleVal [S.tupleVal [S.intVal l0, S.intVal l1, S.intVal l2, S.intVal l3]]

/-- The value of a private `U512` whose `u128` words, least significant first, are
`w0 w1 w2 w3`: the struct `U512([w0, w1, w2, w3])`. -/
def u512Value {PROP : Type} [BI PROP] (S : Vocab PROP) (w0 w1 w2 w3 : Nat) : S.Value :=
  S.tupleVal [S.tupleVal [S.intVal w0, S.intVal w1, S.intVal w2, S.intVal w3]]

/-- Four `u64` limbs. -/
def Limbs (l0 l1 l2 l3 : Nat) : Prop :=
  l0 < 2 ^ 64 ∧ l1 < 2 ^ 64 ∧ l2 < 2 ^ 64 ∧ l3 < 2 ^ 64

/-- Four `u128` words. -/
def Words (w0 w1 w2 w3 : Nat) : Prop :=
  w0 < 2 ^ 128 ∧ w1 < 2 ^ 128 ∧ w2 < 2 ^ 128 ∧ w3 < 2 ^ 128

/-- The bytes of the string literal `"U256 division by zero"`. -/
def msgU256DivZero : List UInt8 := "U256 division by zero".toUTF8.toList

/-- The bytes of the string literal `"U512 division by zero"`. -/
def msgU512DivZero : List UInt8 := "U512 division by zero".toUTF8.toList

/-- The bytes of the string literal `"remainder must fit U256"`. -/
def msgRemainderFit : List UInt8 := "remainder must fit U256".toUTF8.toList

/-- The program's read-only constants read by `U512::checked_div_rem_by_u256` on its
non-panicking paths: the promoted constant `Ordering::Less` (one byte, `0xFF`) and the
`expect` message literal `"remainder must fit U256"`. -/
def checkedDivRemConstants {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(S.global Mir.Global.«const [1 bytes] 165fc9f0fda32787» 1 [255] ∗
    S.global Mir.Global.«"remainder must fit U256"» 1 msgRemainderFit)

/-- A postcondition allowing only unwinding: the call never returns normally. -/
def neverReturns {PROP : Type} [BI PROP] (S : Vocab PROP) :
    CallOutcome S.Value → List S.Ptr → PROP
  | .returned _, _ => iprop(False)
  | .unwound, _ => iprop(True)

/-! ## C1 and C2: `div_rem_u256` -/

/--
`div_rem_u256(num, den)` with `val(den) = 0` never returns normally.
-/
def div_rem_u256_zero_spec : Property := fun S =>
  -- For any numerator limbs and any panic payload stack,
  ∀ (n0 n1 n2 n3 : Nat) (before : List S.Ptr),
    -- PRECONDITIONS
    -- if the numerator limbs are `u64`s,
    Limbs n0 n1 n2 n3 →
    -- and the panic message literal is in read-only memory,
    S.global Mir.Global.«"U256 division by zero"» 1 msgU256DivZero ⊢
      -- then `div_rem_u256(num, U256::ZERO)` never returns
      -- normally (it can only unwind).
      S.callUnwind Mir.Fn.«percolator::wide_math::div_rem_u256»
        [(u256Value S n0 n1 n2 n3,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 0),
         (u256Value S 0 0 0 0,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 1)]
        before (neverReturns S)

/--
`div_rem_u256(num, den)` with `val(den) ≠ 0` does not unwind, and on normal return gives
`(q, r)` with `val(num) = val(q) * val(den) + val(r)` and `val(r) < val(den)`.
-/
def div_rem_u256_nonzero_spec : Property := fun S =>
  -- For any numerator and divisor limbs,
  ∀ (n0 n1 n2 n3 d0 d1 d2 d3 : Nat),
    -- PRECONDITIONS
    -- if all limbs are `u64`s
    Limbs n0 n1 n2 n3 → Limbs d0 d1 d2 d3 →
    -- and the divisor is nonzero,
    u256Val d0 d1 d2 d3 ≠ 0 →
    ⊢ -- then `div_rem_u256(num, den)` does not unwind;
      -- on normal return,
      S.call Mir.Fn.«percolator::wide_math::div_rem_u256»
        [(u256Value S n0 n1 n2 n3,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 0),
         (u256Value S d0 d1 d2 d3,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 1)]
        (S.returns fun v => iprop(
          -- POSTCONDITIONS
          -- there are limbs of `q` and `r`
          ∃ q0 q1 q2 q3 r0 r1 r2 r3 : Nat,
            -- which are `u64`s,
            ⌊Limbs q0 q1 q2 q3 ∧ Limbs r0 r1 r2 r3⌋ ∗
            -- the returned value is the pair `(q, r)`,
            ⌊v = S.tupleVal [u256Value S q0 q1 q2 q3, u256Value S r0 r1 r2 r3]⌋ ∗
            -- and num = q * den + r with r < den.
            ⌊DivRemResult (u256Val n0 n1 n2 n3) (u256Val d0 d1 d2 d3)
              (u256Val q0 q1 q2 q3) (u256Val r0 r1 r2 r3)⌋))

/-- C1 in partial-correctness form: `div_rem_u256` never returns normally when
`val(den) = 0`, and never unwinds when `val(den) ≠ 0`. -/
def div_rem_u256_panics_iff_zero : Property := fun S =>
  div_rem_u256_zero_spec S ∧
  -- For any numerator and divisor limbs,
  ∀ (n0 n1 n2 n3 d0 d1 d2 d3 : Nat),
    -- PRECONDITIONS
    -- if all limbs are `u64`s and the divisor is nonzero,
    Limbs n0 n1 n2 n3 → Limbs d0 d1 d2 d3 → u256Val d0 d1 d2 d3 ≠ 0 →
    -- then `div_rem_u256(num, den)` does not unwind.
    ⊢ S.call Mir.Fn.«percolator::wide_math::div_rem_u256»
        [(u256Value S n0 n1 n2 n3,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 0),
         (u256Value S d0 d1 d2 d3,
            S.argTy Mir.Fn.«percolator::wide_math::div_rem_u256» 1)]
        (S.returns fun _ => iprop(True))

/-! ## D1: `widening_mul_u128` -/

/--
The private `widening_mul_u128(a, b)` does not unwind, and on normal return gives `(lo, hi)`
with `lo + 2^128 * hi = a * b`.
-/
def widening_mul_u128_spec : Property := fun S =>
  -- For any `a` and `b`,
  ∀ (a b : Nat),
    -- PRECONDITIONS
    -- if both are `u128`s,
    a < 2 ^ 128 → b < 2 ^ 128 →
    ⊢ -- then `widening_mul_u128(a, b)` does not unwind;
      -- on normal return,
      S.call Mir.Fn.«percolator::wide_math::widening_mul_u128»
        [(S.intVal a, S.argTy Mir.Fn.«percolator::wide_math::widening_mul_u128» 0),
         (S.intVal b, S.argTy Mir.Fn.«percolator::wide_math::widening_mul_u128» 1)]
        (S.returns fun v => iprop(
          -- POSTCONDITIONS
          -- there are `u128`s `lo` and `hi`
          ∃ lo hi : Nat, ⌊lo < 2 ^ 128 ∧ hi < 2 ^ 128⌋ ∗
            -- such that the returned value is `(lo, hi)`
            ⌊v = S.tupleVal [S.intVal lo, S.intVal hi]⌋ ∗
            -- and lo + 2^128 * hi = a * b.
            ⌊WideningMulResult a b lo hi⌋))

/-! ## D2: `U512::mul_u256` -/

/--
The private `U512::mul_u256(a, b)` does not unwind, and on normal return gives a `U512` whose
value is `val(a) * val(b)`.
-/
def mul_u256_spec : Property := fun S =>
  -- For any limbs of `a` and `b`,
  ∀ (a0 a1 a2 a3 b0 b1 b2 b3 : Nat),
    -- PRECONDITIONS
    -- if all limbs are `u64`s,
    Limbs a0 a1 a2 a3 → Limbs b0 b1 b2 b3 →
    ⊢ -- then `U512::mul_u256(a, b)` does not unwind;
      -- on normal return,
      S.call Mir.Fn.«percolator::wide_math::U512::mul_u256»
        [(u256Value S a0 a1 a2 a3, S.argTy Mir.Fn.«percolator::wide_math::U512::mul_u256» 0),
         (u256Value S b0 b1 b2 b3, S.argTy Mir.Fn.«percolator::wide_math::U512::mul_u256» 1)]
        (S.returns fun v => iprop(
          -- POSTCONDITIONS
          -- there are `u128` words of the result
          ∃ w0 w1 w2 w3 : Nat, ⌊Words w0 w1 w2 w3⌋ ∗
            -- such that the returned value is that `U512`
            ⌊v = u512Value S w0 w1 w2 w3⌋ ∗
            -- and its value is val(a) * val(b).
            ⌊MulU256Result (u256Val a0 a1 a2 a3) (u256Val b0 b1 b2 b3)
              (u512Val w0 w1 w2 w3)⌋))

/-! ## D3: `U512::checked_div_rem_by_u256` -/

/--
The private `U512::checked_div_rem_by_u256(self, den)` with `val(den) = 0` never returns
normally.
-/
def checked_div_rem_by_u256_zero_spec : Property := fun S =>
  -- For any words of `self` and any panic payload stack,
  ∀ (w0 w1 w2 w3 : Nat) (before : List S.Ptr),
    -- PRECONDITIONS
    -- if the words are `u128`s,
    Words w0 w1 w2 w3 →
    -- and the panic message literal is in read-only memory,
    S.global Mir.Global.«"U512 division by zero"» 1 msgU512DivZero ⊢
      -- then `self.checked_div_rem_by_u256(U256::ZERO)` never
      -- returns normally (it can only unwind).
      S.callUnwind Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256»
        [(u512Value S w0 w1 w2 w3,
            S.argTy Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256» 0),
         (u256Value S 0 0 0 0,
            S.argTy Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256» 1)]
        before (neverReturns S)

/--
The private `U512::checked_div_rem_by_u256(self, den)` with `val(den) ≠ 0` does not unwind. On
normal return, with `Q = val(self) / val(den)` and `R = val(self) % val(den)`, it gives
`Some((q, r))` with `val(q) = Q` and `val(r) = R` when `Q < 2^256`, and `None` when
`Q ≥ 2^256`.
-/
def checked_div_rem_by_u256_nonzero_spec : Property := fun S =>
  -- For any words of `self` and limbs of `den`,
  ∀ (w0 w1 w2 w3 d0 d1 d2 d3 : Nat),
    -- PRECONDITIONS
    -- if the words are `u128`s and the limbs `u64`s
    Words w0 w1 w2 w3 → Limbs d0 d1 d2 d3 →
    -- and the divisor is nonzero,
    u256Val d0 d1 d2 d3 ≠ 0 →
    -- and the constants the function reads are in read-only
    -- memory,
    checkedDivRemConstants S ⊢
      -- then `self.checked_div_rem_by_u256(den)` does not unwind;
      -- on normal return,
      S.call Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256»
        [(u512Value S w0 w1 w2 w3,
            S.argTy Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256» 0),
         (u256Value S d0 d1 d2 d3,
            S.argTy Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256» 1)]
        (S.returns fun v => iprop(
          -- POSTCONDITIONS
          -- either the result is `Some((q, r))`
          (∃ q0 q1 q2 q3 r0 r1 r2 r3 : Nat,
            -- for `u64` limbs of `q` and `r`
            ⌊Limbs q0 q1 q2 q3 ∧ Limbs r0 r1 r2 r3⌋ ∗
            -- (the returned value is that `Some`),
            ⌊v = S.variantVal 1 (S.tupleVal
              [S.tupleVal [u256Value S q0 q1 q2 q3, u256Value S r0 r1 r2 r3]])⌋ ∗
            -- with Q < 2^256, val(q) = Q and val(r) = R,
            ⌊CheckedDivRemSome (u512Val w0 w1 w2 w3) (u256Val d0 d1 d2 d3)
              (u256Val q0 q1 q2 q3) (u256Val r0 r1 r2 r3)⌋) ∨
          -- or the result is `None`
          (⌊v = S.variantVal 0 (S.tupleVal [])⌋ ∗
            -- and Q ≥ 2^256.
            ⌊CheckedDivRemNone (u512Val w0 w1 w2 w3) (u256Val d0 d1 d2 d3)⌋)))

/-- D3 in partial-correctness form: the zero-divisor case never returns normally, and the
nonzero case never unwinds and returns as `checked_div_rem_by_u256_nonzero_spec` says. -/
def checked_div_rem_by_u256_property : Property := fun S =>
  checked_div_rem_by_u256_zero_spec S ∧ checked_div_rem_by_u256_nonzero_spec S

end Spec.WideMath

end
