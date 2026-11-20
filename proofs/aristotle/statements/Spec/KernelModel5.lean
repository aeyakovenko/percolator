module

public import Spec.KernelModel4

/-!
# Independent mathematical model for the selection, classification and accounting-delta kernels

A mathematical reading of the `kani::requires` / `kani::ensures` / `kani::modifies` clauses of
these functions of `src/v16.rs`:

* Selection and classification (associated functions of `V16Core`):
  K19 `first_actionable_slot`, K20 `select_auto_crank_plan`,
  K21 `build_trade_request_guard_summary`, K10 `kernel_accumulate_batch_trade`,
  K30 `kernel_advance_leg_b_snap`, K02 `kernel_mark_kf_stale_cohorts`,
  K03 `kernel_settle_kf_stale_cohort`.
* Accounting deltas: K01 `apply_backing_provider_earnings_withdraw` (free function),
  K26 `V16Core::apply_bankruptcy_residual_chunk_to_loss_side`,
  K49 `TokenValueFlowProofV16::debit`, and the associated functions of
  `impl<'a, T> MarketGroupV16ViewMut<'a, T>` K50 `apply_total_delta`,
  K51 `credit_backing_provider_earnings_delta`, K52 `set_domain_insurance_spent_delta`,
  K53 `set_domain_insurance_budget_delta`, K54 `withdraw_domain_insurance_delta`,
  K55 `credit_account_from_insurance_delta`, K56 `retire_terminal_unbudgeted_insurance_delta`.

Conventions (as in `Spec.KernelModel`):

* Rust unsigned integers are modelled by `Nat`, signed ones by `Int`; `U64`, `U128`, `I128`
  (and `U32` below) say a value is representable in its Rust type, and `*.WF` says so of every
  field of a struct.
* `wrapping_add` / `wrapping_sub` are arithmetic modulo `2^n` (`wrapAdd128`, `wrapSub128`,
  `wrapAdd64`, `wrapSub64`, `wrapAdd32`); `saturating_sub` on `u128` is `satSub128`
  (truncated subtraction); `x as i128` for a `u128` is `castU128I128`, and `i128::wrapping_sub`
  is `wrapI128` of the exact difference.
* Plain `+` / `-` inside an `ensures` clause are read as exact integer arithmetic.
* Parameter names denote the values the caller passed, also where the body mutates or rebinds
  a by-value parameter (`mut asset`, `mut outcome`, `mut leg`). For a `&mut` parameter with a
  `modifies` clause, `old(x)` is the value at entry and `x` the value at return.
* A `V16Error` is identified by its Rust discriminant (`InvalidConfig = 0`, ...,
  `CounterUnderflow = 11`). An `_ensures` predicate constrains the `Ok` payload; where the
  `Err` arm of an `ensures` is not `true`, `_err` states it.
* Derived `PartialEq` on these `Copy` structs is structural equality, i.e. `=`.
-/

@[expose] public section

namespace Spec.Kernel

/-! ## Shared definitions -/

/-- `2^32` -/
def two32 : Nat := 2 ^ 32

/-- A `u32` value. -/
def U32 (x : Nat) : Prop := x < two32

/-- Rust `a.wrapping_add(b)` on `u32`: addition modulo `2^32`. -/
def wrapAdd32 (a b : Nat) : Nat := (a + b) % two32

/-- Rust `a.wrapping_sub(b)` on `u64`: subtraction modulo `2^64`. -/
def wrapSub64 (a b : Nat) : Nat := (((a : Int) - (b : Int)) % (two64 : Int)).toNat

/-- `MAX_ORACLE_PRICE` (`src/lib.rs`): `1_000_000_000_000`. -/
def MAX_ORACLE_PRICE : Nat := 1000000000000

/-- `V16_MAX_PORTFOLIO_ASSETS_N` (`src/v16.rs`): `16`. -/
def V16_MAX_PORTFOLIO_ASSETS_N : Nat := 16

/-- `V16_TOKEN_VALUE_CLASS_COUNT` (`src/v16.rs`, non-`kani` build): `17`. -/
def V16_TOKEN_VALUE_CLASS_COUNT : Nat := 17

/-! ## K19 `first_actionable_slot(flags: [bool; V16_MAX_PORTFOLIO_ASSETS_N]) -> Option<usize>`

`flags` is modelled by the list of its 16 elements, `flags[i]` by `flags.getD i false`. -/

/-- K19 `ensures`, with the two `while` loops of the clause read as the quantifiers they
compute: for `Some(i)`, `i < V16_MAX_PORTFOLIO_ASSETS_N && flags[i]` and every earlier slot
`j < i` has `!flags[j]` (`earlier_clear`); for `None`, no slot `j < V16_MAX_PORTFOLIO_ASSETS_N`
has `flags[j]` (`!any`). -/
def first_actionable_slot_ensures (flags : List Bool) : Option Nat → Prop
  | some i => i < V16_MAX_PORTFOLIO_ASSETS_N ∧ flags.getD i false = true ∧
      ∀ j, j < i → flags.getD j false = false
  | none => ∀ j, j < V16_MAX_PORTFOLIO_ASSETS_N → flags.getD j false = false

/-! ## K20 `select_auto_crank_plan(summary: ActionableSummaryV16, b_stale_slot: usize,
liq_slot: usize, refresh_asset: Option<usize>,
recovery_reason: PermissionlessRecoveryReasonV16) -> AutoCrankPlanV16`

A `PermissionlessRecoveryReasonV16` is identified by its Rust discriminant `r < 8`
(`BelowProgressFloor = 0`, `BlockedSegmentHeadroomOrRepresentability = 1`,
`AccountBSettlementCannotProgress = 2`, `BIndexHeadroomExhausted = 3`,
`ActiveBankruptCloseCannotProgress = 4`, `ExplicitLossOrDustAuditOverflow = 5`,
`OracleOrTargetUnavailableByAuthenticatedPolicy = 6`,
`CounterOrEpochOverflowDeclaredRecovery = 7`). -/

/-- Rust `enum AutoCrankPlanV16` (variants in declaration order; `usize` payloads as `Nat`, the
recovery reason as its discriminant). -/
inductive AutoCrankPlan where
  | NoAction
  | RefreshAccount (asset_index : Option Nat)
  | SettleBChunk (asset_index : Nat)
  | Liquidate (asset_index : Nat)
  | ReleaseSourceLiens
  | AdvanceClose
  | DeclareRecovery (reason : Nat)
  | FinalizeRecovery
  | CloseResolved
  deriving DecidableEq, Repr

/-- `ActionableSummaryV16::is_actionable(self)`: the disjunction of its eight flags. -/
def ActionableSummary.is_actionable (s : ActionableSummary) : Bool :=
  s.stale || s.b_stale || s.pending_close || s.expired_close || s.liquidatable ||
    s.source_liens_releasable || s.recovery_eligible || s.resolved_winner

/-- K20 `ensures`, arm by arm, with `recovery = summary.expired_close ||
summary.recovery_eligible`. -/
def select_auto_crank_plan_ensures (summary : ActionableSummary) (b_stale_slot liq_slot : Nat)
    (refresh_asset : Option Nat) (recovery_reason : Nat) : AutoCrankPlan → Prop
  | .NoAction => summary.is_actionable = false
  | .DeclareRecovery reason =>
      (summary.expired_close || summary.recovery_eligible) = true ∧ reason = recovery_reason
  | .FinalizeRecovery => False
  | .CloseResolved =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = true
  | .AdvanceClose =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = false ∧ summary.pending_close = true
  | .SettleBChunk asset_index =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = false ∧ summary.pending_close = false ∧
        summary.b_stale = true ∧ asset_index = b_stale_slot
  | .Liquidate asset_index =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = false ∧ summary.pending_close = false ∧
        summary.b_stale = false ∧ summary.liquidatable = true ∧ asset_index = liq_slot
  | .ReleaseSourceLiens =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = false ∧ summary.pending_close = false ∧
        summary.b_stale = false ∧ summary.liquidatable = false ∧
        summary.source_liens_releasable = true
  | .RefreshAccount asset_index =>
      (summary.expired_close || summary.recovery_eligible) = false ∧
        summary.resolved_winner = false ∧ summary.pending_close = false ∧
        summary.b_stale = false ∧ summary.liquidatable = false ∧
        summary.source_liens_releasable = false ∧ summary.stale = true ∧
        asset_index = refresh_asset

/-! ## K21 `build_trade_request_guard_summary(request: TradeRequestV16, max_market_slots: u32,
max_trading_fee_bps: u64) -> TradeRequestGuardSummaryV16` -/

/-- Rust `struct TradeRequestV16` (`#[repr(C)]`; all fields, in declaration order). -/
structure TradeRequest where
  /-- `asset_index: usize` -/
  asset_index : Nat
  /-- `size_q: i128` -/
  size_q : Int
  /-- `exec_price: u64` -/
  exec_price : Nat
  /-- `fee_bps: u64` -/
  fee_bps : Nat
  deriving DecidableEq, Repr

/-- Every field of a `TradeRequestV16` value is in range of its Rust type (`usize` is 64-bit). -/
def TradeRequest.WF (r : TradeRequest) : Prop :=
  U64 r.asset_index ∧ I128 r.size_q ∧ U64 r.exec_price ∧ U64 r.fee_bps

/-- Rust `struct TradeRequestGuardSummaryV16` (all fields, in declaration order). -/
structure TradeRequestGuardSummary where
  /-- `asset_configured: bool` -/
  asset_configured : Bool
  /-- `size_nonzero: bool` -/
  size_nonzero : Bool
  /-- `size_in_cap: bool` -/
  size_in_cap : Bool
  /-- `price_nonzero: bool` -/
  price_nonzero : Bool
  /-- `price_in_range: bool` -/
  price_in_range : Bool
  /-- `fee_bps_in_cap: bool` -/
  fee_bps_in_cap : Bool
  deriving DecidableEq, Repr

/-- K21 `ensures`: `s.asset_configured == ((request.asset_index as u64) < max_market_slots as
u64)` (both casts are exact: `usize` is 64-bit), `s.size_nonzero == (request.size_q != 0)`,
`s.size_in_cap == (request.size_q.unsigned_abs() <= MAX_TRADE_SIZE_Q)`,
`s.price_nonzero == (request.exec_price != 0)`,
`s.price_in_range == (request.exec_price <= MAX_ORACLE_PRICE)`,
`s.fee_bps_in_cap == (request.fee_bps <= max_trading_fee_bps)`. -/
def build_trade_request_guard_summary_ensures (request : TradeRequest)
    (max_market_slots max_trading_fee_bps : Nat) (s : TradeRequestGuardSummary) : Prop :=
  s.asset_configured = decide (request.asset_index < max_market_slots) ∧
  s.size_nonzero = decide (request.size_q ≠ 0) ∧
  s.size_in_cap = decide (request.size_q.natAbs ≤ MAX_TRADE_SIZE_Q) ∧
  s.price_nonzero = decide (request.exec_price ≠ 0) ∧
  s.price_in_range = decide (request.exec_price ≤ MAX_ORACLE_PRICE) ∧
  s.fee_bps_in_cap = decide (request.fee_bps ≤ max_trading_fee_bps)

/-! ## K10 `kernel_accumulate_batch_trade(outcome: BatchTradeOutcomeV16, risk_increasing: bool,
long_has_source_claims: bool, short_has_source_claims: bool, applied: TradeApplyOutcomeV16)
-> V16Result<(BatchTradeOutcomeV16, bool, bool, bool)>` -/

/-- Rust `struct BatchTradeOutcomeV16` (`#[repr(C)]`; all fields, in declaration order). -/
structure BatchOutcome where
  /-- `fill_count: u32` -/
  fill_count : Nat
  /-- `fee_a: u128` -/
  fee_a : Nat
  /-- `fee_b: u128` -/
  fee_b : Nat
  /-- `notional: u128` -/
  notional : Nat
  deriving DecidableEq, Repr

/-- Every field of a `BatchTradeOutcomeV16` value is in range of its Rust type. -/
def BatchOutcome.WF (o : BatchOutcome) : Prop :=
  U32 o.fill_count ∧ U128 o.fee_a ∧ U128 o.fee_b ∧ U128 o.notional

/-- Rust `struct TradeApplyOutcomeV16` (all fields, in declaration order). -/
structure TradeApply where
  /-- `fee_a: u128` -/
  fee_a : Nat
  /-- `fee_b: u128` -/
  fee_b : Nat
  /-- `notional: u128` -/
  notional : Nat
  /-- `risk_increasing: bool` -/
  risk_increasing : Bool
  /-- `long_requires_initial_margin: bool` -/
  long_requires_initial_margin : Bool
  /-- `short_requires_initial_margin: bool` -/
  short_requires_initial_margin : Bool
  /-- `long_has_source_claims: bool` -/
  long_has_source_claims : Bool
  /-- `short_has_source_claims: bool` -/
  short_has_source_claims : Bool
  deriving DecidableEq, Repr

/-- Every integer field of a `TradeApplyOutcomeV16` value is in range of its Rust type. -/
def TradeApply.WF (a : TradeApply) : Prop :=
  U128 a.fee_a ∧ U128 a.fee_b ∧ U128 a.notional

/-- K10 `ensures` for `Ok((o, ri, lhsc, shsc))`: `o.fill_count ==
outcome.fill_count.wrapping_add(1)` (`u32`), `o.fee_a == outcome.fee_a.wrapping_add(applied.fee_a)`,
`o.fee_b == outcome.fee_b.wrapping_add(applied.fee_b)`,
`o.notional == outcome.notional.wrapping_add(applied.notional)` (`u128`),
`ri == (risk_increasing || applied.risk_increasing)`,
`lhsc == (long_has_source_claims || applied.long_has_source_claims)`,
`shsc == (short_has_source_claims || applied.short_has_source_claims)`. -/
def accumulate_batch_trade_ensures (outcome : BatchOutcome)
    (risk_increasing long_has_source_claims short_has_source_claims : Bool) (applied : TradeApply)
    (o : BatchOutcome) (ri lhsc shsc : Bool) : Prop :=
  o.fill_count = wrapAdd32 outcome.fill_count 1 ∧
  o.fee_a = wrapAdd128 outcome.fee_a applied.fee_a ∧
  o.fee_b = wrapAdd128 outcome.fee_b applied.fee_b ∧
  o.notional = wrapAdd128 outcome.notional applied.notional ∧
  ri = (risk_increasing || applied.risk_increasing) ∧
  lhsc = (long_has_source_claims || applied.long_has_source_claims) ∧
  shsc = (short_has_source_claims || applied.short_has_source_claims)

/-! ## K30 `kernel_advance_leg_b_snap(leg: PortfolioLegV16, delta_b: u128, new_remainder: u128,
remaining_after: u128) -> V16Result<PortfolioLegV16>` -/

/-- Rust `struct PortfolioLegV16` (all fields, in declaration order). -/
structure Leg where
  /-- `active: bool` -/
  active : Bool
  /-- `asset_index: u32` -/
  asset_index : Nat
  /-- `market_id: u64` -/
  market_id : Nat
  /-- `side: SideV16` -/
  side : Side
  /-- `basis_pos_q: i128` -/
  basis_pos_q : Int
  /-- `a_basis: u128` -/
  a_basis : Nat
  /-- `k_snap: i128` -/
  k_snap : Int
  /-- `f_snap: i128` -/
  f_snap : Int
  /-- `kf_epoch_snap: u64` -/
  kf_epoch_snap : Nat
  /-- `epoch_snap: u64` -/
  epoch_snap : Nat
  /-- `loss_weight: u128` -/
  loss_weight : Nat
  /-- `b_snap: u128` -/
  b_snap : Nat
  /-- `b_rem: u128` -/
  b_rem : Nat
  /-- `b_epoch_snap: u64` -/
  b_epoch_snap : Nat
  /-- `b_stale: bool` -/
  b_stale : Bool
  /-- `stale: bool` -/
  stale : Bool
  deriving DecidableEq, Repr

/-- Every field of a `PortfolioLegV16` value is in range of its Rust type. -/
def Leg.WF (l : Leg) : Prop :=
  U32 l.asset_index ∧ U64 l.market_id ∧ I128 l.basis_pos_q ∧ U128 l.a_basis ∧ I128 l.k_snap ∧
  I128 l.f_snap ∧ U64 l.kf_epoch_snap ∧ U64 l.epoch_snap ∧ U128 l.loss_weight ∧
  U128 l.b_snap ∧ U128 l.b_rem ∧ U64 l.b_epoch_snap

/-- K30 `ensures` for `Ok(l)`: `l.b_snap == leg.b_snap.wrapping_add(delta_b)`,
`l.b_snap >= leg.b_snap`, `l.b_rem == new_remainder`, `l.b_stale == (remaining_after != 0)`,
and the thirteen other fields of `l` equal those of `leg`. -/
def advance_leg_b_snap_ensures (leg : Leg) (delta_b new_remainder remaining_after : Nat)
    (l : Leg) : Prop :=
  l.b_snap = wrapAdd128 leg.b_snap delta_b ∧
  l.b_snap ≥ leg.b_snap ∧
  l.b_rem = new_remainder ∧
  l.b_stale = decide (remaining_after ≠ 0) ∧
  l.active = leg.active ∧ l.asset_index = leg.asset_index ∧ l.market_id = leg.market_id ∧
  l.side = leg.side ∧ l.basis_pos_q = leg.basis_pos_q ∧ l.a_basis = leg.a_basis ∧
  l.k_snap = leg.k_snap ∧ l.f_snap = leg.f_snap ∧ l.kf_epoch_snap = leg.kf_epoch_snap ∧
  l.epoch_snap = leg.epoch_snap ∧ l.loss_weight = leg.loss_weight ∧
  l.b_epoch_snap = leg.b_epoch_snap ∧ l.stale = leg.stale

/-! ## `AssetStateV16` (K02, K03, K26) -/

/-- Rust `enum SideModeV16 { Normal, DrainOnly, ResetPending }`. -/
inductive SideMode where
  | Normal
  | DrainOnly
  | ResetPending
  deriving DecidableEq, Repr

/-- Rust `struct AssetStateV16` (all 44 fields, in declaration order). -/
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
  U128 a.oi_eff_short_q ∧ U64 a.stored_pos_count_long ∧ U64 a.stored_pos_count_short ∧
  U64 a.stale_account_count_long ∧ U64 a.stale_account_count_short ∧
  U64 a.pending_obligation_count_long ∧ U64 a.pending_obligation_count_short ∧
  U128 a.loss_weight_sum_long ∧ U128 a.loss_weight_sum_short ∧
  U128 a.social_loss_remainder_long_num ∧ U128 a.social_loss_remainder_short_num ∧
  U128 a.social_loss_dust_long_num ∧ U128 a.social_loss_dust_short_num ∧
  U128 a.explicit_unallocated_loss_long ∧ U128 a.explicit_unallocated_loss_short ∧
  U64 a.epoch_long ∧ U64 a.epoch_short

/-- The `cfg(kani)` helper `V16Core::kf_cohort_frame_eq(lhs, rhs)`, read from its body: `lhs`
and `rhs` are equal after setting `kf_epoch_long`, `kf_epoch_short`,
`stale_account_count_long` and `stale_account_count_short` to `0` in both, i.e. they agree on
every other field. -/
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

/-! ## K02 `kernel_mark_kf_stale_cohorts(mut asset: AssetStateV16, long_changed: bool,
short_changed: bool, cohort_epoch: u64) -> V16Result<AssetStateV16>` -/

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

/-! ## K03 `kernel_settle_kf_stale_cohort(mut asset: AssetStateV16, side: SideV16,
leg_kf_epoch_snap: u64) -> V16Result<(AssetStateV16, u64)>` -/

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

/-! ## K01 `apply_backing_provider_earnings_withdraw(vault: u128, bucket_earnings: u128,
amount: u128) -> V16Result<(u128, u128)>` -/

/-- K01 `ensures` for `Ok((v, e))`: `v == vault.wrapping_sub(amount)` and
`e == bucket_earnings.wrapping_sub(amount)`. -/
def earnings_withdraw_ensures (vault bucket_earnings amount : Nat) (v e : Nat) : Prop :=
  v = wrapSub128 vault amount ∧ e = wrapSub128 bucket_earnings amount

/-! ## K26 `apply_bankruptcy_residual_chunk_to_loss_side(asset: &mut AssetStateV16, opp: SideV16,
engine_chunk: u128, residual_remaining: u128)
-> V16Result<Option<BResidualBookingOutcomeV16>>`, `modifies(asset)` -/

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

/-- The K26 `ensures` clause for a returned `Ok(Some(o))`: `o.explicit_loss == 0`,
`o.booked_loss == engine_chunk`, `o.delta_b > 0` and
`o.remaining_after.checked_add(o.booked_loss) == Some(residual_remaining)`. -/
def residual_chunk_booked_ensures (engine_chunk residual_remaining : Nat) (o : BResidualOutcome) :
    Prop :=
  o.explicit_loss = 0 ∧ o.booked_loss = engine_chunk ∧ o.delta_b > 0 ∧
    checkedAdd128 o.remaining_after o.booked_loss = some residual_remaining

/-! ## K49 `TokenValueFlowProofV16::debit(&mut self, class: TokenValueClassV16, amount: u128)
-> V16Result<()>`, `modifies(self)`

A `TokenValueClassV16` is identified by its Rust discriminant `class < 17` (`#[repr(u8)]`,
`TokenVault = 0`, ..., `UnallocatedProtocolSurplus = 16`); `class as usize` is that
discriminant. -/

/-- Rust `struct TokenValueFlowProofV16` (non-`kani` build; all fields, in declaration order).
The two arrays `[u128; V16_TOKEN_VALUE_CLASS_COUNT]` are modelled by the lists of their
elements. -/
structure TokenValueFlowProof where
  /-- `debits: [u128; 17]` -/
  debits : List Nat
  /-- `credits: [u128; 17]` -/
  credits : List Nat
  /-- `external_quote_in: u128` -/
  external_quote_in : Nat
  /-- `external_quote_out: u128` -/
  external_quote_out : Nat
  /-- `vault_before: u128` -/
  vault_before : Nat
  /-- `vault_after: u128` -/
  vault_after : Nat
  deriving DecidableEq, Repr

/-- A `TokenValueFlowProofV16` value: both arrays have 17 elements, and every number is a
`u128`. -/
def TokenValueFlowProof.WF (p : TokenValueFlowProof) : Prop :=
  p.debits.length = V16_TOKEN_VALUE_CLASS_COUNT ∧ p.credits.length = V16_TOKEN_VALUE_CLASS_COUNT ∧
  (∀ x ∈ p.debits, U128 x) ∧ (∀ x ∈ p.credits, U128 x) ∧
  U128 p.external_quote_in ∧ U128 p.external_quote_out ∧ U128 p.vault_before ∧
  U128 p.vault_after

/-- K49 `ensures` for `Ok(())`, with `old_self` the value of `*self` at entry and `new_self` its
value at return: `self.debits[class as usize] ==
old(self.debits[class as usize]).wrapping_add(amount)`. -/
def debit_ensures (old_self : TokenValueFlowProof) («class» amount : Nat)
    (new_self : TokenValueFlowProof) : Prop :=
  new_self.debits.getD «class» 0 = wrapAdd128 (old_self.debits.getD «class» 0) amount

/-- Every other part of `*self` is unchanged: all `debits` entries other than `class`, the whole
`credits` array and the four scalar fields. (`modifies(self)` would allow them to change; this
states that they do not.) -/
def debit_frame (old_self : TokenValueFlowProof) («class» : Nat) (new_self : TokenValueFlowProof) :
    Prop :=
  new_self.debits.length = old_self.debits.length ∧
  (∀ j, j ≠ «class» → new_self.debits.getD j 0 = old_self.debits.getD j 0) ∧
  new_self.credits = old_self.credits ∧
  new_self.external_quote_in = old_self.external_quote_in ∧
  new_self.external_quote_out = old_self.external_quote_out ∧
  new_self.vault_before = old_self.vault_before ∧
  new_self.vault_after = old_self.vault_after

/-! ## K50 `apply_total_delta(total: u128, old: u128, new: u128) -> V16Result<u128>` -/

/-- K50 `ensures` for `Ok(t)`: `(t as i128).wrapping_sub(total as i128) ==
(new as i128).wrapping_sub(old as i128)`. -/
def apply_total_delta_ensures (total old new : Nat) (t : Nat) : Prop :=
  wrapI128 (castU128I128 t - castU128I128 total) = wrapI128 (castU128I128 new - castU128I128 old)

/-! ## K51 `credit_backing_provider_earnings_delta(vault: u128, c_tot: u128, insurance: u128,
earnings_total: u128, bucket_earnings: u128, amount: u128) -> V16Result<(u128, u128)>` -/

/-- K51 `ensures` for `Ok((et, be))`: `et == earnings_total.wrapping_add(amount)`,
`be == bucket_earnings.wrapping_add(amount)` and `c_tot.wrapping_add(insurance) + et <= vault`
(the outer `+` exact). -/
def credit_earnings_ensures (vault c_tot insurance earnings_total bucket_earnings amount : Nat)
    (et be : Nat) : Prop :=
  et = wrapAdd128 earnings_total amount ∧ be = wrapAdd128 bucket_earnings amount ∧
  wrapAdd128 c_tot insurance + et ≤ vault

/-! ## K52 `set_domain_insurance_spent_delta(total_remaining: u128, insurance: u128,
budget: u128, old_spent: u128, new_spent: u128) -> V16Result<u128>` -/

/-- K52 `requires`: `old_spent <= budget && new_spent <= budget`. -/
def set_spent_requires (budget old_spent new_spent : Nat) : Prop :=
  old_spent ≤ budget ∧ new_spent ≤ budget

/-- The common shape of the K52 and K53 `ensures` for `Ok(t)`: with
`old_remaining`, `new_remaining` as given, `(if new_remaining >= old_remaining
{ t == total_remaining + new_remaining.wrapping_sub(old_remaining) } else
{ t == total_remaining - old_remaining.wrapping_sub(new_remaining) }) && t <= cap`
(outer `+` / `-` exact). -/
def remaining_shift_ensures (total_remaining cap old_remaining new_remaining : Nat) (t : Nat) :
    Prop :=
  (if new_remaining ≥ old_remaining then
    (t : Int) = (total_remaining : Int) + (wrapSub128 new_remaining old_remaining : Int)
  else
    (t : Int) = (total_remaining : Int) - (wrapSub128 old_remaining new_remaining : Int)) ∧
  t ≤ cap

/-- K52 `ensures` for `Ok(t)`, with `old_remaining = budget.wrapping_sub(old_spent)` and
`new_remaining = budget.wrapping_sub(new_spent)`, capped by `insurance`. -/
def set_spent_ensures (total_remaining insurance budget old_spent new_spent : Nat) (t : Nat) :
    Prop :=
  remaining_shift_ensures total_remaining insurance (wrapSub128 budget old_spent)
    (wrapSub128 budget new_spent) t

/-! ## K53 `set_domain_insurance_budget_delta(total_remaining: u128, insurance_limit: u128,
old_budget: u128, spent: u128, new_budget: u128) -> V16Result<u128>` -/

/-- K53 `requires`: `spent <= old_budget && spent <= new_budget`. -/
def set_budget_requires (old_budget spent new_budget : Nat) : Prop :=
  spent ≤ old_budget ∧ spent ≤ new_budget

/-- K53 `ensures` for `Ok(t)`, with `old_remaining = old_budget.wrapping_sub(spent)` and
`new_remaining = new_budget.wrapping_sub(spent)`, capped by `insurance_limit`. -/
def set_budget_ensures (total_remaining insurance_limit old_budget spent new_budget : Nat)
    (t : Nat) : Prop :=
  remaining_shift_ensures total_remaining insurance_limit (wrapSub128 old_budget spent)
    (wrapSub128 new_budget spent) t

/-! ## K54 `withdraw_domain_insurance_delta(vault: u128, insurance: u128,
source_reserved_atoms: u128, budget: u128, spent: u128, domain_reserved_atoms: u128,
amount: u128) -> V16Result<(u128, u128, u128)>` -/

/-- K54 `ensures` for `Ok((v, i, b))`: `v == vault.wrapping_sub(amount)`,
`i == insurance.wrapping_sub(amount)`, `b == budget.wrapping_sub(amount)`,
`amount <= insurance.saturating_sub(source_reserved_atoms)` and
`amount <= budget.saturating_sub(spent).saturating_sub(domain_reserved_atoms)`. -/
def withdraw_insurance_ensures (vault insurance source_reserved_atoms budget spent
    domain_reserved_atoms amount : Nat) (v i b : Nat) : Prop :=
  v = wrapSub128 vault amount ∧ i = wrapSub128 insurance amount ∧ b = wrapSub128 budget amount ∧
  amount ≤ satSub128 insurance source_reserved_atoms ∧
  amount ≤ satSub128 (satSub128 budget spent) domain_reserved_atoms

/-! ## K55 `credit_account_from_insurance_delta(insurance: u128, budget_remaining: u128,
c_tot: u128, capital: u128, amount: u128) -> V16Result<(u128, u128, u128)>` -/

/-- K55 `ensures` for `Ok((i, c, cap))`: `i == insurance.wrapping_sub(amount)`,
`c == c_tot.wrapping_add(amount)`, `cap == capital.wrapping_add(amount)` and
`i >= budget_remaining`. -/
def credit_from_insurance_ensures (insurance budget_remaining c_tot capital amount : Nat)
    (i c cap : Nat) : Prop :=
  i = wrapSub128 insurance amount ∧ c = wrapAdd128 c_tot amount ∧
  cap = wrapAdd128 capital amount ∧ i ≥ budget_remaining

/-! ## K56 `retire_terminal_unbudgeted_insurance_delta(vault: u128, insurance: u128,
budget_remaining: u128, source_reserved_atoms: u128) -> V16Result<(u128, u128, u128)>` -/

/-- K56 `ensures` for `Ok((retired, next_vault, next_insurance))`: `retired == vault`,
`next_vault == 0`, `next_insurance == 0`, `insurance <= vault`, `budget_remaining == 0` and
`source_reserved_atoms == 0`. -/
def retire_insurance_ensures (vault insurance budget_remaining source_reserved_atoms : Nat)
    (retired next_vault next_insurance : Nat) : Prop :=
  retired = vault ∧ next_vault = 0 ∧ next_insurance = 0 ∧ insurance ≤ vault ∧
  budget_remaining = 0 ∧ source_reserved_atoms = 0

end Spec.Kernel
