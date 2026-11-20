module

public import Spec.Kernel5
public import Spec.KernelModelC
public import Spec.Rust.KernelC

/-!
# Public contracts for the kernels of engine revision `c030aa40`

These properties are about the functions of `src/v16.rs` at engine revision `c030aa40`
(program `Mir.KernelC`), for:

* the new kernels K59 `V16Core::kernel_side_adl_index` and
  K60 `V16Core::kernel_raw_basis_after_adl_reduction`, with the consequences K59.exposure and
  K60.progress of the spec file's last section;
* the changed kernels K35 `V16Core::kernel_attach_leg`, K07 `V16Core::kernel_resize_leg_same_side`,
  K34 `V16Core::kernel_clear_leg` and K32 `V16Core::kernel_retain_leg_as_pending_obligation`;
* K02 `V16Core::kernel_mark_kf_stale_cohorts`, K03 `V16Core::kernel_settle_kf_stale_cohort` and
  K26 `V16Core::apply_bankruptcy_residual_chunk_to_loss_side` on the enlarged `AssetStateV16`.

For each function, two properties of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires`, if the call
  returns, the returned value satisfies the `ensures` (the predicates of `Spec.KernelModelC`, in
  which parameter names denote the values passed by the caller). For K26, which takes a `&mut`
  argument with a `modifies` clause, the caller owns the referenced value; on return it gets back
  ownership of the same place, now holding a value related to the old one by the `ensures`.
  Nothing else is owned by the call, so nothing outside the `modifies` place can change.
* `Kxx.nopanic` (`*_nopanic` below): for all such inputs, no execution of the call panics (it
  never unwinds; overflow and bounds checks are on in the release profile). This says nothing
  about termination.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`.
-/

@[expose] public section

namespace Spec.KernelC
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (U32 U64 U128 I128 Side Leg okValue errValue v16ErrorValue ifReturns
  sideValue legValue lifecycleValue sideModeValue bookedValue MAX_OI_SIDE_Q BResidualOutcome)

/-! ## Values and types -/

/-- The Rust value of an `AssetStateV16` (revision `c030aa40`), fields in declaration order. -/
def assetValue {PROP : Type} [BI PROP] (S : Vocab PROP) (a : AssetState) : S.Value :=
  S.tupleVal [S.intVal a.market_id, S.intVal a.retired_slot, lifecycleValue S a.lifecycle,
    S.intVal a.raw_oracle_target_price, S.intVal a.effective_price, S.intVal a.fund_px_last,
    S.intVal a.slot_last, S.intVal a.a_long, S.intVal a.a_short, S.intVal a.k_long,
    S.intVal a.k_short, S.intVal a.f_long_num, S.intVal a.f_short_num,
    S.intVal a.kf_epoch_long, S.intVal a.kf_epoch_short, S.intVal a.k_epoch_start_long,
    S.intVal a.k_epoch_start_short, S.intVal a.f_epoch_start_long_num,
    S.intVal a.f_epoch_start_short_num, S.intVal a.b_long_num, S.intVal a.b_short_num,
    S.intVal a.b_epoch_start_long_num, S.intVal a.b_epoch_start_short_num,
    S.intVal a.oi_eff_long_q, S.intVal a.oi_eff_short_q, S.intVal a.raw_basis_sum_long_q,
    S.intVal a.raw_basis_sum_short_q, S.intVal a.stored_pos_count_long,
    S.intVal a.stored_pos_count_short, S.intVal a.stale_account_count_long,
    S.intVal a.stale_account_count_short, S.intVal a.pending_obligation_count_long,
    S.intVal a.pending_obligation_count_short, S.intVal a.loss_weight_sum_long,
    S.intVal a.loss_weight_sum_short, S.intVal a.social_loss_remainder_long_num,
    S.intVal a.social_loss_remainder_short_num, S.intVal a.social_loss_dust_long_num,
    S.intVal a.social_loss_dust_short_num, S.intVal a.explicit_unallocated_loss_long,
    S.intVal a.explicit_unallocated_loss_short, S.intVal a.epoch_long, S.intVal a.epoch_short,
    sideModeValue S a.mode_long, sideModeValue S a.mode_short]

/-- The Rust type `AssetStateV16`, named as the type of the first parameter of
`V16Core::kernel_mark_kf_stale_cohorts`. -/
def assetTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 0

/-- A `V16Result<u128>` value that is either `Ok(x)` for a `u128` `x` with `ens x`, or some
`Err(e)` where `E` holds. -/
def U128Result {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Nat → Prop) (E : Prop)
    (v : S.Value) : Prop :=
  (∃ x : Nat, U128 x ∧ v = okValue S (S.intVal x) ∧ ens x) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- A `V16Result<AssetStateV16>` value that is either `Ok(a)` for an in-range `a` with `ens a`,
or some `Err(e)` where `E` holds. -/
def AssetResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : AssetState → Prop) (E : Prop)
    (v : S.Value) : Prop :=
  (∃ a, a.WF ∧ v = okValue S (assetValue S a) ∧ ens a) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- A `V16Result<(PortfolioLegV16, AssetStateV16)>` value that is either `Ok((l, a))` for
in-range `l`, `a` with `ens l a`, or some `Err(e)` (the `Err(_) => true` arm). -/
def LegAssetResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Leg → AssetState → Prop)
    (v : S.Value) : Prop :=
  (∃ l a, l.WF ∧ a.WF ∧ v = okValue S (S.tupleVal [legValue S l, assetValue S a]) ∧ ens l a) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- A `V16Result<(AssetStateV16, PortfolioLegV16)>` value that is either `Ok((a, l))` for
in-range `a`, `l` with `ens a l`, or some `Err(e)` (the `Err(_) => true` arm). -/
def AssetLegResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : AssetState → Leg → Prop)
    (v : S.Value) : Prop :=
  (∃ a l, a.WF ∧ l.WF ∧ v = okValue S (S.tupleVal [assetValue S a, legValue S l]) ∧ ens a l) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-! ## K59 `kernel_side_adl_index` -/

/-- The call `V16Core::kernel_side_adl_index(oi_eff_q, raw_basis_sum_q, current_a)` with
postcondition `Φ`. -/
def callSideAdlIndex {PROP : Type} [BI PROP] (S : Vocab PROP) (oi_eff_q raw_basis_sum_q current_a : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_side_adl_index»
    [(S.intVal oi_eff_q, S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_side_adl_index» 0),
     (S.intVal raw_basis_sum_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_side_adl_index» 1),
     (S.intVal current_a,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_side_adl_index» 2)] Φ

/-- K59.contract: under the `requires`, if `kernel_side_adl_index(..)` returns, its result is
`Ok(a)` satisfying the `Ok` arm of the `ensures` (`side_adl_index_ensures`), or
`Err(V16Error::InvalidLeg)` with `raw_basis_sum_q != 0 && oi_eff_q > raw_basis_sum_q`. -/
def kernel_side_adl_index_contract : Property := fun S =>
  -- For any effective OI, raw basis sum and current index,
  ∀ (oi_eff_q raw_basis_sum_q current_a : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values
    U128 oi_eff_q → U128 raw_basis_sum_q → U128 current_a →
    -- satisfying the `requires` (`raw_basis_sum_q <= ADL_ONE`),
    side_adl_index_requires raw_basis_sum_q →
    ⊢ -- then, on normal return of the call,
      callSideAdlIndex S oi_eff_q raw_basis_sum_q current_a (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(a)` with the `Ok` arm of K59's `ensures`,
        (∃ a : Nat, U128 a ∧ v = okValue S (S.intVal a) ∧
          side_adl_index_ensures oi_eff_q raw_basis_sum_q current_a a) ∨
        -- or `Err(V16Error::InvalidLeg)` with the `Err` arm.
        (∃ e : Nat, v = errValue S (v16ErrorValue S e) ∧
          side_adl_index_err oi_eff_q raw_basis_sum_q e))

/-- K59.nopanic: under the `requires`, `kernel_side_adl_index(..)` does not panic. -/
def kernel_side_adl_index_nopanic : Property := fun S =>
  -- For any effective OI, raw basis sum and current index,
  ∀ (oi_eff_q raw_basis_sum_q current_a : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values satisfying the `requires`,
    U128 oi_eff_q → U128 raw_basis_sum_q → U128 current_a →
    side_adl_index_requires raw_basis_sum_q →
    ⊢ -- then the call does not unwind (panic).
      callSideAdlIndex S oi_eff_q raw_basis_sum_q current_a (S.returns fun _ => iprop(True))

/-- K59.exposure: for `raw_basis_sum_q <= 2 * MAX_OI_SIDE_Q`, if `kernel_side_adl_index(..)`
returns `Ok(A)` with `raw_basis_sum_q > 0` and `oi_eff_q > 0`, then
`A * raw_basis_sum_q <= ADL_ONE * oi_eff_q < (A + 1) * raw_basis_sum_q` (exact arithmetic). -/
def kernel_side_adl_index_exposure : Property := fun S =>
  -- For any effective OI, raw basis sum and current index,
  ∀ (oi_eff_q raw_basis_sum_q current_a : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values and the side holds at most `2 * MAX_OI_SIDE_Q` raw basis,
    U128 oi_eff_q → U128 raw_basis_sum_q → U128 current_a →
    raw_basis_sum_q ≤ 2 * MAX_OI_SIDE_Q →
    ⊢ -- then, on normal return of the call,
      callSideAdlIndex S oi_eff_q raw_basis_sum_q current_a (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(A)` where (if `raw_basis_sum_q > 0` and `oi_eff_q > 0`)
        -- `A * raw_basis_sum_q <= ADL_ONE * oi_eff_q < (A + 1) * raw_basis_sum_q`,
        -- or some `Err(_)`.
        U128Result S (side_adl_index_exposure oi_eff_q raw_basis_sum_q) True v)

/-! ## K60 `kernel_raw_basis_after_adl_reduction` -/

/-- The call `V16Core::kernel_raw_basis_after_adl_reduction(raw_abs_q, effective_abs_q,
next_effective_abs_q, oi_eff_q, raw_basis_sum_q)` with postcondition `Φ`. -/
def callRawBasisAfterReduction {PROP : Type} [BI PROP] (S : Vocab PROP)
    (raw_abs_q effective_abs_q next_effective_abs_q oi_eff_q raw_basis_sum_q : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction»
    [(S.intVal raw_abs_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction» 0),
     (S.intVal effective_abs_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction» 1),
     (S.intVal next_effective_abs_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction» 2),
     (S.intVal oi_eff_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction» 3),
     (S.intVal raw_basis_sum_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_raw_basis_after_adl_reduction» 4)]
    Φ

/-- K60.contract: under the `requires`, if `kernel_raw_basis_after_adl_reduction(..)` returns,
its result is `Ok(next_raw)` satisfying the `Ok` arm of the `ensures`
(`raw_basis_after_adl_reduction_ensures`), or an `Err(_)` (the `Err(_) => true` arm). -/
def kernel_raw_basis_after_adl_reduction_contract : Property := fun S =>
  -- For any raw basis, effective quantities, effective OI and raw basis sum,
  ∀ (raw_abs_q effective_abs_q next_effective_abs_q oi_eff_q raw_basis_sum_q : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values
    U128 raw_abs_q → U128 effective_abs_q → U128 next_effective_abs_q → U128 oi_eff_q →
    U128 raw_basis_sum_q →
    -- satisfying the `requires` (`raw_basis_sum_q <= ADL_ONE && raw_abs_q <= raw_basis_sum_q`),
    raw_basis_after_adl_reduction_requires raw_abs_q raw_basis_sum_q →
    ⊢ -- then, on normal return of the call,
      callRawBasisAfterReduction S raw_abs_q effective_abs_q next_effective_abs_q oi_eff_q
        raw_basis_sum_q (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(next_raw)` with the `Ok` arm of K60's `ensures`, or an `Err(_)`.
        U128Result S (raw_basis_after_adl_reduction_ensures raw_abs_q effective_abs_q
          next_effective_abs_q oi_eff_q raw_basis_sum_q) True v)

/-- K60.nopanic: under the `requires`, `kernel_raw_basis_after_adl_reduction(..)` does not
panic. -/
def kernel_raw_basis_after_adl_reduction_nopanic : Property := fun S =>
  -- For any raw basis, effective quantities, effective OI and raw basis sum,
  ∀ (raw_abs_q effective_abs_q next_effective_abs_q oi_eff_q raw_basis_sum_q : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values satisfying the `requires`,
    U128 raw_abs_q → U128 effective_abs_q → U128 next_effective_abs_q → U128 oi_eff_q →
    U128 raw_basis_sum_q →
    raw_basis_after_adl_reduction_requires raw_abs_q raw_basis_sum_q →
    ⊢ -- then the call does not unwind (panic).
      callRawBasisAfterReduction S raw_abs_q effective_abs_q next_effective_abs_q oi_eff_q
        raw_basis_sum_q (S.returns fun _ => iprop(True))

/-- K60.progress: for `raw_basis_sum_q <= 2 * MAX_OI_SIDE_Q`, if
`kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)` returns `Ok(next_raw)`, then
`1 <= next_raw < raw`, `oi - (e - e2) <= (raw_sum - raw) + next_raw` (exact arithmetic), and
`oi == raw_sum` implies `next_raw == e2`. -/
def kernel_raw_basis_after_adl_reduction_progress : Property := fun S =>
  -- For any raw basis, effective quantities, effective OI and raw basis sum,
  ∀ (raw e e2 oi raw_sum : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values and the side holds at most `2 * MAX_OI_SIDE_Q` raw basis,
    U128 raw → U128 e → U128 e2 → U128 oi → U128 raw_sum →
    raw_sum ≤ 2 * MAX_OI_SIDE_Q →
    ⊢ -- then, on normal return of the call,
      callRawBasisAfterReduction S raw e e2 oi raw_sum (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(next_raw)` with the progress statement, or some `Err(_)`.
        U128Result S (raw_basis_after_adl_reduction_progress raw e e2 oi raw_sum) True v)

/-! ## K35 `kernel_attach_leg` -/

/-- The call `V16Core::kernel_attach_leg(asset, side, basis_pos_q, loss_weight,
asset_index_u32)` with postcondition `Φ`. -/
def callAttachLeg {PROP : Type} [BI PROP] (S : Vocab PROP) (asset : AssetState) (side : Side)
    (basis_pos_q : Int) (loss_weight asset_index_u32 : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg»
    [(assetValue S asset, S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg» 0),
     (sideValue S side, S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg» 1),
     (S.intVal basis_pos_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg» 2),
     (S.intVal loss_weight,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg» 3),
     (S.intVal asset_index_u32,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_attach_leg» 4)] Φ

/-- K35.contract: under the `requires`, if `kernel_attach_leg(..)` returns, its result is
`Ok((a, l))` satisfying the `ensures` (`attach_leg_ensures`), or an `Err(_)`. -/
def kernel_attach_leg_contract : Property := fun S =>
  -- For any asset state, side, signed basis, loss weight and asset index,
  ∀ (asset : AssetState) (side : Side) (basis_pos_q : Int) (loss_weight asset_index_u32 : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types
    asset.WF → I128 basis_pos_q → U128 loss_weight → U32 asset_index_u32 →
    -- and satisfy the `requires` (`basis_pos_q != 0 && basis_pos_q > i128::MIN`),
    attach_leg_requires basis_pos_q →
    ⊢ -- then, on normal return of the call,
      callAttachLeg S asset side basis_pos_q loss_weight asset_index_u32 (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((a, l))` with K35's `ensures`, or an `Err(_)`.
        AssetLegResult S (attach_leg_ensures asset side basis_pos_q loss_weight
          asset_index_u32) v)

/-- K35.nopanic: under the `requires`, `kernel_attach_leg(..)` does not panic. -/
def kernel_attach_leg_nopanic : Property := fun S =>
  -- For any asset state, side, signed basis, loss weight and asset index,
  ∀ (asset : AssetState) (side : Side) (basis_pos_q : Int) (loss_weight asset_index_u32 : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types and satisfy the `requires`,
    asset.WF → I128 basis_pos_q → U128 loss_weight → U32 asset_index_u32 →
    attach_leg_requires basis_pos_q →
    ⊢ -- then the call does not unwind (panic).
      callAttachLeg S asset side basis_pos_q loss_weight asset_index_u32
        (S.returns fun _ => iprop(True))

/-! ## K07 `kernel_resize_leg_same_side` -/

/-- The call `V16Core::kernel_resize_leg_same_side(leg, asset, new_signed, new_weight,
preserve_pending_obligation_weight, old_effective_abs, new_effective_abs)` with
postcondition `Φ`. -/
def callResizeLeg {PROP : Type} [BI PROP] (S : Vocab PROP) (leg : Leg) (asset : AssetState)
    (new_signed : Int) (new_weight : Nat) (preserve : Bool)
    (old_effective_abs new_effective_abs : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side»
    [(legValue S leg,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 0),
     (assetValue S asset,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 1),
     (S.intVal new_signed,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 2),
     (S.intVal new_weight,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 3),
     (S.boolVal preserve,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 4),
     (S.intVal old_effective_abs,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 5),
     (S.intVal new_effective_abs,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_resize_leg_same_side» 6)] Φ

/-- K07.contract: under the `requires`, if `kernel_resize_leg_same_side(..)` returns, its result
is `Ok((l, a))` satisfying the `ensures` (`resize_leg_ensures`), or an `Err(_)`. -/
def kernel_resize_leg_same_side_contract : Property := fun S =>
  -- For any leg, asset state, new basis, new weight, flag and effective quantities,
  ∀ (leg : Leg) (asset : AssetState) (new_signed : Int) (new_weight : Nat)
    (preserve_pending_obligation_weight : Bool) (old_effective_abs new_effective_abs : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types
    leg.WF → asset.WF → I128 new_signed → U128 new_weight → U128 old_effective_abs →
    U128 new_effective_abs →
    -- and satisfy the `requires` (`new_signed != 0`),
    resize_leg_requires new_signed →
    ⊢ -- then, on normal return of the call,
      callResizeLeg S leg asset new_signed new_weight preserve_pending_obligation_weight
        old_effective_abs new_effective_abs (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((l, a))` with K07's `ensures`, or an `Err(_)`.
        LegAssetResult S (resize_leg_ensures leg asset new_signed new_weight
          preserve_pending_obligation_weight old_effective_abs new_effective_abs) v)

/-- K07.nopanic: under the `requires`, `kernel_resize_leg_same_side(..)` does not panic. -/
def kernel_resize_leg_same_side_nopanic : Property := fun S =>
  -- For any leg, asset state, new basis, new weight, flag and effective quantities,
  ∀ (leg : Leg) (asset : AssetState) (new_signed : Int) (new_weight : Nat)
    (preserve_pending_obligation_weight : Bool) (old_effective_abs new_effective_abs : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types and satisfy the `requires`,
    leg.WF → asset.WF → I128 new_signed → U128 new_weight → U128 old_effective_abs →
    U128 new_effective_abs → resize_leg_requires new_signed →
    ⊢ -- then the call does not unwind (panic).
      callResizeLeg S leg asset new_signed new_weight preserve_pending_obligation_weight
        old_effective_abs new_effective_abs (S.returns fun _ => iprop(True))

/-! ## K34 `kernel_clear_leg` -/

/-- The bytes of the promoted constant `&SideModeV16::ResetPending` that `kernel_clear_leg`
compares the side's `mode_*` with: the single discriminant byte `2`, in read-only memory. -/
def resetPendingConstant {PROP : Type} [BI PROP] (S : Vocab PROP) : PROP :=
  S.global Mir.KernelC.Global.«const [1 bytes] 165f0cf0fda1e660» 1 [2]

/-- The call `V16Core::kernel_clear_leg(leg, asset, clear_effective_oi_q)` with
postcondition `Φ`. -/
def callClearLeg {PROP : Type} [BI PROP] (S : Vocab PROP) (leg : Leg) (asset : AssetState)
    (clear_effective_oi_q : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_clear_leg»
    [(legValue S leg, S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_clear_leg» 0),
     (assetValue S asset, S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_clear_leg» 1),
     (S.intVal clear_effective_oi_q,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_clear_leg» 2)] Φ

/-- K34.contract: if `kernel_clear_leg(..)` returns, its result is `Ok(a)` satisfying the
`ensures` (`clear_leg_ensures`), or an `Err(_)`. -/
def kernel_clear_leg_contract : Property := fun S =>
  -- For any leg, asset state and effective OI to clear,
  ∀ (leg : Leg) (asset : AssetState) (clear_effective_oi_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → asset.WF → U128 clear_effective_oi_q →
    -- and the promoted constant `&SideModeV16::ResetPending` is in read-only memory,
    resetPendingConstant S ⊢
      -- then, on normal return of the call,
      callClearLeg S leg asset clear_effective_oi_q (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(a)` with K34's `ensures`, or an `Err(_)`.
        AssetResult S (clear_leg_ensures leg asset clear_effective_oi_q) True v)

/-- K34.nopanic: `kernel_clear_leg(..)` does not panic. -/
def kernel_clear_leg_nopanic : Property := fun S =>
  -- For any leg, asset state and effective OI to clear,
  ∀ (leg : Leg) (asset : AssetState) (clear_effective_oi_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → asset.WF → U128 clear_effective_oi_q →
    -- and the promoted constant `&SideModeV16::ResetPending` is in read-only memory,
    resetPendingConstant S ⊢
      -- then the call does not unwind (panic).
      callClearLeg S leg asset clear_effective_oi_q (S.returns fun _ => iprop(True))

/-! ## K32 `kernel_retain_leg_as_pending_obligation` -/

/-- The call `V16Core::kernel_retain_leg_as_pending_obligation(leg, asset,
retained_effective_oi_q)` with postcondition `Φ`. -/
def callRetainLeg {PROP : Type} [BI PROP] (S : Vocab PROP) (leg : Leg) (asset : AssetState)
    (retained_effective_oi_q : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_retain_leg_as_pending_obligation»
    [(legValue S leg, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_retain_leg_as_pending_obligation» 0),
     (assetValue S asset, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_retain_leg_as_pending_obligation» 1),
     (S.intVal retained_effective_oi_q, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_retain_leg_as_pending_obligation» 2)] Φ

/-- K32.contract: if `kernel_retain_leg_as_pending_obligation(..)` returns, its result is
`Ok((l, a))` satisfying the `ensures` (`retain_leg_ensures`), or an `Err(_)`. -/
def kernel_retain_leg_as_pending_obligation_contract : Property := fun S =>
  -- For any leg, asset state and retained effective OI,
  ∀ (leg : Leg) (asset : AssetState) (retained_effective_oi_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → asset.WF → U128 retained_effective_oi_q →
    ⊢ -- then, on normal return of the call,
      callRetainLeg S leg asset retained_effective_oi_q (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((l, a))` with K32's `ensures`, or an `Err(_)`.
        LegAssetResult S (retain_leg_ensures leg asset retained_effective_oi_q) v)

/-- K32.nopanic: `kernel_retain_leg_as_pending_obligation(..)` does not panic. -/
def kernel_retain_leg_as_pending_obligation_nopanic : Property := fun S =>
  -- For any leg, asset state and retained effective OI,
  ∀ (leg : Leg) (asset : AssetState) (retained_effective_oi_q : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → asset.WF → U128 retained_effective_oi_q →
    ⊢ -- then the call does not unwind (panic).
      callRetainLeg S leg asset retained_effective_oi_q (S.returns fun _ => iprop(True))

/-! ## K02 `kernel_mark_kf_stale_cohorts` -/

/-- The call `V16Core::kernel_mark_kf_stale_cohorts(asset, long_changed, short_changed,
cohort_epoch)` with postcondition `Φ`. -/
def callMarkKfStaleCohorts {PROP : Type} [BI PROP] (S : Vocab PROP) (asset : AssetState)
    (long_changed short_changed : Bool) (cohort_epoch : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts»
    [(assetValue S asset, assetTy S),
     (S.boolVal long_changed,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 1),
     (S.boolVal short_changed,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 2),
     (S.intVal cohort_epoch,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 3)] Φ

/-- K02.contract: if `kernel_mark_kf_stale_cohorts(..)` returns, its result is `Ok(a)`
satisfying the `Ok` arm of the `ensures` (`mark_kf_stale_cohorts_ensures`, whose frame
`kf_cohort_frame_eq` covers every field of the new record), or an `Err(_)` satisfying its `Err`
arm (`mark_kf_stale_cohorts_err`). -/
def kernel_mark_kf_stale_cohorts_contract : Property := fun S =>
  -- For any asset state, change flags and cohort epoch,
  ∀ (asset : AssetState) (long_changed short_changed : Bool) (cohort_epoch : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    asset.WF → U64 cohort_epoch →
    ⊢ -- then, on normal return of the call,
      callMarkKfStaleCohorts S asset long_changed short_changed cohort_epoch
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok(a)` with the `Ok` arm of K02's `ensures`, or an `Err(_)` with
          -- its `Err` arm.
          AssetResult S (mark_kf_stale_cohorts_ensures asset long_changed short_changed
              cohort_epoch)
            (mark_kf_stale_cohorts_err asset long_changed short_changed cohort_epoch) v)

/-- K02.nopanic: `kernel_mark_kf_stale_cohorts(..)` does not panic. -/
def kernel_mark_kf_stale_cohorts_nopanic : Property := fun S =>
  -- For any asset state, change flags and cohort epoch,
  ∀ (asset : AssetState) (long_changed short_changed : Bool) (cohort_epoch : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    asset.WF → U64 cohort_epoch →
    ⊢ -- then the call does not unwind (panic).
      callMarkKfStaleCohorts S asset long_changed short_changed cohort_epoch
        (S.returns fun _ => iprop(True))

/-! ## K03 `kernel_settle_kf_stale_cohort` -/

/-- The call `V16Core::kernel_settle_kf_stale_cohort(asset, side, leg_kf_epoch_snap)` with
postcondition `Φ`. -/
def callSettleKfStaleCohort {PROP : Type} [BI PROP] (S : Vocab PROP) (asset : AssetState)
    (side : Side) (leg_kf_epoch_snap : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort»
    [(assetValue S asset,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 0),
     (sideValue S side,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 1),
     (S.intVal leg_kf_epoch_snap,
        S.argTy Mir.KernelC.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 2)] Φ

/-- A `V16Result<(AssetStateV16, u64)>` value that is either `Ok((a, epoch))` for in-range `a`,
`epoch` with `ens a epoch`, or some `Err(e)` where `E` holds. -/
def AssetEpochResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : AssetState → Nat → Prop)
    (E : Prop) (v : S.Value) : Prop :=
  (∃ a epoch, a.WF ∧ U64 epoch ∧
    v = okValue S (S.tupleVal [assetValue S a, S.intVal epoch]) ∧ ens a epoch) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- K03.contract: if `kernel_settle_kf_stale_cohort(..)` returns, its result is
`Ok((a, epoch))` satisfying the `Ok` arm of the `ensures` (`settle_kf_stale_cohort_ensures`),
or an `Err(_)` satisfying its `Err` arm (`settle_kf_stale_cohort_err`). -/
def kernel_settle_kf_stale_cohort_contract : Property := fun S =>
  -- For any asset state, side and leg epoch snapshot,
  ∀ (asset : AssetState) (side : Side) (leg_kf_epoch_snap : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    asset.WF → U64 leg_kf_epoch_snap →
    ⊢ -- then, on normal return of the call,
      callSettleKfStaleCohort S asset side leg_kf_epoch_snap (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((a, epoch))` with the `Ok` arm of K03's `ensures`, or an
        -- `Err(_)` with its `Err` arm.
        AssetEpochResult S (settle_kf_stale_cohort_ensures asset side leg_kf_epoch_snap)
          (settle_kf_stale_cohort_err asset side leg_kf_epoch_snap) v)

/-- K03.nopanic: `kernel_settle_kf_stale_cohort(..)` does not panic. -/
def kernel_settle_kf_stale_cohort_nopanic : Property := fun S =>
  -- For any asset state, side and leg epoch snapshot,
  ∀ (asset : AssetState) (side : Side) (leg_kf_epoch_snap : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    asset.WF → U64 leg_kf_epoch_snap →
    ⊢ -- then the call does not unwind (panic).
      callSettleKfStaleCohort S asset side leg_kf_epoch_snap (S.returns fun _ => iprop(True))

/-! ## K26 `apply_bankruptcy_residual_chunk_to_loss_side` -/

/-- The call `V16Core::apply_bankruptcy_residual_chunk_to_loss_side(asset, opp, engine_chunk,
residual_remaining)`, with `asset` the reference `p`, with postcondition `Φ`. -/
def callResidualChunk {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (opp : Side)
    (engine_chunk residual_remaining : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.KernelC.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side»
    [(S.ptrVal p, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 0),
     (sideValue S opp, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 1),
     (S.intVal engine_chunk, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 2),
     (S.intVal residual_remaining, S.argTy
        Mir.KernelC.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 3)]
    Φ

/-- The returned `V16Result<Option<BResidualBookingOutcomeV16>>` value of K26 is `Ok(None)`,
`Ok(Some(o))` for an in-range `o` satisfying the `Ok(Some(o))` clause of the `ensures`
(`residual_chunk_booked_ensures`), or some `Err(e)` (the `_ => true` arm). -/
def ResidualChunkResult {PROP : Type} [BI PROP] (S : Vocab PROP)
    (engine_chunk residual_remaining : Nat) (v : S.Value) : Prop :=
  v = okValue S (bookedValue S none) ∨
  (∃ o : BResidualOutcome, o.WF ∧ v = okValue S (bookedValue S (some o)) ∧
    residual_chunk_booked_ensures engine_chunk residual_remaining o) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- K26.contract: given ownership of `*asset`, if
`apply_bankruptcy_residual_chunk_to_loss_side(asset, opp, engine_chunk, residual_remaining)`
returns, the caller gets back ownership of `*asset`, whose new value differs from the old one at
most in the loss side's `b_*_num` and `social_loss_remainder_*_num` (so `raw_basis_sum_long_q`
and `raw_basis_sum_short_q` are unchanged), the side-isolation part of the `ensures` holds
between the old and new values, and the result satisfies the rest of the `ensures`. Only
`*asset` is accessed (`modifies(asset)`). -/
def apply_bankruptcy_residual_chunk_to_loss_side_contract : Property := fun S =>
  -- For any reference, asset state, loss side and amounts,
  ∀ (p : S.Ptr) (asset : AssetState) (opp : Side) (engine_chunk residual_remaining : Nat),
    -- PRECONDITIONS
    -- if the values are in range of their Rust types (no `requires`),
    asset.WF → U128 engine_chunk → U128 residual_remaining →
    -- and we own the `AssetStateV16` at `p`, holding `asset`,
    S.pointsTo p (assetTy S) (assetValue S asset) ⊢
      -- then, on normal return of the call (with `p` passed as `asset`),
      callResidualChunk S p opp engine_chunk residual_remaining (fun o => match o with
        | .returned v => iprop(
          -- POSTCONDITIONS
          -- there is a new value `asset'` of `*asset` such that
          ∃ asset' : AssetState,
            -- we own the `AssetStateV16` at `p` again, now holding `asset'`,
            S.pointsTo p (assetTy S) (assetValue S asset') ∗
            -- `asset'` is in range,
            ⌊asset'.WF⌋ ∗
            -- only the loss side's B-index and social-loss remainder may have changed,
            ⌊residual_chunk_frame opp asset asset'⌋ ∗
            -- the loss side's B-index only grew and the other side's B-index and
            -- social-loss remainder are unchanged (`old(asset)` is `asset`),
            ⌊residual_chunk_isolated opp asset asset'⌋ ∗
            -- and the result satisfies the rest of K26's `ensures`.
            ⌊ResidualChunkResult S engine_chunk residual_remaining v⌋)
        -- (an unwinding call is not constrained here).
        | .unwound => iprop(True))

/-- K26.nopanic: given ownership of `*asset`,
`apply_bankruptcy_residual_chunk_to_loss_side(..)` does not panic. -/
def apply_bankruptcy_residual_chunk_to_loss_side_nopanic : Property := fun S =>
  -- For any reference, asset state, loss side and amounts,
  ∀ (p : S.Ptr) (asset : AssetState) (opp : Side) (engine_chunk residual_remaining : Nat),
    -- PRECONDITIONS
    -- if the values are in range of their Rust types (no `requires`),
    asset.WF → U128 engine_chunk → U128 residual_remaining →
    -- and we own the `AssetStateV16` at `p`, holding `asset`,
    S.pointsTo p (assetTy S) (assetValue S asset) ⊢
      -- then the call does not unwind (panic).
      callResidualChunk S p opp engine_chunk residual_remaining (S.returns fun _ => iprop(True))

end Spec.KernelC
