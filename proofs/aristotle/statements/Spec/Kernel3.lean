module

public import Spec.Kernel2
public import Spec.KernelModel3

/-!
# Public contracts for nine more `V16Core` kernels of `src/v16.rs`

For each of K22 `kernel_cert_is_current`, K23 `actionable_summary_from_signals`,
K24 `kernel_open_close_snapshot_is_stale`, K25 `kernel_social_loss_chunk_cap`,
K27 `kernel_bresidual_step`, K33 `kernel_recovery_pending_obligation_release_allowed`,
K16 `kernel_classify_position_delta`, K17 `kernel_trade_side_oi_cap_gate` and
K18 `kernel_reduce_position_delta`, two properties of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModel3`, in
  which parameter names denote the values passed by the caller).
* `Kxx.nopanic` (`*_nopanic` below): for all inputs satisfying the `requires`, no execution of
  the call panics (it never unwinds; overflow checks are on in the release profile). This says
  nothing about termination.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`.

The core library functions that a kernel calls and whose bodies are not verified here are
covered by explicit contracts (from `Spec.Kernel` or below), which the property takes as
premises.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## K22 `kernel_cert_is_current` -/

/-- The call `V16Core::kernel_cert_is_current(cert, oracle_epoch, funding_epoch, risk_epoch,
asset_set_epoch, account_bitmap)` with postcondition `Φ`; `account_bitmap: [u64; 1]` is the
one-element array of the word `account_bitmap`. -/
def callCertIsCurrent {PROP : Type} [BI PROP] (S : Vocab PROP) (cert : HealthCert)
    (oracle_epoch funding_epoch risk_epoch asset_set_epoch account_bitmap : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current»
    [(healthCertValue S cert,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 0),
     (S.intVal oracle_epoch,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 1),
     (S.intVal funding_epoch,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 2),
     (S.intVal risk_epoch,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 3),
     (S.intVal asset_set_epoch,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 4),
     (S.tupleVal [S.intVal account_bitmap],
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_cert_is_current» 5)] Φ

/-- K22.contract: if `kernel_cert_is_current(..)` returns, its result is the `bool` given by its
`ensures` (`cert_is_current_ensures`). -/
def kernel_cert_is_current_contract : Property := fun S =>
  -- For any certificate, epochs and bitmap word,
  ∀ (cert : HealthCert)
    (oracle_epoch funding_epoch risk_epoch asset_set_epoch account_bitmap : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    cert.WF → U64 oracle_epoch → U64 funding_epoch → U64 risk_epoch →
    U64 asset_set_epoch → U64 account_bitmap →
    ⊢ -- then, on normal return of the call,
      callCertIsCurrent S cert oracle_epoch funding_epoch risk_epoch asset_set_epoch
        account_bitmap (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a `bool` satisfying the `ensures` of K22.
        ∃ b : Bool, v = S.boolVal b ∧
          cert_is_current_ensures cert oracle_epoch funding_epoch risk_epoch asset_set_epoch
            account_bitmap b)

/-- K22.nopanic: `kernel_cert_is_current(..)` does not panic. -/
def kernel_cert_is_current_nopanic : Property := fun S =>
  -- For any certificate, epochs and bitmap word,
  ∀ (cert : HealthCert)
    (oracle_epoch funding_epoch risk_epoch asset_set_epoch account_bitmap : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    cert.WF → U64 oracle_epoch → U64 funding_epoch → U64 risk_epoch →
    U64 asset_set_epoch → U64 account_bitmap →
    ⊢ -- then the call does not unwind (panic).
      callCertIsCurrent S cert oracle_epoch funding_epoch risk_epoch asset_set_epoch
        account_bitmap (S.returns fun _ => iprop(True))

/-! ## K23 `actionable_summary_from_signals` -/

/-- The Rust value of an `ActionableSummaryV16`, fields in declaration order. -/
def actionableSummaryValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : ActionableSummary) :
    S.Value :=
  S.tupleVal [S.boolVal r.stale, S.boolVal r.b_stale, S.boolVal r.pending_close,
    S.boolVal r.expired_close, S.boolVal r.liquidatable, S.boolVal r.source_liens_releasable,
    S.boolVal r.recovery_eligible, S.boolVal r.resolved_winner]

/-- The call `V16Core::actionable_summary_from_signals(stale, b_stale, pending_close,
expired_close, liquidatable, source_liens_releasable, recovery_eligible, resolved_winner)` with
postcondition `Φ`. -/
def callActionableSummary {PROP : Type} [BI PROP] (S : Vocab PROP)
    (stale b_stale pending_close expired_close liquidatable source_liens_releasable
      recovery_eligible resolved_winner : Bool) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals»
    [(S.boolVal stale,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 0),
     (S.boolVal b_stale,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 1),
     (S.boolVal pending_close,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 2),
     (S.boolVal expired_close,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 3),
     (S.boolVal liquidatable,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 4),
     (S.boolVal source_liens_releasable,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 5),
     (S.boolVal recovery_eligible,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 6),
     (S.boolVal resolved_winner,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::actionable_summary_from_signals» 7)] Φ

/-- K23.contract: if `actionable_summary_from_signals(..)` returns `r`, every flag of `r` equals
the corresponding signal (its `ensures`). -/
def actionable_summary_from_signals_contract : Property := fun S =>
  -- For any eight signals (no `requires`),
  ∀ (stale b_stale pending_close expired_close liquidatable source_liens_releasable
      recovery_eligible resolved_winner : Bool),
    ⊢ -- on normal return of the call,
      callActionableSummary S stale b_stale pending_close expired_close liquidatable
        source_liens_releasable recovery_eligible resolved_winner (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a summary `r` satisfying the `ensures` of K23.
        ∃ r : ActionableSummary, v = actionableSummaryValue S r ∧
          actionable_summary_from_signals_ensures stale b_stale pending_close expired_close
            liquidatable source_liens_releasable recovery_eligible resolved_winner r)

/-- K23.nopanic: `actionable_summary_from_signals(..)` does not panic. -/
def actionable_summary_from_signals_nopanic : Property := fun S =>
  -- For any eight signals (no `requires`),
  ∀ (stale b_stale pending_close expired_close liquidatable source_liens_releasable
      recovery_eligible resolved_winner : Bool),
    ⊢ -- the call does not unwind (panic).
      callActionableSummary S stale b_stale pending_close expired_close liquidatable
        source_liens_releasable recovery_eligible resolved_winner (S.returns fun _ => iprop(True))

/-! ## K24 `kernel_open_close_snapshot_is_stale` -/

/-- The call `V16Core::kernel_open_close_snapshot_is_stale(originating_leg_active,
originating_asset_slot, drift_reference_slot)` with postcondition `Φ`. -/
def callSnapshotIsStale {PROP : Type} [BI PROP] (S : Vocab PROP) (originating_leg_active : Bool)
    (originating_asset_slot drift_reference_slot : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_open_close_snapshot_is_stale»
    [(S.boolVal originating_leg_active,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_open_close_snapshot_is_stale» 0),
     (S.intVal originating_asset_slot,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_open_close_snapshot_is_stale» 1),
     (S.intVal drift_reference_slot,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_open_close_snapshot_is_stale» 2)]
    Φ

/-- K24.contract: if `kernel_open_close_snapshot_is_stale(..)` returns, its result is
`originating_leg_active && originating_asset_slot > drift_reference_slot`. -/
def kernel_open_close_snapshot_is_stale_contract : Property := fun S =>
  -- For any arguments,
  ∀ (originating_leg_active : Bool) (originating_asset_slot drift_reference_slot : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 originating_asset_slot → U64 drift_reference_slot →
    ⊢ -- then, on normal return of the call,
      callSnapshotIsStale S originating_leg_active originating_asset_slot drift_reference_slot
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a `bool` satisfying the `ensures` of K24.
        ∃ b : Bool, v = S.boolVal b ∧
          open_close_snapshot_is_stale_ensures originating_leg_active originating_asset_slot
            drift_reference_slot b)

/-- K24.nopanic: `kernel_open_close_snapshot_is_stale(..)` does not panic. -/
def kernel_open_close_snapshot_is_stale_nopanic : Property := fun S =>
  -- For any arguments,
  ∀ (originating_leg_active : Bool) (originating_asset_slot drift_reference_slot : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 originating_asset_slot → U64 drift_reference_slot →
    ⊢ -- then the call does not unwind (panic).
      callSnapshotIsStale S originating_leg_active originating_asset_slot drift_reference_slot
        (S.returns fun _ => iprop(True))

/-! ## K25 `kernel_social_loss_chunk_cap` -/

/-- The call `V16Core::kernel_social_loss_chunk_cap(residual_remaining, public_chunk_cap)` with
postcondition `Φ`. -/
def callChunkCap {PROP : Type} [BI PROP] (S : Vocab PROP)
    (residual_remaining public_chunk_cap : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_social_loss_chunk_cap»
    [(S.intVal residual_remaining,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_social_loss_chunk_cap» 0),
     (S.intVal public_chunk_cap,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_social_loss_chunk_cap» 1)] Φ

/-- K25.contract: if `kernel_social_loss_chunk_cap(residual_remaining, public_chunk_cap)`
returns, its result is the smaller of the two, and at most each of them (its `ensures`). -/
def kernel_social_loss_chunk_cap_contract : Property := fun S =>
  -- ASSUMED: the contract of `<u128 as Ord>::min`,
  MinU128Contract S →
  -- for any arguments,
  ∀ (residual_remaining public_chunk_cap : Nat),
    -- PRECONDITIONS
    -- if they are in range of `u128` (no `requires`),
    U128 residual_remaining → U128 public_chunk_cap →
    ⊢ -- then, on normal return of the call,
      callChunkCap S residual_remaining public_chunk_cap (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a `u128` satisfying the `ensures` of K25.
        ∃ r : Nat, U128 r ∧ v = S.intVal r ∧
          social_loss_chunk_cap_ensures residual_remaining public_chunk_cap r)

/-- K25.nopanic: `kernel_social_loss_chunk_cap(..)` does not panic. -/
def kernel_social_loss_chunk_cap_nopanic : Property := fun S =>
  -- ASSUMED: the contract of `<u128 as Ord>::min`,
  MinU128Contract S →
  -- for any arguments,
  ∀ (residual_remaining public_chunk_cap : Nat),
    -- PRECONDITIONS
    -- if they are in range of `u128` (no `requires`),
    U128 residual_remaining → U128 public_chunk_cap →
    ⊢ -- then the call does not unwind (panic).
      callChunkCap S residual_remaining public_chunk_cap (S.returns fun _ => iprop(True))

/-! ## K27 `kernel_bresidual_step` -/

/-- The Rust value of a `BResidualBookingOutcomeV16`, fields in declaration order. -/
def bresidualOutcomeValue {PROP : Type} [BI PROP] (S : Vocab PROP) (o : BResidualOutcome) :
    S.Value :=
  S.tupleVal [S.intVal o.booked_loss, S.intVal o.explicit_loss, S.intVal o.delta_b,
    S.intVal o.remaining_after]

/-- The Rust value of an `Option<BResidualBookingOutcomeV16>`. -/
def bookedValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Option BResidualOutcome → S.Value
  | none => noneValue S
  | some o => someValue S (bresidualOutcomeValue S o)

/-- The Rust value of a `BResidualStepV16` (`Outcome(o)` has discriminant `0`,
`DeclareRecovery` discriminant `1`). -/
def bresidualStepValue {PROP : Type} [BI PROP] (S : Vocab PROP) : BResidualStep → S.Value
  | .Outcome o => S.variantVal 0 (S.tupleVal [bresidualOutcomeValue S o])
  | .DeclareRecovery => S.variantVal 1 (S.tupleVal [])

/-- Every field of an optional outcome is in range of its Rust type. -/
def BookedWF : Option BResidualOutcome → Prop
  | none => True
  | some o => o.WF

/-- The call `V16Core::kernel_bresidual_step(residual_remaining, booked, resolved)` with
postcondition `Φ`. -/
def callBResidualStep {PROP : Type} [BI PROP] (S : Vocab PROP) (residual_remaining : Nat)
    (booked : Option BResidualOutcome) (resolved : Bool) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_bresidual_step»
    [(S.intVal residual_remaining,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_bresidual_step» 0),
     (bookedValue S booked,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_bresidual_step» 1),
     (S.boolVal resolved,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_bresidual_step» 2)] Φ

/-- K27.contract: under its `requires` (a booked outcome conserves `residual_remaining`), if
`kernel_bresidual_step(residual_remaining, booked, resolved)` returns a step, every `Outcome(o)`
step conserves `residual_remaining` (its `ensures`). -/
def kernel_bresidual_step_contract : Property := fun S =>
  -- For any arguments,
  ∀ (residual_remaining : Nat) (booked : Option BResidualOutcome) (resolved : Bool),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 residual_remaining → BookedWF booked →
    -- and they satisfy the `requires`,
    bresidual_step_requires residual_remaining booked →
    ⊢ -- then, on normal return of the call,
      callBResidualStep S residual_remaining booked resolved (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a step satisfying the `ensures` of K27.
        ∃ r : BResidualStep, v = bresidualStepValue S r ∧
          bresidual_step_ensures residual_remaining booked resolved r)

/-- K27.nopanic: under its `requires`, `kernel_bresidual_step(..)` does not panic. -/
def kernel_bresidual_step_nopanic : Property := fun S =>
  -- For any arguments,
  ∀ (residual_remaining : Nat) (booked : Option BResidualOutcome) (resolved : Bool),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 residual_remaining → BookedWF booked →
    -- and they satisfy the `requires`,
    bresidual_step_requires residual_remaining booked →
    ⊢ -- then the call does not unwind (panic).
      callBResidualStep S residual_remaining booked resolved (S.returns fun _ => iprop(True))

/-! ## K33 `kernel_recovery_pending_obligation_release_allowed` -/

/-- The Rust discriminant of an `AssetLifecycleV16` (`Disabled = 0`, ..., `Recovery = 5`). -/
def lifecycleDisc : AssetLifecycle → Int
  | .Disabled => 0
  | .PendingActivation => 1
  | .Active => 2
  | .DrainOnly => 3
  | .Retired => 4
  | .Recovery => 5

/-- The Rust value of an `AssetLifecycleV16`. -/
def lifecycleValue {PROP : Type} [BI PROP] (S : Vocab PROP) (l : AssetLifecycle) : S.Value :=
  S.variantVal (lifecycleDisc l) S.unitVal

/-- The bytes of the promoted constant `&AssetLifecycleV16::Recovery` that
`kernel_recovery_pending_obligation_release_allowed` compares `lifecycle` with: the single
discriminant byte `5`. -/
def recoveryConstant {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  S.global Mir.Kernel.Global.«const [1 bytes] 165f13f0fda1f245» 1 [5]

/-- The call `V16Core::kernel_recovery_pending_obligation_release_allowed(lifecycle,
opposite_stored_count, opposite_pending_count)` with postcondition `Φ`. -/
def callReleaseAllowed {PROP : Type} [BI PROP] (S : Vocab PROP) (lifecycle : AssetLifecycle)
    (opposite_stored_count opposite_pending_count : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_recovery_pending_obligation_release_allowed»
    [(lifecycleValue S lifecycle, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_recovery_pending_obligation_release_allowed» 0),
     (S.intVal opposite_stored_count, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_recovery_pending_obligation_release_allowed» 1),
     (S.intVal opposite_pending_count, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_recovery_pending_obligation_release_allowed» 2)]
    Φ

/-- K33.contract: if `kernel_recovery_pending_obligation_release_allowed(..)` returns, it is
`Ok(allowed)` with `allowed == (lifecycle != Recovery || opposite_stored_count ==
opposite_pending_count)`, or `Err(_)` with `opposite_pending_count > opposite_stored_count`. -/
def kernel_recovery_pending_obligation_release_allowed_contract : Property := fun S =>
  -- For any arguments,
  ∀ (lifecycle : AssetLifecycle) (opposite_stored_count opposite_pending_count : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 opposite_stored_count → U64 opposite_pending_count →
    -- and the promoted constant `&AssetLifecycleV16::Recovery` is in read-only memory,
    recoveryConstant S ⊢
      -- then, on normal return of the call,
      callReleaseAllowed S lifecycle opposite_stored_count opposite_pending_count
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(allowed)` with the `Ok` arm of the `ensures`,
        (∃ allowed : Bool, v = okValue S (S.boolVal allowed) ∧
          recovery_pending_obligation_release_allowed_ensures lifecycle opposite_stored_count
            opposite_pending_count allowed) ∨
        -- or some `Err(_)` with the `Err` arm of the `ensures`.
        (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧
          recovery_pending_obligation_release_allowed_err lifecycle opposite_stored_count
            opposite_pending_count))

/-- K33.nopanic: `kernel_recovery_pending_obligation_release_allowed(..)` does not panic. -/
def kernel_recovery_pending_obligation_release_allowed_nopanic : Property := fun S =>
  -- For any arguments,
  ∀ (lifecycle : AssetLifecycle) (opposite_stored_count opposite_pending_count : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 opposite_stored_count → U64 opposite_pending_count →
    -- and the promoted constant `&AssetLifecycleV16::Recovery` is in read-only memory,
    recoveryConstant S ⊢
      -- then the call does not unwind (panic).
      callReleaseAllowed S lifecycle opposite_stored_count opposite_pending_count
        (S.returns fun _ => iprop(True))

/-! ## K16 `kernel_classify_position_delta` -/

/-- The Rust discriminant of a `PositionRouteV16` (`Attach = 0`, `Clear = 1`, `Flip = 2`,
`Resize = 3`). -/
def positionRouteDisc : PositionRoute → Int
  | .Attach => 0
  | .Clear => 1
  | .Flip => 2
  | .Resize => 3

/-- The call `V16Core::kernel_classify_position_delta(current, new)` with postcondition `Φ`. -/
def callClassifyPosition {PROP : Type} [BI PROP] (S : Vocab PROP) (current new : Int)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_classify_position_delta»
    [(S.intVal current,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_classify_position_delta» 0),
     (S.intVal new,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_classify_position_delta» 1)] Φ

/-- K16.contract: if `kernel_classify_position_delta(current, new)` returns a route, the route
satisfies its `ensures` (`classify_position_delta_ensures`). -/
def kernel_classify_position_delta_contract : Property := fun S =>
  -- For any positions,
  ∀ (current new : Int),
    -- PRECONDITIONS
    -- if they are in range of `i128` (no `requires`),
    I128 current → I128 new →
    ⊢ -- then, on normal return of the call,
      callClassifyPosition S current new (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a route satisfying the `ensures` of K16.
        ∃ r : PositionRoute, v = S.variantVal (positionRouteDisc r) S.unitVal ∧
          classify_position_delta_ensures current new r)

/-- K16.nopanic: `kernel_classify_position_delta(current, new)` does not panic. -/
def kernel_classify_position_delta_nopanic : Property := fun S =>
  -- For any positions,
  ∀ (current new : Int),
    -- PRECONDITIONS
    -- if they are in range of `i128` (no `requires`),
    I128 current → I128 new →
    ⊢ -- then the call does not unwind (panic).
      callClassifyPosition S current new (S.returns fun _ => iprop(True))

/-! ## K17 `kernel_trade_side_oi_cap_gate` -/

/-- The call `V16Core::kernel_trade_side_oi_cap_gate(oi_long_before_q, oi_short_before_q,
oi_long_after_q, oi_short_after_q)` with postcondition `Φ`. -/
def callOiCapGate {PROP : Type} [BI PROP] (S : Vocab PROP)
    (oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate»
    [(S.intVal oi_long_before_q,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate» 0),
     (S.intVal oi_short_before_q,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate» 1),
     (S.intVal oi_long_after_q,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate» 2),
     (S.intVal oi_short_after_q,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate» 3)] Φ

/-- K17.contract: if `kernel_trade_side_oi_cap_gate(..)` returns, it is `Ok(())` with neither side
grown over `MAX_OI_SIDE_Q`, or `Err(V16Error::InvalidLeg)` with some side grown over it (its
`ensures`). -/
def kernel_trade_side_oi_cap_gate_contract : Property := fun S =>
  -- For any side OI values,
  ∀ (oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of `u128` (no `requires`),
    U128 oi_long_before_q → U128 oi_short_before_q → U128 oi_long_after_q →
    U128 oi_short_after_q →
    ⊢ -- then, on normal return of the call,
      callOiCapGate S oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q
        (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(())` with the `Ok` arm of the `ensures`,
        (v = okValue S (S.tupleVal []) ∧
          trade_side_oi_cap_gate_ensures oi_long_before_q oi_short_before_q oi_long_after_q
            oi_short_after_q) ∨
        -- or `Err(e)` with the `Err` arm (in particular `e == V16Error::InvalidLeg`).
        (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧
          trade_side_oi_cap_gate_err oi_long_before_q oi_short_before_q oi_long_after_q
            oi_short_after_q d))

/-- K17.nopanic: `kernel_trade_side_oi_cap_gate(..)` does not panic. -/
def kernel_trade_side_oi_cap_gate_nopanic : Property := fun S =>
  -- For any side OI values,
  ∀ (oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of `u128` (no `requires`),
    U128 oi_long_before_q → U128 oi_short_before_q → U128 oi_long_after_q →
    U128 oi_short_after_q →
    ⊢ -- then the call does not unwind (panic).
      callOiCapGate S oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q
        (S.returns fun _ => iprop(True))

/-! ## K18 `kernel_reduce_position_delta` -/

/-- The Rust discriminant of a `SideV16` (`Long = 0`, `Short = 1`). -/
def sideDisc : Side → Int
  | .Long => 0
  | .Short => 1

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` of `kernel_reduce_position_delta`, returns `Ok(x)`. -/
def MapErrReducePositionContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_reduce_position_delta::{closure#0}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_reduce_position_delta::{closure#0}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_reduce_position_delta::{closure#0}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- `i128::checked_neg(x)` returns `Some(-x)` if `-x` is an `i128` (that is, `x != i128::MIN`),
else `None`. -/
def CheckedNegI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::num::<impl i128>::checked_neg»
        [(S.intVal x, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::checked_neg» 0)]
        (S.returns fun w => iprop(⌊w = if x = -(2 ^ 127 : Int) then noneValue S
          else someValue S (S.intVal (-x))⌋))

/-- The core library contracts `kernel_reduce_position_delta` relies on. -/
def ReducePositionCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  UnsignedAbsI128Contract S ∧ MinU128Contract S ∧ TryFromU128I128Contract S ∧
    MapErrReducePositionContract S ∧ BranchI128Contract S ∧ CheckedNegI128Contract S ∧
    OkOrI128Contract S

/-- The call `V16Core::kernel_reduce_position_delta(pre_basis_signed, side, requested)` with
postcondition `Φ`. -/
def callReducePosition {PROP : Type} [BI PROP] (S : Vocab PROP) (pre_basis_signed : Int)
    (side : Side) (requested : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_reduce_position_delta»
    [(S.intVal pre_basis_signed,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_reduce_position_delta» 0),
     (S.variantVal (sideDisc side) S.unitVal,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_reduce_position_delta» 1),
     (S.intVal requested,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_reduce_position_delta» 2)] Φ

/-- K18.contract: under its `requires`, if `kernel_reduce_position_delta(pre_basis_signed, side,
requested)` returns, it is `Ok((reduce_q, delta))` satisfying its `ensures`
(`reduce_position_delta_ensures`), or an `Err(_)` (`Err(_) => true`). -/
def kernel_reduce_position_delta_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  ReducePositionCoreContracts S →
  -- for any arguments,
  ∀ (pre_basis_signed : Int) (side : Side) (requested : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    I128 pre_basis_signed → U128 requested →
    -- and they satisfy the `requires`,
    reduce_position_delta_requires pre_basis_signed side →
    ⊢ -- then, on normal return of the call,
      callReducePosition S pre_basis_signed side requested (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((reduce_q, delta))` with the `ensures` of K18,
        (∃ (reduce_q : Nat) (delta : Int), U128 reduce_q ∧ I128 delta ∧
          v = okValue S (S.tupleVal [S.intVal reduce_q, S.intVal delta]) ∧
          reduce_position_delta_ensures pre_basis_signed side requested reduce_q delta) ∨
        -- or some `Err(_)`.
        (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d)))

/-- K18.nopanic: under its `requires`, `kernel_reduce_position_delta(..)` does not panic. -/
def kernel_reduce_position_delta_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  ReducePositionCoreContracts S →
  -- for any arguments,
  ∀ (pre_basis_signed : Int) (side : Side) (requested : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    I128 pre_basis_signed → U128 requested →
    -- and they satisfy the `requires`,
    reduce_position_delta_requires pre_basis_signed side →
    ⊢ -- then the call does not unwind (panic).
      callReducePosition S pre_basis_signed side requested (S.returns fun _ => iprop(True))

end Spec.Kernel

end
