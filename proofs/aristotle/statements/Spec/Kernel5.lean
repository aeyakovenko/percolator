module

public import Spec.Kernel4
public import Spec.KernelModel5

/-!
# Public contracts for selection, classification and accounting-delta kernels of `src/v16.rs`

For each of K19 `V16Core::first_actionable_slot`, K20 `V16Core::select_auto_crank_plan`,
K21 `V16Core::build_trade_request_guard_summary`, K10 `V16Core::kernel_accumulate_batch_trade`,
K30 `V16Core::kernel_advance_leg_b_snap`, K02 `V16Core::kernel_mark_kf_stale_cohorts`,
K03 `V16Core::kernel_settle_kf_stale_cohort`, K01 `apply_backing_provider_earnings_withdraw`,
K26 `V16Core::apply_bankruptcy_residual_chunk_to_loss_side` and
K49 `TokenValueFlowProofV16::debit`, two properties of `aristotle/kernel_contracts_spec.md`:

* `Kxx.contract` (`*_contract` below): for all inputs satisfying the `requires` (none of these
  functions has one), if the call returns, the returned value satisfies the `ensures` (the
  predicates of `Spec.KernelModel5`, in which parameter names denote the values passed by the
  caller). For K26 and K49, which take a `&mut` argument with a `modifies` clause, the caller
  owns the referenced value; on return it gets back ownership of the same place, now holding a
  value related to the old one by the `ensures`. Nothing else is owned by the call, so nothing
  outside the `modifies` place can change.
* `Kxx.nopanic` (`*_nopanic` below): for all such inputs, no execution of the call panics (it
  never unwinds; overflow and bounds checks are on in the release profile). This says nothing
  about termination.

They are about the deployed build: default features, no `cfg(kani)`, release profile with
`overflow-checks = true` and `panic = "unwind"`. Every function these kernels call
(`social_loss_book_split` and core library functions) is covered by a proof of its body, so no
contract of a called function is assumed.
-/

@[expose] public section

namespace Spec.Kernel
open Iris Iris.BI Rust2Lean.Public

/-! ## Values -/

/-- The Rust value of an `Option<usize>`. -/
def optionUsizeValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Option Nat → S.Value
  | none => noneValue S
  | some i => someValue S (S.intVal i)

/-- The Rust value of a `PermissionlessRecoveryReasonV16` with discriminant `r`. -/
def recoveryReasonValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : Nat) : S.Value :=
  S.variantVal r S.unitVal

/-- The Rust value of an `AutoCrankPlanV16` (discriminants `NoAction = 0`, ...,
`CloseResolved = 8`, in declaration order). -/
def autoCrankPlanValue {PROP : Type} [BI PROP] (S : Vocab PROP) : AutoCrankPlan → S.Value
  | .NoAction => S.variantVal 0 S.unitVal
  | .RefreshAccount a => S.variantVal 1 (S.tupleVal [optionUsizeValue S a])
  | .SettleBChunk i => S.variantVal 2 (S.tupleVal [S.intVal i])
  | .Liquidate i => S.variantVal 3 (S.tupleVal [S.intVal i])
  | .ReleaseSourceLiens => S.variantVal 4 S.unitVal
  | .AdvanceClose => S.variantVal 5 S.unitVal
  | .DeclareRecovery r => S.variantVal 6 (S.tupleVal [recoveryReasonValue S r])
  | .FinalizeRecovery => S.variantVal 7 S.unitVal
  | .CloseResolved => S.variantVal 8 S.unitVal

/-- The `usize` payloads of an `AutoCrankPlanV16` are in range, and its recovery reason is one
of the eight variants. -/
def AutoCrankPlan.WF : AutoCrankPlan → Prop
  | .RefreshAccount (some i) => U64 i
  | .SettleBChunk i => U64 i
  | .Liquidate i => U64 i
  | .DeclareRecovery r => r < 8
  | _ => True

/-- The Rust value of a `TradeRequestV16`, fields in declaration order. -/
def tradeRequestValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : TradeRequest) : S.Value :=
  S.tupleVal [S.intVal r.asset_index, S.intVal r.size_q, S.intVal r.exec_price,
    S.intVal r.fee_bps]

/-- The Rust value of a `TradeRequestGuardSummaryV16`, fields in declaration order. -/
def guardSummaryValue {PROP : Type} [BI PROP] (S : Vocab PROP) (s : TradeRequestGuardSummary) :
    S.Value :=
  S.tupleVal [S.boolVal s.asset_configured, S.boolVal s.size_nonzero, S.boolVal s.size_in_cap,
    S.boolVal s.price_nonzero, S.boolVal s.price_in_range, S.boolVal s.fee_bps_in_cap]

/-- The Rust value of a `BatchTradeOutcomeV16`, fields in declaration order. -/
def batchOutcomeValue {PROP : Type} [BI PROP] (S : Vocab PROP) (o : BatchOutcome) : S.Value :=
  S.tupleVal [S.intVal o.fill_count, S.intVal o.fee_a, S.intVal o.fee_b, S.intVal o.notional]

/-- The Rust value of a `TradeApplyOutcomeV16`, fields in declaration order. -/
def tradeApplyValue {PROP : Type} [BI PROP] (S : Vocab PROP) (a : TradeApply) : S.Value :=
  S.tupleVal [S.intVal a.fee_a, S.intVal a.fee_b, S.intVal a.notional,
    S.boolVal a.risk_increasing, S.boolVal a.long_requires_initial_margin,
    S.boolVal a.short_requires_initial_margin, S.boolVal a.long_has_source_claims,
    S.boolVal a.short_has_source_claims]

/-- The Rust value of a `SideV16`. -/
def sideValue {PROP : Type} [BI PROP] (S : Vocab PROP) (s : Side) : S.Value :=
  S.variantVal (sideDisc s) S.unitVal

/-- The Rust value of a `PortfolioLegV16`, fields in declaration order. -/
def legValue {PROP : Type} [BI PROP] (S : Vocab PROP) (l : Leg) : S.Value :=
  S.tupleVal [S.boolVal l.active, S.intVal l.asset_index, S.intVal l.market_id,
    sideValue S l.side, S.intVal l.basis_pos_q, S.intVal l.a_basis, S.intVal l.k_snap,
    S.intVal l.f_snap, S.intVal l.kf_epoch_snap, S.intVal l.epoch_snap,
    S.intVal l.loss_weight, S.intVal l.b_snap, S.intVal l.b_rem, S.intVal l.b_epoch_snap,
    S.boolVal l.b_stale, S.boolVal l.stale]

/-- The Rust discriminant of a `SideModeV16` (`Normal = 0`, `DrainOnly = 1`,
`ResetPending = 2`). -/
def sideModeDisc : SideMode → Int
  | .Normal => 0
  | .DrainOnly => 1
  | .ResetPending => 2

/-- The Rust value of a `SideModeV16`. -/
def sideModeValue {PROP : Type} [BI PROP] (S : Vocab PROP) (m : SideMode) : S.Value :=
  S.variantVal (sideModeDisc m) S.unitVal

/-- The Rust value of an `AssetStateV16`, fields in declaration order. -/
def assetValue {PROP : Type} [BI PROP] (S : Vocab PROP) (a : AssetState) : S.Value :=
  S.tupleVal [S.intVal a.market_id, S.intVal a.retired_slot, lifecycleValue S a.lifecycle,
    S.intVal a.raw_oracle_target_price, S.intVal a.effective_price, S.intVal a.fund_px_last,
    S.intVal a.slot_last, S.intVal a.a_long, S.intVal a.a_short, S.intVal a.k_long,
    S.intVal a.k_short, S.intVal a.f_long_num, S.intVal a.f_short_num,
    S.intVal a.kf_epoch_long, S.intVal a.kf_epoch_short, S.intVal a.k_epoch_start_long,
    S.intVal a.k_epoch_start_short, S.intVal a.f_epoch_start_long_num,
    S.intVal a.f_epoch_start_short_num, S.intVal a.b_long_num, S.intVal a.b_short_num,
    S.intVal a.b_epoch_start_long_num, S.intVal a.b_epoch_start_short_num,
    S.intVal a.oi_eff_long_q, S.intVal a.oi_eff_short_q, S.intVal a.stored_pos_count_long,
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
  S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 0

/-- The Rust value of a `TokenValueFlowProofV16`, fields in declaration order (arrays as the
`tupleVal` of their elements). -/
def tokenValueFlowProofValue {PROP : Type} [BI PROP] (S : Vocab PROP) (p : TokenValueFlowProof) :
    S.Value :=
  S.tupleVal [S.tupleVal (p.debits.map fun (x : Nat) => S.intVal (x : Int)),
    S.tupleVal (p.credits.map fun (x : Nat) => S.intVal (x : Int)),
    S.intVal p.external_quote_in, S.intVal p.external_quote_out, S.intVal p.vault_before,
    S.intVal p.vault_after]

/-- The Rust type `TokenValueFlowProofV16`, named as the return type of
`TokenValueFlowProofV16::empty`. -/
def tokenValueFlowProofTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Kernel.Fn.«percolator::v16::TokenValueFlowProofV16::empty»

/-- The Rust value of the `TokenValueClassV16` with discriminant `c`. -/
def tokenValueClassValue {PROP : Type} [BI PROP] (S : Vocab PROP) (c : Nat) : S.Value :=
  S.variantVal c S.unitVal

/-! ## K19 `first_actionable_slot` -/

/-- The call `V16Core::first_actionable_slot(flags)` with postcondition `Φ`. -/
def callFirstActionableSlot {PROP : Type} [BI PROP] (S : Vocab PROP) (flags : List Bool)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::first_actionable_slot»
    [(S.tupleVal (flags.map S.boolVal),
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::first_actionable_slot» 0)] Φ

/-- K19.contract: if `first_actionable_slot(flags)` returns `r : Option<usize>`, then `r`
satisfies the `ensures` (`first_actionable_slot_ensures`). -/
def first_actionable_slot_contract : Property := fun S =>
  -- For any array of flags,
  ∀ (flags : List Bool),
    -- PRECONDITIONS
    -- if it has `V16_MAX_PORTFOLIO_ASSETS_N = 16` elements (no `requires`),
    flags.length = V16_MAX_PORTFOLIO_ASSETS_N →
    ⊢ -- then, on normal return of the call,
      callFirstActionableSlot S flags (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Some(i)` or `None`, and satisfies the `ensures` of K19.
        ∃ r : Option Nat, v = optionUsizeValue S r ∧ first_actionable_slot_ensures flags r)

/-- K19.nopanic: `first_actionable_slot(flags)` does not panic. -/
def first_actionable_slot_nopanic : Property := fun S =>
  -- For any array of flags,
  ∀ (flags : List Bool),
    -- PRECONDITIONS
    -- if it has 16 elements (no `requires`),
    flags.length = V16_MAX_PORTFOLIO_ASSETS_N →
    ⊢ -- then the call does not unwind (panic).
      callFirstActionableSlot S flags (S.returns fun _ => iprop(True))

/-! ## K20 `select_auto_crank_plan` -/

/-- The call `V16Core::select_auto_crank_plan(summary, b_stale_slot, liq_slot, refresh_asset,
recovery_reason)` with postcondition `Φ`. -/
def callSelectAutoCrankPlan {PROP : Type} [BI PROP] (S : Vocab PROP)
    (summary : ActionableSummary) (b_stale_slot liq_slot : Nat) (refresh_asset : Option Nat)
    (recovery_reason : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan»
    [(actionableSummaryValue S summary,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan» 0),
     (S.intVal b_stale_slot,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan» 1),
     (S.intVal liq_slot,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan» 2),
     (optionUsizeValue S refresh_asset,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan» 3),
     (recoveryReasonValue S recovery_reason,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::select_auto_crank_plan» 4)] Φ

/-- K20.contract: if `select_auto_crank_plan(..)` returns a plan, the plan satisfies the
`ensures` (`select_auto_crank_plan_ensures`). -/
def select_auto_crank_plan_contract : Property := fun S =>
  -- For any summary, slots, refresh asset and recovery reason,
  ∀ (summary : ActionableSummary) (b_stale_slot liq_slot : Nat) (refresh_asset : Option Nat)
      (recovery_reason : Nat),
    -- PRECONDITIONS
    -- if the `usize` values are in range and the reason is one of the eight variants
    -- (no `requires`),
    U64 b_stale_slot → U64 liq_slot → (∀ i, refresh_asset = some i → U64 i) →
    recovery_reason < 8 →
    ⊢ -- then, on normal return of the call,
      callSelectAutoCrankPlan S summary b_stale_slot liq_slot refresh_asset recovery_reason
        (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is a plan satisfying the `ensures` of K20.
          ∃ plan : AutoCrankPlan, v = autoCrankPlanValue S plan ∧
            select_auto_crank_plan_ensures summary b_stale_slot liq_slot refresh_asset
              recovery_reason plan)

/-- K20.nopanic: `select_auto_crank_plan(..)` does not panic. -/
def select_auto_crank_plan_nopanic : Property := fun S =>
  -- For any summary, slots, refresh asset and recovery reason,
  ∀ (summary : ActionableSummary) (b_stale_slot liq_slot : Nat) (refresh_asset : Option Nat)
      (recovery_reason : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    U64 b_stale_slot → U64 liq_slot → (∀ i, refresh_asset = some i → U64 i) →
    recovery_reason < 8 →
    ⊢ -- then the call does not unwind (panic).
      callSelectAutoCrankPlan S summary b_stale_slot liq_slot refresh_asset recovery_reason
        (S.returns fun _ => iprop(True))

/-! ## K21 `build_trade_request_guard_summary` -/

/-- The call `V16Core::build_trade_request_guard_summary(request, max_market_slots,
max_trading_fee_bps)` with postcondition `Φ`. -/
def callGuardSummary {PROP : Type} [BI PROP] (S : Vocab PROP) (request : TradeRequest)
    (max_market_slots max_trading_fee_bps : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::build_trade_request_guard_summary»
    [(tradeRequestValue S request,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::build_trade_request_guard_summary» 0),
     (S.intVal max_market_slots,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::build_trade_request_guard_summary» 1),
     (S.intVal max_trading_fee_bps,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::build_trade_request_guard_summary» 2)] Φ

/-- K21.contract: if `build_trade_request_guard_summary(..)` returns a summary, it satisfies
the `ensures` (`build_trade_request_guard_summary_ensures`). -/
def build_trade_request_guard_summary_contract : Property := fun S =>
  -- For any request and limits,
  ∀ (request : TradeRequest) (max_market_slots max_trading_fee_bps : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    request.WF → U32 max_market_slots → U64 max_trading_fee_bps →
    ⊢ -- then, on normal return of the call,
      callGuardSummary S request max_market_slots max_trading_fee_bps (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is a summary satisfying the `ensures` of K21.
        ∃ s : TradeRequestGuardSummary, v = guardSummaryValue S s ∧
          build_trade_request_guard_summary_ensures request max_market_slots
            max_trading_fee_bps s)

/-- K21.nopanic: `build_trade_request_guard_summary(..)` does not panic. -/
def build_trade_request_guard_summary_nopanic : Property := fun S =>
  -- For any request and limits,
  ∀ (request : TradeRequest) (max_market_slots max_trading_fee_bps : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    request.WF → U32 max_market_slots → U64 max_trading_fee_bps →
    ⊢ -- then the call does not unwind (panic).
      callGuardSummary S request max_market_slots max_trading_fee_bps
        (S.returns fun _ => iprop(True))

/-! ## K10 `kernel_accumulate_batch_trade` -/

/-- The call `V16Core::kernel_accumulate_batch_trade(outcome, risk_increasing,
long_has_source_claims, short_has_source_claims, applied)` with postcondition `Φ`. -/
def callAccumulateBatchTrade {PROP : Type} [BI PROP] (S : Vocab PROP) (outcome : BatchOutcome)
    (risk_increasing long_has_source_claims short_has_source_claims : Bool)
    (applied : TradeApply) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade»
    [(batchOutcomeValue S outcome,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade» 0),
     (S.boolVal risk_increasing,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade» 1),
     (S.boolVal long_has_source_claims,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade» 2),
     (S.boolVal short_has_source_claims,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade» 3),
     (tradeApplyValue S applied,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_accumulate_batch_trade» 4)] Φ

/-- A `V16Result<(BatchTradeOutcomeV16, bool, bool, bool)>` value that is either
`Ok((o, ri, lhsc, shsc))` for an in-range `o` with `ens o ri lhsc shsc`, or some `Err(e)` (the
`Err(_) => true` arm). -/
def BatchTradeResult {PROP : Type} [BI PROP] (S : Vocab PROP)
    (ens : BatchOutcome → Bool → Bool → Bool → Prop) (v : S.Value) : Prop :=
  (∃ o ri lhsc shsc, o.WF ∧
    v = okValue S (S.tupleVal [batchOutcomeValue S o, S.boolVal ri, S.boolVal lhsc,
      S.boolVal shsc]) ∧
    ens o ri lhsc shsc) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- K10.contract: if `kernel_accumulate_batch_trade(..)` returns, its result is
`Ok((o, ri, lhsc, shsc))` satisfying the `ensures` (`accumulate_batch_trade_ensures`), or an
`Err(_)`. -/
def kernel_accumulate_batch_trade_contract : Property := fun S =>
  -- For any running outcome, flags and fill outcome,
  ∀ (outcome : BatchOutcome) (risk_increasing long_has_source_claims short_has_source_claims : Bool)
      (applied : TradeApply),
    -- PRECONDITIONS
    -- if the integers are in range of their Rust types (no `requires`),
    outcome.WF → applied.WF →
    ⊢ -- then, on normal return of the call,
      callAccumulateBatchTrade S outcome risk_increasing long_has_source_claims
        short_has_source_claims applied (ifReturns S fun v =>
          -- POSTCONDITIONS
          -- the result is `Ok(..)` with the `ensures` of K10, or an `Err(_)`.
          BatchTradeResult S
            (accumulate_batch_trade_ensures outcome risk_increasing long_has_source_claims
              short_has_source_claims applied) v)

/-- K10.nopanic: `kernel_accumulate_batch_trade(..)` does not panic. -/
def kernel_accumulate_batch_trade_nopanic : Property := fun S =>
  -- For any running outcome, flags and fill outcome,
  ∀ (outcome : BatchOutcome) (risk_increasing long_has_source_claims short_has_source_claims : Bool)
      (applied : TradeApply),
    -- PRECONDITIONS
    -- if the integers are in range of their Rust types (no `requires`),
    outcome.WF → applied.WF →
    ⊢ -- then the call does not unwind (panic).
      callAccumulateBatchTrade S outcome risk_increasing long_has_source_claims
        short_has_source_claims applied (S.returns fun _ => iprop(True))

/-! ## K30 `kernel_advance_leg_b_snap` -/

/-- The call `V16Core::kernel_advance_leg_b_snap(leg, delta_b, new_remainder, remaining_after)`
with postcondition `Φ`. -/
def callAdvanceLegBSnap {PROP : Type} [BI PROP] (S : Vocab PROP) (leg : Leg)
    (delta_b new_remainder remaining_after : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_advance_leg_b_snap»
    [(legValue S leg, S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_advance_leg_b_snap» 0),
     (S.intVal delta_b,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_advance_leg_b_snap» 1),
     (S.intVal new_remainder,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_advance_leg_b_snap» 2),
     (S.intVal remaining_after,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_advance_leg_b_snap» 3)] Φ

/-- A `V16Result<PortfolioLegV16>` value that is either `Ok(l)` for an in-range `l` with
`ens l`, or some `Err(e)`. -/
def LegResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : Leg → Prop) (v : S.Value) : Prop :=
  (∃ l, l.WF ∧ v = okValue S (legValue S l) ∧ ens l) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d))

/-- K30.contract: if `kernel_advance_leg_b_snap(..)` returns, its result is `Ok(l)` satisfying
the `ensures` (`advance_leg_b_snap_ensures`), or an `Err(_)`. -/
def kernel_advance_leg_b_snap_contract : Property := fun S =>
  -- For any leg and amounts,
  ∀ (leg : Leg) (delta_b new_remainder remaining_after : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → U128 delta_b → U128 new_remainder → U128 remaining_after →
    ⊢ -- then, on normal return of the call,
      callAdvanceLegBSnap S leg delta_b new_remainder remaining_after (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok(l)` with the `ensures` of K30, or an `Err(_)`.
        LegResult S (advance_leg_b_snap_ensures leg delta_b new_remainder remaining_after) v)

/-- K30.nopanic: `kernel_advance_leg_b_snap(..)` does not panic. -/
def kernel_advance_leg_b_snap_nopanic : Property := fun S =>
  -- For any leg and amounts,
  ∀ (leg : Leg) (delta_b new_remainder remaining_after : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    leg.WF → U128 delta_b → U128 new_remainder → U128 remaining_after →
    ⊢ -- then the call does not unwind (panic).
      callAdvanceLegBSnap S leg delta_b new_remainder remaining_after
        (S.returns fun _ => iprop(True))

/-! ## K02 `kernel_mark_kf_stale_cohorts` -/

/-- The call `V16Core::kernel_mark_kf_stale_cohorts(asset, long_changed, short_changed,
cohort_epoch)` with postcondition `Φ`. -/
def callMarkKfStaleCohorts {PROP : Type} [BI PROP] (S : Vocab PROP) (asset : AssetState)
    (long_changed short_changed : Bool) (cohort_epoch : Nat) (Φ : CallOutcome S.Value → PROP) :
    PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts»
    [(assetValue S asset, assetTy S),
     (S.boolVal long_changed,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 1),
     (S.boolVal short_changed,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 2),
     (S.intVal cohort_epoch,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_mark_kf_stale_cohorts» 3)] Φ

/-- A `V16Result<AssetStateV16>` value that is either `Ok(a)` for an in-range `a` with `ens a`,
or some `Err(e)` where `E` holds. -/
def AssetResult {PROP : Type} [BI PROP] (S : Vocab PROP) (ens : AssetState → Prop) (E : Prop)
    (v : S.Value) : Prop :=
  (∃ a, a.WF ∧ v = okValue S (assetValue S a) ∧ ens a) ∨
  (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ E)

/-- K02.contract: if `kernel_mark_kf_stale_cohorts(..)` returns, its result is `Ok(a)`
satisfying the `Ok` arm of the `ensures` (`mark_kf_stale_cohorts_ensures`), or an `Err(_)`
satisfying its `Err` arm (`mark_kf_stale_cohorts_err`). -/
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
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort»
    [(assetValue S asset,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 0),
     (sideValue S side,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 1),
     (S.intVal leg_kf_epoch_snap,
        S.argTy Mir.Kernel.Fn.«percolator::v16::V16Core::kernel_settle_kf_stale_cohort» 2)] Φ

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

/-! ## K01 `apply_backing_provider_earnings_withdraw` -/

/-- The call `apply_backing_provider_earnings_withdraw(vault, bucket_earnings, amount)` with
postcondition `Φ`. -/
def callEarningsWithdraw {PROP : Type} [BI PROP] (S : Vocab PROP)
    (vault bucket_earnings amount : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::apply_backing_provider_earnings_withdraw»
    [(S.intVal vault,
        S.argTy Mir.Kernel.Fn.«percolator::v16::apply_backing_provider_earnings_withdraw» 0),
     (S.intVal bucket_earnings,
        S.argTy Mir.Kernel.Fn.«percolator::v16::apply_backing_provider_earnings_withdraw» 1),
     (S.intVal amount,
        S.argTy Mir.Kernel.Fn.«percolator::v16::apply_backing_provider_earnings_withdraw» 2)] Φ

/-- K01.contract: if `apply_backing_provider_earnings_withdraw(..)` returns, its result is
`Ok((v, e))` satisfying the `ensures` (`earnings_withdraw_ensures`), or an `Err(_)`. -/
def apply_backing_provider_earnings_withdraw_contract : Property := fun S =>
  -- For any vault, bucket earnings and amount,
  ∀ (vault bucket_earnings amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 bucket_earnings → U128 amount →
    ⊢ -- then, on normal return of the call,
      callEarningsWithdraw S vault bucket_earnings amount (ifReturns S fun v =>
        -- POSTCONDITIONS
        -- the result is `Ok((v, e))` with the `ensures` of K01, or an `Err(_)`
        -- (`PairResult` of `Spec.Kernel2`, with no condition on the error).
        PairResult S (earnings_withdraw_ensures vault bucket_earnings amount) True v)

/-- K01.nopanic: `apply_backing_provider_earnings_withdraw(..)` does not panic. -/
def apply_backing_provider_earnings_withdraw_nopanic : Property := fun S =>
  -- For any vault, bucket earnings and amount,
  ∀ (vault bucket_earnings amount : Nat),
    -- PRECONDITIONS
    -- if they are `u128` values (no `requires`),
    U128 vault → U128 bucket_earnings → U128 amount →
    ⊢ -- then the call does not unwind (panic).
      callEarningsWithdraw S vault bucket_earnings amount (S.returns fun _ => iprop(True))

/-! ## K26 `apply_bankruptcy_residual_chunk_to_loss_side` -/

/-- The call `V16Core::apply_bankruptcy_residual_chunk_to_loss_side(asset, opp, engine_chunk,
residual_remaining)`, with `asset` the reference `p`, with postcondition `Φ`. -/
def callResidualChunk {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (opp : Side)
    (engine_chunk residual_remaining : Nat) (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side»
    [(S.ptrVal p, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 0),
     (sideValue S opp, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 1),
     (S.intVal engine_chunk, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 2),
     (S.intVal residual_remaining, S.argTy
        Mir.Kernel.Fn.«percolator::v16::V16Core::apply_bankruptcy_residual_chunk_to_loss_side» 3)]
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
returns, the caller gets back ownership of `*asset`, the side-isolation part of the `ensures`
holds between its old and new values, and the result satisfies the rest of the `ensures`. Only
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

/-! ## K49 `TokenValueFlowProofV16::debit` -/

/-- The call `TokenValueFlowProofV16::debit(self, class, amount)`, with `self` the reference
`p`, with postcondition `Φ`. -/
def callDebit {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) («class» amount : Nat)
    (Φ : CallOutcome S.Value → PROP) : PROP :=
  S.call Mir.Kernel.Fn.«percolator::v16::TokenValueFlowProofV16::debit»
    [(S.ptrVal p, S.argTy Mir.Kernel.Fn.«percolator::v16::TokenValueFlowProofV16::debit» 0),
     (tokenValueClassValue S «class»,
        S.argTy Mir.Kernel.Fn.«percolator::v16::TokenValueFlowProofV16::debit» 1),
     (S.intVal amount, S.argTy Mir.Kernel.Fn.«percolator::v16::TokenValueFlowProofV16::debit» 2)]
    Φ

/-- K49.contract: given ownership of `*self`, if `debit(self, class, amount)` returns, the
caller gets back ownership of `*self`; if the result is `Ok(())`, the `ensures` holds between
the old and new values of `*self`, and every other part of `*self` is unchanged; if it is
`Err(_)`, `*self` is unchanged. Only `*self` is accessed (`modifies(self)`). -/
def debit_contract : Property := fun S =>
  -- For any reference, flow proof, class and amount,
  ∀ (p : S.Ptr) (self : TokenValueFlowProof) («class» amount : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    self.WF → «class» < V16_TOKEN_VALUE_CLASS_COUNT → U128 amount →
    -- and we own the `TokenValueFlowProofV16` at `p`, holding `self`,
    S.pointsTo p (tokenValueFlowProofTy S) (tokenValueFlowProofValue S self) ⊢
      -- then, on normal return of the call (with `p` passed as `self`),
      callDebit S p «class» amount (fun o => match o with
        | .returned v => iprop(
          -- POSTCONDITIONS
          -- there is a new value `self'` of `*self` such that
          ∃ self' : TokenValueFlowProof,
            -- we own the `TokenValueFlowProofV16` at `p` again, now holding `self'`,
            S.pointsTo p (tokenValueFlowProofTy S) (tokenValueFlowProofValue S self') ∗
            -- `self'` is in range,
            ⌊self'.WF⌋ ∗
            -- and either the result is `Ok(())`, K49's `ensures` holds and nothing else
            -- in `*self` changed,
            ⌊(v = okValue S (S.tupleVal []) ∧ debit_ensures self «class» amount self' ∧
                debit_frame self «class» self') ∨
              -- or the result is some `Err(e)` and `*self` is unchanged.
              (∃ d : Nat, d < 12 ∧ v = errValue S (v16ErrorValue S d) ∧ self' = self)⌋)
        -- (an unwinding call is not constrained here).
        | .unwound => iprop(True))

/-- K49.nopanic: given ownership of `*self`, `debit(self, class, amount)` does not panic (in
particular, the index `class as usize` is in bounds). -/
def debit_nopanic : Property := fun S =>
  -- For any reference, flow proof, class and amount,
  ∀ (p : S.Ptr) (self : TokenValueFlowProof) («class» amount : Nat),
    -- PRECONDITIONS
    -- if they are in range of their Rust types (no `requires`),
    self.WF → «class» < V16_TOKEN_VALUE_CLASS_COUNT → U128 amount →
    -- and we own the `TokenValueFlowProofV16` at `p`, holding `self`,
    S.pointsTo p (tokenValueFlowProofTy S) (tokenValueFlowProofValue S self) ⊢
      -- then the call does not unwind (panic).
      callDebit S p «class» amount (S.returns fun _ => iprop(True))

end Spec.Kernel
