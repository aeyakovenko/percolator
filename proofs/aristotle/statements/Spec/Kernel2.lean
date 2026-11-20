module

public import Spec.Kernel
public import Spec.KernelModel2

/-!
# Public contracts for eight more `V16Core` kernels of `src/v16.rs`

For each of K11 `kernel_settle_principal`, K29 `kernel_settle_resolved_pnl_after_booking`,
K09 `kernel_locked_margin_gate`, K08 `kernel_initial_margin_gate`,
K14 `kernel_released_pnl_conversion_partition`, K15 `kernel_retain_terminal_source_haircut`,
K31 `kernel_normalize_social_loss_carry` and K28 `kernel_forfeit_residual_step`, two properties
of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModel2`, in
  which parameter names denote the values passed by the caller).
* `Kxx.nopanic` (`*_nopanic` below): for all inputs satisfying the `requires`, the call does
  not panic (it never unwinds; overflow checks are on in the release profile).

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. Contracts describe partial correctness: they
say what holds if a call returns and whether it may unwind; they do not say that it terminates.

The core library functions that a kernel calls and whose bodies are not verified here are
covered by explicit contracts (from `Spec.Kernel` or below), which the property takes as
premises.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## K11 `kernel_settle_principal` -/

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` of `kernel_settle_principal`, returns `Ok(x)`. -/
def MapErrSettlePrincipalContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_principal::{closure#0}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_principal::{closure#0}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_principal::{closure#0}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- The core library contracts `kernel_settle_principal` relies on. (`u128::checked_sub` is
verified, not assumed.) -/
def SettlePrincipalCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  UnsignedAbsI128Contract S ∧ MinU128Contract S ∧ TryFromU128I128Contract S ∧
    MapErrSettlePrincipalContract S ∧ CheckedAddI128Contract S ∧ OkOrU128Contract S ∧
    OkOrI128Contract S ∧ BranchU128Contract S ∧ BranchI128Contract S ∧
    FromResidualTuple4Contract S

/-- The call `V16Core::kernel_settle_principal(capital, c_tot, pnl)` with postcondition `Φ`. -/
def callSettlePrincipal {PROP : Type} [BI PROP] (S : Vocab PROP) (capital c_tot : Nat)
    (pnl : Int) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_principal»
    [(S.intVal capital, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_principal» 0),
     (S.intVal c_tot, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_principal» 1),
     (S.intVal pnl, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_principal» 2)] Φ

/-- K11.contract: under its `requires` (`pnl <= 0 && pnl > i128::MIN`), if
`kernel_settle_principal(capital, c_tot, pnl)` returns, its result is
`Ok((paid, new_capital, new_c_tot, new_pnl))` satisfying the `ensures`
(`settle_principal_ensures`), or an `Err(_)`. -/
def kernel_settle_principal_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  SettlePrincipalCoreContracts S →
  -- for any arguments,
  ∀ (capital c_tot : Nat) (pnl : Int),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 capital → U128 c_tot → I128 pnl →
    -- and they satisfy the `requires`,
    settle_principal_requires pnl →
    ⊢ -- then, on normal return of the call,
      callSettlePrincipal S capital c_tot pnl (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(..)` with the `ensures` of K11, or an `Err(_)`.
        ConsumeResult S (settle_principal_ensures capital c_tot pnl) v)

/-- K11.nopanic: under its `requires`, `kernel_settle_principal(..)` does not panic. -/
def kernel_settle_principal_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  SettlePrincipalCoreContracts S →
  -- for any arguments,
  ∀ (capital c_tot : Nat) (pnl : Int),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 capital → U128 c_tot → I128 pnl →
    -- and they satisfy the `requires`,
    settle_principal_requires pnl →
    ⊢ -- then the call does not unwind (panic).
      callSettlePrincipal S capital c_tot pnl (S.returns fun _ => iprop(True))

/-! ## K29 `kernel_settle_resolved_pnl_after_booking` -/

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` of `kernel_settle_resolved_pnl_after_booking`, returns
`Ok(x)`. -/
def MapErrSettleResolvedContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking::{closure#0}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking::{closure#0}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking::{closure#0}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- The `?` operator's `from_residual(Err(e))` for `V16Result<i128>` returns `Err(e)`. -/
def FromResidualI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<i128, percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋))

/-- The core library contracts `kernel_settle_resolved_pnl_after_booking` relies on. -/
def SettleResolvedCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  UnsignedAbsI128Contract S ∧ CheckedAddU128Contract S ∧ OkOrU128Contract S ∧
    BranchU128Contract S ∧ MinU128Contract S ∧ TryFromU128I128Contract S ∧
    MapErrSettleResolvedContract S ∧ BranchI128Contract S ∧ CheckedAddI128Contract S ∧
    OkOrI128Contract S ∧ FromResidualI128Contract S

/-- A `V16Result<i128>` value that is either `Ok(x)` for an in-range `x` with `ens x`, or some
`Err(e)`. -/
def I128Result {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Int → Prop) (v : S.Value) :
    Prop :=
  (∃ x : Int, I128 x ∧ v = okValue S (S.intVal x) ∧ ens x) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- The call `V16Core::kernel_settle_resolved_pnl_after_booking(pnl, booked_loss,
explicit_loss)` with postcondition `Φ`. -/
def callSettleResolved {PROP : Type} [BI PROP] (S : Vocab PROP) (pnl : Int)
    (booked_loss explicit_loss : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking»
    [(S.intVal pnl, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking» 0),
     (S.intVal booked_loss, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking» 1),
     (S.intVal explicit_loss, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_resolved_pnl_after_booking» 2)] Φ

/-- K29.contract: under its `requires` (`pnl < 0 && pnl > i128::MIN`), if
`kernel_settle_resolved_pnl_after_booking(pnl, booked_loss, explicit_loss)` returns, its result
is `Ok(new_pnl)` satisfying the `ensures` (`settle_resolved_pnl_ensures`), or an `Err(_)`. -/
def kernel_settle_resolved_pnl_after_booking_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  SettleResolvedCoreContracts S →
  -- for any arguments,
  ∀ (pnl : Int) (booked_loss explicit_loss : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    I128 pnl → U128 booked_loss → U128 explicit_loss →
    -- and they satisfy the `requires`,
    settle_resolved_pnl_requires pnl →
    ⊢ -- then, on normal return of the call,
      callSettleResolved S pnl booked_loss explicit_loss (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(new_pnl)` with the `ensures` of K29, or an `Err(_)`.
        I128Result S (settle_resolved_pnl_ensures pnl booked_loss explicit_loss) v)

/-- K29.nopanic: under its `requires`, `kernel_settle_resolved_pnl_after_booking(..)` does not
panic. -/
def kernel_settle_resolved_pnl_after_booking_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  SettleResolvedCoreContracts S →
  -- for any arguments,
  ∀ (pnl : Int) (booked_loss explicit_loss : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    I128 pnl → U128 booked_loss → U128 explicit_loss →
    -- and they satisfy the `requires`,
    settle_resolved_pnl_requires pnl →
    ⊢ -- then the call does not unwind (panic).
      callSettleResolved S pnl booked_loss explicit_loss (S.returns fun _ => iprop(True))

/-! ## K09 `kernel_locked_margin_gate` -/

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` number `#0` of `kernel_locked_margin_gate`, returns `Ok(x)`. -/
def MapErrLockedMargin0Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#0}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#0}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#0}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- `Result::<i128, TryFromIntError>::map_err(Ok(x), f)`, with `f` the (zero-sized) closure
`|_| V16Error::ArithmeticOverflow` number `#1` of `kernel_locked_margin_gate`, returns `Ok(x)`. -/
def MapErrLockedMargin1Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ x : Int, I128 x →
    ⊢ S.call Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#1}}>»
        [(okValue S (S.intVal x), S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#1}}>» 0),
         (S.tupleVal [], S.argTy Mir.Kernel.Fn.«core::result::Result::<i128, core::num::error::TryFromIntError>::map_err::<percolator::v16::V16Error, {closure@percolator::v16::V16Core::kernel_locked_margin_gate::{closure#1}}>» 1)]
        (S.returns fun w => iprop(⌊w = okValue S (S.intVal x)⌋))

/-- `<i128 as Ord>::min(a, b)` returns `min(a, b)`. -/
def MinI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Int, I128 a → I128 b →
    ⊢ S.call Mir.Kernel.Fn.«<i128 as core::cmp::Ord>::min»
        [(S.intVal a, S.argTy Mir.Kernel.Fn.«<i128 as core::cmp::Ord>::min» 0),
         (S.intVal b, S.argTy Mir.Kernel.Fn.«<i128 as core::cmp::Ord>::min» 1)]
        (S.returns fun w => iprop(⌊w = S.intVal (min a b)⌋))

/-- `i128::checked_sub(a, b)` returns `Some(a - b)` if `a - b` is an `i128`, else `None`. -/
def CheckedSubI128Contract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ a b : Int, I128 a → I128 b →
    ⊢ S.call Mir.Kernel.Fn.«core::num::<impl i128>::checked_sub»
        [(S.intVal a, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::checked_sub» 0),
         (S.intVal b, S.argTy Mir.Kernel.Fn.«core::num::<impl i128>::checked_sub» 1)]
        (S.returns fun w => iprop(⌊w = if -(2 ^ 127 : Int) ≤ a - b ∧ a - b < 2 ^ 127 then
          someValue S (S.intVal (a - b)) else noneValue S⌋))

/-- The `?` operator's `from_residual(Err(e))` for `V16Result<()>` returns `Err(e)`. -/
def FromResidualUnitContract {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  ∀ d : Nat, d < 12 →
    ⊢ S.call Mir.Kernel.Fn.«<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual»
        [(errValue S (v16ErrorValue S d), S.argTy Mir.Kernel.Fn.«<core::result::Result<(), percolator::v16::V16Error> as core::ops::try_trait::FromResidual<core::result::Result<core::convert::Infallible, percolator::v16::V16Error>>>::from_residual» 0)]
        (S.returns fun w => iprop(⌊w = errValue S (v16ErrorValue S d)⌋))

/-- The core library contracts `kernel_locked_margin_gate` relies on. -/
def LockedMarginCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  TryFromU128I128Contract S ∧ MapErrLockedMargin0Contract S ∧ MapErrLockedMargin1Contract S ∧
    BranchI128Contract S ∧ UnsignedAbsI128Contract S ∧ MinI128Contract S ∧
    CheckedAddI128Contract S ∧ CheckedSubI128Contract S ∧ OkOrI128Contract S ∧
    FromResidualUnitContract S

/-- A `V16Result<()>` value that is either `Ok(())`, where `P` holds, or some `Err(e)`, where `E`
holds. -/
def UnitResult {PROP : Type} [BI PROP] (S : Vocab PROP) (P E : Prop) (v : S.Value) : Prop :=
  (v = okValue S (S.tupleVal []) ∧ P) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- The call `V16Core::kernel_locked_margin_gate(capital, pnl, fee_credits,
certified_initial_req)` with postcondition `Φ`. -/
def callLockedMargin {PROP : Type} [BI PROP] (S : Vocab PROP) (capital : Nat)
    (pnl fee_credits : Int) (certified_initial_req : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_locked_margin_gate»
    [(S.intVal capital, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_locked_margin_gate» 0),
     (S.intVal pnl, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_locked_margin_gate» 1),
     (S.intVal fee_credits, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_locked_margin_gate» 2),
     (S.intVal certified_initial_req, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_locked_margin_gate» 3)] Φ

/-- K09.contract: under its `requires` (`pnl > i128::MIN && fee_credits > i128::MIN &&
capital < 1u128 << 100`), if `kernel_locked_margin_gate(capital, pnl, fee_credits,
certified_initial_req)` returns, its result is `Ok(())` with the `ensures`
(`locked_margin_ensures`), or an `Err(_)`. -/
def kernel_locked_margin_gate_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  LockedMarginCoreContracts S →
  -- for any arguments,
  ∀ (capital : Nat) (pnl fee_credits : Int) (certified_initial_req : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 capital → I128 pnl → I128 fee_credits → U128 certified_initial_req →
    -- and they satisfy the `requires`,
    locked_margin_requires capital pnl fee_credits →
    ⊢ -- then, on normal return of the call,
      callLockedMargin S capital pnl fee_credits certified_initial_req (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(())` with the `ensures` of K09, or an `Err(_)` (`Err(_) => true`).
        UnitResult S (locked_margin_ensures capital pnl fee_credits certified_initial_req) True v)

/-- K09.nopanic: under its `requires`, `kernel_locked_margin_gate(..)` does not panic. -/
def kernel_locked_margin_gate_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  LockedMarginCoreContracts S →
  -- for any arguments,
  ∀ (capital : Nat) (pnl fee_credits : Int) (certified_initial_req : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types,
    U128 capital → I128 pnl → I128 fee_credits → U128 certified_initial_req →
    -- and they satisfy the `requires`,
    locked_margin_requires capital pnl fee_credits →
    ⊢ -- then the call does not unwind (panic).
      callLockedMargin S capital pnl fee_credits certified_initial_req
        (S.returns fun _ => iprop(True))

/-! ## K08 `kernel_initial_margin_gate` -/

/-- The Rust value of a `HealthCertV16`, fields in declaration order
(`active_bitmap_at_cert: [u64; 1]` is the one-element array of `active_bitmap_word`). -/
def healthCertValue {PROP : Type} [BI PROP] (S : Vocab PROP) (c : HealthCert) : S.Value :=
  S.tupleVal [S.intVal c.certified_equity, S.intVal c.certified_initial_req,
    S.intVal c.certified_maintenance_req, S.intVal c.certified_liq_deficit,
    S.intVal c.certified_worst_case_loss, S.intVal c.cert_oracle_epoch,
    S.intVal c.cert_funding_epoch, S.intVal c.cert_risk_epoch, S.intVal c.cert_asset_set_epoch,
    S.tupleVal [S.intVal c.active_bitmap_word], S.boolVal c.valid]

/-- The call `V16Core::kernel_initial_margin_gate(cert)` with postcondition `Φ`. -/
def callInitialMargin {PROP : Type} [BI PROP] (S : Vocab PROP) (cert : HealthCert)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_initial_margin_gate»
    [(healthCertValue S cert, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_initial_margin_gate» 0)] Φ

/-- K08.contract: if `kernel_initial_margin_gate(cert)` returns, its result is `Ok(())` exactly
when `initial_margin_admit cert` holds (`cert.valid && cert.certified_equity >= 0 &&
(cert.certified_equity as u128) >= cert.certified_initial_req`), and some `Err(_)` exactly when
it does not. -/
def kernel_initial_margin_gate_contract : Property := fun S =>
  -- For any certificate,
  ∀ (cert : HealthCert),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    cert.WF →
    ⊢ -- then, on normal return of the call,
      callInitialMargin S cert (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(())` and `admit` holds, or an `Err(_)` and `admit` does not hold.
        UnitResult S (initial_margin_admit cert) (¬ initial_margin_admit cert) v)

/-- K08.nopanic: `kernel_initial_margin_gate(cert)` does not panic. -/
def kernel_initial_margin_gate_nopanic : Property := fun S =>
  -- For any certificate,
  ∀ (cert : HealthCert),
    -- PRECONDITIONS
    -- if every field is in range of its Rust type (no `requires`),
    cert.WF →
    ⊢ -- then the call does not unwind (panic).
      callInitialMargin S cert (S.returns fun _ => iprop(True))

/-! ## K14 `kernel_released_pnl_conversion_partition` -/

/-- A `V16Result<(u128, u128)>` value that is either `Ok((a, b))` for in-range `a`, `b` with
`ens a b`, or some `Err(e)` where `E` holds. -/
def PairResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Nat → Nat → Prop) (E : Prop)
    (v : S.Value) : Prop :=
  (∃ a b : Nat, U128 a ∧ U128 b ∧ v = okValue S (S.tupleVal [S.intVal a, S.intVal b]) ∧
    ens a b) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- The call `V16Core::kernel_released_pnl_conversion_partition(positive_face, converted,
source_face_burn, retain_haircut_face)` with postcondition `Φ`. -/
def callConversionPartition {PROP : Type} [BI PROP] (S : Vocab PROP)
    (positive_face converted source_face_burn : Nat) (retain_haircut_face : Bool)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_released_pnl_conversion_partition»
    [(S.intVal positive_face, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_released_pnl_conversion_partition» 0),
     (S.intVal converted, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_released_pnl_conversion_partition» 1),
     (S.intVal source_face_burn, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_released_pnl_conversion_partition» 2),
     (S.boolVal retain_haircut_face, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_released_pnl_conversion_partition» 3)] Φ

/-- K14.contract: if `kernel_released_pnl_conversion_partition(positive_face, converted,
source_face_burn, retain_haircut_face)` returns, its result is `Ok((pnl_debit,
retained_haircut_face))` with the `Ok` arm of the `ensures` (`conversion_partition_ensures`), or
an `Err(_)` with the `Err` arm (`conversion_partition_err`). -/
def kernel_released_pnl_conversion_partition_contract : Property := fun S =>
  -- For any arguments,
  ∀ (positive_face converted source_face_burn : Nat) (retain_haircut_face : Bool),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 positive_face → U128 converted → U128 source_face_burn →
    ⊢ -- then, on normal return of the call,
      callConversionPartition S positive_face converted source_face_burn retain_haircut_face
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the `Ok` arm or the `Err` arm of the `ensures` of K14 holds.
          PairResult S
            (conversion_partition_ensures positive_face converted source_face_burn
              retain_haircut_face)
            (conversion_partition_err positive_face converted source_face_burn) v)

/-- K14.nopanic: `kernel_released_pnl_conversion_partition(..)` does not panic. -/
def kernel_released_pnl_conversion_partition_nopanic : Property := fun S =>
  -- For any arguments,
  ∀ (positive_face converted source_face_burn : Nat) (retain_haircut_face : Bool),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 positive_face → U128 converted → U128 source_face_burn →
    ⊢ -- then the call does not unwind (panic).
      callConversionPartition S positive_face converted source_face_burn retain_haircut_face
        (S.returns fun _ => iprop(True))


/-! ## K15 `kernel_retain_terminal_source_haircut` -/

/-- The core library contracts `kernel_retain_terminal_source_haircut` relies on. -/
def RetainHaircutCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  CheckedAddU128Contract S ∧ OkOrU128Contract S

/-- A `V16Result<u128>` value that is either `Ok(x)` for an in-range `x` with `ens x`, or some
`Err(e)` where `E` holds. -/
def U128Result {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Nat → Prop) (E : Prop)
    (v : S.Value) : Prop :=
  (∃ x : Nat, U128 x ∧ v = okValue S (S.intVal x) ∧ ens x) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- The call `V16Core::kernel_retain_terminal_source_haircut(positive_pnl, reserved_pnl,
source_attributed_face, converted)` with postcondition `Φ`. -/
def callRetainHaircut {PROP : Type} [BI PROP] (S : Vocab PROP)
    (positive_pnl reserved_pnl source_attributed_face converted : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_retain_terminal_source_haircut»
    [(S.intVal positive_pnl, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_retain_terminal_source_haircut» 0),
     (S.intVal reserved_pnl, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_retain_terminal_source_haircut» 1),
     (S.intVal source_attributed_face, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_retain_terminal_source_haircut» 2),
     (S.intVal converted, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_retain_terminal_source_haircut» 3)] Φ

/-- K15.contract: if `kernel_retain_terminal_source_haircut(positive_pnl, reserved_pnl,
source_attributed_face, converted)` returns, its result is `Ok(next_reserved)` with the `Ok` arm
of the `ensures` (`retain_haircut_ensures`), or an `Err(_)` with the `Err` arm
(`retain_haircut_err`). -/
def kernel_retain_terminal_source_haircut_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  RetainHaircutCoreContracts S →
  -- for any arguments,
  ∀ (positive_pnl reserved_pnl source_attributed_face converted : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 positive_pnl → U128 reserved_pnl → U128 source_attributed_face → U128 converted →
    ⊢ -- then, on normal return of the call,
      callRetainHaircut S positive_pnl reserved_pnl source_attributed_face converted
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the `Ok` arm or the `Err` arm of the `ensures` of K15 holds.
          U128Result S
            (retain_haircut_ensures positive_pnl reserved_pnl source_attributed_face converted)
            (retain_haircut_err positive_pnl reserved_pnl source_attributed_face converted) v)

/-- K15.nopanic: `kernel_retain_terminal_source_haircut(..)` does not panic. -/
def kernel_retain_terminal_source_haircut_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  RetainHaircutCoreContracts S →
  -- for any arguments,
  ∀ (positive_pnl reserved_pnl source_attributed_face converted : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 positive_pnl → U128 reserved_pnl → U128 source_attributed_face → U128 converted →
    ⊢ -- then the call does not unwind (panic).
      callRetainHaircut S positive_pnl reserved_pnl source_attributed_face converted
        (S.returns fun _ => iprop(True))


/-! ## K31 `kernel_normalize_social_loss_carry` -/

/-- The core library contracts `kernel_normalize_social_loss_carry` relies on.
(`u128::saturating_add` and `u128::from(bool)` are verified, not assumed.) -/
def NormalizeCarryCoreContracts {PROP : Type} [BI PROP] (S : Vocab PROP) : Prop :=
  CheckedAddU128Contract S ∧ OkOrU128Contract S ∧ BranchU128Contract S

/-- The call `V16Core::kernel_normalize_social_loss_carry(remainder, dust, explicit_loss)` with
postcondition `Φ`. -/
def callNormalizeCarry {PROP : Type} [BI PROP] (S : Vocab PROP)
    (remainder dust explicit_loss : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_normalize_social_loss_carry»
    [(S.intVal remainder, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_normalize_social_loss_carry» 0),
     (S.intVal dust, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_normalize_social_loss_carry» 1),
     (S.intVal explicit_loss, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_normalize_social_loss_carry» 2)] Φ

/-- K31.contract: if `kernel_normalize_social_loss_carry(remainder, dust, explicit_loss)`
returns, its result is `Ok((next_dust, next_explicit))` with the `Ok` arm of the `ensures`
(`normalize_carry_ensures`), or an `Err(_)` with the `Err` arm (`normalize_carry_err`). -/
def kernel_normalize_social_loss_carry_contract : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  NormalizeCarryCoreContracts S →
  -- for any arguments,
  ∀ (remainder dust explicit_loss : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 remainder → U128 dust → U128 explicit_loss →
    ⊢ -- then, on normal return of the call,
      callNormalizeCarry S remainder dust explicit_loss
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the `Ok` arm or the `Err` arm of the `ensures` of K31 holds.
          PairResult S (normalize_carry_ensures remainder dust explicit_loss)
            (normalize_carry_err remainder dust) v)

/-- K31.nopanic: `kernel_normalize_social_loss_carry(..)` does not panic. -/
def kernel_normalize_social_loss_carry_nopanic : Property := fun S =>
  -- ASSUMED: the contracts of the core functions it calls,
  NormalizeCarryCoreContracts S →
  -- for any arguments,
  ∀ (remainder dust explicit_loss : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 remainder → U128 dust → U128 explicit_loss →
    ⊢ -- then the call does not unwind (panic).
      callNormalizeCarry S remainder dust explicit_loss (S.returns fun _ => iprop(True))


/-! ## K28 `kernel_forfeit_residual_step` -/

/-- The Rust discriminant of a `ForfeitResidualStepV16`
(`NoResidual = 0`, `Book = 1`, `CommitRecovery = 2`). -/
def forfeitStepDisc : ForfeitStep → Int
  | .NoResidual => 0
  | .Book => 1
  | .CommitRecovery => 2

/-- The Rust value of a `ForfeitResidualStepV16`. -/
def forfeitStepValue {PROP : Type} [BI PROP] (S : Vocab PROP) (s : ForfeitStep) : S.Value :=
  S.variantVal (forfeitStepDisc s) S.unitVal

/-- The call `V16Core::kernel_forfeit_residual_step(residual_remaining, booking_capacity)` with
postcondition `Φ`. -/
def callForfeitStep {PROP : Type} [BI PROP] (S : Vocab PROP)
    (residual_remaining booking_capacity : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_forfeit_residual_step»
    [(S.intVal residual_remaining, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_forfeit_residual_step» 0),
     (S.intVal booking_capacity, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_forfeit_residual_step» 1)] Φ

/-- K28.contract: if `kernel_forfeit_residual_step(residual_remaining, booking_capacity)`
returns, its result is a `ForfeitResidualStepV16` satisfying the `ensures`
(`forfeit_step_ensures`). -/
def kernel_forfeit_residual_step_contract : Property := fun S =>
  -- For any arguments,
  ∀ (residual_remaining booking_capacity : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 residual_remaining → U128 booking_capacity →
    ⊢ -- then, on normal return of the call,
      callForfeitStep S residual_remaining booking_capacity
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is some `ForfeitResidualStepV16` value satisfying the `ensures` of K28.
          ∃ step : ForfeitStep, v = forfeitStepValue S step ∧
            forfeit_step_ensures residual_remaining booking_capacity step)

/-- K28.nopanic: `kernel_forfeit_residual_step(..)` does not panic. -/
def kernel_forfeit_residual_step_nopanic : Property := fun S =>
  -- For any arguments,
  ∀ (residual_remaining booking_capacity : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U128 residual_remaining → U128 booking_capacity →
    ⊢ -- then the call does not unwind (panic).
      callForfeitStep S residual_remaining booking_capacity (S.returns fun _ => iprop(True))

end Spec.Kernel

end
