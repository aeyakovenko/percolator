module

public import Spec.Kernel
public import Spec.KernelModelB
public import Spec.Rust.KernelB

/-!
# Public contracts for `V16Core::kernel_b_settlement_chunk` (engine revision `a3efa566`)

`V16Core::kernel_b_settlement_chunk(b_snap, b_rem, loss_weight, target, chunk_atoms,
endpoint_loss_atom_budget) -> V16Result<AccountBSettlementChunkV16>` of `src/v16.rs`, as compiled
in the program `Mir.KernelB` (handles in `Spec.Rust.KernelB`). Four properties, for every input
(all six parameters range over the whole of `u128`):

* KB.contract (`kernel_b_settlement_chunk_contract`): if the call returns, the result satisfies
  the `ensures` clause above the function (`b_settlement_ensures` on `Ok`, together with the
  exact reading `b_settlement_exact` of its last equation; `b_settlement_err` on `Err`).
* KB.nopanic (`kernel_b_settlement_chunk_nopanic`): no execution of the call panics (it never
  unwinds; overflow checks are on in the release profile). This says nothing about termination.
* KB.progress (`kernel_b_settlement_chunk_progress`): if the call returns `Ok(chunk)` with
  `target > b_snap`, then `chunk.delta_b >= 1`.
* KB.refusals (`kernel_b_settlement_chunk_refusals`): if the call returns, it returns
  `Err(RecoveryRequired)` exactly when `b_settlement_recovery` holds, and
  `Err(ArithmeticOverflow)` exactly when `b_settlement_overflow` holds.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. Contracts describe partial correctness: they say
what holds if a call returns and whether it may unwind. Every function the kernel calls (its three
closures, `Option::and_then`, `Option::ok_or`, `u128::checked_add`, `u128::checked_sub`,
`u128::checked_mul`, `Ord::min` on `u128`, and the `?` machinery) is covered by a proof of its
body, so no contract of a called function is assumed. `u128` division and remainder are the
language's own operators (with their divide-by-zero checks), not library calls.
-/

@[expose] public section

namespace Spec.KernelB
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (U128 okValue errValue v16ErrorValue ifReturns)

/-- The Rust value of an `AccountBSettlementChunkV16`, fields in declaration order. -/
def chunkValue {PROP : Type} [BI PROP] (S : Vocab PROP) (c : BChunk) : S.Value :=
  S.tupleVal [S.intVal c.delta_b, S.intVal c.loss, S.intVal c.new_remainder,
    S.intVal c.remaining_after]

/-- A `V16Result<AccountBSettlementChunkV16>` value that is either `Ok(chunk)` for an in-range
`chunk` with `ens chunk`, or some `Err(e)` with `E e`. -/
def ChunkResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : BChunk → Prop) (E : Nat → Prop)
    (v : S.Value) : Prop :=
  (∃ c : BChunk, c.WF ∧ v = okValue S (chunkValue S c) ∧ ens c) ∨
  (∃ e : Nat, e < 12 ∧ v = errValue S (v16ErrorValue S e) ∧ E e)

/-- The call `V16Core::kernel_b_settlement_chunk(b_snap, b_rem, loss_weight, target, chunk_atoms,
endpoint_loss_atom_budget)` with postcondition `Φ`. -/
def callBSettlementChunk {PROP : Type} [BI PROP] (S : Vocab PROP)
    (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk»
    [(S.intVal b_snap,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 0),
     (S.intVal b_rem,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 1),
     (S.intVal loss_weight,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 2),
     (S.intVal target,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 3),
     (S.intVal chunk_atoms,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 4),
     (S.intVal endpoint_loss_atom_budget,
        S.argTy Mir.KernelB.Fn.«percolator::v16::V16Core::kernel_b_settlement_chunk» 5)] Φ

/-- KB.contract: for all `u128` inputs, if `kernel_b_settlement_chunk(..)` returns, its result is
`Ok(chunk)` with the `Ok` arm of the `ensures` clause (`b_settlement_ensures`) and the exact,
overflow-free reading of its last equation (`b_settlement_exact`), or `Err(e)` with
`e == RecoveryRequired || e == ArithmeticOverflow`. -/
def kernel_b_settlement_chunk_contract : Property := fun S =>
  -- For any snapshot, remainder, weight, target, chunk size and budget,
  ∀ (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (there is no `requires`),
    U128 b_snap → U128 b_rem → U128 loss_weight → U128 target → U128 chunk_atoms →
    U128 endpoint_loss_atom_budget →
    ⊢ -- then, on normal return of the call,
      callBSettlementChunk S b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(chunk)` satisfying the `ensures` clause, with the last equation
        -- holding exactly and without overflow, or `Err(RecoveryRequired | ArithmeticOverflow)`.
        ChunkResult S (fun c =>
            b_settlement_ensures b_snap b_rem loss_weight target chunk_atoms
              endpoint_loss_atom_budget c ∧
            b_settlement_exact b_rem loss_weight c)
          b_settlement_err v)

/-- KB.nopanic: for all `u128` inputs, `kernel_b_settlement_chunk(..)` does not panic. -/
def kernel_b_settlement_chunk_nopanic : Property := fun S =>
  -- For any snapshot, remainder, weight, target, chunk size and budget,
  ∀ (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_snap → U128 b_rem → U128 loss_weight → U128 target → U128 chunk_atoms →
    U128 endpoint_loss_atom_budget →
    ⊢ -- then the call does not unwind (panic).
      callBSettlementChunk S b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget
        (S.returns fun _ => iprop(True))

/-- KB.progress: for all `u128` inputs, if `kernel_b_settlement_chunk(..)` returns `Ok(chunk)`
and `target > b_snap`, then `chunk.delta_b >= 1`. -/
def kernel_b_settlement_chunk_progress : Property := fun S =>
  -- For any snapshot, remainder, weight, target, chunk size and budget,
  ∀ (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_snap → U128 b_rem → U128 loss_weight → U128 target → U128 chunk_atoms →
    U128 endpoint_loss_atom_budget →
    ⊢ -- then, on normal return of the call,
      callBSettlementChunk S b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(chunk)` with `delta_b >= 1` whenever `target > b_snap`,
        -- or some `Err(_)`.
        ChunkResult S (fun c => b_snap < target → 1 ≤ c.delta_b) (fun _ => True) v)

/-- KB.refusals: for all `u128` inputs, if `kernel_b_settlement_chunk(..)` returns, then it
returned `Err(RecoveryRequired)` exactly when `b_settlement_recovery` holds, and
`Err(ArithmeticOverflow)` exactly when `b_settlement_overflow` holds. -/
def kernel_b_settlement_chunk_refusals : Property := fun S =>
  -- For any snapshot, remainder, weight, target, chunk size and budget,
  ∀ (b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values,
    U128 b_snap → U128 b_rem → U128 loss_weight → U128 target → U128 chunk_atoms →
    U128 endpoint_loss_atom_budget →
    ⊢ -- then, on normal return of the call,
      callBSettlementChunk S b_snap b_rem loss_weight target chunk_atoms endpoint_loss_atom_budget
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- `Err(RecoveryRequired)` exactly under `b_settlement_recovery`,
        (v = errValue S (v16ErrorValue S errRecoveryRequired) ↔
          b_settlement_recovery b_snap b_rem loss_weight target chunk_atoms
            endpoint_loss_atom_budget) ∧
        -- and `Err(ArithmeticOverflow)` exactly under `b_settlement_overflow`.
        (v = errValue S (v16ErrorValue S errArithmeticOverflow) ↔
          b_settlement_overflow b_snap loss_weight target chunk_atoms endpoint_loss_atom_budget))

end Spec.KernelB
