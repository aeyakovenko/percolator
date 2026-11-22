module

public import Spec.Kernel
public import Spec.ClampModel
public import Spec.Rust.Clamp

/-!
# Public contracts for the program's price clamp (Part 4)

`oracle_v16::clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots) -> u64` and
`oracle_v16::effective_price_from_target(anchor, target, max_change_bps, dt_slots, exposed) -> u64`
of `wrapper_ebcded58/oracle_v16_clamp.rs`, built on its own as a one-module library crate
(`harness/oracle_clamp`, release profile with `overflow-checks = true` and `panic = "unwind"`),
as compiled in the program `Mir.Clamp` (handles in `Spec.Rust.Clamp`).

Inputs range over the whole of `u64` (and `bool`). Exact integers. Every property is a
partial-correctness statement (`ifReturns`: the postcondition holds if the call returns;
`S.returns`: additionally, no execution panics). Nothing is claimed about termination (the
functions have no loops).
-/

@[expose] public section

namespace Spec.Clamp
open Iris Iris.BI Rust2Lean.Public
open Spec.Kernel (U64 ifReturns)

/-! ## Calls -/

/-- The call `clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots)` with outcome
predicate `Φ`. -/
def callClamp {PROP : Type} [BI PROP] (S : Vocab PROP) (p_last target cap_bps dt_slots : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::clamp_toward_engine_dt»
    [(S.intVal p_last, S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::clamp_toward_engine_dt» 0),
     (S.intVal target, S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::clamp_toward_engine_dt» 1),
     (S.intVal cap_bps,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::clamp_toward_engine_dt» 2),
     (S.intVal dt_slots,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::clamp_toward_engine_dt» 3)] Φ

/-- The call `effective_price_from_target(anchor, target, max_change_bps, dt_slots, exposed)` with
outcome predicate `Φ`. -/
def callEffective {PROP : Type} [BI PROP] (S : Vocab PROP)
    (anchor target max_change_bps dt_slots : Nat) (exposed : Bool)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target»
    [(S.intVal anchor,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target» 0),
     (S.intVal target,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target» 1),
     (S.intVal max_change_bps,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target» 2),
     (S.intVal dt_slots,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target» 3),
     (S.boolVal exposed,
        S.argTy Mir.Clamp.Fn.«oracle_v16_clamp::oracle_v16::effective_price_from_target» 4)] Φ

/-! ## KC.nopanic -/

/-- **KC.nopanic.** Neither `clamp_toward_engine_dt(..)` nor `effective_price_from_target(..)`
panics, on any input. -/
def clamp_nopanic : Property := fun S =>
  -- `clamp_toward_engine_dt`: for any `u64` inputs,
  (∀ (p_last target cap_bps dt_slots : Nat),
    U64 p_last → U64 target → U64 cap_bps → U64 dt_slots →
    ⊢ -- the call does not unwind (panic);
      callClamp S p_last target cap_bps dt_slots (S.returns fun _ => iprop(True))) ∧
  -- `effective_price_from_target`: for any `u64` inputs and flag,
  (∀ (anchor target max_change_bps dt_slots : Nat) (exposed : Bool),
    U64 anchor → U64 target → U64 max_change_bps → U64 dt_slots →
    ⊢ -- the call does not unwind (panic).
      callEffective S anchor target max_change_bps dt_slots exposed
        (S.returns fun _ => iprop(True)))

/-! ## KC.envelope -/

/-- **KC.envelope.** For all `u64` inputs with `p_last > 0` and `target > 0`, if
`r = clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots)` returns, then
`min(p_last, target) <= r <= max(p_last, target)` and
`|r - p_last| * 10000 <= p_last * cap_bps * dt_slots` (exact integers). -/
def clamp_envelope : Property := fun S =>
  -- For any last price, target, cap and slot count,
  ∀ (p_last target cap_bps dt_slots : Nat),
    -- PRECONDITIONS
    -- if they are `u64` values,
    U64 p_last → U64 target → U64 cap_bps → U64 dt_slots →
    -- and both prices are positive,
    0 < p_last → 0 < target →
    ⊢ -- then, if the call returns `r`,
      callClamp S p_last target cap_bps dt_slots (ifReturns S fun v => ∃ r : Nat,
        v = S.intVal r ∧
        -- POSTCONDITIONS
        -- `r` lies between the last price and the target,
        min p_last target ≤ r ∧ r ≤ max p_last target ∧
        -- and moved at most `p_last * cap_bps * dt_slots / 10000`.
        absDiff r p_last * 10000 ≤ p_last * cap_bps * dt_slots)

/-! ## KC.reaches -/

/-- **KC.reaches.** For all `u64` inputs with `p_last > 0`, `target > 0` and
`|target - p_last| * 10000 <= p_last * cap_bps * dt_slots`, if
`clamp_toward_engine_dt(p_last, target, cap_bps, dt_slots)` returns, it returns `target`. -/
def clamp_reaches : Property := fun S =>
  -- For any last price, target, cap and slot count,
  ∀ (p_last target cap_bps dt_slots : Nat),
    -- PRECONDITIONS
    -- if they are `u64` values,
    U64 p_last → U64 target → U64 cap_bps → U64 dt_slots →
    -- both prices are positive,
    0 < p_last → 0 < target →
    -- and the move is inside the envelope,
    absDiff target p_last * 10000 ≤ p_last * cap_bps * dt_slots →
    ⊢ -- then, if the call returns, it returns `target`.
      callClamp S p_last target cap_bps dt_slots (ifReturns S fun v => v = S.intVal target)

/-! ## KC.select -/

/-- **KC.select.** For all inputs, if `effective_price_from_target(anchor, target,
max_change_bps, dt_slots, exposed)` returns, it returns the value
`clamp_toward_engine_dt(anchor, target, max_change_bps, dt_slots)` computes (`clampModel`) when
`exposed`, and `target` otherwise; and `clamp_toward_engine_dt(p_last, target, cap_bps,
dt_slots)` itself, if it returns, returns `clampModel p_last target cap_bps dt_slots`. -/
def effective_price_select : Property := fun S =>
  -- `clamp_toward_engine_dt` computes `clampModel`: for any `u64` inputs,
  (∀ (p_last target cap_bps dt_slots : Nat),
    U64 p_last → U64 target → U64 cap_bps → U64 dt_slots →
    ⊢ -- if the call returns, it returns `clampModel p_last target cap_bps dt_slots`;
      callClamp S p_last target cap_bps dt_slots
        (ifReturns S fun v => v = S.intVal (clampModel p_last target cap_bps dt_slots))) ∧
  -- `effective_price_from_target` selects: for any `u64` inputs and flag,
  (∀ (anchor target max_change_bps dt_slots : Nat) (exposed : Bool),
    U64 anchor → U64 target → U64 max_change_bps → U64 dt_slots →
    ⊢ -- if the call returns, it returns the clamped price when `exposed` and `target` otherwise.
      callEffective S anchor target max_change_bps dt_slots exposed
        (ifReturns S fun v => v = S.intVal
          (if exposed then clampModel anchor target max_change_bps dt_slots else target)))

end Spec.Clamp
