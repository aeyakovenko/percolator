module

public import Spec.Rust.Consts
public import Spec.WideMathLoopFree
public import SepLogic.Public

/-!
# Public contracts for the associated constants of `U256` and `I256` (A2, A6)

The constants `U256::ZERO`, `U256::ONE`, `U256::MAX` and `I256::ZERO`, `I256::ONE`,
`I256::MINUS_ONE`, `I256::MAX`, `I256::MIN` of `src/wide_math.rs` (non-`kani` build) have no
code of their own. They are observed through the verification harness
`harness/wide_math_consts`, which compiles the unchanged `src/wide_math.rs` as a module with the
crate's release settings (`overflow-checks = true`) and returns each constant from a function
(`u256_zero`, `u256_one`, `u256_max`, `i256_zero`, `i256_one`, `i256_minus_one`, `i256_max`,
`i256_min`). The handles `Mir.Consts.Fn.*` name those harness functions.

Values are given as in `Spec.WideMathLoopFree`: `u256Of S x` is the `U256` with `val = x`, and
`i256Of S s` is the `I256` with `sval = s`.
-/

@[expose] public section

namespace Spec.WideMath
open Iris Iris.BI Rust2Lean.Public

/-- Call the argument-less function `f`. -/
def rcall0 {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call f [] Φ

/--
A2 (constants). `U256::ZERO`, `U256::ONE` and `U256::MAX` have values `0`, `1` and
`2^256 - 1`: the harness functions returning them do not panic and return exactly these values.
-/
def A2_u256_constants : Property := fun S =>
  -- `U256::ZERO` has value 0;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::u256_zero»
      (S.returns fun v => iprop(⌊v = u256Of S 0⌋))) ∧
  -- `U256::ONE` has value 1;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::u256_one»
      (S.returns fun v => iprop(⌊v = u256Of S 1⌋))) ∧
  -- `U256::MAX` has value 2^256 - 1.
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::u256_max»
      (S.returns fun v => iprop(⌊v = u256Of S (2 ^ 256 - 1)⌋)))

/--
A6 (constants). `I256::ZERO`, `I256::ONE`, `I256::MINUS_ONE`, `I256::MAX` and `I256::MIN` have
two's-complement values `0`, `1`, `-1`, `2^255 - 1` and `-2^255`: the harness functions
returning them do not panic and return exactly these values.
-/
def A6_i256_constants : Property := fun S =>
  -- `I256::ZERO` has sval 0;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::i256_zero»
      (S.returns fun v => iprop(⌊v = i256Of S 0⌋))) ∧
  -- `I256::ONE` has sval 1;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::i256_one»
      (S.returns fun v => iprop(⌊v = i256Of S 1⌋))) ∧
  -- `I256::MINUS_ONE` has sval -1;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::i256_minus_one»
      (S.returns fun v => iprop(⌊v = i256Of S (-1)⌋))) ∧
  -- `I256::MAX` has sval 2^255 - 1;
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::i256_max»
      (S.returns fun v => iprop(⌊v = i256Of S (2 ^ 255 - 1)⌋))) ∧
  -- `I256::MIN` has sval -2^255.
  (⊢ rcall0 S Mir.Consts.Fn.«wide_math_consts_harness::i256_min»
      (S.returns fun v => iprop(⌊v = i256Of S (-2 ^ 255)⌋)))

end Spec.WideMath

end
