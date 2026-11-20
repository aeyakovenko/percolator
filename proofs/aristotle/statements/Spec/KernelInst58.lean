module

public import Spec.Rust.Inst58
public import Spec.Kernel
public import Spec.KernelModel3
public import SepLogic.Public

/-!
# Public contracts for K58 `trade_signed_size_deltas` at `T = u64`

`trade_signed_size_deltas` is an associated function of the generic
`impl<'a, T> MarketGroupV16ViewMut<'a, T>` of `src/v16.rs`. These two properties are about its
instance `MarketGroupV16ViewMut::<'_, u64>::trade_signed_size_deltas`, as compiled into a crate
that uses the library with `T = u64` (the instance reached through the public method
`execute_batch_with_fee_loss_stale_scoped_not_atomic`):

* K58.contract (`trade_signed_size_deltas_u64_contract`): for every `size_q: i128` (no
  `requires`), if the call returns, the result is `Ok((abs, long_d, short_d))` satisfying the
  `ensures` (`trade_signed_size_deltas_ensures` of `Spec.KernelModel3`), or an `Err(_)`.
* K58.nopanic (`trade_signed_size_deltas_u64_nopanic`): for every `size_q: i128`, including
  `i128::MIN`, no execution of the call panics (it never unwinds). This says nothing about
  termination.

They are about the release profile (`overflow-checks = true`, `panic = "unwind"`), default
features, no `cfg(kani)`. The constant `MAX_TRADE_SIZE_Q` is `100_000_000_000_000`.

The core library functions the body calls are not verified here; their documented behaviour, at
the arguments the body can pass them, is taken as the explicit premise
`TradeDeltasU64CoreContracts` (`i128::unsigned_abs`, `i128::checked_neg`, `Option::ok_or` on
`Some`, and `Try::branch` on `Ok`).
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Contracts of the core library functions called by `trade_signed_size_deltas` -/

/-- `i128::unsigned_abs(x)` returns `|x|` (as a `u128`). -/
def UnsignedAbsI128U64InstContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Inst58.Fn.«core::num::<impl i128>::unsigned_abs»
        [(S.intVal x, S.argTy Mir.Inst58.Fn.«core::num::<impl i128>::unsigned_abs» 0)]
        (S.returns fun w => iprop(⌊w = S.intVal (x.natAbs : Nat)⌋))

/-- `i128::checked_neg(x)` returns `Some(-x)` if `-x` is an `i128` (that is, `x != i128::MIN`),
else `None`. -/
def CheckedNegI128U64InstContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Inst58.Fn.«core::num::<impl i128>::checked_neg»
        [(S.intVal x, S.argTy Mir.Inst58.Fn.«core::num::<impl i128>::checked_neg» 0)]
        (S.returns fun w => iprop(⌊w = if x = -(2 ^ 127 : Int) then noneValue S
          else someValue S (S.intVal (-x))⌋))

/-- `Option::<i128>::ok_or(Some(x), e)` returns `Ok(x)`, for every `V16Error` `e`. (The call
`ok_or(None, e)` cannot happen in `trade_signed_size_deltas`: `checked_neg` returns `None` only for
`i128::MIN`, which is rejected earlier as larger than `MAX_TRADE_SIZE_Q`.) -/
def OkOrSomeI128U64InstContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ (x : Int) (d : Nat), I128 x → d < 12 →
    ⊢ S.call Mir.Inst58.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>»
        [(someValue S (S.intVal x),
            S.argTy Mir.Inst58.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 0),
         (v16ErrorValue S d,
            S.argTy Mir.Inst58.Fn.«core::option::Option::<i128>::ok_or::<percolator::v16::V16Error>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- The `?` operator's `<Result<i128, V16Error> as Try>::branch(Ok(x))` gives
`ControlFlow::Continue(x)`. (The `?` is only ever applied to an `Ok` here, for the same reason;
in particular `from_residual` is never called.) -/
def BranchOkI128U64InstContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Inst58.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch»
        [(okValue S (S.intVal x), S.argTy Mir.Inst58.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::Try>::branch» 0)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- The core library contracts `trade_signed_size_deltas` relies on. -/
def TradeDeltasU64CoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  UnsignedAbsI128U64InstContract S ∧ CheckedNegI128U64InstContract S ∧
    OkOrSomeI128U64InstContract S ∧ BranchOkI128U64InstContract S

/-! ## K58 `MarketGroupV16ViewMut::<'_, u64>::trade_signed_size_deltas` -/

/-- The call `MarketGroupV16ViewMut::<'_, u64>::trade_signed_size_deltas(size_q)` with
postcondition `Φ`. -/
def callTradeDeltasU64 {PROP : Type} [BI PROP] (S : Vocab PROP) (size_q : Int)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Inst58.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::trade_signed_size_deltas»
    [(S.intVal size_q,
        S.argTy Mir.Inst58.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, u64>::trade_signed_size_deltas» 0)]
    Φ

/-- K58.contract (at `T = u64`): if `trade_signed_size_deltas(size_q)` returns, it is
`Ok((abs, long_d, short_d))` satisfying its `ensures` (`trade_signed_size_deltas_ensures`), or an
`Err(_)` (`Err(_) => true`). -/
def trade_signed_size_deltas_u64_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  TradeDeltasU64CoreContracts S →
  -- for any argument,
  ∀ (size_q : Int),
    -- PRECONDITIONS
    -- if it is an `i128` (no `requires`),
    I128 size_q →
    ⊢ -- then, on normal return of the call,
      callTradeDeltasU64 S size_q (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((abs, long_d, short_d))` with the `ensures` of K58,
        (∃ (abs : Nat) (long_d short_d : Int), U128 abs ∧ I128 long_d ∧ I128 short_d ∧
          v = okValue S (S.tupleVal [S.intVal abs, S.intVal long_d, S.intVal short_d]) ∧
          trade_signed_size_deltas_ensures size_q abs long_d short_d) ∨
        -- or some `Err(_)`.
        (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d)))

/-- K58.nopanic (at `T = u64`): `trade_signed_size_deltas(size_q)` does not panic, for every
`i128` `size_q` (including `i128::MIN`). -/
def trade_signed_size_deltas_u64_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  TradeDeltasU64CoreContracts S →
  -- for any argument,
  ∀ (size_q : Int),
    -- PRECONDITIONS
    -- if it is an `i128` (no `requires`),
    I128 size_q →
    ⊢ -- then the call does not unwind (panic).
      callTradeDeltasU64 S size_q (S.returns fun _ => iprop(True))

end Spec.Kernel

end
