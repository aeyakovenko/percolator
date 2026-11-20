module

public import Spec.KernelModel2

/-!
# Independent mathematical model for twelve more kernel contracts

A mathematical reading of the `kani::requires` and `kani::ensures` clauses of these functions
of `src/v16.rs`:

* K22 `V16Core::kernel_cert_is_current`
* K23 `V16Core::actionable_summary_from_signals`
* K24 `V16Core::kernel_open_close_snapshot_is_stale`
* K25 `V16Core::kernel_social_loss_chunk_cap`
* K27 `V16Core::kernel_bresidual_step`
* K33 `V16Core::kernel_recovery_pending_obligation_release_allowed`
* K57 `MarketGroupV16ViewMut::asset_restart_next_counters`
* K58 `trade_signed_size_deltas`
* K16 `V16Core::kernel_classify_position_delta`
* K17 `V16Core::kernel_trade_side_oi_cap_gate`
* K18 `V16Core::kernel_reduce_position_delta`

Conventions (as in `Spec.KernelModel` and `Spec.KernelModel2`):

* Rust unsigned integers are modelled by `Nat`, `i128` by `Int`, `bool` by `Bool`; `U64`,
  `U128`, `I128` say a value is representable in its Rust type.
* `wrapping_*`, `checked_add`, `signum` and `unsigned_abs` are the functions defined below (or
  in the imported models), with their Rust meaning.
* Plain `+` / `-` inside an `ensures` clause are read as exact integer arithmetic. Each such
  term is evaluated (because of `&&` short-circuiting) only where it does not overflow.
* Parameter names denote the values the caller passed. For functions returning `V16Result<T>`,
  `<name>_ensures` constrains the `Ok` payload and `<name>_err` (when the clause is not `true`)
  the `Err` case.
* A `V16Error` is identified by its Rust discriminant (`InvalidConfig = 0`, ...,
  `CounterUnderflow = 11`), as in `Spec.Kernel`.
-/

@[expose] public section

namespace Spec.Kernel

/-! ## Shared definitions -/

/-- Rust `a.wrapping_add(b)` on `u64`: addition modulo `2^64`. -/
def wrapAdd64 (a b : Nat) : Nat := (a + b) % two64

/-- Rust `a.checked_add(b)` on `u128` (for `u128` arguments): `Some(a + b)` exactly when
`a + b < 2^128`, otherwise `None`. -/
def checkedAdd128 (a b : Nat) : Option Nat := if a + b < two128 then some (a + b) else none

/-- Rust `x.signum()` on `i128`: `-1`, `0` or `1`. -/
def signumI128 (x : Int) : Int := if x < 0 then -1 else if x = 0 then 0 else 1

/-- `MAX_POSITION_ABS_Q` (`src/lib.rs`): `100_000_000_000_000`. -/
def MAX_POSITION_ABS_Q : Nat := 100000000000000

/-- `MAX_TRADE_SIZE_Q` (`src/lib.rs`): `MAX_POSITION_ABS_Q`. -/
def MAX_TRADE_SIZE_Q : Nat := MAX_POSITION_ABS_Q

/-- `MAX_OI_SIDE_Q` (`src/lib.rs`): `100_000_000_000_000`. -/
def MAX_OI_SIDE_Q : Nat := 100000000000000

/-- Rust `enum V16Error` (variants in declaration order). -/
inductive V16Error where
  | InvalidConfig
  | ArithmeticOverflow
  | ProvenanceMismatch
  | HiddenLeg
  | InvalidLeg
  | Stale
  | BStale
  | LockActive
  | NonProgress
  | RecoveryRequired
  | CounterOverflow
  | CounterUnderflow
  deriving DecidableEq, Repr

/-- The Rust discriminant of a `V16Error` (its position in declaration order). -/
def V16Error.disc : V16Error → Nat
  | .InvalidConfig => 0
  | .ArithmeticOverflow => 1
  | .ProvenanceMismatch => 2
  | .HiddenLeg => 3
  | .InvalidLeg => 4
  | .Stale => 5
  | .BStale => 6
  | .LockActive => 7
  | .NonProgress => 8
  | .RecoveryRequired => 9
  | .CounterOverflow => 10
  | .CounterUnderflow => 11

/-- Rust `enum SideV16 { Long, Short }`. -/
inductive Side where
  | Long
  | Short
  deriving DecidableEq, Repr

/-! ## K22 `kernel_cert_is_current(cert: HealthCertV16, oracle_epoch: u64, funding_epoch: u64,
risk_epoch: u64, asset_set_epoch: u64, account_bitmap: V16ActiveBitmap)`

`V16ActiveBitmap` is `[u64; 1]`, modelled (as in `HealthCert`) by its only word. -/

/-- K22 `ensures`: `*result == (cert.valid && cert.cert_oracle_epoch == oracle_epoch &&
cert.cert_funding_epoch == funding_epoch && cert.cert_risk_epoch == risk_epoch &&
cert.cert_asset_set_epoch == asset_set_epoch && cert.active_bitmap_at_cert == account_bitmap)`. -/
def cert_is_current_ensures (cert : HealthCert)
    (oracle_epoch funding_epoch risk_epoch asset_set_epoch account_bitmap : Nat) (result : Bool) :
    Prop :=
  (result = true ↔
    (cert.valid = true ∧ cert.cert_oracle_epoch = oracle_epoch ∧
      cert.cert_funding_epoch = funding_epoch ∧ cert.cert_risk_epoch = risk_epoch ∧
      cert.cert_asset_set_epoch = asset_set_epoch ∧ cert.active_bitmap_word = account_bitmap))

/-! ## K23 `actionable_summary_from_signals(stale, b_stale, pending_close, expired_close,
liquidatable, source_liens_releasable, recovery_eligible, resolved_winner: bool)` -/

/-- Rust `struct ActionableSummaryV16` (all fields, in declaration order). -/
structure ActionableSummary where
  /-- `stale: bool` (A1) -/
  stale : Bool
  /-- `b_stale: bool` (A2) -/
  b_stale : Bool
  /-- `pending_close: bool` (A3) -/
  pending_close : Bool
  /-- `expired_close: bool` (A4) -/
  expired_close : Bool
  /-- `liquidatable: bool` (A5) -/
  liquidatable : Bool
  /-- `source_liens_releasable: bool` (A6) -/
  source_liens_releasable : Bool
  /-- `recovery_eligible: bool` (A7) -/
  recovery_eligible : Bool
  /-- `resolved_winner: bool` (A8) -/
  resolved_winner : Bool
  deriving DecidableEq, Repr

/-- K23 `ensures`: `r.stale == stale && r.b_stale == b_stale && r.pending_close ==
pending_close && r.expired_close == expired_close && r.liquidatable == liquidatable &&
r.source_liens_releasable == source_liens_releasable && r.recovery_eligible ==
recovery_eligible && r.resolved_winner == resolved_winner`. -/
def actionable_summary_from_signals_ensures (stale b_stale pending_close expired_close
    liquidatable source_liens_releasable recovery_eligible resolved_winner : Bool)
    (r : ActionableSummary) : Prop :=
  r.stale = stale ∧ r.b_stale = b_stale ∧ r.pending_close = pending_close ∧
  r.expired_close = expired_close ∧ r.liquidatable = liquidatable ∧
  r.source_liens_releasable = source_liens_releasable ∧
  r.recovery_eligible = recovery_eligible ∧ r.resolved_winner = resolved_winner

/-! ## K24 `kernel_open_close_snapshot_is_stale(originating_leg_active: bool,
originating_asset_slot: u64, drift_reference_slot: u64)` -/

/-- K24 `ensures`: `*result == (originating_leg_active && originating_asset_slot >
drift_reference_slot)`. -/
def open_close_snapshot_is_stale_ensures (originating_leg_active : Bool)
    (originating_asset_slot drift_reference_slot : Nat) (result : Bool) : Prop :=
  (result = true ↔ (originating_leg_active = true ∧ originating_asset_slot > drift_reference_slot))

/-! ## K25 `kernel_social_loss_chunk_cap(residual_remaining: u128, public_chunk_cap: u128)` -/

/-- K25 `ensures`: `*result == if residual_remaining < public_chunk_cap { residual_remaining }
else { public_chunk_cap } && *result <= residual_remaining && *result <= public_chunk_cap`. -/
def social_loss_chunk_cap_ensures (residual_remaining public_chunk_cap result : Nat) : Prop :=
  result = (if residual_remaining < public_chunk_cap then residual_remaining
    else public_chunk_cap) ∧
  result ≤ residual_remaining ∧
  result ≤ public_chunk_cap

/-! ## K27 `kernel_bresidual_step(residual_remaining: u128,
booked: Option<BResidualBookingOutcomeV16>, resolved: bool)` -/

/-- Rust `struct BResidualBookingOutcomeV16` (`#[repr(C)]`; all fields, in declaration
order). -/
structure BResidualOutcome where
  /-- `booked_loss: u128` -/
  booked_loss : Nat
  /-- `explicit_loss: u128` -/
  explicit_loss : Nat
  /-- `delta_b: u128` -/
  delta_b : Nat
  /-- `remaining_after: u128` -/
  remaining_after : Nat
  deriving DecidableEq, Repr

/-- Every field of a `BResidualBookingOutcomeV16` value is in range of its Rust type. -/
def BResidualOutcome.WF (o : BResidualOutcome) : Prop :=
  U128 o.booked_loss ∧ U128 o.explicit_loss ∧ U128 o.delta_b ∧ U128 o.remaining_after

/-- Rust `enum BResidualStepV16 { Outcome(BResidualBookingOutcomeV16), DeclareRecovery }`. -/
inductive BResidualStep where
  | Outcome (o : BResidualOutcome)
  | DeclareRecovery
  deriving DecidableEq, Repr

/-- Residual conservation of an outcome: `o.booked_loss.checked_add(o.explicit_loss)
.and_then(|v| v.checked_add(o.remaining_after)) == Some(residual_remaining)`, read with
`checked_add(a, b) == Some(a + b)` exactly when `a + b < 2^128` (and `None` otherwise): both
partial sums fit in `u128` and the exact sum equals `residual_remaining`. -/
def bresidual_conserves (residual_remaining : Nat) (o : BResidualOutcome) : Prop :=
  o.booked_loss + o.explicit_loss < two128 ∧
  o.booked_loss + o.explicit_loss + o.remaining_after < two128 ∧
  o.booked_loss + o.explicit_loss + o.remaining_after = residual_remaining

/-- K27 `requires`: `match booked { Some(o) => o.booked_loss.checked_add(o.explicit_loss)
.and_then(|v| v.checked_add(o.remaining_after)) == Some(residual_remaining), None => true }`. -/
def bresidual_step_requires (residual_remaining : Nat) : Option BResidualOutcome → Prop
  | some o => bresidual_conserves residual_remaining o
  | none => True

/-- K27 `ensures`: `match r { BResidualStepV16::Outcome(o) => o.booked_loss
.checked_add(o.explicit_loss).and_then(|v| v.checked_add(o.remaining_after)) ==
Some(residual_remaining), BResidualStepV16::DeclareRecovery => true }`. -/
def bresidual_step_ensures (residual_remaining : Nat) (_booked : Option BResidualOutcome)
    (_resolved : Bool) : BResidualStep → Prop
  | .Outcome o => bresidual_conserves residual_remaining o
  | .DeclareRecovery => True

/-! ## K33 `kernel_recovery_pending_obligation_release_allowed(lifecycle: AssetLifecycleV16,
opposite_stored_count: u64, opposite_pending_count: u64)` -/

/-- Rust `enum AssetLifecycleV16 { Disabled, PendingActivation, Active, DrainOnly, Retired,
Recovery }`. -/
inductive AssetLifecycle where
  | Disabled
  | PendingActivation
  | Active
  | DrainOnly
  | Retired
  | Recovery
  deriving DecidableEq, Repr

/-- K33 `ensures` for `Ok(allowed)`: `*allowed == (lifecycle != AssetLifecycleV16::Recovery ||
opposite_stored_count == opposite_pending_count)`. -/
def recovery_pending_obligation_release_allowed_ensures (lifecycle : AssetLifecycle)
    (opposite_stored_count opposite_pending_count : Nat) (allowed : Bool) : Prop :=
  (allowed = true ↔
    (lifecycle ≠ AssetLifecycle.Recovery ∨ opposite_stored_count = opposite_pending_count))

/-- K33 `ensures` for `Err(_)`: `opposite_pending_count > opposite_stored_count`. -/
def recovery_pending_obligation_release_allowed_err (_lifecycle : AssetLifecycle)
    (opposite_stored_count opposite_pending_count : Nat) : Prop :=
  opposite_pending_count > opposite_stored_count

/-! ## K57 `MarketGroupV16ViewMut::asset_restart_next_counters(next_market_id_before: u64,
activation_count_before: u64, asset_set_epoch_before: u64, risk_epoch_before: u64)` -/

/-- K57 `ensures` for `Ok((m, a, ase, re))`: `*m == next_market_id_before.wrapping_add(1) &&
*a == activation_count_before.wrapping_add(1) && *ase == asset_set_epoch_before.wrapping_add(1)
&& *re == risk_epoch_before.wrapping_add(1)` (`u64` wrapping additions). -/
def asset_restart_next_counters_ensures (next_market_id_before activation_count_before
    asset_set_epoch_before risk_epoch_before : Nat) (m a ase re : Nat) : Prop :=
  m = wrapAdd64 next_market_id_before 1 ∧
  a = wrapAdd64 activation_count_before 1 ∧
  ase = wrapAdd64 asset_set_epoch_before 1 ∧
  re = wrapAdd64 risk_epoch_before 1

/-! ## K58 `trade_signed_size_deltas(size_q: i128)` -/

/-- K58 `ensures` for `Ok((abs, long_d, short_d))`: `*abs == size_q.unsigned_abs() &&
*abs <= MAX_TRADE_SIZE_Q && *long_d == size_q && *short_d == -size_q &&
*long_d + *short_d == 0 && size_q != 0` (`-size_q` and `+` read as exact integer
arithmetic). -/
def trade_signed_size_deltas_ensures (size_q : Int) (abs : Nat) (long_d short_d : Int) : Prop :=
  abs = size_q.natAbs ∧
  abs ≤ MAX_TRADE_SIZE_Q ∧
  long_d = size_q ∧
  short_d = -size_q ∧
  long_d + short_d = 0 ∧
  size_q ≠ 0

/-! ## K16 `kernel_classify_position_delta(current: i128, new: i128)` -/

/-- Rust `enum PositionRouteV16 { Attach, Clear, Flip, Resize }`. -/
inductive PositionRoute where
  | Attach
  | Clear
  | Flip
  | Resize
  deriving DecidableEq, Repr

/-- K16 `ensures`: `Attach => current == 0`, `Clear => current != 0 && new == 0`,
`Flip => current != 0 && new != 0 && current.signum() != new.signum()`,
`Resize => current != 0 && new != 0 && current.signum() == new.signum()`. -/
def classify_position_delta_ensures (current new : Int) : PositionRoute → Prop
  | .Attach => current = 0
  | .Clear => current ≠ 0 ∧ new = 0
  | .Flip => current ≠ 0 ∧ new ≠ 0 ∧ signumI128 current ≠ signumI128 new
  | .Resize => current ≠ 0 ∧ new ≠ 0 ∧ signumI128 current = signumI128 new

/-! ## K17 `kernel_trade_side_oi_cap_gate(oi_long_before_q: u128, oi_short_before_q: u128,
oi_long_after_q: u128, oi_short_after_q: u128)` -/

/-- K17 `ensures` for `Ok(())`: `(oi_long_after_q <= MAX_OI_SIDE_Q || oi_long_after_q <=
oi_long_before_q) && (oi_short_after_q <= MAX_OI_SIDE_Q || oi_short_after_q <=
oi_short_before_q)`. -/
def trade_side_oi_cap_gate_ensures
    (oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q : Nat) : Prop :=
  (oi_long_after_q ≤ MAX_OI_SIDE_Q ∨ oi_long_after_q ≤ oi_long_before_q) ∧
  (oi_short_after_q ≤ MAX_OI_SIDE_Q ∨ oi_short_after_q ≤ oi_short_before_q)

/-- K17 `ensures` for `Err(e)`, with `d` the discriminant of `e`: `*e == V16Error::InvalidLeg &&
((oi_long_after_q > MAX_OI_SIDE_Q && oi_long_after_q > oi_long_before_q) ||
(oi_short_after_q > MAX_OI_SIDE_Q && oi_short_after_q > oi_short_before_q))`. -/
def trade_side_oi_cap_gate_err
    (oi_long_before_q oi_short_before_q oi_long_after_q oi_short_after_q : Nat) (d : Nat) :
    Prop :=
  d = V16Error.InvalidLeg.disc ∧
  ((oi_long_after_q > MAX_OI_SIDE_Q ∧ oi_long_after_q > oi_long_before_q) ∨
    (oi_short_after_q > MAX_OI_SIDE_Q ∧ oi_short_after_q > oi_short_before_q))

/-! ## K18 `kernel_reduce_position_delta(pre_basis_signed: i128, side: SideV16,
requested: u128)` -/

/-- K18 `requires`: `pre_basis_signed > i128::MIN && match side { SideV16::Long =>
pre_basis_signed >= 0, SideV16::Short => pre_basis_signed <= 0 }`. -/
def reduce_position_delta_requires (pre_basis_signed : Int) (side : Side) : Prop :=
  pre_basis_signed > -(2 ^ 127 : Int) ∧
  (match side with
    | .Long => pre_basis_signed ≥ 0
    | .Short => pre_basis_signed ≤ 0)

/-- K18 `ensures` for `Ok((reduce_q, delta))`, with `pre_abs = pre_basis_signed.unsigned_abs()`
and `new_signed = pre_basis_signed.wrapping_add(*delta)`:
`*reduce_q == if requested < pre_abs { requested } else { pre_abs } && *reduce_q <= pre_abs &&
new_signed.unsigned_abs() == pre_abs - *reduce_q && new_signed.unsigned_abs() <= pre_abs &&
(*reduce_q != pre_abs || new_signed == 0) && (!(pre_abs > 0 && requested > 0) ||
(*reduce_q > 0 && new_signed.unsigned_abs() < pre_abs))`. (`pre_abs - *reduce_q` is evaluated
only after `*reduce_q <= pre_abs`; it is read as an exact difference.) -/
def reduce_position_delta_ensures (pre_basis_signed : Int) (_side : Side) (requested : Nat)
    (reduce_q : Nat) (delta : Int) : Prop :=
  let pre_abs := pre_basis_signed.natAbs
  let new_signed := wrapI128 (pre_basis_signed + delta)
  reduce_q = (if requested < pre_abs then requested else pre_abs) ∧
  reduce_q ≤ pre_abs ∧
  (new_signed.natAbs : Int) = (pre_abs : Int) - (reduce_q : Int) ∧
  new_signed.natAbs ≤ pre_abs ∧
  (reduce_q ≠ pre_abs ∨ new_signed = 0) ∧
  (¬(pre_abs > 0 ∧ requested > 0) ∨ (reduce_q > 0 ∧ new_signed.natAbs < pre_abs))

end Spec.Kernel

end
