module

public import Spec.LayerTRep
public import Spec.LayerTConst

/-!
# Layer T: the market shape is preserved by the entry points of classes local and frame

`H(m)` is "`MarketGroupV16View::validate_shape` returns `Ok(())` on the market `m`" in a default
build; its mathematical model is `MarketShapeOk n h` (`Spec.LayerTModel`), for the header fields
`h` and the number `n` of market slots. `Hh(h)` is `HeaderShapeOk h`: every check of
`validate_shape` after its first statement.

Every property below has the form: for all inputs and all starting states satisfying `H` (or
`Hh`), if the call returns `Ok(_)`, then `H` (or `Hh`) holds of the state the call leaves behind.
Each also states exactly which header fields an `Ok` return can have changed, as a record update
of the starting header `h`. A panic (unwinding) is not constrained, and none of the properties
says that the call terminates.

The entry points are compiled at `T = ()`; `activate_empty_market_slot_not_atomic::<S>` at
`S = EngineAssetSlotV16Account` (the instance `activate_empty_asset_slot_not_atomic` calls) and at
`S = Market<()>`.

Every entry point except `finalize_side_reset_not_atomic` takes the address of read-only
constants of the program (for example the promoted `&MarketModeV16::Resolved` that
`kernel_advance_resolved_slot` compares the mode with, or panic-message strings); its property
therefore also assumes `Constants S` (`Spec.LayerTConst`): those constants hold the bytes the
compiler emitted for them, as they do in every run.
-/

@[expose] public section

namespace Spec.LayerT
open Iris Iris.BI Rust2Lean.Public

/-- A call outcome predicate: a normal return satisfies `Q`; unwinding is not constrained. -/
def onReturn {PROP : Type} [BI PROP] (S : Vocab PROP) (Q : S.Value → PROP) :
    CallOutcome S.Value → PROP
  | .returned v => Q v
  | .unwound => iprop(True)

/-- A call of `f` with arguments `args`, under any incoming panic-payload stack: if it returns a
value `r`, then `Q r` holds. A panic (unwinding out of the call, with any payload) is allowed and
not constrained, and nothing says that the call terminates. -/
def callR {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (args : List (S.Value × S.Ty))
    (Q : S.Value → PROP) : PROP :=
  iprop(∀ before : List S.Ptr, S.callUnwind f args before (fun o _ => onReturn S Q o))

/-- `Ok(_)` of a `V16Result<_>`, with any payload. -/
def IsOk {PROP : Type} [BI PROP] (S : Vocab PROP) (r : S.Value) : Prop :=
  ∃ v, r = Spec.Kernel.okValue S v

/-- The value of a `SideV16`: `Long` has discriminant 0, `Short` 1. -/
def sideValue {PROP : Type} [BI PROP] (S : Vocab PROP) (short : Bool) : S.Value :=
  S.variantVal (if short then 1 else 0) S.unitVal

/-- The starting state of a view entry point: the header fields are in range, every slot holds
1365 bytes, and `H` holds. -/
def ViewStart (h : HeaderAccount) (ms : List (List Nat)) : Prop :=
  h.InRange ∧ (∀ bs ∈ ms, SlotBytes bs) ∧ MarketShapeOk ms.length h

/-- The state an `Ok` return of a view entry point leaves: the header fields are in range, every
slot still holds 1365 bytes, the slice has the same length, and `H` holds. -/
def ViewEnd (h' : HeaderAccount) (ms ms' : List (List Nat)) : Prop :=
  h'.InRange ∧ (∀ bs ∈ ms', SlotBytes bs) ∧ ms'.length = ms.length ∧
    MarketShapeOk ms'.length h'

/-! ## T09 `advance_resolved_slot_not_atomic` -/

/-- **T09.** For every market view whose state satisfies `H`, and every `authenticated_slot`:
if `advance_resolved_slot_not_atomic(authenticated_slot)` returns `Ok(())`, then the header it
leaves is the starting header with `current_slot = authenticated_slot` and nothing else changed,
the market slots are unchanged, and `H` holds. -/
def t09_header : Property := fun S =>
  -- For every view cell `self` with header `hp` and slots `mp`, every starting header `h`, slot
  -- contents `ms` and every `authenticated_slot`,
  ∀ (self hp mp : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (authenticated_slot : Nat),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- and `authenticated_slot` is a `u64`,
    authenticated_slot < 2 ^ 64 →
    -- the read-only constants of the program hold their contents (`Constants`),
    -- and we own the view,
    Constants S ∗ ViewOwned S self hp mp h ms ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_resolved_slot_not_atomic»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_resolved_slot_not_atomic» 0),
         (S.intVal authenticated_slot, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_resolved_slot_not_atomic» 1)]
        (fun r => iprop(
          -- POSTCONDITIONS
          -- on return there are a header `h'` and slot contents `ms'` such that
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the view, now holding them,
            ViewOwned S self hp mp h' ms' ∗
            -- and if the result is `Ok(())`,
            ⌊r = okUnit S →
              -- the only header field written is `current_slot`, now `authenticated_slot`,
              h' = { h with current_slot := authenticated_slot } ∧
              -- the slots are unchanged,
              ms' = ms ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-! ## T33 `finalize_side_reset_not_atomic` -/

/-- **T33.** For every market view whose state satisfies `H`, every `asset_index` and `side`: if
`finalize_side_reset_not_atomic(asset_index, side)` returns `Ok(())`, then either nothing in the
header changed (the side was already `Normal`), or the only header fields written are
`resolved_payout_blocker_count` (by `set_asset_state`) and `risk_epoch` (raised by one); the slice
of market slots keeps its length; and `H` holds. -/
def t33_header : Property := fun S =>
  ∀ (self hp mp : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (asset_index : Nat)
    (short : Bool),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- and `asset_index` is a `usize`,
    asset_index < 2 ^ 64 →
    -- and we own the view,
    ViewOwned S self hp mp h ms ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::finalize_side_reset_not_atomic»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::finalize_side_reset_not_atomic» 0),
         (S.intVal asset_index, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::finalize_side_reset_not_atomic» 1),
         (sideValue S short, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::finalize_side_reset_not_atomic» 2)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the view, now holding `h'` and `ms'`,
            ViewOwned S self hp mp h' ms' ∗
            -- and if the result is `Ok(())`,
            ⌊r = okUnit S →
              -- either the header is unchanged, or only `resolved_payout_blocker_count` and
              -- `risk_epoch` (raised by one) were written,
              (h' = h ∨ ∃ blockers : Nat,
                h' = { h with resolved_payout_blocker_count := blockers,
                              risk_epoch := h.risk_epoch + 1 }) ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-! ## T02 `accrue_asset_to_not_atomic` -/

/-- **T02.** For every market view whose state satisfies `H` and all arguments: if
`accrue_asset_to_not_atomic(asset_index, now_slot, effective_price, funding_rate_e9,
protective_progress_committed)` returns `Ok(_)`, then the only header fields written are
`current_slot` (now `max(current_slot, now_slot)`), `slot_last` (now at most `now_slot`),
`loss_stale_active` (now 0 or 1), `oracle_epoch`, `funding_epoch` and
`resolved_payout_blocker_count`; the slice keeps its length; and `H` holds. -/
def t02_header : Property := fun S =>
  ∀ (self hp mp : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (asset_index now_slot effective_price : Nat) (funding_rate_e9 : Int)
    (protective_progress_committed : Bool),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- and the arguments are a `usize`, two `u64`s and an `i128`,
    asset_index < 2 ^ 64 → now_slot < 2 ^ 64 → effective_price < 2 ^ 64 →
    -2 ^ 127 ≤ funding_rate_e9 → funding_rate_e9 < 2 ^ 127 →
    -- the read-only constants of the program hold their contents (`Constants`),
    -- and we own the view,
    Constants S ∗ ViewOwned S self hp mp h ms ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 0),
         (S.intVal asset_index, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 1),
         (S.intVal now_slot, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 2),
         (S.intVal effective_price, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 3),
         (S.intVal funding_rate_e9, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 4),
         (S.boolVal protective_progress_committed, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_to_not_atomic» 5)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the view, now holding `h'` and `ms'`,
            ViewOwned S self hp mp h' ms' ∗
            -- and if the result is `Ok(_)`,
            ⌊IsOk S r →
              -- only these six header fields were written,
              (∃ slot_last loss_stale_active oracle_epoch funding_epoch blockers : Nat,
                h' = { h with current_slot := max h.current_slot now_slot,
                              slot_last := slot_last,
                              loss_stale_active := loss_stale_active,
                              oracle_epoch := oracle_epoch,
                              funding_epoch := funding_epoch,
                              resolved_payout_blocker_count := blockers } ∧
                -- the new `slot_last` is at most `now_slot`,
                slot_last ≤ now_slot ∧
                -- and `loss_stale_active` encodes a bool;
                (loss_stale_active = 0 ∨ loss_stale_active = 1)) ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-! ## T01 `accrue_asset_path_to_not_atomic` -/

/-- An `AccrualStepV16` (`effective_price: u64`, `funding_rate_e9: i128`,
`price_move_remainder_before_bps_num: u16`, `price_move_remainder_after_bps_num: u16`). -/
structure AccrualStep where
  effective_price : Nat
  funding_rate_e9 : Int
  price_move_remainder_before_bps_num : Nat
  price_move_remainder_after_bps_num : Nat

/-- The fields of an `AccrualStep` fit their Rust types. -/
def AccrualStep.InRange (s : AccrualStep) : Prop :=
  s.effective_price < 2 ^ 64 ∧ -2 ^ 127 ≤ s.funding_rate_e9 ∧ s.funding_rate_e9 < 2 ^ 127 ∧
    s.price_move_remainder_before_bps_num < 2 ^ 16 ∧ s.price_move_remainder_after_bps_num < 2 ^ 16

/-- The Rust type `AccrualStepV16` (48 bytes, alignment 16). -/
def stepTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::step_type»

/-- An `AccrualStepV16` value. -/
def stepValue {PROP : Type} [BI PROP] (S : Vocab PROP) (s : AccrualStep) : S.Value :=
  S.tupleVal [S.intVal s.effective_price, S.intVal s.funding_rate_e9,
    S.intVal s.price_move_remainder_before_bps_num, S.intVal s.price_move_remainder_after_bps_num]

/-- **T01.** For every market view whose state satisfies `H` and all arguments: if
`accrue_asset_path_to_not_atomic(asset_index, now_slot, raw_oracle_target_price, steps,
protective_progress_committed)` returns `Ok(_)`, then the only header fields written are
`current_slot` (now `now_slot`, which is at least the old `current_slot`), `slot_last` (now at
most `now_slot`), `loss_stale_active` (now 0 or 1), `oracle_epoch`, `funding_epoch` and
`resolved_payout_blocker_count`; the slice keeps its length; the steps are unchanged; and `H`
holds. -/
def t01_header : Property := fun S =>
  ∀ (self hp mp sp : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (asset_index now_slot raw_oracle_target_price : Nat) (steps : List AccrualStep) (q : Qp)
    (protective_progress_committed : Bool),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- the arguments are a `usize` and two `u64`s,
    asset_index < 2 ^ 64 → now_slot < 2 ^ 64 → raw_oracle_target_price < 2 ^ 64 →
    -- every step's fields fit their types,
    (∀ s ∈ steps, s.InRange) →
    -- the slice pointer `steps` is not null and is aligned to 16, as for every Rust reference,
    S.address sp ≠ 0 → S.address sp % 16 = 0 →
    -- the read-only constants of the program hold their contents (`Constants`),
    -- and we own the view and a share `q` of the steps,
    Constants S ∗ ViewOwned S self hp mp h ms ∗
      S.array sp (stepTy S) 48 q (steps.map (stepValue S)) ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 0),
         (S.intVal asset_index, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 1),
         (S.intVal now_slot, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 2),
         (S.intVal raw_oracle_target_price, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 3),
         (S.slicePtrVal sp steps.length, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 4),
         (S.boolVal protective_progress_committed, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::accrue_asset_path_to_not_atomic» 5)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the view, now holding `h'` and `ms'`,
            ViewOwned S self hp mp h' ms' ∗
            -- we get our share of the unchanged steps back,
            S.array sp (stepTy S) 48 q (steps.map (stepValue S)) ∗
            -- and if the result is `Ok(_)`,
            ⌊IsOk S r →
              -- `now_slot` was at least the old `current_slot`,
              h.current_slot ≤ now_slot ∧
              -- only these six header fields were written,
              (∃ slot_last loss_stale_active oracle_epoch funding_epoch blockers : Nat,
                h' = { h with current_slot := now_slot,
                              slot_last := slot_last,
                              loss_stale_active := loss_stale_active,
                              oracle_epoch := oracle_epoch,
                              funding_epoch := funding_epoch,
                              resolved_payout_blocker_count := blockers } ∧
                -- the new `slot_last` is at most `now_slot`,
                slot_last ≤ now_slot ∧
                -- and `loss_stale_active` encodes a bool;
                (loss_stale_active = 0 ∨ loss_stale_active = 1)) ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-! ## T38 `grow_asset_slot_capacity_not_atomic` (header account) -/

/-- **T38.** For every header account `h` satisfying `Hh` and all `u32` arguments: if
`grow_asset_slot_capacity_not_atomic(new_asset_slot_capacity, new_max_market_slots)` returns
`Ok(())`, then the header it leaves is `h` with `asset_slot_capacity =
new_asset_slot_capacity`, `config.max_market_slots = new_max_market_slots` (every other config
field unchanged: `V16ConfigAccount::from_runtime` of the decoded and modified config stores
exactly that), and `asset_set_epoch` and `risk_epoch` raised by one, nothing else changed;
`new_max_market_slots ≤ new_asset_slot_capacity`; and `Hh` holds. So `H` holds for every view of
this header whose slice has exactly `new_asset_slot_capacity` slots. -/
def t38_header : Property := fun S =>
  ∀ (hp : S.Ptr) (h : HeaderAccount) (new_asset_slot_capacity new_max_market_slots : Nat),
    -- PRECONDITIONS
    -- if the header fields are in range and satisfy Hh,
    h.InRange → HeaderShapeOk h →
    -- and both arguments are `u32`s,
    new_asset_slot_capacity < 2 ^ 32 → new_max_market_slots < 2 ^ 32 →
    -- the read-only constants of the program hold their contents (`Constants`),
    -- and we own the header account,
    Constants S ∗ S.pointsTo hp (headerTy S) (headerValue S h) ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::grow_asset_slot_capacity_not_atomic»
        [(S.ptrVal hp, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::grow_asset_slot_capacity_not_atomic» 0),
         (S.intVal new_asset_slot_capacity, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::grow_asset_slot_capacity_not_atomic» 1),
         (S.intVal new_max_market_slots, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::grow_asset_slot_capacity_not_atomic» 2)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ h' : HeaderAccount,
            -- we own the header account, now holding `h'`,
            S.pointsTo hp (headerTy S) (headerValue S h') ∗
            -- and if the result is `Ok(())`,
            ⌊r = okUnit S →
              -- exactly these four fields were written,
              h' = { h with asset_slot_capacity := new_asset_slot_capacity,
                            config := { h.config with max_market_slots := new_max_market_slots },
                            asset_set_epoch := h.asset_set_epoch + 1,
                            risk_epoch := h.risk_epoch + 1 } ∧
              -- the new maximum fits in the new capacity,
              new_max_market_slots ≤ new_asset_slot_capacity ∧
              -- and the header is in range and satisfies Hh.
              h'.InRange ∧ HeaderShapeOk h'⌋))

/-! ## T05 `activate_empty_market_slot_not_atomic` and T03 `activate_empty_asset_slot_not_atomic` -/

/-- The header an `Ok` activation leaves: `next_market_id`, `asset_activation_count`,
`asset_set_epoch` and `risk_epoch` raised by one, `current_slot` and
`last_asset_activation_slot` set to `now_slot`, nothing else changed. -/
def activatedHeader (h : HeaderAccount) (now_slot : Nat) : HeaderAccount :=
  { h with next_market_id := h.next_market_id + 1,
           current_slot := now_slot,
           asset_activation_count := h.asset_activation_count + 1,
           last_asset_activation_slot := now_slot,
           asset_set_epoch := h.asset_set_epoch + 1,
           risk_epoch := h.risk_epoch + 1 }

/-- The common shape of T05 and T03, given the assertion `C` about the read-only constants, for the function `f`, whose slot argument has type `slotTy`
and whose slot value with bytes `bs` is `slotValue bs`: for every header account `h` satisfying
`Hh`, every slot and all arguments, if `f(asset_index, slot, authenticated_price, now_slot)`
returns `Ok(())`, then `now_slot` was at least the old `current_slot`, the header it leaves is
`activatedHeader h now_slot` (so `asset_slot_capacity` and `config` are unchanged), and `Hh`
holds. So `H` is preserved for the same slice length. -/
def ActivateHeaderWith {PROP : Type} [BI PROP] (S : Vocab PROP) (C : PROP) (f : RustFn)
    (slotTy : S.Ty) (slotValue : List Nat → S.Value) : Prop :=
  ∀ (hp sp : S.Ptr) (h : HeaderAccount) (bs : List Nat)
    (asset_index authenticated_price now_slot : Nat),
    -- PRECONDITIONS
    -- if the header fields are in range and satisfy Hh,
    h.InRange → HeaderShapeOk h →
    -- the slot holds 1365 bytes,
    SlotBytes bs →
    -- the arguments are a `u32` and two `u64`s,
    asset_index < 2 ^ 32 → authenticated_price < 2 ^ 64 → now_slot < 2 ^ 64 →
    -- the read-only constants of the program hold their contents (`C`),
    -- and we own the header account and the slot,
    C ∗ S.pointsTo hp (headerTy S) (headerValue S h) ∗
      S.pointsTo sp slotTy (slotValue bs) ⊢
      callR S f
        [(S.ptrVal hp, S.argTy f 0), (S.intVal asset_index, S.argTy f 1),
         (S.ptrVal sp, S.argTy f 2), (S.intVal authenticated_price, S.argTy f 3),
         (S.intVal now_slot, S.argTy f 4)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (bs' : List Nat),
            -- we own the header account and the slot, now holding `h'` and `bs'`,
            S.pointsTo hp (headerTy S) (headerValue S h') ∗ S.pointsTo sp slotTy (slotValue bs') ∗
            -- and if the result is `Ok(())`,
            ⌊r = okUnit S →
              -- `now_slot` was at least the old `current_slot`,
              h.current_slot ≤ now_slot ∧
              -- exactly the six activation fields were written,
              h' = activatedHeader h now_slot ∧
              -- and the header is in range and satisfies Hh.
              h'.InRange ∧ HeaderShapeOk h'⌋))

/-- `ActivateHeaderWith` with `C = Constants S`. (This form cannot be established: the constants
`&InsuranceCreditReservationV16::EMPTY` and `&SourceCreditStateV16::EMPTY` that the activation
passes by reference end in uninitialised padding, which `Constants S` cannot describe.) -/
def ActivateHeader {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (slotTy : S.Ty)
    (slotValue : List Nat → S.Value) : Prop :=
  ActivateHeaderWith S (Constants S) f slotTy slotValue

/-- **T05** at `S = EngineAssetSlotV16Account`. -/
def t05_header_engine_slot : Property := fun S =>
  ActivateHeader S
    Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::activate_empty_market_slot_not_atomic::<percolator::v16::EngineAssetSlotV16Account>»
    (engineSlotTy S) (engineSlotValue S)

/-- **T05** at `S = Market<()>`. -/
def t05_header_market : Property := fun S =>
  ActivateHeader S
    Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::activate_empty_market_slot_not_atomic::<percolator::v16::Market<()>>»
    (marketTy S) (marketValue S)

/-- **T03** `activate_empty_asset_slot_not_atomic`, a call to T05 at
`S = EngineAssetSlotV16Account`: the same statement. -/
def t03_header : Property := fun S =>
  ActivateHeader S
    Mir.LayerT.Fn.«percolator::v16::MarketGroupV16HeaderAccount::activate_empty_asset_slot_not_atomic»
    (engineSlotTy S) (engineSlotValue S)

/-! ## T47 `register_empty_materialized_portfolio_not_atomic` and T29
`deregister_empty_materialized_portfolio_not_atomic` -/

/-- The common shape of T47 and T29 for the function `f`, given the assertion `C` about the
read-only constants: for every market view whose state
satisfies `H` and every portfolio view `account` (any share `q`): if `f(account)` returns
`Ok(())`, then the header it leaves is `newHeader h` (only `materialized_portfolio_count`
changes), the market slots and the portfolio account are unchanged, and `H` holds. -/
def PortfolioCountHeaderWith {PROP : Type} [BI PROP] (S : Vocab PROP) (C : PROP) (f : RustFn)
    (newHeader : HeaderAccount → HeaderAccount) : Prop :=
  ∀ (self hp mp account ap : S.Ptr) (q : Qp) (h : HeaderAccount) (ms : List (List Nat))
    (bs : List Nat),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- the portfolio account holds 9419 bytes,
    PortfolioBytes bs →
    -- the read-only constants of the program hold their contents (`C`),
    -- and we own the market view and a share `q` of the portfolio view,
    C ∗ ViewOwned S self hp mp h ms ∗ PortfolioViewOwned S account ap q bs ⊢
      callR S f [(S.ptrVal self, S.argTy f 0), (S.ptrVal account, S.argTy f 1)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the market view, now holding `h'` and `ms'`,
            ViewOwned S self hp mp h' ms' ∗
            -- we get our share of the unchanged portfolio view back,
            PortfolioViewOwned S account ap q bs ∗
            -- and if the result is `Ok(())`,
            ⌊r = okUnit S →
              -- only `materialized_portfolio_count` was written,
              h' = newHeader h ∧
              -- the slots are unchanged,
              ms' = ms ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-- `PortfolioCountHeaderWith` with `C = Constants S`. (This form cannot be established: the
constant `&SourceCreditStateV16::EMPTY` that the portfolio validation passes by reference ends in
uninitialised padding, which `Constants S` cannot describe.) -/
def PortfolioCountHeader {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn)
    (newHeader : HeaderAccount → HeaderAccount) : Prop :=
  PortfolioCountHeaderWith S (Constants S) f newHeader

/-- **T47.** `register_empty_materialized_portfolio_not_atomic`: on `Ok(())` only
`materialized_portfolio_count` changes (raised by one) and `H` holds. -/
def t47_header : Property := fun S =>
  PortfolioCountHeader S
    Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::register_empty_materialized_portfolio_not_atomic»
    (fun h => { h with materialized_portfolio_count := h.materialized_portfolio_count + 1 })

/-- **T29.** `deregister_empty_materialized_portfolio_not_atomic`: on `Ok(())` only
`materialized_portfolio_count` changes (lowered by one) and `H` holds. -/
def t29_header : Property := fun S =>
  PortfolioCountHeader S
    Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::deregister_empty_materialized_portfolio_not_atomic»
    (fun h => { h with materialized_portfolio_count := h.materialized_portfolio_count - 1 })

/-! ## T10 `advance_terminal_slab_not_atomic`, its two `ScanProgress` returns -/

/-- `Ok(TerminalSlabOutcomeV16::ScanProgress { next_asset_index })` (`ScanProgress` is the first
variant, discriminant 0). -/
def scanProgressValue {PROP : Type} [BI PROP] (S : Vocab PROP) (next_asset_index : Nat) :
    S.Value :=
  Spec.Kernel.okValue S (S.variantVal 0 (S.tupleVal [S.intVal next_asset_index]))

/-- **T10, `ScanProgress` returns**, given the assertion `C` about the read-only constants. For every market view whose state satisfies `H` and all
arguments: if `advance_terminal_slab_not_atomic(authenticated_slot, scan_start_asset_index)`
returns `Ok(ScanProgress { .. })`, then the only header field written is `current_slot` (now
`authenticated_slot`), the market slots are unchanged, and `H` holds. -/
def t10ScanProgressWith {PROP : Type} [BI PROP] (S : Vocab PROP) (C : PROP) : Prop :=
  ∀ (self hp mp : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (authenticated_slot scan_start_asset_index : Nat),
    -- PRECONDITIONS
    -- if the starting state is in range and satisfies H,
    ViewStart h ms →
    -- the arguments are a `u64` and a `usize`,
    authenticated_slot < 2 ^ 64 → scan_start_asset_index < 2 ^ 64 →
    -- the read-only constants of the program hold their contents (`C`),
    -- and we own the view,
    C ∗ ViewOwned S self hp mp h ms ⊢
      callR S Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_terminal_slab_not_atomic»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_terminal_slab_not_atomic» 0),
         (S.intVal authenticated_slot, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_terminal_slab_not_atomic» 1),
         (S.intVal scan_start_asset_index, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::advance_terminal_slab_not_atomic» 2)]
        (fun r => iprop(
          -- POSTCONDITIONS
          ∃ (h' : HeaderAccount) (ms' : List (List Nat)),
            -- we own the view, now holding `h'` and `ms'`,
            ViewOwned S self hp mp h' ms' ∗
            -- and if the result is `Ok(ScanProgress { next_asset_index })`,
            ⌊(∃ next_asset_index, r = scanProgressValue S next_asset_index) →
              -- the only header field written is `current_slot`, now `authenticated_slot`,
              h' = { h with current_slot := authenticated_slot } ∧
              -- the slots are unchanged,
              ms' = ms ∧
              -- and the state satisfies H.
              ViewEnd h' ms ms'⌋))

/-- **T10, `ScanProgress` returns**: `t10ScanProgressWith` with `C = Constants S`. (This form
cannot be established: the constants `&ResolvedPayoutLedgerV16::EMPTY`,
`&InsuranceCreditReservationV16::EMPTY` and `&SourceCreditStateV16::EMPTY` that the entry
validation passes by reference end in uninitialised padding, which `Constants S` cannot
describe.) -/
def t10_scan_progress_header : Property := fun S => t10ScanProgressWith S (Constants S)

/-! ## The model of `validate_shape` -/

/-- **H model**, given the assertion `C` about the read-only constants. `MarketGroupV16ViewMut::validate_shape` (which calls
`MarketGroupV16View::validate_shape` on `as_view()`) in a default build returns `Ok(())` exactly
when `MarketShapeOk n h` holds, for the header fields `h` and the number `n` of market slots, and
`Err(_)` otherwise; it changes nothing. This is the decomposition `H(m) ⟺ Hh(m.header) ∧
m.markets.len() = asset_slot_capacity ≥ config.max_market_slots` with the checks H1 to H17 of
`Spec.LayerTModel`. -/
def validateShapeWith {PROP : Type} [BI PROP] (S : Vocab PROP) (C : PROP) : Prop :=
  ∀ (self hp mp : S.Ptr) (q : Qp) (h : HeaderAccount) (ms : List (List Nat)),
    -- PRECONDITIONS
    -- if the header fields are in range, every slot holds 1365 bytes,
    h.InRange → (∀ bs ∈ ms, SlotBytes bs) →
    -- the slice pointer is not null (as for every Rust reference),
    S.address mp ≠ 0 →
    -- the slice is not empty (as every market satisfying H has) and, as every Rust object,
    -- occupies fewer than `2^56` bytes,
    0 < ms.length → ms.length * 1365 < 2 ^ 56 →
    -- the read-only constants of the program hold their contents (`C`),
    -- and we own a share `q` of the view cell, of the header and of the slots,
    C ∗
    S.pointsToF self q (viewMutTy S) (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length]) ∗
    S.pointsToF hp q (headerTy S) (headerValue S h) ∗
    S.array mp (marketTy S) marketStride q (ms.map (marketValue S)) ⊢
      S.call Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::validate_shape»
        [(S.ptrVal self, S.argTy
            Mir.LayerT.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::validate_shape» 0)]
        (onReturn S fun r => iprop(
          -- POSTCONDITIONS
          -- on return we get our shares back, unchanged,
          S.pointsToF self q (viewMutTy S) (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length]) ∗
          S.pointsToF hp q (headerTy S) (headerValue S h) ∗
          S.array mp (marketTy S) marketStride q (ms.map (marketValue S)) ∗
          -- the result is `Ok(())` if H holds,
          ⌊(MarketShapeOk ms.length h → r = okUnit S) ∧
          -- and `Err(_)` otherwise.
            (¬ MarketShapeOk ms.length h → ∃ e, r = Spec.Kernel.errValue S e)⌋))

/-- **H model**: `validateShapeWith` with `C = Constants S`. (This form cannot be established:
`&ResolvedPayoutLedgerV16::EMPTY`, passed by reference to `!=`, ends in uninitialised padding,
which `Constants S` cannot describe.) -/
def validate_shape_model : Property := fun S => validateShapeWith S (Constants S)

end Spec.LayerT
