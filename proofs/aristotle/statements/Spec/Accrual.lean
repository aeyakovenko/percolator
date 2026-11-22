module

public import Spec.Kernel
public import Spec.AccrualModel
public import Spec.Rust.Accrual

/-!
# Public contracts for `canonical_accrual_price_step_v16` (engine revision `f24a561c`)

`canonical_accrual_price_step_v16(current, target, cap_anchor, max_change_bps, exposed,
remainder_before_bps_num) -> V16Result<(u64, u16)>` of `src/v16.rs`, as compiled in the program
`Mir.Accrual` (handles in `Spec.Rust.Accrual`). Four properties, for every input (`current`,
`target`, `cap_anchor`, `max_change_bps` over the whole of `u64`, `exposed` over `bool`,
`remainder_before_bps_num` over the whole of `u16`):

* KP.contract (`canonical_accrual_price_step_v16_contract`): if the call returns, its result
  satisfies the `ensures` clause above the function (`accrual_ensures`).
* KP.nopanic (`canonical_accrual_price_step_v16_nopanic`): no execution of the call panics (it
  never unwinds; overflow checks are on in the release profile). Nothing is said about termination.
* KP.envelope (`canonical_accrual_price_step_v16_envelope`): for a chain of `n` calls with fixed
  `target`, `cap_anchor`, `max_change_bps` and `exposed = true`, each call fed the price and
  remainder returned by the previous one (`callChain`), if all `n` calls return `Ok`, the final
  price `p_n` satisfies `|p_n - p_0| * MAX_MARGIN_BPS <= n * cap_anchor * max_change_bps + r_0`
  and lies between `p_0` and `target`. Since this holds for every `n`, it holds for every
  intermediate price `p_i` (the end of the chain of the first `i` calls).
* KP.reaches (`canonical_accrual_price_step_v16_reaches`): under the same chain, if
  `n * cap_anchor * max_change_bps + r_0 >= |target - p_0| * MAX_MARGIN_BPS` and all `n` calls
  return `Ok`, then `p_n == target`. As literally stated this fails for the empty chain `n = 0`
  when `r_0 >= MAX_MARGIN_BPS` (a `u16` remainder can be up to `65_535`): for example `p_0 = 1`,
  `target = 2`, `r_0 = 10_000` meets the premise with no call made, but `p_0 != target`.
* KP.reaches with an added precondition (`canonical_accrual_price_step_v16_reaches_of_valid`): the
  same statement under `n >= 1 || r_0 < MAX_MARGIN_BPS`. (For `n >= 1` no premise is added: a
  first call with `r_0 >= MAX_MARGIN_BPS` returns `Err(InvalidConfig)`, so such chains are not all
  `Ok`. Every remainder the function returns is below `MAX_MARGIN_BPS`.)

The chain of calls is stated with the function's own call contract (`S.call` of the generated
function handle), applied repeatedly: KP.envelope and KP.reaches are properties of the function
itself, not of a model of it.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. Contracts describe partial correctness: they say
what holds if a call returns and whether it may unwind. Every function the step calls (its three
closures, `u128::checked_mul`, `u128::checked_add`, `u64::checked_add`, `u64::checked_sub`,
`u64::abs_diff`, `u16::try_from(u128)`, `u64::try_from(u128)`, `Option::and_then`,
`Option::ok_or`, `Result::map_err`, and the `?` machinery) is covered by a proof of its body, so no
contract of a called function is assumed. `u128` division and remainder by `MAX_MARGIN_BPS` are
the language's own operators, not library calls.
-/

@[expose] public section

namespace Spec.Accrual
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (U64 okValue errValue v16ErrorValue ifReturns)

/-- The Rust value of a `(u64, u16)` pair `(price, remainder)`. -/
def pairValue {PROP : Type} [BI PROP] (S : Vocab PROP) (price remainder : Nat) : S.Value :=
  S.tupleVal [S.intVal price, S.intVal remainder]

/-- The Rust value of a `V16Result<(u64, u16)>`: `Ok((p, r))` or `Err(e)` (by discriminant). -/
def resultValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Except Nat (Nat × Nat) → S.Value
  | .ok (p, r) => okValue S (pairValue S p r)
  | .error d => errValue S (v16ErrorValue S d)

/-- A result is well formed: `Ok((p, r))` with `p` a `u64` and `r` a `u16`, or `Err(e)` for one of
the twelve `V16Error` variants. -/
def ResultWF : Except Nat (Nat × Nat) → Prop
  | .ok (p, r) => U64 p ∧ U16 r
  | .error d => d < 12

/-- The call `canonical_accrual_price_step_v16(current, target, cap_anchor, max_change_bps,
exposed, remainder_before_bps_num)` with outcome predicate `Φ`. -/
def callStep {PROP : Type} [BI PROP] (S : Vocab PROP)
    (current target cap_anchor max_change_bps : Nat) (exposed : Bool)
    (remainder_before_bps_num : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16»
    [(S.intVal current, S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 0),
     (S.intVal target, S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 1),
     (S.intVal cap_anchor,
        S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 2),
     (S.intVal max_change_bps,
        S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 3),
     (S.boolVal exposed,
        S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 4),
     (S.intVal remainder_before_bps_num,
        S.argTy Mir.Accrual.Fn.«percolator::v16::canonical_accrual_price_step_v16» 5)] Φ

/-- What follows one call of a chain: if it returns `Ok((p, r))`, continue with `k p r`; if it
returns `Err(_)` or unwinds, the chain stops (nothing more is claimed). -/
def onOk {PROP : Type} [BI PROP] (S : Vocab PROP) (k : Nat → Nat → PROP) :
    CallOutcome S.Value → PROP
  | .returned v => iprop(∀ p : Nat, ∀ r : Nat, ⌜v = okValue S (pairValue S p r)⌝ -∗ k p r)
  | .unwound => iprop(True)

/-- `callChain S target cap_anchor max_change_bps n p₀ r₀ k`: run `n` calls
`canonical_accrual_price_step_v16(pᵢ, target, cap_anchor, max_change_bps, true, rᵢ)` in sequence,
starting from price `p₀` and remainder `r₀`, each call taking the price and remainder returned
(`Ok((pᵢ₊₁, rᵢ₊₁))`) by the previous one; if all `n` calls return `Ok`, the final price `pₙ` and
remainder `rₙ` satisfy `k pₙ rₙ`. -/
def callChain {PROP : Type} [BI PROP] (S : Vocab PROP) (target cap_anchor max_change_bps : Nat) :
    Nat → Nat → Nat → (Nat → Nat → PROP) → PROP
  | 0, p, r, k => k p r
  | n + 1, p, r, k =>
    callStep S p target cap_anchor max_change_bps true r
      (onOk S fun p' r' => callChain S target cap_anchor max_change_bps n p' r' k)

/-- KP.contract: for all inputs, if `canonical_accrual_price_step_v16(..)` returns, its result
satisfies the `ensures` clause above the function. -/
def canonical_accrual_price_step_v16_contract : Property := fun S =>
  -- For any price, target, anchor, cap, exposure flag and carried remainder,
  ∀ (current target cap_anchor max_change_bps : Nat) (exposed : Bool)
    (remainder_before_bps_num : Nat),
    -- PRECONDITIONS
    -- if they are values of their Rust types (there is no `requires`),
    U64 current → U64 target → U64 cap_anchor → U64 max_change_bps →
    U16 remainder_before_bps_num →
    ⊢ -- then, on normal return of the call,
      callStep S current target cap_anchor max_change_bps exposed remainder_before_bps_num
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the returned value is a well-formed `V16Result<(u64, u16)>` satisfying the clause.
        ∃ res, ResultWF res ∧ v = resultValue S res ∧
          accrual_ensures current target cap_anchor max_change_bps exposed
            remainder_before_bps_num res)

/-- KP.nopanic: for all inputs, `canonical_accrual_price_step_v16(..)` does not panic. -/
def canonical_accrual_price_step_v16_nopanic : Property := fun S =>
  -- For any price, target, anchor, cap, exposure flag and carried remainder,
  ∀ (current target cap_anchor max_change_bps : Nat) (exposed : Bool)
    (remainder_before_bps_num : Nat),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U64 current → U64 target → U64 cap_anchor → U64 max_change_bps →
    U16 remainder_before_bps_num →
    ⊢ -- then the call does not unwind (panic).
      callStep S current target cap_anchor max_change_bps exposed remainder_before_bps_num
        (S.returns fun _ => iprop(True))

/-- KP.envelope: for fixed `target`, `cap_anchor`, `max_change_bps` and `exposed = true`, after a
chain of `n` calls from price `p₀` and remainder `r₀` that all return `Ok`, the final price `pₙ`
satisfies `|pₙ - p₀| * MAX_MARGIN_BPS <= n * cap_anchor * max_change_bps + r₀` (exact integers)
and lies between `p₀` and `target` inclusive. -/
def canonical_accrual_price_step_v16_envelope : Property := fun S =>
  -- For any target, anchor, cap, starting price and remainder, and any number of calls `n`,
  ∀ (target cap_anchor max_change_bps p₀ r₀ n : Nat),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U64 target → U64 cap_anchor → U64 max_change_bps → U64 p₀ → U16 r₀ →
    ⊢ -- then, if all `n` chained calls return `Ok`, the final price `pₙ`
      callChain S target cap_anchor max_change_bps n p₀ r₀ (fun pₙ _ =>
        -- POSTCONDITIONS
        -- moved at most the summed allowance,
        iprop(⌊absDiff pₙ p₀ * MAX_MARGIN_BPS ≤ n * cap_anchor * max_change_bps + r₀ ∧
        -- and lies between the starting price and the target.
         Between p₀ target pₙ⌋))

/-- KP.reaches: for fixed `target`, `cap_anchor`, `max_change_bps` and `exposed = true`, if
`n * cap_anchor * max_change_bps + r₀ >= |target - p₀| * MAX_MARGIN_BPS`, then after a chain of
`n` calls from price `p₀` and remainder `r₀` that all return `Ok`, the final price is `target`. -/
def canonical_accrual_price_step_v16_reaches : Property := fun S =>
  -- For any target, anchor, cap, starting price and remainder, and any number of calls `n`,
  ∀ (target cap_anchor max_change_bps p₀ r₀ n : Nat),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U64 target → U64 cap_anchor → U64 max_change_bps → U64 p₀ → U16 r₀ →
    -- and the summed allowance covers the distance to the target,
    absDiff target p₀ * MAX_MARGIN_BPS ≤ n * cap_anchor * max_change_bps + r₀ →
    ⊢ -- then, if all `n` chained calls return `Ok`, the final price is the target.
      callChain S target cap_anchor max_change_bps n p₀ r₀ (fun pₙ _ =>
        -- POSTCONDITION
        iprop(⌊pₙ = target⌋))

/-- KP.reaches with an added precondition: as `canonical_accrual_price_step_v16_reaches`, for a
chain of at least one call or a starting remainder below `MAX_MARGIN_BPS` (the literal statement
fails for the empty chain `n = 0` with `r₀ ≥ MAX_MARGIN_BPS`). -/
def canonical_accrual_price_step_v16_reaches_of_valid : Property := fun S =>
  -- For any target, anchor, cap, starting price and remainder, and any number of calls `n`,
  ∀ (target cap_anchor max_change_bps p₀ r₀ n : Nat),
    -- PRECONDITIONS
    -- if they are values of their Rust types,
    U64 target → U64 cap_anchor → U64 max_change_bps → U64 p₀ → U16 r₀ →
    -- the chain makes at least one call or starts from a remainder below `MAX_MARGIN_BPS`
    -- (added precondition),
    (1 ≤ n ∨ r₀ < MAX_MARGIN_BPS) →
    -- and the summed allowance covers the distance to the target,
    absDiff target p₀ * MAX_MARGIN_BPS ≤ n * cap_anchor * max_change_bps + r₀ →
    ⊢ -- then, if all `n` chained calls return `Ok`, the final price is the target.
      callChain S target cap_anchor max_change_bps n p₀ r₀ (fun pₙ _ =>
        -- POSTCONDITION
        iprop(⌊pₙ = target⌋))

end Spec.Accrual
