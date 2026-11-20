module

public import Spec.Rust.Inst57
public import Spec.Kernel
public import Spec.KernelModel3

/-!
# Public contracts for K57 `asset_restart_next_counters` at `T = u64`

`asset_restart_next_counters` is a private associated function of the generic
`impl<'a, T> MarketGroupV16ViewMut<'a, T>` of `src/v16.rs`. These two properties are about its
instance at `T = u64`, as compiled in the program of the harness crate `harness/k57_k58`
(which reaches it through the public method
`MarketGroupV16ViewMut::<u64>::restart_empty_asset_preserving_insurance_budget_not_atomic`),
whose function handles are in `Spec.Rust.Inst57`:

* `asset_restart_next_counters_u64_contract` (K57.contract): for all `u64` arguments (there is
  no `requires`), if the call returns, its result is `Ok((m, a, ase, re))` with `u64`
  components satisfying the `ensures` (`asset_restart_next_counters_ensures` of
  `Spec.KernelModel3`), or `Err(e)` for some `V16Error` `e` (the `Err(_) => true` arm).
* `asset_restart_next_counters_u64_nopanic` (K57.nopanic): for all `u64` arguments, the call
  does not panic (it never unwinds).

They are about the release build (`overflow-checks = true`, `panic = "unwind"`), and describe
partial correctness: they do not say that the call terminates.

The four standard-library functions the body calls are not verified here; their documented
behaviour, at the argument values the body passes, is an explicit premise
(`AssetRestartU64CoreContracts`).
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Contracts of the core library functions called by the `u64` instance -/

/-- `u64::checked_add(a, b)` returns `Some(a + b)` if `a + b < 2^64`, else `None`. -/
def CheckedAddU64Inst57Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Nat, U64 a → U64 b →
    ⊢ S.call Mir.Inst57.Fn.«core::num::<impl u64>::checked_add»
        [(S.intVal a, S.argTy Mir.Inst57.Fn.«core::num::<impl u64>::checked_add» 0),
         (S.intVal b, S.argTy Mir.Inst57.Fn.«core::num::<impl u64>::checked_add» 1)]
        (S.returns fun w => iprop(⌊w = if a + b < 2 ^ 64 then someValue S (S.intVal (a + b : Nat))
          else noneValue S⌋))

/-- `Option::<u64>::ok_or(Some(x), e)` returns `Ok(x)` and `ok_or(None, e)` returns `Err(e)`,
for every `V16Error` `e`. -/
def OkOrU64Inst57Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ (x d : Nat), U64 x → d < 12 →
    ⊢ S.call Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>»
        [(someValue S (S.intVal x),
            S.argTy Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>»
        [(noneValue S,
            S.argTy Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Inst57.Fn.«core::option::Option::<u64>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋)))

/-- The `?` operator's `<Result<u64, V16Error> as Try>::branch(r)`: `Ok(x)` gives
`ControlFlow::Continue(x)` and `Err(e)` gives `ControlFlow::Break(Err(e))`. -/
def BranchU64Inst57Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  (∀ x : Nat, U64 x →
    ⊢ S.call Mir.Inst57.Fn.«<core::result::Result<u64, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(okValue S (S.intVal x), S.argTy Mir.Inst57.Fn.«<core::result::Result<u64, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))) ∧
  (∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Inst57.Fn.«<core::result::Result<u64, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Inst57.Fn.«<core::result::Result<u64, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (errValue S (v16ErrorValue S d))⌋)))

/-- The `?` operator's `from_residual(Err(e))` for `V16Result<(u64, u64, u64, u64)>` returns
`Err(e)` (the conversion `From<V16Error> for V16Error` is the identity). -/
def FromResidualU64x4Inst57Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Inst57.Fn.«<core::result::Result<(u64, u64, u64, u64), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Inst57.Fn.«<core::result::Result<(u64, u64, u64, u64), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋))

/-! ## K57 `MarketGroupV16ViewMut::<u64>::asset_restart_next_counters` -/

/-- The core library contracts `asset_restart_next_counters` (at `T = u64`) relies on. -/
def AssetRestartU64CoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  CheckedAddU64Inst57Contract S ∧ OkOrU64Inst57Contract S ∧ BranchU64Inst57Contract S ∧
    FromResidualU64x4Inst57Contract S

/-- A `V16Result<(u64, u64, u64, u64)>` value that is either `Ok((m, a, ase, re))` for `u64`
components with `ens m a ase re`, or some `Err(e)` (the `Err(_) => true` arm). -/
def U64x4Result {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Nat → Nat → Nat → Nat → Prop)
    (v : S.Value) : Prop :=
  (∃ m a ase re : Nat, U64 m ∧ U64 a ∧ U64 ase ∧ U64 re ∧
    v = okValue S (S.tupleVal [S.intVal m, S.intVal a, S.intVal ase, S.intVal re]) ∧
    ens m a ase re) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- The call `MarketGroupV16ViewMut::<u64>::asset_restart_next_counters(next_market_id_before,
activation_count_before, asset_set_epoch_before, risk_epoch_before)` with postcondition `Φ`. -/
def callAssetRestartU64 {PROP : Type} [BI PROP] (S : Vocab PROP)
    (next_market_id_before activation_count_before asset_set_epoch_before risk_epoch_before : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst57.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::asset_restart_next_counters»
    [(S.intVal next_market_id_before, S.argTy Mir.Inst57.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::asset_restart_next_counters» 0),
     (S.intVal activation_count_before, S.argTy Mir.Inst57.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::asset_restart_next_counters» 1),
     (S.intVal asset_set_epoch_before, S.argTy Mir.Inst57.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::asset_restart_next_counters» 2),
     (S.intVal risk_epoch_before, S.argTy Mir.Inst57.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::asset_restart_next_counters» 3)] Φ

/-- K57.contract at `T = u64`: if `asset_restart_next_counters(next_market_id_before,
activation_count_before, asset_set_epoch_before, risk_epoch_before)` returns, its result is
`Ok((m, a, ase, re))` with the `ensures` (`asset_restart_next_counters_ensures`: each component
is the corresponding argument `.wrapping_add(1)`), or an `Err(_)`. -/
def asset_restart_next_counters_u64_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  AssetRestartU64CoreContracts S →
  -- for any arguments,
  ∀ (next_market_id_before activation_count_before asset_set_epoch_before
      risk_epoch_before : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 next_market_id_before → U64 activation_count_before → U64 asset_set_epoch_before →
    U64 risk_epoch_before →
    ⊢ -- then, on normal return of the call,
      callAssetRestartU64 S next_market_id_before activation_count_before asset_set_epoch_before
        risk_epoch_before
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the `Ok` arm (with the `ensures` of K57) or the `Err(_)` arm holds.
          U64x4Result S
            (asset_restart_next_counters_ensures next_market_id_before activation_count_before
              asset_set_epoch_before risk_epoch_before) v)

/-- K57.nopanic at `T = u64`: `asset_restart_next_counters(..)` does not panic. -/
def asset_restart_next_counters_u64_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  AssetRestartU64CoreContracts S →
  -- for any arguments,
  ∀ (next_market_id_before activation_count_before asset_set_epoch_before
      risk_epoch_before : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 next_market_id_before → U64 activation_count_before → U64 asset_set_epoch_before →
    U64 risk_epoch_before →
    ⊢ -- then the call does not unwind (panic).
      callAssetRestartU64 S next_market_id_before activation_count_before asset_set_epoch_before
        risk_epoch_before (S.returns fun _ => iprop(True))

end Spec.Kernel
