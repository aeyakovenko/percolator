module

public import Spec.Rust
public import Spec.WideMathLoopFree
public import SepLogic.Public

/-!
# Public contracts for the division layer of `percolator::wide_math` (step 1)

Properties C1 (nonzero divisor), C2 and D3 (nonzero divisor) of `aristotle/wide_math_spec.md`,
about `src/wide_math.rs` compiled without `cfg(kani)`, default features, release profile with
`overflow-checks = true`. Values are given as in `Spec.WideMathLoopFree` (`u256Of`, `i256Of`);
`u512Of S x` is the private `U512` of value `x`.

All contracts are partial-correctness statements. `S.returns` excludes unwinding: a contract
ending in it says that **no execution panics**, and what holds if the call returns normally.
`neverReturns S` excludes normal return: **every execution panics**. Neither claims
termination.

## Assumed library behaviour

The following functions of the Rust core library are reached by the verified code and are not
covered by proof; their documented behaviour is an explicit hypothesis of every property that
reaches them:

* `<U256 as PartialOrd>::gt` and `<U256 as PartialOrd>::ge` (the provided methods of
  `PartialOrd`, instantiated at `U256`): `a > b` holds exactly when `a.partial_cmp(&b)` is
  `Some(Greater)`, and `a >= b` exactly when it is `Some(Greater)` or `Some(Equal)`
  (`GtU256Contract`, `GeU256Contract`);
* `Option::<U256>::expect` (`ExpectU256Contract`): returns the value inside `Some`, and
  panics on `None`;
* `<Range<usize> as Iterator>::next` and `<Range<usize> as DoubleEndedIterator>::next_back`
  (`RangeNextContract`, `RangeNextBackContract`): the standard stepping of a `usize` range.
-/

@[expose] public section

namespace Spec.WideMath
open Iris Iris.BI Rust2Lean.Public

/-! ## Vocabulary -/

/-- The private `U512` with value `x` (`x < 2^512`): `U512([w0, w1, w2, w3])` with `w0 .. w3`
the base-`2^128` digits of `x`, least significant first. -/
def u512Of {PROP : Type} [BI PROP] (S : Vocab PROP) (x : Nat) : S.Value :=
  u512Value S (x % 2 ^ 128) (x / 2 ^ 128 % 2 ^ 128) (x / 2 ^ 256 % 2 ^ 128) (x / 2 ^ 384 % 2 ^ 128)

/-- `ge` on `U256` behaves as documented for `PartialOrd::ge`: given that `partial_cmp`
returns `Some(compare x y)` on references to values `x` and `y`, `x >= y` returns
`compare x y != Less`, does not panic and gives the shares back. -/
def GeU256Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x y : Nat, x < 2 ^ 256 → y < 2 ^ 256 →
    -- if `partial_cmp` returns `Some(compare x y)` on any references to x and y,
    (∀ (r r' : S.Ptr) (q q' : Qp), u256At S r q x ∗ u256At S r' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialOrd>::partial_cmp»
        (S.ptrVal r) (S.ptrVal r')
        (S.returns fun v => iprop(⌊v = someVal S (orderingVal S (compare x y))⌋ ∗
          u256At S r q x ∗ u256At S r' q' y))) →
    -- then `ge` on references to x and y returns `compare x y != Less`.
    (∀ (p p' : S.Ptr) (q q' : Qp), u256At S p q x ∗ u256At S p' q' y ⊢
      rcall2 S Mir.Fn.«<percolator::wide_math::U256 as core::cmp::PartialOrd>::ge»
        (S.ptrVal p) (S.ptrVal p')
        (S.returns fun v => iprop(⌊v = S.boolVal (compare x y != .lt)⌋ ∗
          u256At S p q x ∗ u256At S p' q' y)))

/-- The type `Range<usize>` (the return type of `<Range<usize> as IntoIterator>::into_iter`). -/
def rangeUsizeTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Fn.«<core::ops::range::Range<usize> as core::iter::traits::collect::IntoIterator>::into_iter»

/-- The range `a..b` of `usize`s. -/
def rangeVal {PROP : Type} [BI PROP] (S : Vocab PROP) (a b : Nat) : S.Value :=
  S.tupleVal [S.intVal a, S.intVal b]

/-- `<Range<usize> as Iterator>::next` behaves as documented: on `&mut (a..b)` it does not
panic; if `a < b` it returns `Some(a)` and leaves `a+1..b`, otherwise it returns `None` and
leaves the range unchanged. -/
def RangeNextContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ (p : S.Ptr) (a b : Nat), a < 2 ^ 64 → b < 2 ^ 64 →
    S.pointsTo p (rangeUsizeTy S) (rangeVal S a b) ⊢
      rcall1 S Mir.Fn.«core::iter::range::<impl core::iter::traits::iterator::Iterator for core::ops::range::Range<usize>>::next»
        (S.ptrVal p)
        (S.returns fun v => iprop(
          ⌊v = if a < b then someVal S (S.intVal a) else noneVal S⌋ ∗
          S.pointsTo p (rangeUsizeTy S) (rangeVal S (if a < b then a + 1 else a) b)))

/-- `<Range<usize> as DoubleEndedIterator>::next_back` behaves as documented: on
`&mut (a..b)` it does not panic; if `a < b` it returns `Some(b-1)` and leaves `a..b-1`,
otherwise it returns `None` and leaves the range unchanged. -/
def RangeNextBackContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ (p : S.Ptr) (a b : Nat), a < 2 ^ 64 → b < 2 ^ 64 →
    S.pointsTo p (rangeUsizeTy S) (rangeVal S a b) ⊢
      rcall1 S Mir.Fn.«core::iter::range::<impl core::iter::traits::double_ended::DoubleEndedIterator for core::ops::range::Range<usize>>::next_back»
        (S.ptrVal p)
        (S.returns fun v => iprop(
          ⌊v = if a < b then someVal S (S.intVal (b - 1)) else noneVal S⌋ ∗
          S.pointsTo p (rangeUsizeTy S) (rangeVal S a (if a < b then b - 1 else b))))

/-- The read-only constants `U512::checked_div_rem_by_u256` reads on its non-panicking paths:
the promoted constant `Ordering::Less` and the `expect` message `"remainder must fit U256"`. -/
def checkedDivRemConsts {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(S.global Mir.Global.«const [1 bytes] 165fc9f0fda32787» 1 [255] ∗
    S.global Mir.Global.«"remainder must fit U256"» 1 (strBytes "remainder must fit U256"))

/-- **Assumed library behaviour** of `core::option::Option::<u128>::expect` (its documented
contract): on `Some(v)` it returns `v` without panicking; on `None` it panics (never returns
normally). The message is an ASCII `&str` of which the caller lends a share. -/
def ExpectU128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  -- On `Some(x)` with an ASCII message `bs` at `m` (share `q`),
  (∀ (x : Nat) (m : S.Ptr) (q : Qp) (bs : List UInt8), x < 2 ^ 128 →
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcall2 S Mir.Fn.«core::option::Option::<u128>::expect»
        (someVal S (S.intVal x)) (S.slicePtrVal m bs.length)
        -- `expect` does not panic, returns x and gives the share back;
        (S.returns fun v => iprop(⌊v = S.intVal x⌋ ∗ strAt S m q bs))) ∧
  -- on `None`, it never returns normally.
  (∀ (m : S.Ptr) (q : Qp) (bs : List UInt8) (before : List S.Ptr),
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcallUnwind2 S Mir.Fn.«core::option::Option::<u128>::expect»
        (noneVal S) (S.slicePtrVal m bs.length) before (neverReturns S))

/-- **Assumed library behaviour** of `core::option::Option::<I256>::expect` (its documented
contract): on `Some(v)` it returns `v` without panicking; on `None` it panics (never returns
normally). The message is an ASCII `&str` of which the caller lends a share. -/
def ExpectI256Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  -- On `Some(x)` with an ASCII message `bs` at `m` (share `q`),
  (∀ (s : Int) (m : S.Ptr) (q : Qp) (bs : List UInt8), InI256 s →
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcall2 S Mir.Fn.«core::option::Option::<percolator::wide_math::I256>::expect»
        (someVal S (i256Of S s)) (S.slicePtrVal m bs.length)
        -- `expect` does not panic, returns x and gives the share back;
        (S.returns fun v => iprop(⌊v = i256Of S s⌋ ∗ strAt S m q bs))) ∧
  -- on `None`, it never returns normally.
  (∀ (m : S.Ptr) (q : Qp) (bs : List UInt8) (before : List S.Ptr),
    (∀ b ∈ bs, b.toNat < 128) →
    strAt S m q bs ⊢
      rcallUnwind2 S Mir.Fn.«core::option::Option::<percolator::wide_math::I256>::expect»
        (noneVal S) (S.slicePtrVal m bs.length) before (neverReturns S))

/-- A read-only string literal `s` (alignment 1) of the program, holding the UTF-8 bytes of
`s`. -/
def litAt {PROP : Type} [BI PROP] (S : Vocab PROP) (g : RustGlobal) (s : String) : PROP :=
  S.global g 1 (strBytes s)

/-- The read-only data of `src/wide_math.rs` that the division functions read: the promoted
constants `Ordering::Less` and `I256::MIN`, and the string literals of their panic and `expect`
messages. Every such global is placed by the program at start-up; this is an environment fact,
not a restriction on inputs. -/
def divGlobals {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  iprop(checkedDivRemConsts S ∗ i256MinConst S ∗
    litAt S Mir.Global.«"U256 division by zero"» "U256 division by zero" ∗
    litAt S Mir.Global.«"U512 division by zero"» "U512 division by zero" ∗
    litAt S Mir.Global.«"mul_div quotient must fit U256"» "mul_div quotient must fit U256" ∗
    litAt S Mir.Global.«"U256 div by zero"» "U256 div by zero" ∗
    litAt S Mir.Global.«"U256 rem by zero"» "U256 rem by zero" ∗
    litAt S Mir.Global.«"ceil_div overflow"» "ceil_div overflow" ∗
    litAt S Mir.Global.«"ceil_div_positive_checked: zero denominator"»
      "ceil_div_positive_checked: zero denominator" ∗
    litAt S Mir.Global.«"mul_div_floor_u256: zero denominator"»
      "mul_div_floor_u256: zero denominator" ∗
    litAt S Mir.Global.«"mul_div_floor_u256_with_rem: zero denominator"»
      "mul_div_floor_u256_with_rem: zero denominator" ∗
    litAt S Mir.Global.«"mul_div_ceil overflow"» "mul_div_ceil overflow" ∗
    litAt S Mir.Global.«"mul_div_ceil_u256: zero denominator"»
      "mul_div_ceil_u256: zero denominator" ∗
    litAt S Mir.Global.«"mul_div_floor_u128: a*b overflow"» "mul_div_floor_u128: a*b overflow" ∗
    litAt S Mir.Global.«"mul_div_floor_u128: division by zero"»
      "mul_div_floor_u128: division by zero" ∗
    litAt S Mir.Global.«"mul_div_ceil_u128: division by zero"»
      "mul_div_ceil_u128: division by zero" ∗
    litAt S Mir.Global.«"mul_div_ceil_u128: a*b overflow"» "mul_div_ceil_u128: a*b overflow" ∗
    litAt S Mir.Global.«"wide_mul_div_floor_u128: result exceeds u128"»
      "wide_mul_div_floor_u128: result exceeds u128" ∗
    litAt S Mir.Global.«"wide_mul_div_floor_u128: division by zero"»
      "wide_mul_div_floor_u128: division by zero" ∗
    litAt S Mir.Global.«"wide_mul_div_ceil_u128_or_over_i"… [56 bytes] 7943dd33cea49962»
      "wide_mul_div_ceil_u128_or_over_i128max: division by zero" ∗
    litAt S Mir.Global.«"floor_div_signed_conservative: zero denominator"»
      "floor_div_signed_conservative: zero denominator" ∗
    litAt S Mir.Global.«"abs of negative I256"» "abs of negative I256" ∗
    litAt S Mir.Global.«"floor_div quotient overflow"» "floor_div quotient overflow" ∗
    litAt S Mir.Global.«"floor_div result out of range"» "floor_div result out of range" ∗
    litAt S Mir.Global.«"floor_div_signed_conservative_i1"… [52 bytes] f6ba71dbb1700623»
      "floor_div_signed_conservative_i128: zero denominator" ∗
    litAt S Mir.Global.«"floor_div_signed_conservative_i1"… [55 bytes] 2550b6a279c23806»
      "floor_div_signed_conservative_i128: result out of range" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor: zero denominator"»
      "wide_signed_mul_div_floor: zero denominator" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor: k_diff == I256::MIN"»
      "wide_signed_mul_div_floor: k_diff == I256::MIN" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor quotient overflow"»
      "wide_signed_mul_div_floor quotient overflow" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor result"… [50 bytes] 20fb0dd9970266e6»
      "wide_signed_mul_div_floor result out of I256 range" ∗
    litAt S Mir.Global.«"abs_u256 called on I256::MIN"» "abs_u256 called on I256::MIN" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor_from_k_pair: den == 0"»
      "wide_signed_mul_div_floor_from_k_pair: den == 0" ∗
    litAt S Mir.Global.«"K-diff overflow in wide"» "K-diff overflow in wide" ∗
    litAt S Mir.Global.«"wide product overflow"» "wide product overflow" ∗
    litAt S Mir.Global.«"mag overflow"» "mag overflow" ∗
    litAt S Mir.Global.«"mag exceeds u128"» "mag exceeds u128" ∗
    litAt S Mir.Global.«"quotient exceeds u128"» "quotient exceeds u128" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor_from_k"… [54 bytes] dee417b6e8e69fe6»
      "wide_signed_mul_div_floor_from_k_pair: mag > i128::MAX" ∗
    litAt S Mir.Global.«"wide_signed_mul_div_floor_from_k"… [52 bytes] 224fe64f38e15a8a»
      "wide_signed_mul_div_floor_from_k_pair: q > i128::MAX")

/-! ## C1 and C2: `div_rem_u256` with a nonzero divisor -/

/--
C1 (nonzero divisor). Assuming the documented behaviour of `PartialOrd::gt` and
`PartialOrd::ge` on `U256`: for every `num` and every `den` with `val(den) != 0`, no execution of
`div_rem_u256(num, den)` panics. (The zero-divisor half of C1, every execution panics, is
`div_rem_u256_zero_spec`.)
-/
def C1_div_rem_u256_nonzero_no_panic : Property := fun S =>
  -- ASSUMPTIONS: `gt` and `ge` on `U256` behave as documented.
  GtU256Contract S → GeU256Contract S →
  -- For any `U256` values n and d with d ≠ 0,
  ∀ n d : Nat, n < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    -- no execution of `div_rem_u256(num, den)` panics.
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::div_rem_u256» (u256Of S n) (u256Of S d)
        (S.returns fun _ => iprop(True))

/--
C2. Assuming the documented behaviour of `PartialOrd::gt` and `PartialOrd::ge` on `U256`: for
every `num` and every `den` with `val(den) != 0`, if `div_rem_u256(num, den)` returns `(q, r)`,
then `val(num) = val(q) * val(den) + val(r)` and `val(r) < val(den)`.
-/
def C2_div_rem_u256_value : Property := fun S =>
  -- ASSUMPTIONS: `gt` and `ge` on `U256` behave as documented.
  GtU256Contract S → GeU256Contract S →
  -- For any `U256` values n and d with d ≠ 0,
  ∀ n d : Nat, n < 2 ^ 256 → d < 2 ^ 256 → d ≠ 0 →
    ⊢ rcall2 S Mir.Fn.«percolator::wide_math::div_rem_u256» (u256Of S n) (u256Of S d)
        (S.returns fun v => iprop(
          -- if the call returns, it returns a pair (q, r) of `U256` values
          ∃ q r : Nat, ⌊q < 2 ^ 256 ∧ r < 2 ^ 256⌋ ∗
            ⌊v = pairVal S (u256Of S q) (u256Of S r)⌋ ∗
            -- with n = q * d + r and r < d.
            ⌊n = q * d + r ∧ r < d⌋))

/-! ## D3: `U512::checked_div_rem_by_u256` with a nonzero divisor -/

/--
D3 (nonzero divisor). Assuming the documented behaviour of `PartialOrd::gt`, `PartialOrd::ge`,
`Option::<U256>::expect` and the `next`/`next_back` of `Range<usize>`: for every `self` and
every `den` with `val(den) != 0`, with `Q = floor(val(self)/val(den))` and
`R = val(self) mod val(den)`, no execution of the private `self.checked_div_rem_by_u256(den)`
panics, and if it returns, the result is `Some((q, r))` with `val(q) = Q` and `val(r) = R` when
`Q < 2^256`, and `None` when `Q >= 2^256`. (The zero-divisor half of D3, every execution panics,
is `checked_div_rem_by_u256_zero_spec`.)
-/
def D3_checked_div_rem_nonzero : Property := fun S =>
  -- ASSUMPTIONS: the library functions behave as documented.
  GtU256Contract S → GeU256Contract S → ExpectU256Contract S →
  RangeNextContract S → RangeNextBackContract S →
  -- For any `U512` value x and `U256` value d with d ≠ 0,
  ∀ x d : Nat, x < 2 ^ 512 → d < 2 ^ 256 → d ≠ 0 →
    -- given the read-only constants the function reads,
    checkedDivRemConsts S ⊢
      -- no execution panics, and if the call returns,
      rcall2 S Mir.Fn.«percolator::wide_math::U512::checked_div_rem_by_u256»
        (u512Of S x) (u256Of S d)
        (S.returns fun v => iprop(
          -- the result is `Some((x / d, x % d))` if x / d < 2^256, otherwise `None`.
          ⌊v = if x / d < 2 ^ 256 then
                someVal S (pairVal S (u256Of S (x / d)) (u256Of S (x % d)))
              else noneVal S⌋))

end Spec.WideMath

end
