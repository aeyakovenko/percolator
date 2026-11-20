module

public import Spec.KernelModel5

/-!
# Independent mathematical model for the kernels of engine revision `c030aa40`

A mathematical reading of the `kani::requires` / `kani::ensures` / `kani::modifies` clauses of
these associated functions of `V16Core` in `src/v16.rs` at engine revision `c030aa40`:

* new kernels: K59 `kernel_side_adl_index`, K60 `kernel_raw_basis_after_adl_reduction`;
* kernels whose bodies and contracts changed: K35 `kernel_attach_leg`,
  K07 `kernel_resize_leg_same_side`, K34 `kernel_clear_leg`,
  K32 `kernel_retain_leg_as_pending_obligation`;
* kernels with unchanged text that take or return the enlarged `AssetStateV16`:
  K02 `kernel_mark_kf_stale_cohorts`, K03 `kernel_settle_kf_stale_cohort`,
  K26 `apply_bankruptcy_residual_chunk_to_loss_side`.

Conventions (as in `Spec.KernelModel` ... `Spec.KernelModel5`, whose definitions are reused):

* Rust unsigned integers are modelled by `Nat`, signed ones by `Int`; `U64`, `U128`, `I128`,
  `U32` say a value is representable in its Rust type, and `*.WF` says so of every field.
* `wrapping_add` / `wrapping_sub` / `wrapping_mul` on `u128` are arithmetic modulo `2^128`
  (`wrapAdd128`, `wrapSub128`, `wrapMul128`); on `u64`, `wrapAdd64`, `wrapSub64`.
  `saturating_add` on `u128` is `satAdd128`; `checked_add(1)` on `u64` is `checkedAdd64 · 1`.
* Parameter names denote the values the caller passed, also where the body mutates or rebinds
  a by-value parameter (`mut asset`, `mut leg`, `let mut leg = leg`).
* A `V16Error` is identified by its Rust discriminant (`InvalidConfig = 0`, `InvalidLeg = 4`,
  ..., `CounterUnderflow = 11`).
* Derived `PartialEq` on these `Copy` structs is structural equality, i.e. `=`.
-/

@[expose] public section

namespace Spec.KernelC

open Spec.Kernel (U32 U64 U128 I128 two64 two128 wrapAdd128 wrapSub128 wrapAdd64 wrapSub64
  satAdd128 checkedAdd128 SOCIAL_LOSS_DEN MAX_OI_SIDE_Q Side Leg AssetLifecycle SideMode
  BResidualOutcome)

/-! ## Shared definitions -/

/-- `ADL_ONE` (`src/lib.rs`): `1_000_000_000_000_000`. -/
def ADL_ONE : Nat := 1000000000000000

/-- Rust `a.wrapping_mul(b)` on `u128`: multiplication modulo `2^128`. -/
def wrapMul128 (a b : Nat) : Nat := (a * b) % two128

/-- Rust `a.checked_add(b)` on `u64`. -/
def checkedAdd64 (a b : Nat) : Option Nat := if a + b < two64 then some (a + b) else none

/-- Rust `x.unsigned_abs()` for an `i128` `x`: its absolute value. -/
def uabs (x : Int) : Nat := x.natAbs

/-- The discriminant of `V16Error::InvalidLeg`. -/
def InvalidLeg : Nat := 4

/-! ## `AssetStateV16` at revision `c030aa40` -/

/-- Rust `struct AssetStateV16` at revision `c030aa40` (all 46 fields, in declaration order;
`raw_basis_sum_long_q` and `raw_basis_sum_short_q` are new). -/
structure AssetState where
  /-- `market_id: u64` -/
  market_id : Nat
  /-- `retired_slot: u64` -/
  retired_slot : Nat
  /-- `lifecycle: AssetLifecycleV16` -/
  lifecycle : AssetLifecycle
  /-- `raw_oracle_target_price: u64` -/
  raw_oracle_target_price : Nat
  /-- `effective_price: u64` -/
  effective_price : Nat
  /-- `fund_px_last: u64` -/
  fund_px_last : Nat
  /-- `slot_last: u64` -/
  slot_last : Nat
  /-- `a_long: u128` -/
  a_long : Nat
  /-- `a_short: u128` -/
  a_short : Nat
  /-- `k_long: i128` -/
  k_long : Int
  /-- `k_short: i128` -/
  k_short : Int
  /-- `f_long_num: i128` -/
  f_long_num : Int
  /-- `f_short_num: i128` -/
  f_short_num : Int
  /-- `kf_epoch_long: u64` -/
  kf_epoch_long : Nat
  /-- `kf_epoch_short: u64` -/
  kf_epoch_short : Nat
  /-- `k_epoch_start_long: i128` -/
  k_epoch_start_long : Int
  /-- `k_epoch_start_short: i128` -/
  k_epoch_start_short : Int
  /-- `f_epoch_start_long_num: i128` -/
  f_epoch_start_long_num : Int
  /-- `f_epoch_start_short_num: i128` -/
  f_epoch_start_short_num : Int
  /-- `b_long_num: u128` -/
  b_long_num : Nat
  /-- `b_short_num: u128` -/
  b_short_num : Nat
  /-- `b_epoch_start_long_num: u128` -/
  b_epoch_start_long_num : Nat
  /-- `b_epoch_start_short_num: u128` -/
  b_epoch_start_short_num : Nat
  /-- `oi_eff_long_q: u128` -/
  oi_eff_long_q : Nat
  /-- `oi_eff_short_q: u128` -/
  oi_eff_short_q : Nat
  /-- `raw_basis_sum_long_q: u128` (new) -/
  raw_basis_sum_long_q : Nat
  /-- `raw_basis_sum_short_q: u128` (new) -/
  raw_basis_sum_short_q : Nat
  /-- `stored_pos_count_long: u64` -/
  stored_pos_count_long : Nat
  /-- `stored_pos_count_short: u64` -/
  stored_pos_count_short : Nat
  /-- `stale_account_count_long: u64` -/
  stale_account_count_long : Nat
  /-- `stale_account_count_short: u64` -/
  stale_account_count_short : Nat
  /-- `pending_obligation_count_long: u64` -/
  pending_obligation_count_long : Nat
  /-- `pending_obligation_count_short: u64` -/
  pending_obligation_count_short : Nat
  /-- `loss_weight_sum_long: u128` -/
  loss_weight_sum_long : Nat
  /-- `loss_weight_sum_short: u128` -/
  loss_weight_sum_short : Nat
  /-- `social_loss_remainder_long_num: u128` -/
  social_loss_remainder_long_num : Nat
  /-- `social_loss_remainder_short_num: u128` -/
  social_loss_remainder_short_num : Nat
  /-- `social_loss_dust_long_num: u128` -/
  social_loss_dust_long_num : Nat
  /-- `social_loss_dust_short_num: u128` -/
  social_loss_dust_short_num : Nat
  /-- `explicit_unallocated_loss_long: u128` -/
  explicit_unallocated_loss_long : Nat
  /-- `explicit_unallocated_loss_short: u128` -/
  explicit_unallocated_loss_short : Nat
  /-- `epoch_long: u64` -/
  epoch_long : Nat
  /-- `epoch_short: u64` -/
  epoch_short : Nat
  /-- `mode_long: SideModeV16` -/
  mode_long : SideMode
  /-- `mode_short: SideModeV16` -/
  mode_short : SideMode
  deriving DecidableEq, Repr

/-- Every integer field of an `AssetStateV16` value is in range of its Rust type. -/
def AssetState.WF (a : AssetState) : Prop :=
  U64 a.market_id ∧ U64 a.retired_slot ∧ U64 a.raw_oracle_target_price ∧
  U64 a.effective_price ∧ U64 a.fund_px_last ∧ U64 a.slot_last ∧ U128 a.a_long ∧
  U128 a.a_short ∧ I128 a.k_long ∧ I128 a.k_short ∧ I128 a.f_long_num ∧ I128 a.f_short_num ∧
  U64 a.kf_epoch_long ∧ U64 a.kf_epoch_short ∧ I128 a.k_epoch_start_long ∧
  I128 a.k_epoch_start_short ∧ I128 a.f_epoch_start_long_num ∧
  I128 a.f_epoch_start_short_num ∧ U128 a.b_long_num ∧ U128 a.b_short_num ∧
  U128 a.b_epoch_start_long_num ∧ U128 a.b_epoch_start_short_num ∧ U128 a.oi_eff_long_q ∧
  U128 a.oi_eff_short_q ∧ U128 a.raw_basis_sum_long_q ∧ U128 a.raw_basis_sum_short_q ∧
  U64 a.stored_pos_count_long ∧ U64 a.stored_pos_count_short ∧
  U64 a.stale_account_count_long ∧ U64 a.stale_account_count_short ∧
  U64 a.pending_obligation_count_long ∧ U64 a.pending_obligation_count_short ∧
  U128 a.loss_weight_sum_long ∧ U128 a.loss_weight_sum_short ∧
  U128 a.social_loss_remainder_long_num ∧ U128 a.social_loss_remainder_short_num ∧
  U128 a.social_loss_dust_long_num ∧ U128 a.social_loss_dust_short_num ∧
  U128 a.explicit_unallocated_loss_long ∧ U128 a.explicit_unallocated_loss_short ∧
  U64 a.epoch_long ∧ U64 a.epoch_short

/-! ## K59 `kernel_side_adl_index(oi_eff_q: u128, raw_basis_sum_q: u128, current_a: u128)
-> V16Result<u128>` -/

/-- K59 `requires(raw_basis_sum_q <= ADL_ONE)`. -/
def side_adl_index_requires (raw_basis_sum_q : Nat) : Prop := raw_basis_sum_q ≤ ADL_ONE

/-- K59 `ensures` for `Ok(a)`: if `raw_basis_sum_q == 0` then `a == current_a`; otherwise
`oi_eff_q <= raw_basis_sum_q`, `1 <= a <= ADL_ONE`, `(a == ADL_ONE) == (oi_eff_q ==
raw_basis_sum_q)`, `oi_eff_q == 0 || a.wrapping_mul(raw_basis_sum_q) <=
ADL_ONE.wrapping_mul(oi_eff_q)`, `ADL_ONE.wrapping_mul(oi_eff_q) <
a.wrapping_add(1).wrapping_mul(raw_basis_sum_q)` and `oi_eff_q != 0 || a == 1`. -/
def side_adl_index_ensures (oi_eff_q raw_basis_sum_q current_a : Nat) (a : Nat) : Prop :=
  if raw_basis_sum_q = 0 then a = current_a
  else
    oi_eff_q ≤ raw_basis_sum_q ∧ a ≥ 1 ∧ a ≤ ADL_ONE ∧
    (a = ADL_ONE ↔ oi_eff_q = raw_basis_sum_q) ∧
    (oi_eff_q = 0 ∨ wrapMul128 a raw_basis_sum_q ≤ wrapMul128 ADL_ONE oi_eff_q) ∧
    wrapMul128 ADL_ONE oi_eff_q < wrapMul128 (wrapAdd128 a 1) raw_basis_sum_q ∧
    (oi_eff_q ≠ 0 ∨ a = 1)

/-- K59 `ensures` for `Err(e)`: `e == V16Error::InvalidLeg && raw_basis_sum_q != 0 &&
oi_eff_q > raw_basis_sum_q` (`e` given by its discriminant). -/
def side_adl_index_err (oi_eff_q raw_basis_sum_q : Nat) (e : Nat) : Prop :=
  e = InvalidLeg ∧ raw_basis_sum_q ≠ 0 ∧ oi_eff_q > raw_basis_sum_q

/-- K59.exposure (stated in the spec file's last section), for an `Ok(a)` result with
`raw_basis_sum_q > 0` and `oi_eff_q > 0`: `a * raw_basis_sum_q <= ADL_ONE * oi_eff_q <
(a + 1) * raw_basis_sum_q`, in exact integer arithmetic. -/
def side_adl_index_exposure (oi_eff_q raw_basis_sum_q : Nat) (a : Nat) : Prop :=
  raw_basis_sum_q > 0 → oi_eff_q > 0 →
    a * raw_basis_sum_q ≤ ADL_ONE * oi_eff_q ∧ ADL_ONE * oi_eff_q < (a + 1) * raw_basis_sum_q

/-! ## K60 `kernel_raw_basis_after_adl_reduction(raw_abs_q: u128, effective_abs_q: u128,
next_effective_abs_q: u128, oi_eff_q: u128, raw_basis_sum_q: u128) -> V16Result<u128>` -/

/-- K60 `requires(raw_basis_sum_q <= ADL_ONE && raw_abs_q <= raw_basis_sum_q)`. -/
def raw_basis_after_adl_reduction_requires (raw_abs_q raw_basis_sum_q : Nat) : Prop :=
  raw_basis_sum_q ≤ ADL_ONE ∧ raw_abs_q ≤ raw_basis_sum_q

/-- K60 `ensures` for `Ok(next_raw)`, with `reduce_q = effective_abs_q.wrapping_sub(
next_effective_abs_q)`, `oi_after = oi_eff_q.wrapping_sub(reduce_q)` and
`other_raw = raw_basis_sum_q.wrapping_sub(raw_abs_q)`: `next_effective_abs_q != 0`,
`next_effective_abs_q < effective_abs_q <= oi_eff_q <= raw_basis_sum_q`,
`1 <= next_raw < raw_abs_q`, `oi_after <= other_raw.wrapping_add(next_raw)` and
`oi_eff_q != raw_basis_sum_q || next_raw == next_effective_abs_q`. (The `Err` arm is `true`.) -/
def raw_basis_after_adl_reduction_ensures (raw_abs_q effective_abs_q next_effective_abs_q
    oi_eff_q raw_basis_sum_q : Nat) (next_raw : Nat) : Prop :=
  let reduce_q := wrapSub128 effective_abs_q next_effective_abs_q
  let oi_after := wrapSub128 oi_eff_q reduce_q
  let other_raw := wrapSub128 raw_basis_sum_q raw_abs_q
  next_effective_abs_q ≠ 0 ∧ next_effective_abs_q < effective_abs_q ∧
    effective_abs_q ≤ oi_eff_q ∧ oi_eff_q ≤ raw_basis_sum_q ∧
    next_raw ≥ 1 ∧ next_raw < raw_abs_q ∧
    oi_after ≤ wrapAdd128 other_raw next_raw ∧
    (oi_eff_q ≠ raw_basis_sum_q ∨ next_raw = next_effective_abs_q)

/-- K60.progress (stated in the spec file's last section), for an `Ok(next_raw)` result of
`kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)`: `1 <= next_raw < raw`,
`oi - (e - e2) <= (raw_sum - raw) + next_raw` in exact integer arithmetic, and
`oi == raw_sum` implies `next_raw == e2`. -/
def raw_basis_after_adl_reduction_progress (raw e e2 oi raw_sum : Nat) (next_raw : Nat) : Prop :=
  1 ≤ next_raw ∧ next_raw < raw ∧
    (oi : Int) - ((e : Int) - (e2 : Int)) ≤ ((raw_sum : Int) - (raw : Int)) + (next_raw : Int) ∧
    (oi = raw_sum → next_raw = e2)

/-! ## K35 `kernel_attach_leg(mut asset: AssetStateV16, side: SideV16, basis_pos_q: i128,
loss_weight: u128, asset_index_u32: u32) -> V16Result<(AssetStateV16, PortfolioLegV16)>` -/

/-- K35 `requires(basis_pos_q != 0 && basis_pos_q > i128::MIN)`. -/
def attach_leg_requires (basis_pos_q : Int) : Prop :=
  basis_pos_q ≠ 0 ∧ basis_pos_q > -(2 ^ 127 : Int)

/-- The leg the K35 `ensures` describes, field by field: `active`, `asset_index ==
asset_index_u32`, `market_id == asset.market_id`, `side`, `basis_pos_q`, `loss_weight`,
`b_rem == 0`, `!b_stale`, `!stale`, and `a_basis`, `k_snap`, `f_snap`, `kf_epoch_snap`,
`b_snap`, `epoch_snap`, `b_epoch_snap` taken from the side's `a_*`, `k_*`, `f_*_num`,
`kf_epoch_*`, `b_*_num`, `epoch_*`, `epoch_*` (every field of `PortfolioLegV16`). -/
def attach_leg_leg (asset : AssetState) (side : Side) (basis_pos_q : Int)
    (loss_weight asset_index_u32 : Nat) : Leg :=
  match side with
  | .Long =>
    { active := true, asset_index := asset_index_u32, market_id := asset.market_id,
      side := side, basis_pos_q := basis_pos_q, a_basis := asset.a_long,
      k_snap := asset.k_long, f_snap := asset.f_long_num, kf_epoch_snap := asset.kf_epoch_long,
      epoch_snap := asset.epoch_long, loss_weight := loss_weight, b_snap := asset.b_long_num,
      b_rem := 0, b_epoch_snap := asset.epoch_long, b_stale := false, stale := false }
  | .Short =>
    { active := true, asset_index := asset_index_u32, market_id := asset.market_id,
      side := side, basis_pos_q := basis_pos_q, a_basis := asset.a_short,
      k_snap := asset.k_short, f_snap := asset.f_short_num,
      kf_epoch_snap := asset.kf_epoch_short, epoch_snap := asset.epoch_short,
      loss_weight := loss_weight, b_snap := asset.b_short_num, b_rem := 0,
      b_epoch_snap := asset.epoch_short, b_stale := false, stale := false }

/-- The asset the K35 `ensures` describes: on `side`, `oi_eff_*_q` and `raw_basis_sum_*_q`
grow by `abs_q = basis_pos_q.unsigned_abs()`, `loss_weight_sum_*` by `loss_weight` and
`stored_pos_count_*` by one (all `wrapping_add`); every other field equals `asset`'s. (The
clause leaves the other side's `stored_pos_count_*` open; this record fixes it too.) -/
def attach_leg_asset (asset : AssetState) (side : Side) (basis_pos_q : Int)
    (loss_weight : Nat) : AssetState :=
  match side with
  | .Long =>
    { asset with
      oi_eff_long_q := wrapAdd128 asset.oi_eff_long_q (uabs basis_pos_q)
      raw_basis_sum_long_q := wrapAdd128 asset.raw_basis_sum_long_q (uabs basis_pos_q)
      loss_weight_sum_long := wrapAdd128 asset.loss_weight_sum_long loss_weight
      stored_pos_count_long := wrapAdd64 asset.stored_pos_count_long 1 }
  | .Short =>
    { asset with
      oi_eff_short_q := wrapAdd128 asset.oi_eff_short_q (uabs basis_pos_q)
      raw_basis_sum_short_q := wrapAdd128 asset.raw_basis_sum_short_q (uabs basis_pos_q)
      loss_weight_sum_short := wrapAdd128 asset.loss_weight_sum_short loss_weight
      stored_pos_count_short := wrapAdd64 asset.stored_pos_count_short 1 }

/-- K35 `ensures` for `Ok((a, l))`: `l` is exactly the leg built from the declared sources
(`attach_leg_leg`), `l.a_basis == ADL_ONE`, and `a` is `attach_leg_asset`. -/
def attach_leg_ensures (asset : AssetState) (side : Side) (basis_pos_q : Int)
    (loss_weight asset_index_u32 : Nat) (a : AssetState) (l : Leg) : Prop :=
  l = attach_leg_leg asset side basis_pos_q loss_weight asset_index_u32 ∧
    l.a_basis = ADL_ONE ∧ a = attach_leg_asset asset side basis_pos_q loss_weight

/-! ## K07 `kernel_resize_leg_same_side(leg: PortfolioLegV16, asset: AssetStateV16,
new_signed: i128, new_weight: u128, preserve_pending_obligation_weight: bool,
old_effective_abs: u128, new_effective_abs: u128)
-> V16Result<(PortfolioLegV16, AssetStateV16)>` -/

/-- K07 `requires(new_signed != 0)`. -/
def resize_leg_requires (new_signed : Int) : Prop := new_signed ≠ 0

/-- The leg the K07 `ensures` describes: `basis_pos_q == new_signed`, `loss_weight ==
(if preserve_pending_obligation_weight { leg.loss_weight } else { new_weight })`, and the
fourteen other fields equal `leg`'s (the clause lists every field). -/
def resize_leg_leg (leg : Leg) (new_signed : Int) (new_weight : Nat) (preserve : Bool) : Leg :=
  { leg with
    basis_pos_q := new_signed
    loss_weight := if preserve then leg.loss_weight else new_weight }

/-- The asset the K07 `ensures` describes (the clause lists every field): on `leg.side`,
`oi_eff_*_q == asset.oi_eff_*_q.wrapping_sub(old_effective_abs).wrapping_add(new_effective_abs)`,
`raw_basis_sum_*_q == asset.raw_basis_sum_*_q.wrapping_sub(leg.basis_pos_q.unsigned_abs())
.wrapping_add(new_signed.unsigned_abs())` and, unless `preserve_pending_obligation_weight`,
`loss_weight_sum_* == asset.loss_weight_sum_*.wrapping_sub(leg.loss_weight)
.wrapping_add(new_weight)`; every other field equals `asset`'s. -/
def resize_leg_asset (leg : Leg) (asset : AssetState) (new_signed : Int) (new_weight : Nat)
    (preserve : Bool) (old_effective_abs new_effective_abs : Nat) : AssetState :=
  match leg.side with
  | .Long =>
    { asset with
      oi_eff_long_q :=
        wrapAdd128 (wrapSub128 asset.oi_eff_long_q old_effective_abs) new_effective_abs
      raw_basis_sum_long_q :=
        wrapAdd128 (wrapSub128 asset.raw_basis_sum_long_q (uabs leg.basis_pos_q))
          (uabs new_signed)
      loss_weight_sum_long :=
        if preserve then asset.loss_weight_sum_long
        else wrapAdd128 (wrapSub128 asset.loss_weight_sum_long leg.loss_weight) new_weight }
  | .Short =>
    { asset with
      oi_eff_short_q :=
        wrapAdd128 (wrapSub128 asset.oi_eff_short_q old_effective_abs) new_effective_abs
      raw_basis_sum_short_q :=
        wrapAdd128 (wrapSub128 asset.raw_basis_sum_short_q (uabs leg.basis_pos_q))
          (uabs new_signed)
      loss_weight_sum_short :=
        if preserve then asset.loss_weight_sum_short
        else wrapAdd128 (wrapSub128 asset.loss_weight_sum_short leg.loss_weight) new_weight }

/-- K07 `ensures` for `Ok((l, a))`. -/
def resize_leg_ensures (leg : Leg) (asset : AssetState) (new_signed : Int) (new_weight : Nat)
    (preserve : Bool) (old_effective_abs new_effective_abs : Nat) (l : Leg) (a : AssetState) :
    Prop :=
  l = resize_leg_leg leg new_signed new_weight preserve ∧
    a = resize_leg_asset leg asset new_signed new_weight preserve old_effective_abs
      new_effective_abs

/-! ## K34 `kernel_clear_leg(leg: PortfolioLegV16, mut asset: AssetStateV16,
clear_effective_oi_q: u128) -> V16Result<AssetStateV16>` -/

/-- `prior_reset` of the K34 `ensures`: the leg's side is `ResetPending` and
`leg.epoch_snap.checked_add(1) == Some(asset.epoch_*)`. -/
def clear_leg_prior_reset (leg : Leg) (asset : AssetState) : Prop :=
  match leg.side with
  | .Long => asset.mode_long = .ResetPending ∧ checkedAdd64 leg.epoch_snap 1 = some asset.epoch_long
  | .Short =>
    asset.mode_short = .ResetPending ∧ checkedAdd64 leg.epoch_snap 1 = some asset.epoch_short

/-- `obligation` of the K34 `ensures`: `leg.basis_pos_q == 0 && leg.loss_weight != 0`. -/
def clear_leg_obligation (leg : Leg) : Prop := leg.basis_pos_q = 0 ∧ leg.loss_weight ≠ 0

/-- The new social-loss dust of the K34 `ensures` for a side with dust `dust`:
`dust` if `leg.b_rem == 0`, else `dust.wrapping_add(leg.b_rem)` reduced by `SOCIAL_LOSS_DEN`
(`wrapping_sub`) if it reaches `SOCIAL_LOSS_DEN`. -/
def clear_leg_dust (b_rem dust : Nat) : Nat :=
  if b_rem = 0 then dust
  else if wrapAdd128 dust b_rem ≥ SOCIAL_LOSS_DEN then
    wrapSub128 (wrapAdd128 dust b_rem) SOCIAL_LOSS_DEN
  else wrapAdd128 dust b_rem

/-- The new explicit unallocated loss of the K34 `ensures`:
`explicit.saturating_add(if leg.b_rem != 0 && dust.wrapping_add(leg.b_rem) >= SOCIAL_LOSS_DEN
{ 1 } else { 0 })`. -/
def clear_leg_explicit (b_rem dust explicit : Nat) : Nat :=
  satAdd128 explicit
    (if b_rem ≠ 0 ∧ wrapAdd128 dust b_rem ≥ SOCIAL_LOSS_DEN then 1 else 0)

open Classical in
/-- The asset the K34 `ensures` describes (the clause determines every field). On `leg.side`:
`stored_pos_count_*` falls by one (`wrapping_sub`), `pending_obligation_count_*` falls by one if
`obligation`, and, unless `prior_reset`, `oi_eff_*_q` falls by `clear_effective_oi_q`,
`raw_basis_sum_*_q` by `leg.basis_pos_q.unsigned_abs()` and `loss_weight_sum_*` by
`leg.loss_weight` (all `wrapping_sub`), and the side's dust and explicit unallocated loss are
`clear_leg_dust` / `clear_leg_explicit`. Every other field equals `asset`'s. -/
noncomputable def clear_leg_asset (leg : Leg) (asset : AssetState) (clear_effective_oi_q : Nat) :
    AssetState :=
  match leg.side with
  | .Long =>
    let a1 := { asset with
      stored_pos_count_long := wrapSub64 asset.stored_pos_count_long 1
      pending_obligation_count_long :=
        if clear_leg_obligation leg then wrapSub64 asset.pending_obligation_count_long 1
        else asset.pending_obligation_count_long }
    if clear_leg_prior_reset leg asset then a1
    else { a1 with
      oi_eff_long_q := wrapSub128 asset.oi_eff_long_q clear_effective_oi_q
      raw_basis_sum_long_q := wrapSub128 asset.raw_basis_sum_long_q (uabs leg.basis_pos_q)
      loss_weight_sum_long := wrapSub128 asset.loss_weight_sum_long leg.loss_weight
      social_loss_dust_long_num := clear_leg_dust leg.b_rem asset.social_loss_dust_long_num
      explicit_unallocated_loss_long := clear_leg_explicit leg.b_rem
        asset.social_loss_dust_long_num asset.explicit_unallocated_loss_long }
  | .Short =>
    let a1 := { asset with
      stored_pos_count_short := wrapSub64 asset.stored_pos_count_short 1
      pending_obligation_count_short :=
        if clear_leg_obligation leg then wrapSub64 asset.pending_obligation_count_short 1
        else asset.pending_obligation_count_short }
    if clear_leg_prior_reset leg asset then a1
    else { a1 with
      oi_eff_short_q := wrapSub128 asset.oi_eff_short_q clear_effective_oi_q
      raw_basis_sum_short_q := wrapSub128 asset.raw_basis_sum_short_q (uabs leg.basis_pos_q)
      loss_weight_sum_short := wrapSub128 asset.loss_weight_sum_short leg.loss_weight
      social_loss_dust_short_num := clear_leg_dust leg.b_rem asset.social_loss_dust_short_num
      explicit_unallocated_loss_short := clear_leg_explicit leg.b_rem
        asset.social_loss_dust_short_num asset.explicit_unallocated_loss_short }

/-- K34 `ensures` for `Ok(a)`: `a` is `clear_leg_asset leg asset clear_effective_oi_q`. -/
def clear_leg_ensures (leg : Leg) (asset : AssetState) (clear_effective_oi_q : Nat)
    (a : AssetState) : Prop :=
  a = clear_leg_asset leg asset clear_effective_oi_q

/-! ## K32 `kernel_retain_leg_as_pending_obligation(mut leg: PortfolioLegV16,
mut asset: AssetStateV16, retained_effective_oi_q: u128)
-> V16Result<(PortfolioLegV16, AssetStateV16)>` -/

/-- The asset of a K32 `Ok` result: on `leg.side`, `oi_eff_*_q` falls by
`retained_effective_oi_q` and `raw_basis_sum_*_q` by `leg.basis_pos_q.unsigned_abs()`
(`wrapping_sub`), and `pending_obligation_count_*` grows by one (`wrapping_add`); every other
field equals `asset`'s. (The `ensures` states this for the side's and the other side's
`oi_eff_*_q`, `raw_basis_sum_*_q`, `pending_obligation_count_*`, and for
`loss_weight_sum_*` and `stored_pos_count_*`; this record also fixes the remaining fields.) -/
def retain_leg_asset (leg : Leg) (asset : AssetState) (retained_effective_oi_q : Nat) :
    AssetState :=
  match leg.side with
  | .Long =>
    { asset with
      oi_eff_long_q := wrapSub128 asset.oi_eff_long_q retained_effective_oi_q
      raw_basis_sum_long_q := wrapSub128 asset.raw_basis_sum_long_q (uabs leg.basis_pos_q)
      pending_obligation_count_long := wrapAdd64 asset.pending_obligation_count_long 1 }
  | .Short =>
    { asset with
      oi_eff_short_q := wrapSub128 asset.oi_eff_short_q retained_effective_oi_q
      raw_basis_sum_short_q := wrapSub128 asset.raw_basis_sum_short_q (uabs leg.basis_pos_q)
      pending_obligation_count_short := wrapAdd64 asset.pending_obligation_count_short 1 }

/-- K32 `ensures` for `Ok((l, a))`: `l.active`, `l.basis_pos_q == 0` and the fourteen other
fields of `l` equal `leg`'s; `a` is `retain_leg_asset leg asset retained_effective_oi_q`. -/
def retain_leg_ensures (leg : Leg) (asset : AssetState) (retained_effective_oi_q : Nat)
    (l : Leg) (a : AssetState) : Prop :=
  l.active = true ∧ l = { leg with basis_pos_q := 0 } ∧
    a = retain_leg_asset leg asset retained_effective_oi_q

/-! ## K02, K03: the KF stale-cohort kernels on the new record -/

/-- The `cfg(kani)` helper `V16Core::kf_cohort_frame_eq(lhs, rhs)`, read from its body: `lhs`
and `rhs` are equal after setting `kf_epoch_long`, `kf_epoch_short`,
`stale_account_count_long` and `stale_account_count_short` to `0` in both, i.e. they agree on
every other field (including `raw_basis_sum_long_q` and `raw_basis_sum_short_q`). -/
def kf_cohort_frame_eq (lhs rhs : AssetState) : Prop :=
  { lhs with
      kf_epoch_long := 0
      kf_epoch_short := 0
      stale_account_count_long := 0
      stale_account_count_short := 0 } =
    { rhs with
      kf_epoch_long := 0
      kf_epoch_short := 0
      stale_account_count_long := 0
      stale_account_count_short := 0 }

/-- K02 `ensures` for `Ok(a)`. -/
def mark_kf_stale_cohorts_ensures (asset : AssetState) (long_changed short_changed : Bool)
    (cohort_epoch : Nat) (a : AssetState) : Prop :=
  a.kf_epoch_long = (if long_changed then cohort_epoch else asset.kf_epoch_long) ∧
  a.kf_epoch_short = (if short_changed then cohort_epoch else asset.kf_epoch_short) ∧
  a.stale_account_count_long =
    (if long_changed then asset.stored_pos_count_long else asset.stale_account_count_long) ∧
  a.stale_account_count_short =
    (if short_changed then asset.stored_pos_count_short else asset.stale_account_count_short) ∧
  a.stored_pos_count_long = asset.stored_pos_count_long ∧
  a.stored_pos_count_short = asset.stored_pos_count_short ∧
  kf_cohort_frame_eq a asset

/-- K02 `ensures` for `Err(_)`: `(long_changed && cohort_epoch <= asset.kf_epoch_long) ||
(short_changed && cohort_epoch <= asset.kf_epoch_short)`. -/
def mark_kf_stale_cohorts_err (asset : AssetState) (long_changed short_changed : Bool)
    (cohort_epoch : Nat) : Prop :=
  (long_changed = true ∧ cohort_epoch ≤ asset.kf_epoch_long) ∨
  (short_changed = true ∧ cohort_epoch ≤ asset.kf_epoch_short)

/-- K03 `ensures` for `Ok((a, epoch))`. -/
def settle_kf_stale_cohort_ensures (asset : AssetState) (side : Side) (leg_kf_epoch_snap : Nat)
    (a : AssetState) (epoch : Nat) : Prop :=
  epoch = (match side with
    | .Long => asset.kf_epoch_long
    | .Short => asset.kf_epoch_short) ∧
  a.stale_account_count_long =
    (if side = .Long ∧ leg_kf_epoch_snap < asset.kf_epoch_long then
      wrapSub64 asset.stale_account_count_long 1
    else asset.stale_account_count_long) ∧
  a.stale_account_count_short =
    (if side = .Short ∧ leg_kf_epoch_snap < asset.kf_epoch_short then
      wrapSub64 asset.stale_account_count_short 1
    else asset.stale_account_count_short) ∧
  a.stored_pos_count_long = asset.stored_pos_count_long ∧
  a.stored_pos_count_short = asset.stored_pos_count_short ∧
  a.kf_epoch_long = asset.kf_epoch_long ∧
  a.kf_epoch_short = asset.kf_epoch_short ∧
  kf_cohort_frame_eq a asset

/-- K03 `ensures` for `Err(_)`. -/
def settle_kf_stale_cohort_err (asset : AssetState) (side : Side) (leg_kf_epoch_snap : Nat) :
    Prop :=
  match side with
  | .Long => leg_kf_epoch_snap > asset.kf_epoch_long ∨
      (leg_kf_epoch_snap < asset.kf_epoch_long ∧ asset.stale_account_count_long = 0)
  | .Short => leg_kf_epoch_snap > asset.kf_epoch_short ∨
      (leg_kf_epoch_snap < asset.kf_epoch_short ∧ asset.stale_account_count_short = 0)

/-! ## K26 `apply_bankruptcy_residual_chunk_to_loss_side(asset: &mut AssetStateV16, opp: SideV16,
engine_chunk: u128, residual_remaining: u128)`, `modifies(asset)` -/

/-- The side-isolation part of the K26 `ensures` (it holds on every path), for the value
`old_asset` of `*asset` at entry and its value `new_asset` at return: the loss side `opp`'s
B-index only grows, and the other side's B-index and social-loss remainder are unchanged. -/
def residual_chunk_isolated (opp : Side) (old_asset new_asset : AssetState) : Prop :=
  match opp with
  | .Long => new_asset.b_long_num ≥ old_asset.b_long_num ∧
      new_asset.b_short_num = old_asset.b_short_num ∧
      new_asset.social_loss_remainder_short_num = old_asset.social_loss_remainder_short_num
  | .Short => new_asset.b_short_num ≥ old_asset.b_short_num ∧
      new_asset.b_long_num = old_asset.b_long_num ∧
      new_asset.social_loss_remainder_long_num = old_asset.social_loss_remainder_long_num

/-- Which fields of `*asset` may move in K26: only the loss side `opp`'s `b_*_num` and
`social_loss_remainder_*_num`; every other field, among them `raw_basis_sum_long_q` and
`raw_basis_sum_short_q`, keeps its value. (This is more than the `ensures` states.) -/
def residual_chunk_frame (opp : Side) (old_asset new_asset : AssetState) : Prop :=
  match opp with
  | .Long => new_asset = { old_asset with
      b_long_num := new_asset.b_long_num
      social_loss_remainder_long_num := new_asset.social_loss_remainder_long_num }
  | .Short => new_asset = { old_asset with
      b_short_num := new_asset.b_short_num
      social_loss_remainder_short_num := new_asset.social_loss_remainder_short_num }

/-- The K26 `ensures` clause for a returned `Ok(Some(o))`: `o.explicit_loss == 0`,
`o.booked_loss == engine_chunk`, `o.delta_b > 0` and
`o.remaining_after.checked_add(o.booked_loss) == Some(residual_remaining)`. -/
def residual_chunk_booked_ensures (engine_chunk residual_remaining : Nat) (o : BResidualOutcome) :
    Prop :=
  o.explicit_loss = 0 ∧ o.booked_loss = engine_chunk ∧ o.delta_b > 0 ∧
    checkedAdd128 o.remaining_after o.booked_loss = some residual_remaining

end Spec.KernelC
