module

public import Spec.KernelModel

/-!
# Independent mathematical model for eight more `V16Core` kernel contracts

A mathematical reading of the `kani::requires` and `kani::ensures` clauses of these
associated functions of `V16Core` in `src/v16.rs`:

* K11 `kernel_settle_principal`
* K29 `kernel_settle_resolved_pnl_after_booking`
* K09 `kernel_locked_margin_gate`
* K08 `kernel_initial_margin_gate`
* K14 `kernel_released_pnl_conversion_partition`
* K15 `kernel_retain_terminal_source_haircut`
* K31 `kernel_normalize_social_loss_carry`
* K28 `kernel_forfeit_residual_step`

Conventions (as in `Spec.KernelModel`):

* Rust unsigned integers are modelled by `Nat`, `i128` by `Int`; `U128`, `U64`, `I128` say a
  value is representable in its Rust type.
* `as` casts, `wrapping_*`, `saturating_*`, `min` and `unsigned_abs` are the functions defined
  below (or in `Spec.KernelModel`), with their Rust meaning.
* Plain `+` / `-` inside an `ensures` clause are read as exact integer arithmetic. Each such
  term is evaluated (because of `&&` short-circuiting) only where it does not overflow.
* Parameter names denote the values the caller passed.
-/

@[expose] public section

namespace Spec.Kernel

/-- Rust `x as i128` for a `u128` value `x` (two's-complement reinterpretation). -/
def castU128I128 (x : Nat) : Int :=
  if x < 2 ^ 127 then (x : Int) else (x : Int) - 2 ^ 128

/-- Rust `x as u128` for an `i128` value `x` (two's-complement reinterpretation). -/
def castI128U128 (x : Int) : Nat := (x % (2 ^ 128 : Int)).toNat

/-- Reduction of an integer into the `i128` range modulo `2^128` (the result of `i128`
`wrapping_add` / `wrapping_sub`). -/
def wrapI128 (x : Int) : Int := (x + 2 ^ 127) % (2 ^ 128 : Int) - 2 ^ 127

/-- Rust `a.saturating_add(b)` on `u128`. -/
def satAdd128 (a b : Nat) : Nat := min (a + b) (2 ^ 128 - 1)

/-- Rust `a.saturating_sub(b)` on `u128`. -/
def satSub128 (a b : Nat) : Nat := a - b

/-! ## K11 `kernel_settle_principal(capital: u128, c_tot: u128, pnl: i128)` -/

/-- K11 `requires`: `pnl <= 0 && pnl > i128::MIN`. -/
def settle_principal_requires (pnl : Int) : Prop :=
  pnl ≤ 0 ∧ pnl > -(2 ^ 127 : Int)

/-- K11 `ensures` for `Ok((paid, new_capital, new_c_tot, new_pnl))`, with
`loss = pnl.unsigned_abs()`: `paid == if capital < loss { capital } else { loss }`,
`new_capital == capital - paid`, `new_c_tot == c_tot - paid`,
`new_capital.wrapping_add(paid) == capital`, `new_c_tot.wrapping_add(paid) == c_tot`,
`new_pnl == pnl + (paid as i128)` and `new_pnl <= 0`. -/
def settle_principal_ensures (capital c_tot : Nat) (pnl : Int)
    (paid new_capital new_c_tot : Nat) (new_pnl : Int) : Prop :=
  let loss := pnl.natAbs
  paid = (if capital < loss then capital else loss) ∧
  (new_capital : Int) = (capital : Int) - (paid : Int) ∧
  (new_c_tot : Int) = (c_tot : Int) - (paid : Int) ∧
  wrapAdd128 new_capital paid = capital ∧
  wrapAdd128 new_c_tot paid = c_tot ∧
  new_pnl = pnl + castU128I128 paid ∧
  new_pnl ≤ 0

/-! ## K29 `kernel_settle_resolved_pnl_after_booking(pnl: i128, booked_loss: u128,
explicit_loss: u128)` -/

/-- K29 `requires`: `pnl < 0 && pnl > i128::MIN`. -/
def settle_resolved_pnl_requires (pnl : Int) : Prop :=
  pnl < 0 ∧ pnl > -(2 ^ 127 : Int)

/-- K29 `ensures` for `Ok(new_pnl)`, with `loss = pnl.unsigned_abs()` and
`cleared = booked_loss.saturating_add(explicit_loss).min(loss)`:
`new_pnl == pnl + (cleared as i128)`, `new_pnl <= 0`, `new_pnl >= pnl` and
`new_pnl.unsigned_abs() == loss - cleared`. -/
def settle_resolved_pnl_ensures (pnl : Int) (booked_loss explicit_loss : Nat) (new_pnl : Int) :
    Prop :=
  let loss := pnl.natAbs
  let cleared := min (satAdd128 booked_loss explicit_loss) loss
  new_pnl = pnl + castU128I128 cleared ∧
  new_pnl ≤ 0 ∧
  new_pnl ≥ pnl ∧
  (new_pnl.natAbs : Int) = (loss : Int) - (cleared : Int)

/-! ## K09 `kernel_locked_margin_gate(capital: u128, pnl: i128, fee_credits: i128,
certified_initial_req: u128)` -/

/-- K09 `requires`: `pnl > i128::MIN && fee_credits > i128::MIN && capital < 1u128 << 100`. -/
def locked_margin_requires (capital : Nat) (pnl fee_credits : Int) : Prop :=
  pnl > -(2 ^ 127 : Int) ∧ fee_credits > -(2 ^ 127 : Int) ∧ capital < 2 ^ 100

/-- The locked-lane equity of the K09 `ensures`:
`(capital as i128).wrapping_add(if pnl < 0 { pnl } else { 0 })
  .wrapping_sub(fee_credits.unsigned_abs() as i128)`. -/
def locked_margin_equity (capital : Nat) (pnl fee_credits : Int) : Int :=
  wrapI128 (wrapI128 (castU128I128 capital + (if pnl < 0 then pnl else 0)) -
    castU128I128 fee_credits.natAbs)

/-- K09 `ensures` for `Ok(())`: `equity >= 0 && (equity as u128) >= certified_initial_req`. -/
def locked_margin_ensures (capital : Nat) (pnl fee_credits : Int) (certified_initial_req : Nat) :
    Prop :=
  let equity := locked_margin_equity capital pnl fee_credits
  equity ≥ 0 ∧ castI128U128 equity ≥ certified_initial_req

/-! ## K08 `kernel_initial_margin_gate(cert: HealthCertV16)` -/

/-- Rust `struct HealthCertV16` (`#[repr(C)]`; all fields, in declaration order).
`active_bitmap_at_cert: V16ActiveBitmap` is `[u64; 1]`, modelled by its only word. -/
structure HealthCert where
  /-- `certified_equity: i128` -/
  certified_equity : Int
  /-- `certified_initial_req: u128` -/
  certified_initial_req : Nat
  /-- `certified_maintenance_req: u128` -/
  certified_maintenance_req : Nat
  /-- `certified_liq_deficit: u128` -/
  certified_liq_deficit : Nat
  /-- `certified_worst_case_loss: u128` -/
  certified_worst_case_loss : Nat
  /-- `cert_oracle_epoch: u64` -/
  cert_oracle_epoch : Nat
  /-- `cert_funding_epoch: u64` -/
  cert_funding_epoch : Nat
  /-- `cert_risk_epoch: u64` -/
  cert_risk_epoch : Nat
  /-- `cert_asset_set_epoch: u64` -/
  cert_asset_set_epoch : Nat
  /-- `active_bitmap_at_cert[0]: u64` -/
  active_bitmap_word : Nat
  /-- `valid: bool` -/
  valid : Bool
  deriving DecidableEq, Repr

/-- Every field of a `HealthCertV16` value is in range of its Rust type. -/
def HealthCert.WF (c : HealthCert) : Prop :=
  I128 c.certified_equity ∧ U128 c.certified_initial_req ∧ U128 c.certified_maintenance_req ∧
  U128 c.certified_liq_deficit ∧ U128 c.certified_worst_case_loss ∧ U64 c.cert_oracle_epoch ∧
  U64 c.cert_funding_epoch ∧ U64 c.cert_risk_epoch ∧ U64 c.cert_asset_set_epoch ∧
  U64 c.active_bitmap_word

/-- K08 admission predicate of the `ensures`: `cert.valid && cert.certified_equity >= 0 &&
(cert.certified_equity as u128) >= cert.certified_initial_req`. The `ensures` says the result
is `Ok(())` exactly when this holds, and `Err(_)` exactly when it does not. -/
def initial_margin_admit (cert : HealthCert) : Prop :=
  cert.valid = true ∧ cert.certified_equity ≥ 0 ∧
    castI128U128 cert.certified_equity ≥ cert.certified_initial_req

/-! ## K14 `kernel_released_pnl_conversion_partition(positive_face: u128, converted: u128,
source_face_burn: u128, retain_haircut_face: bool)` -/

/-- K14 `ensures` for `Ok((pnl_debit, retained_haircut_face))`. (`positive_face -
source_face_burn` is evaluated only after `source_face_burn <= positive_face`, where the
truncated `Nat` difference is exact.) -/
def conversion_partition_ensures (positive_face converted source_face_burn : Nat)
    (retain_haircut_face : Bool) (pnl_debit retained_haircut_face : Nat) : Prop :=
  converted ≤ source_face_burn ∧
  source_face_burn ≤ positive_face ∧
  pnl_debit ≤ positive_face ∧
  wrapAdd128 (wrapSub128 positive_face pnl_debit) pnl_debit = positive_face ∧
  (if retain_haircut_face then
    pnl_debit = converted ∧
    (retained_haircut_face : Int) = (source_face_burn : Int) - (converted : Int) ∧
    wrapAdd128 (wrapAdd128 converted retained_haircut_face) (positive_face - source_face_burn) =
      positive_face
  else
    pnl_debit = source_face_burn ∧ retained_haircut_face = 0)

/-- K14 `ensures` for `Err(_)`: `converted > source_face_burn || source_face_burn >
positive_face`. -/
def conversion_partition_err (positive_face converted source_face_burn : Nat) : Prop :=
  converted > source_face_burn ∨ source_face_burn > positive_face

/-! ## K15 `kernel_retain_terminal_source_haircut(positive_pnl: u128, reserved_pnl: u128,
source_attributed_face: u128, converted: u128)` -/

/-- K15 `ensures` for `Ok(next_reserved)` (exact integer differences). -/
def retain_haircut_ensures (positive_pnl reserved_pnl source_attributed_face converted : Nat)
    (next_reserved : Nat) : Prop :=
  reserved_pnl ≤ positive_pnl ∧
  (source_attributed_face : Int) ≤ (positive_pnl : Int) - (reserved_pnl : Int) ∧
  converted ≤ source_attributed_face ∧
  (next_reserved : Int) ≤ (positive_pnl : Int) - (converted : Int) ∧
  ((positive_pnl : Int) - (converted : Int)) - (next_reserved : Int) =
    ((positive_pnl : Int) - (reserved_pnl : Int)) - (source_attributed_face : Int)

/-- K15 `ensures` for `Err(_)`: `reserved_pnl > positive_pnl || source_attributed_face >
positive_pnl.saturating_sub(reserved_pnl) || converted > source_attributed_face`. -/
def retain_haircut_err (positive_pnl reserved_pnl source_attributed_face converted : Nat) :
    Prop :=
  reserved_pnl > positive_pnl ∨ source_attributed_face > satSub128 positive_pnl reserved_pnl ∨
    converted > source_attributed_face

/-! ## K31 `kernel_normalize_social_loss_carry(remainder: u128, dust: u128,
explicit_loss: u128)` -/

/-- `SOCIAL_LOSS_DEN` (`src/lib.rs`): `1_000_000_000_000_000_000_000`. -/
def SOCIAL_LOSS_DEN : Nat := 1000000000000000000000

/-- K31 `ensures` for `Ok((next_dust, next_explicit))`, with
`total = remainder.wrapping_add(dust)`. -/
def normalize_carry_ensures (remainder dust explicit_loss : Nat) (next_dust next_explicit : Nat) :
    Prop :=
  let total := wrapAdd128 remainder dust
  remainder < SOCIAL_LOSS_DEN ∧
  dust < SOCIAL_LOSS_DEN ∧
  next_dust = (if total ≥ SOCIAL_LOSS_DEN then wrapSub128 total SOCIAL_LOSS_DEN else total) ∧
  next_explicit = satAdd128 explicit_loss (if total ≥ SOCIAL_LOSS_DEN then 1 else 0) ∧
  next_dust < SOCIAL_LOSS_DEN ∧
  next_explicit ≥ explicit_loss

/-- K31 `ensures` for `Err(_)`: `remainder >= SOCIAL_LOSS_DEN || dust >= SOCIAL_LOSS_DEN`. -/
def normalize_carry_err (remainder dust : Nat) : Prop :=
  remainder ≥ SOCIAL_LOSS_DEN ∨ dust ≥ SOCIAL_LOSS_DEN

/-! ## K28 `kernel_forfeit_residual_step(residual_remaining: u128, booking_capacity: u128)` -/

/-- Rust `enum ForfeitResidualStepV16 { NoResidual, Book, CommitRecovery }`. -/
inductive ForfeitStep where
  | NoResidual
  | Book
  | CommitRecovery
  deriving DecidableEq, Repr

/-- K28 `ensures`: `NoResidual => residual_remaining == 0`,
`Book => residual_remaining != 0 && booking_capacity != 0`,
`CommitRecovery => residual_remaining != 0 && booking_capacity == 0`. -/
def forfeit_step_ensures (residual_remaining booking_capacity : Nat) : ForfeitStep → Prop
  | .NoResidual => residual_remaining = 0
  | .Book => residual_remaining ≠ 0 ∧ booking_capacity ≠ 0
  | .CommitRecovery => residual_remaining ≠ 0 ∧ booking_capacity = 0

end Spec.Kernel

end
