module

public import Spec.Rust.KernelGen
public import Spec.Kernel5
public import SepLogic.PublicGeneric

/-!
# Public generic contracts for K50 `apply_total_delta` and K56
`retire_terminal_unbudgeted_insurance_delta`, for every `T`

`apply_total_delta` (K50) and `retire_terminal_unbudgeted_insurance_delta` (K56) are private
associated functions of the generic `impl<'a, T> MarketGroupV16ViewMut<'a, T>` of `src/v16.rs`.
The crate never instantiates `T`; these properties are about **every instance** that any crate
using `percolator` (the *client*) builds of the two generic bodies, whatever `T` is. The body
handles (`Mir.KernelGen.Body.*`), the source items the bodies call (`Mir.KernelGen.Item.*`) and
the named types (`Mir.KernelGen.TypeName.*`) are in `Spec.Rust.KernelGen`.

Each property quantifies over the client's instance `c` of the body at any const arguments `cs`
and type arguments `ts` (`S.inst … cs ts c`, so any `T`), and states, for all `u128` arguments,
exactly what the `T = u64` properties of `Spec.KernelInst50` state (the same result predicates
`U128Result` / `U128TripleResult` and `ensures` of `Spec.KernelModel5`).

They hold under the following assumptions about the client crate, stated with the public
vocabulary:

* **Value layouts** — the client's representations of the Rust types the bodies build hold the
  values they build: every fieldless `V16Error` variant is a valid `V16Error`
  (`V16ErrorValid`); `None` and every `Some(x)` are valid `Option<u128>` values
  (`OptionU128Valid`, K50 only); the function's own return type `V16Result<u128>` (K50) or
  `V16Result<(u128, u128, u128)>` (K56) holds every `Ok` value with `u128` components and every
  `Err(e)` (`U128ResultValid`, `U128TripleResultValid`). These are facts about rustc's layout
  of these types, which the client's representation table records.
* **Library calls (K50 only)** — `apply_total_delta` calls `u128::checked_add`,
  `u128::checked_sub` and `Option::<u128>::ok_or::<V16Error>`; which function runs is the
  client's resolution of those calls. The premises `CheckedAddU128GContract`,
  `CheckedSubU128GContract` and `OkOrU128GContract` say that each resolved function uses the
  Rust calling convention, has the Rust signature's types (as the client represents them) and
  behaves as the Rust standard library documents, returning without panicking. These are
  assumptions; the bodies of these standard-library functions are not verified here.

`u128` is not a named type, so a type `U` is identified as `u128` by its representation: the
client represents it as an unsigned 16-byte integer (`IsU128`).

Both properties are partial-correctness properties: neither says the call terminates.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Client vocabulary -/

/-- `U` is `u128` as the client sees it: the client represents `U` as an unsigned 128-bit
(16-byte) integer. -/
def IsU128 {PROP : Type} [BI PROP] (S : GVocab PROP) (U : S.RTy) : Prop :=
  S.represents U (S.intTy false 16)

/-! ## Value-layout premises -/

/-- Every fieldless variant of `V16Error` (discriminants `InvalidConfig = 0`, …,
`CounterUnderflow = 11`) is a valid value of the client's representation of `V16Error`. -/
def V16ErrorValid {PROP : Type} [BI PROP] (S : GVocab PROP) : Prop :=
  ∀ (E : S.RTy) (τ : S.Ty), S.isNamed Mir.KernelGen.TypeName.«percolator::v16::V16Error» [] [] E →
    S.represents E τ → ∀ d : Nat, d < 12 → S.loadable (v16ErrorValue S.toVocab d) τ

/-- `None`, and `Some(x)` for every `u128` `x`, are valid values of the client's representation
of `Option<u128>`. -/
def OptionU128Valid {PROP : Type} [BI PROP] (S : GVocab PROP) : Prop :=
  ∀ (U O : S.RTy) (τ : S.Ty), IsU128 S U →
    S.isNamed Mir.KernelGen.TypeName.«core::option::Option» [] [U] O → S.represents O τ →
    S.loadable (noneValue S.toVocab) τ ∧
    ∀ x : Nat, U128 x → S.loadable (someValue S.toVocab (S.intVal x)) τ

/-- The return type of the client's function `c` (`V16Result<u128>`) holds `Ok(x)` for every
`u128` `x` and `Err(e)` for every `V16Error` `e`. -/
def U128ResultValid {PROP : Type} [BI PROP] (S : GVocab PROP) (c : Int) : Prop :=
  (∀ x : Nat, U128 x → S.loadable (okValue S.toVocab (S.intVal x)) (S.fnRetTy c)) ∧
  ∀ d : Nat, d < 12 → S.loadable (errValue S.toVocab (v16ErrorValue S.toVocab d)) (S.fnRetTy c)

/-- The return type of the client's function `c` (`V16Result<(u128, u128, u128)>`) holds
`Ok((a, b, c'))` for all `u128`s `a`, `b`, `c'` and `Err(e)` for every `V16Error` `e`. -/
def U128TripleResultValid {PROP : Type} [BI PROP] (S : GVocab PROP) (c : Int) : Prop :=
  (∀ a b c' : Nat, U128 a → U128 b → U128 c' →
    S.loadable (okValue S.toVocab (S.tupleVal [S.intVal a, S.intVal b, S.intVal c']))
      (S.fnRetTy c)) ∧
  ∀ d : Nat, d < 12 → S.loadable (errValue S.toVocab (v16ErrorValue S.toVocab d)) (S.fnRetTy c)

/-! ## Library-call premises (documented standard-library behaviour) -/

/-- **`u128::checked_add`**, as the client resolves it: the resolved function uses the Rust
calling convention, returns the client's `Option<u128>`, and `checked_add(x, y)` returns
`Some(x + y)` if `x + y < 2^128`, else `None`, without panicking. -/
def CheckedAddU128GContract {PROP : Type} [BI PROP] (S : GVocab PROP) : Prop :=
  ∀ N : Int, S.resolves Mir.KernelGen.Item.«core::num::<impl u128>::checked_add» [] [] N →
    S.fnConvention N .Rust ∧
    (∀ (U O : S.RTy) (τ : S.Ty), IsU128 S U →
      S.isNamed Mir.KernelGen.TypeName.«core::option::Option» [] [U] O → S.represents O τ →
      S.fnRetTy N = τ) ∧
    ∀ x y : Nat, U128 x → U128 y →
      ⊢ S.wp_call N [(S.intVal x, S.intTy false 16), (S.intVal y, S.intTy false 16)]
          (S.returns fun w => iprop(⌊w = if x + y < 2 ^ 128
            then someValue S.toVocab (S.intVal (x + y : Nat)) else noneValue S.toVocab⌋))

/-- **`u128::checked_sub`**, as the client resolves it: the resolved function uses the Rust
calling convention, returns the client's `Option<u128>`, and `checked_sub(x, y)` returns
`Some(x - y)` if `y ≤ x`, else `None`, without panicking. -/
def CheckedSubU128GContract {PROP : Type} [BI PROP] (S : GVocab PROP) : Prop :=
  ∀ N : Int, S.resolves Mir.KernelGen.Item.«core::num::<impl u128>::checked_sub» [] [] N →
    S.fnConvention N .Rust ∧
    (∀ (U O : S.RTy) (τ : S.Ty), IsU128 S U →
      S.isNamed Mir.KernelGen.TypeName.«core::option::Option» [] [U] O → S.represents O τ →
      S.fnRetTy N = τ) ∧
    ∀ x y : Nat, U128 x → U128 y →
      ⊢ S.wp_call N [(S.intVal x, S.intTy false 16), (S.intVal y, S.intTy false 16)]
          (S.returns fun w => iprop(⌊w = if y ≤ x
            then someValue S.toVocab (S.intVal (x - y : Nat)) else noneValue S.toVocab⌋))

/-- **`Option::<u128>::ok_or::<V16Error>`**, as the client resolves it (`U` is `u128`, `E` is
`V16Error`): the resolved function uses the Rust calling convention, takes the client's
`Option<u128>` and `V16Error` and returns the client's `Result<u128, V16Error>`, and
`ok_or(Some(x), e)` returns `Ok(x)` and `ok_or(None, e)` returns `Err(e)`, without panicking. -/
def OkOrU128GContract {PROP : Type} [BI PROP] (S : GVocab PROP) : Prop :=
  ∀ (U E : S.RTy) (N : Int), IsU128 S U →
    S.isNamed Mir.KernelGen.TypeName.«percolator::v16::V16Error» [] [] E →
    S.resolves Mir.KernelGen.Item.«core::option::Option::<T>::ok_or» [] [U, E] N →
    S.fnConvention N .Rust ∧
    (∀ (R : S.RTy) (τr : S.Ty), S.isNamed Mir.KernelGen.TypeName.«core::result::Result» [] [U, E] R →
      S.represents R τr → S.fnRetTy N = τr) ∧
    ∀ (O : S.RTy) (τo τe : S.Ty), S.isNamed Mir.KernelGen.TypeName.«core::option::Option» [] [U] O →
      S.represents O τo → S.represents E τe →
      (∀ x d : Nat, U128 x → d < 12 →
        ⊢ S.wp_call N [(someValue S.toVocab (S.intVal x), τo), (v16ErrorValue S.toVocab d, τe)]
            (S.returns fun w => iprop(⌊w = okValue S.toVocab (S.intVal x)⌋))) ∧
      (∀ d : Nat, d < 12 →
        ⊢ S.wp_call N [(noneValue S.toVocab, τo), (v16ErrorValue S.toVocab d, τe)]
            (S.returns fun w => iprop(⌊w = errValue S.toVocab (v16ErrorValue S.toVocab d)⌋)))

/-! ## K50 `apply_total_delta`, for every `T` -/

/-- The call of the client's function `c` with the `u128` arguments `total`, `old`, `new`. -/
def callApplyTotalDeltaG {PROP : Type} [BI PROP] (S : GVocab PROP) (c : Int)
    (total old new : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.wp_call c [(S.intVal total, S.intTy false 16), (S.intVal old, S.intTy false 16),
    (S.intVal new, S.intTy false 16)] Φ

/-- **K50.contract for every `T`**: for every instance `c` of
`MarketGroupV16ViewMut::<'a, T>::apply_total_delta` in a client crate (at any generic arguments,
so any `T`), under the value-layout and library-call premises, if `apply_total_delta(total, old,
new)` returns, its result is `Ok(t)` satisfying the `ensures` (`apply_total_delta_ensures`), or
an `Err(_)`. -/
def apply_total_delta_generic_contract : GProperty := fun S =>
  -- For every instance `c` of the generic body (any `T`),
  ∀ (cs : List S.RConst) (ts : List S.RTy) (c : Int),
    S.inst Mir.KernelGen.Body.«percolator::v16::MarketGroupV16ViewMut::<'a, T>::apply_total_delta»
      cs ts c →
    -- ASSUMPTIONS about the client crate
    V16ErrorValid S → OptionU128Valid S → U128ResultValid S c →
    CheckedAddU128GContract S → CheckedSubU128GContract S → OkOrU128GContract S →
    -- for any total, old and new values,
    ∀ (total old new : Nat),
    -- PRECONDITIONS: they are `u128` values (no `requires`),
    U128 total → U128 old → U128 new →
    ⊢ -- then, on normal return of the call,
      callApplyTotalDeltaG S c total old new (ifReturns S.toVocab fun v =>
        -- POSTCONDITIONS: the result is `Ok(t)` with the `ensures` of K50, or an `Err(_)`.
        U128Result S.toVocab (apply_total_delta_ensures total old new) True v)

/-- **K50.nopanic for every `T`**: under the same assumptions, `apply_total_delta(total, old,
new)` does not panic (the call never unwinds), at every instance. -/
def apply_total_delta_generic_nopanic : GProperty := fun S =>
  -- For every instance `c` of the generic body (any `T`),
  ∀ (cs : List S.RConst) (ts : List S.RTy) (c : Int),
    S.inst Mir.KernelGen.Body.«percolator::v16::MarketGroupV16ViewMut::<'a, T>::apply_total_delta»
      cs ts c →
    -- ASSUMPTIONS about the client crate
    V16ErrorValid S → OptionU128Valid S → U128ResultValid S c →
    CheckedAddU128GContract S → CheckedSubU128GContract S → OkOrU128GContract S →
    -- for any total, old and new values,
    ∀ (total old new : Nat),
    -- PRECONDITIONS: they are `u128` values (no `requires`),
    U128 total → U128 old → U128 new →
    ⊢ -- then the call does not unwind (panic).
      callApplyTotalDeltaG S c total old new (S.returns fun _ => iprop(True))

/-! ## K56 `retire_terminal_unbudgeted_insurance_delta`, for every `T` -/

/-- The call of the client's function `c` with the `u128` arguments `vault`, `insurance`,
`budget_remaining`, `source_reserved_atoms`. -/
def callRetireInsuranceG {PROP : Type} [BI PROP] (S : GVocab PROP) (c : Int)
    (vault insurance budget_remaining source_reserved_atoms : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.wp_call c [(S.intVal vault, S.intTy false 16), (S.intVal insurance, S.intTy false 16),
    (S.intVal budget_remaining, S.intTy false 16),
    (S.intVal source_reserved_atoms, S.intTy false 16)] Φ

/-- **K56.contract for every `T`**: for every instance `c` of
`MarketGroupV16ViewMut::<'a, T>::retire_terminal_unbudgeted_insurance_delta` in a client crate
(any `T`), under the value-layout premises, if the call returns, its result is
`Ok((retired, next_vault, next_insurance))` satisfying the `ensures`
(`retire_insurance_ensures`), or an `Err(_)`. The body calls no other function. -/
def retire_terminal_unbudgeted_insurance_delta_generic_contract : GProperty := fun S =>
  -- For every instance `c` of the generic body (any `T`),
  ∀ (cs : List S.RConst) (ts : List S.RTy) (c : Int),
    S.inst Mir.KernelGen.Body.«percolator::v16::MarketGroupV16ViewMut::<'a, T>::retire_terminal_unbudgeted_insurance_delta»
      cs ts c →
    -- ASSUMPTIONS about the client crate
    V16ErrorValid S → U128TripleResultValid S c →
    -- for any vault, insurance, budget remaining and source reservation,
    ∀ (vault insurance budget_remaining source_reserved_atoms : Nat),
    -- PRECONDITIONS: they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 budget_remaining → U128 source_reserved_atoms →
    ⊢ -- then, on normal return of the call,
      callRetireInsuranceG S c vault insurance budget_remaining source_reserved_atoms
        (ifReturns S.toVocab fun v =>
          -- POSTCONDITIONS: the result is `Ok((retired, next_vault, next_insurance))` with the
          -- `ensures` of K56, or an `Err(_)`.
          U128TripleResult S.toVocab
            (retire_insurance_ensures vault insurance budget_remaining source_reserved_atoms) v)

/-- **K56.nopanic for every `T`**: under the same assumptions,
`retire_terminal_unbudgeted_insurance_delta(..)` does not panic, at every instance. -/
def retire_terminal_unbudgeted_insurance_delta_generic_nopanic : GProperty := fun S =>
  -- For every instance `c` of the generic body (any `T`),
  ∀ (cs : List S.RConst) (ts : List S.RTy) (c : Int),
    S.inst Mir.KernelGen.Body.«percolator::v16::MarketGroupV16ViewMut::<'a, T>::retire_terminal_unbudgeted_insurance_delta»
      cs ts c →
    -- ASSUMPTIONS about the client crate
    V16ErrorValid S → U128TripleResultValid S c →
    -- for any vault, insurance, budget remaining and source reservation,
    ∀ (vault insurance budget_remaining source_reserved_atoms : Nat),
    -- PRECONDITIONS: they are `u128` values (no `requires`),
    U128 vault → U128 insurance → U128 budget_remaining → U128 source_reserved_atoms →
    ⊢ -- then the call does not unwind (panic).
      callRetireInsuranceG S c vault insurance budget_remaining source_reserved_atoms
        (S.returns fun _ => iprop(True))

end Spec.Kernel
