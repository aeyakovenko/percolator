module

public import Spec.TradeRep
public import Spec.TradeConst

/-!
# Trade execution: contracts of the called methods

Each definition below is the contract of one method called by the three trade methods, stated as a
`Property` about the compiled program. The properties of Parts 1 to 3 (`Spec.Trade`) take the
contracts they rely on as explicit hypotheses; the Part 4 results prove some of them.

All contracts are partial: they constrain a call that returns (with `Ok` or `Err`); a panic is not
constrained, and nothing says that the call terminates. Every contract hands back ownership of
the market view and of the accounts on every return, in a well-formed state (header fields in
range, 1397-byte slots, unchanged slice length, 9419-byte accounts). The view's private scratch
memory `protection_observed` (`obs`) is not stored state: a contract constrains it only where it
says "nothing is written".

TC1 to TC9 are the user's contracts; where one is stated more strongly than in the request, its
docstring says so. Each contract takes a flag `np`: `np = false` is the contract as stated (a
call may panic); `np = true` additionally says that the call does not panic. The others are
contracts of further callees (`ensure_initial_margin`,
`asset_state`, the fee and notional helpers, ...) that the request did not list.
-/

@[expose] public section

namespace Spec.Trade
open Iris Iris.BI Rust2Lean.Public
open Spec.LayerT (HeaderAccount)

/-! ## Function handles -/

namespace F
abbrev finishTradeChecks := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::finish_trade_checks_not_atomic»
abbrev applyTrade := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::apply_trade_after_refresh_not_atomic»
abbrev executeBatch := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::execute_batch_with_fee_loss_stale_scoped_not_atomic»
abbrev chargeFee := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::charge_account_fee_current_not_atomic»
abbrev preflight := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::validate_trade_position_preflight»
abbrev riskChange := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::require_asset_risk_change_allowed»
abbrev recertify := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::recertify_account_after_trade_delta»
abbrev oiCapGate := Mir.Trade.Fn.«percolator::v16::V16Core::kernel_trade_side_oi_cap_gate»
abbrev terminalResidual := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::begin_terminal_trade_residual_if_needed»
abbrev positionDelta := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::apply_current_position_delta_with_lookup»
abbrev imLien := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::create_initial_margin_source_lien_if_needed»
abbrev ensureIM := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::ensure_initial_margin»
abbrev ensureLockedIM := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::ensure_no_positive_credit_initial_margin»
abbrev assetState := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::asset_state»
abbrev notionalFloor := Mir.Trade.Fn.«percolator::v16::trade_notional_floor»
abbrev feeNotionalCeil := Mir.Trade.Fn.«percolator::v16::trade_fee_notional_ceil»
abbrev feeBps := Mir.Trade.Fn.«percolator::v16::checked_fee_bps»
abbrev sizeDeltas := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::trade_signed_size_deltas»
abbrev effAbsQ := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::effective_abs_q_for_raw_basis»
abbrev canIgnore := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::can_ignore_unrelated_loss_stale_for_trade»
abbrev afterTail := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::execute_batch_with_fee_after_tail_validation_not_atomic»
abbrev validateTail := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::validate_unconfigured_market_tail»
abbrev validateShape := Mir.Trade.Fn.«percolator::v16::MarketGroupV16ViewMut::<'_, ()>::validate_shape»
abbrev validateWithMarket := Mir.Trade.Fn.«percolator::v16::PortfolioV16ViewMut::<'_>::validate_with_market::<()>»
end F

/-! ## Contract calls -/

/-- A contract call of `f`: with `np = false`, a call that may panic (`callR`); with `np = true`,
a call that does not panic (`S.call` with `S.returns`). Either way, if it returns a value `r`,
then `r` is a valid value of `f`'s return type and `Q r` holds. -/
def callC {PROP : Type} [BI PROP] (S : Vocab PROP) (np : Bool) (f : RustFn)
    (args : List (S.Value × S.Ty)) (Q : S.Value → PROP) : PROP :=
  if np then S.call f args (S.returns fun r => iprop(⌊S.loadable r (S.retTy f)⌋ ∗ Q r))
  else callR S f args (fun r => iprop(⌊S.loadable r (S.retTy f)⌋ ∗ Q r))

/-- A contract call of a method that is lent a shared borrow of the caller's memory: as `callC`,
and if it panics (`np = false`), the borrowed resources `U` are given back unchanged (a shared
borrow ends when the callee unwinds). -/
def callCU {PROP : Type} [BI PROP] (S : Vocab PROP) (np : Bool) (f : RustFn)
    (args : List (S.Value × S.Ty)) (U : PROP) (Q : S.Value → PROP) : PROP :=
  if np then S.call f args (S.returns fun r => iprop(⌊S.loadable r (S.retTy f)⌋ ∗ Q r))
  else iprop(∀ before : List S.Ptr, S.callUnwind f args before (fun o _ => match o with
    | .returned r => iprop(⌊S.loadable r (S.retTy f)⌋ ∗ Q r)
    | .unwound => U))

/-- The market view at `self` as seen by a `&self` method: a share `q` of the view cell (a
shared retag of the cell needs only a share), and the header and the slots it points to (behind
`&mut`, so not shared by that retag). With `q = 1` this is `ViewOwned`. -/
def ViewCellShared {PROP : Type} [BI PROP] (S : Vocab PROP) (self hp mp : S.Ptr) (q : Qp)
    (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat) : PROP :=
  iprop(
    S.pointsToF self q (viewMutTy S)
      (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length, u16ArrayValue S obs]) ∗
    S.pointsTo hp (headerTy S) (Spec.LayerT.headerValue S h) ∗
    S.array mp (marketTy S) marketStride 1 (ms.map (marketValue S)))

/-! ## Shapes of postconditions -/

/-- After a call that may write the market and one account: ownership of both, in a well-formed
state with the same slice length, and `P` of the result and the new state. -/
def After1 {PROP : Type} [BI PROP] (S : Vocab PROP) (self hp mp acc ap : S.Ptr)
    (ms : List (List Nat))
    (P : S.Value → HeaderAccount → List (List Nat) → List Nat → List Nat → Prop) (r : S.Value) :
    PROP :=
  iprop(∃ (h' : HeaderAccount) (ms' : List (List Nat)) (obs' bs' : List Nat),
    ViewOwned S self hp mp h' ms' obs' ∗ AccountOwned S acc ap bs' ∗
    ⌊MarketOk h' ms' obs' ∧ ms'.length = ms.length ∧ AccountBytes bs' ∧ P r h' ms' obs' bs'⌋)

/-- After a call that may write the market and two accounts. -/
def After2 {PROP : Type} [BI PROP] (S : Vocab PROP) (self hp mp la lap sa sap : S.Ptr)
    (ms : List (List Nat))
    (P : S.Value → HeaderAccount → List (List Nat) → List Nat → List Nat → List Nat → Prop)
    (r : S.Value) : PROP :=
  iprop(∃ (h' : HeaderAccount) (ms' : List (List Nat)) (obs' lbs' sbs' : List Nat),
    ViewOwned S self hp mp h' ms' obs' ∗ AccountOwned S la lap lbs' ∗ AccountOwned S sa sap sbs' ∗
    ⌊MarketOk h' ms' obs' ∧ ms'.length = ms.length ∧ AccountBytes lbs' ∧ AccountBytes sbs' ∧
      P r h' ms' obs' lbs' sbs'⌋)

/-! ## TC1 `charge_account_fee_current_not_atomic` -/

/-- **TC1.** `charge_account_fee_current_not_atomic(account, requested_fee)`: if it returns
`Ok(charged)`, then `charged = chargedFee requested_fee pnl capital` (`0` if the fee is `0` or the
PnL is negative, else `min(requested_fee, capital)`); if `charged = 0` nothing is written;
otherwise exactly `capital` (lowered by `charged`), the header's `c_tot` (lowered by `charged`) and
`insurance` (raised by `charged`) and the certificate's `valid` byte (set to `0`) are written; the
market slots, `vault` and everything else are not. -/
def tc1_charge_fee (np : Bool) : Property := fun S =>
  ∀ (self hp mp acc ap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (obs bs : List Nat)
    (requested_fee : Nat),
    MarketOk h ms obs → AccountBytes bs → requested_fee < 2 ^ 128 →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S acc ap bs ⊢
      callC S np F.chargeFee (argsOf S F.chargeFee
          [S.ptrVal self, S.ptrVal acc, S.intVal requested_fee])
        (After1 S self hp mp acc ap ms fun r h' ms' obs' bs' =>
          ∀ charged : Nat, r = okV S (S.intVal charged) →
            charged = chargedFee requested_fee (accPnl bs) (accCapital bs) ∧
            (charged = 0 → h' = h ∧ ms' = ms ∧ obs' = obs ∧ bs' = bs) ∧
            (charged ≠ 0 →
              charged ≤ h.c_tot ∧
              h' = { h with c_tot := h.c_tot - charged, insurance := h.insurance + charged } ∧
              ms' = ms ∧ obs' = obs ∧
              SameExcept [rCapital, rCertValid] bs bs' ∧
              accCapital bs' = accCapital bs - charged ∧ accCertValid bs' = 0))

/-! ## TC2 `validate_trade_position_preflight` -/

/-- **TC2.** `validate_trade_position_preflight(&long_view, &short_view, request, long_lookup,
short_lookup)` (takes `&self`) writes nothing, and if it returns `Ok(p)`: `p.long_lookup` and
`p.short_lookup` are the lookups passed in; `p.long_old_abs_q = |long_lookup.current_effective_q|`,
`p.long_new_abs_q = |long_lookup.next_effective_q|` and the same for the short side; and
`p.risk_increasing` holds exactly when `|next_effective_q| > |current_effective_q|` for at least
one of the two lookups. If it panics, the borrowed view and account views are given back. -/
def tc2_preflight (np : Bool) : Property := fun S =>
  ∀ (self hp mp lv lap sv sap : S.Ptr) (qv : Qp) (ql qs : Qp) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (req : TradeRequest) (ll sl : Lookup),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs → req.WF → ll.WF → sl.WF →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
      AccountViewShared S sv sap qs sbs ⊢
      callCU S np F.preflight (argsOf S F.preflight
          [S.ptrVal self, S.ptrVal lv, S.ptrVal sv, tradeRequestValue S req, lookupValue S ll,
           lookupValue S sl])
        iprop(ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
          AccountViewShared S sv sap qs sbs)
        (fun r => iprop(ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
          AccountViewShared S sv sap qs sbs ∗
          ⌊∀ p : Preflight, r = okV S (preflightValue S p) →
            p.long_lookup = ll ∧ p.short_lookup = sl ∧
            p.long_old_abs_q = ll.current_effective_q.natAbs ∧
            p.long_new_abs_q = ll.next_effective_q.natAbs ∧
            p.short_old_abs_q = sl.current_effective_q.natAbs ∧
            p.short_new_abs_q = sl.next_effective_q.natAbs ∧
            (p.risk_increasing = true ↔
              (ll.next_effective_q.natAbs > ll.current_effective_q.natAbs ∨
               sl.next_effective_q.natAbs > sl.current_effective_q.natAbs))⌋))

/-! ## TC3 `require_asset_risk_change_allowed` -/

/-- **TC3.** `require_asset_risk_change_allowed(asset_index, risk_increasing)` (takes `&self`)
writes nothing; with `risk_increasing` true it returns `Ok(())` only if slot `asset_index` exists
and its asset's stored `a_long` and `a_short` both equal `ADL_ONE`. -/
def tc3_risk_change (np : Bool) : Property := fun S =>
  ∀ (self hp mp : S.Ptr) (qv : Qp) (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat)
    (asset_index : Nat) (risk_increasing : Bool),
    MarketOk h ms obs → asset_index < 2 ^ 64 →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ⊢
      callC S np F.riskChange (argsOf S F.riskChange
          [S.ptrVal self, S.intVal asset_index, S.boolVal risk_increasing])
        (fun r => iprop(ViewCellShared S self hp mp qv h ms obs ∗
          ⌊r = okUnit S → risk_increasing = true →
            ∃ bs, ms[asset_index]? = some bs ∧ slotALong bs = ADL_ONE ∧ slotAShort bs = ADL_ONE⌋))

/-! ## TC4 `begin_terminal_trade_residual_if_needed` -/

/-- **TC4 (as used here).** `begin_terminal_trade_residual_if_needed(account, asset_index,
lookup, is_final_batch_fill, attributable_asset_before_refresh)`: if it returns `Ok(())`, it has
written no header field other than `resolved_payout_blocker_count`, no account field other than
`close_progress`, and **no market slot other than the slot of `asset_index`** (this slot clause is
added to the user's TC4: the body's `begin_close_progress_ledger` writes the pending-loss-barrier
count of that slot); and it has written nothing at all when `is_final_batch_fill` is false, when
`lookup.current_q = 0` or `lookup.next_q ≠ 0`, or when the account's `pnl ≥ 0`. -/
def tc4_terminal_residual (np : Bool) : Property := fun S =>
  ∀ (self hp mp acc ap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (obs bs : List Nat)
    (asset_index : Nat) (lk : Lookup) (is_final : Bool) (attributable : Option Nat),
    MarketOk h ms obs → AccountBytes bs → asset_index < 2 ^ 64 → lk.WF →
    (∀ x, attributable = some x → x < 2 ^ 64) →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S acc ap bs ⊢
      callC S np F.terminalResidual (argsOf S F.terminalResidual
          [S.ptrVal self, S.ptrVal acc, S.intVal asset_index, lookupValue S lk,
           S.boolVal is_final, optValue S attributable])
        (After1 S self hp mp acc ap ms fun r h' ms' obs' bs' =>
          r = okUnit S →
            (∃ rpbc, h' = { h with resolved_payout_blocker_count := rpbc }) ∧
            SameExcept [rCloseProgress] bs bs' ∧ SlotsSameExcept asset_index ms ms' ∧
            ((is_final = false ∨ lk.current_q = 0 ∨ lk.next_q ≠ 0 ∨ accPnl bs ≥ 0) →
              h' = h ∧ ms' = ms ∧ obs' = obs ∧ bs' = bs))

/-! ## TC5 `apply_current_position_delta_with_lookup` -/

/-- **TC5 (as used here).** `apply_current_position_delta_with_lookup(account, asset_index,
delta_q, lookup)`: if it returns `Ok(())`, it has written no header field other than
`resolved_payout_blocker_count` and `risk_epoch`, no account field other than `active_bitmap`,
`health_cert`, `legs` and `liquidation_lock`, and no market slot other than the slot of
`asset_index`; with `delta_q ≠ 0`, the account's signed raw position on `asset_index` is
`lookup.next_q` afterwards and its legs on other assets are unchanged; and **with `delta_q = 0`
nothing at all is written** (this clause is added to the user's TC5; the body returns `Ok(())` at
once when `delta_q == 0`). -/
def tc5_position_delta (np : Bool) : Property := fun S =>
  ∀ (self hp mp acc ap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (obs bs : List Nat)
    (asset_index : Nat) (delta_q : Int) (lk : Lookup),
    MarketOk h ms obs → AccountBytes bs → asset_index < 2 ^ 64 → I128v delta_q → lk.WF →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S acc ap bs ⊢
      callC S np F.positionDelta (argsOf S F.positionDelta
          [S.ptrVal self, S.ptrVal acc, S.intVal asset_index, S.intVal delta_q, lookupValue S lk])
        (After1 S self hp mp acc ap ms fun r h' ms' obs' bs' =>
          r = okUnit S →
            (∃ rpbc re, h' = { h with resolved_payout_blocker_count := rpbc, risk_epoch := re }) ∧
            SameExcept [rActiveBitmap, rHealthCert, rLegs, rLiquidationLock] bs bs' ∧
            SlotsSameExcept asset_index ms ms' ∧
            (delta_q ≠ 0 → rawPosition bs' asset_index = lk.next_q ∧ OtherLegsSame asset_index bs bs') ∧
            (delta_q = 0 → h' = h ∧ ms' = ms ∧ obs' = obs ∧ bs' = bs))

/-! ## TC7 `recertify_account_after_trade_delta` -/

/-- **TC7.** `recertify_account_after_trade_delta(account, asset_index, old_abs_q, new_abs_q,
price)` (takes `&self`) writes nothing in the market, and in the account at most `health_cert`. -/
def tc7_recertify (np : Bool) : Property := fun S =>
  ∀ (self hp mp acc ap : S.Ptr) (qv : Qp) (h : HeaderAccount) (ms : List (List Nat)) (obs bs : List Nat)
    (asset_index old_abs_q new_abs_q price : Nat),
    MarketOk h ms obs → AccountBytes bs → asset_index < 2 ^ 64 → old_abs_q < 2 ^ 128 →
    new_abs_q < 2 ^ 128 → price < 2 ^ 64 →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ∗ AccountOwned S acc ap bs ⊢
      callC S np F.recertify (argsOf S F.recertify
          [S.ptrVal self, S.ptrVal acc, S.intVal asset_index, S.intVal old_abs_q,
           S.intVal new_abs_q, S.intVal price])
        (fun r => iprop(∃ bs' : List Nat, ViewCellShared S self hp mp qv h ms obs ∗ AccountOwned S acc ap bs' ∗
          ⌊AccountBytes bs' ∧ SameExcept [rHealthCert] bs bs'⌋))

/-! ## TC8 `kernel_trade_side_oi_cap_gate` -/

/-- **TC8.** `V16Core::kernel_trade_side_oi_cap_gate(oi_long_before_q, oi_short_before_q,
oi_long_after_q, oi_short_after_q)` returns `Ok(())` exactly when neither side grew above the cap:
for each side, `after ≤ MAX_OI_SIDE_Q` or `after ≤ before`; otherwise `Err(InvalidLeg)` (K17 of
the kernel contracts, here for this program's copy). -/
def tc8_oi_cap_gate (np : Bool) : Property := fun S =>
  ∀ (lb sb la sa : Nat), lb < 2 ^ 128 → sb < 2 ^ 128 → la < 2 ^ 128 → sa < 2 ^ 128 →
    ⊢ callC S np F.oiCapGate (argsOf S F.oiCapGate [S.intVal lb, S.intVal sb, S.intVal la, S.intVal sa])
        (fun r => iprop(⌊(r = okUnit S ↔
            ((la ≤ MAX_OI_SIDE_Q ∨ la ≤ lb) ∧ (sa ≤ MAX_OI_SIDE_Q ∨ sa ≤ sb))) ∧
          (r ≠ okUnit S → r = Spec.Kernel.errValue S (Spec.Kernel.v16ErrorValue S 4))⌋))

/-! ## TC9 `create_initial_margin_source_lien_if_needed` -/

/-- **TC9.** `create_initial_margin_source_lien_if_needed(account)`: if it returns `Ok(())`, the
account's certificate is valid (`health_cert.valid = 1`) at the return; it has written no account
field other than `capital`, `health_cert` and `source_domains`, and no header field other than
`backing_provider_earnings_total`, `c_tot`, `risk_epoch`, `source_claim_bound_total_num`,
`source_fresh_backing_total_num` and `source_insurance_credit_reserved_total_atoms`. (It is not
passed the other account, so it cannot write it.) -/
def tc9_im_lien (np : Bool) : Property := fun S =>
  ∀ (self hp mp acc ap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat)) (obs bs : List Nat),
    MarketOk h ms obs → AccountBytes bs →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S acc ap bs ⊢
      callC S np F.imLien (argsOf S F.imLien [S.ptrVal self, S.ptrVal acc])
        (After1 S self hp mp acc ap ms fun r h' _ms' _obs' bs' =>
          r = okUnit S →
            accCertValid bs' = 1 ∧
            SameExcept [rCapital, rHealthCert, rSourceDomains] bs bs' ∧
            ∃ a b c d e f, h' =
              { h with
                backing_provider_earnings_total := a
                c_tot := b
                risk_epoch := c
                source_claim_bound_total_num := d
                source_fresh_backing_total_num := e
                source_insurance_credit_reserved_total_atoms := f })

/-! ## Margin checks called by `finish_trade_checks_not_atomic` -/

/-- **EIM** (`ensure_initial_margin(&view)`, an associated function). It writes nothing, and it
returns `Ok(())` only if the stored certificate is valid and `certified_equity ≥ 0` and
`certified_equity ≥ certified_initial_req` (the admission of K08 `kernel_initial_margin_gate`,
applied to the decoded certificate). If it panics, the borrowed account view is given back. -/
def eim_ensure_initial_margin (np : Bool) : Property := fun S =>
  ∀ (v ap : S.Ptr) (q : Qp) (bs : List Nat),
    AccountBytes bs →
    Constants S ∗ AccountViewShared S v ap q bs ⊢
      callCU S np F.ensureIM (argsOf S F.ensureIM [S.ptrVal v]) (AccountViewShared S v ap q bs)
        (fun r => iprop(AccountViewShared S v ap q bs ∗
          ⌊r = okUnit S → accCertValid bs = 1 ∧ accCertEquity bs ≥ 0 ∧
            accCertEquity bs ≥ accCertInitialReq bs⌋))

/-- **ELM** (`ensure_no_positive_credit_initial_margin(&view)`, an associated function). It
writes nothing, and it returns `Ok(())` only if `capital + min(pnl, 0) - |fee_credits|` is not
negative and is at least the stored `certified_initial_req` (the admission of K09
`kernel_locked_margin_gate`, applied to the stored fields). If it panics, the borrowed account view
is given back. -/
def elm_ensure_locked_margin (np : Bool) : Property := fun S =>
  ∀ (v ap : S.Ptr) (q : Qp) (bs : List Nat),
    AccountBytes bs →
    Constants S ∗ AccountViewShared S v ap q bs ⊢
      callCU S np F.ensureLockedIM (argsOf S F.ensureLockedIM [S.ptrVal v])
        (AccountViewShared S v ap q bs)
        (fun r => iprop(AccountViewShared S v ap q bs ∗
          ⌊r = okUnit S →
            (accCapital bs : Int) + min (accPnl bs) 0 - (accFeeCredits bs).natAbs ≥ 0 ∧
            (accCapital bs : Int) + min (accPnl bs) 0 - (accFeeCredits bs).natAbs ≥
              accCertInitialReq bs⌋))

/-! ## Further callees of `apply_trade_after_refresh_not_atomic` -/

/-- **AS** (`asset_state(asset_index)`, takes `&self`). For any share `q` of the view cell: it
writes nothing, and if it returns `Ok(a)`, slot `asset_index` exists and `a.effective_price`,
`a.a_long`, `a.a_short`, `a.oi_eff_long_q` and `a.oi_eff_short_q` are the slot's stored values. If
it panics, the borrowed view is given back. -/
def as_asset_state (np : Bool) : Property := fun S =>
  ∀ (self hp mp : S.Ptr) (q : Qp) (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat)
    (asset_index : Nat),
    MarketOk h ms obs → asset_index < 2 ^ 64 →
    Constants S ∗ ViewCellShared S self hp mp q h ms obs ⊢
      callCU S np F.assetState (argsOf S F.assetState [S.ptrVal self, S.intVal asset_index])
        (ViewCellShared S self hp mp q h ms obs)
        (fun r => iprop(ViewCellShared S self hp mp q h ms obs ∗
          ⌊∀ a, r = okV S a → ∃ bs, ms[asset_index]? = some bs ∧
            AssetStateHas S a (slotEffectivePrice bs) (slotALong bs) (slotAShort bs)
              (slotOiEffLong bs) (slotOiEffShort bs)⌋))

/-- **NF** (`trade_notional_floor(size_q, exec_price)`): if it returns `Ok(n)`, then
`n = floor(size_q * exec_price / POS_SCALE)`. -/
def nf_notional_floor (np : Bool) : Property := fun S =>
  ∀ (size_q exec_price : Nat), size_q < 2 ^ 128 → exec_price < 2 ^ 64 →
    Constants S ⊢ callC S np F.notionalFloor (argsOf S F.notionalFloor [S.intVal size_q, S.intVal exec_price])
      (fun r => iprop(⌊∀ n : Nat, r = okV S (S.intVal n) → n = tradeNotionalFloor size_q exec_price⌋))

/-- **FNC** (`trade_fee_notional_ceil(size_q, exec_price)`): if it returns `Ok(n)`, then
`n = tradeFeeNotionalCeil size_q exec_price`. -/
def fnc_fee_notional_ceil (np : Bool) : Property := fun S =>
  ∀ (size_q exec_price : Nat), size_q < 2 ^ 128 → exec_price < 2 ^ 64 →
    Constants S ⊢ callC S np F.feeNotionalCeil (argsOf S F.feeNotionalCeil [S.intVal size_q, S.intVal exec_price])
      (fun r => iprop(⌊∀ n : Nat, r = okV S (S.intVal n) → n = tradeFeeNotionalCeil size_q exec_price⌋))

/-- **FB** (`checked_fee_bps(notional, fee_bps)`): if it returns `Ok(f)`, then
`f = checkedFeeBps notional fee_bps`. -/
def fb_fee_bps (np : Bool) : Property := fun S =>
  ∀ (notional fee_bps : Nat), notional < 2 ^ 128 → fee_bps < 2 ^ 64 →
    Constants S ⊢ callC S np F.feeBps (argsOf S F.feeBps [S.intVal notional, S.intVal fee_bps])
      (fun r => iprop(⌊∀ f : Nat, r = okV S (S.intVal f) → f = checkedFeeBps notional fee_bps⌋))

/-- **SD** (`trade_signed_size_deltas(size_q)`): if it returns `Ok((abs, d1, d2))`, then
`abs = |size_q|` (and `abs < 2^128`). -/
def sd_size_deltas (np : Bool) : Property := fun S =>
  ∀ (size_q : Int), I128v size_q →
    Constants S ⊢ callC S np F.sizeDeltas (argsOf S F.sizeDeltas [S.intVal size_q])
      (fun r => iprop(⌊∀ (a : Nat) (d1 d2 : Int),
        r = okV S (S.tupleVal [S.intVal a, S.intVal d1, S.intVal d2]) → a = size_q.natAbs⌋))

/-! ## Callees of `execute_batch_with_fee_loss_stale_scoped_not_atomic`

These contracts are parametric: `Θ`, `Γ`, `Λ` and `Ψ` stand for whatever the called method is
known to guarantee. The Part 3 properties hold for every choice that makes the contracts true,
so they apply in particular to the exact behaviour of each callee. -/

/-- **VT** (`validate_unconfigured_market_tail()`, takes `&self`): writes nothing. -/
def vt_validate_tail (np : Bool) : Property := fun S =>
  ∀ (self hp mp : S.Ptr) (qv : Qp) (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat),
    MarketOk h ms obs →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ⊢
      callC S np F.validateTail (argsOf S F.validateTail [S.ptrVal self])
        (fun _ => ViewCellShared S self hp mp qv h ms obs)

/-- **CI(Γ)** (`can_ignore_unrelated_loss_stale_for_trade(&long_view, &short_view,
asset_index)`, takes `&self`): writes nothing, and returns `Ok(b)` only if
`Γ h ms lbs sbs asset_index b`. If it panics, the borrowed views are given back. -/
def ci_can_ignore (Γ : HeaderAccount → List (List Nat) → List Nat → List Nat → Nat → Bool → Prop) (np : Bool) :
    Property := fun S =>
  ∀ (self hp mp lv lap sv sap : S.Ptr) (qv : Qp) (ql qs : Qp) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (asset_index : Nat),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs → asset_index < 2 ^ 64 →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
      AccountViewShared S sv sap qs sbs ⊢
      callCU S np F.canIgnore (argsOf S F.canIgnore
          [S.ptrVal self, S.ptrVal lv, S.ptrVal sv, S.intVal asset_index])
        iprop(ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
          AccountViewShared S sv sap qs sbs)
        (fun r => iprop(ViewCellShared S self hp mp qv h ms obs ∗ AccountViewShared S lv lap ql lbs ∗
          AccountViewShared S sv sap qs sbs ∗
          ⌊∀ b : Bool, r = okV S (S.boolVal b) → Γ h ms lbs sbs asset_index b⌋))

/-- The Rust type `TradeRequestV16`. -/
def tradeRequestTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::trade_request_type»

/-- A share `q` of the slice of trade requests at `rp` (48 bytes each). -/
def RequestsShared {PROP : Type} [BI PROP] (S : Vocab PROP) (rp : S.Ptr) (q : Qp)
    (reqs : List TradeRequest) : PROP :=
  S.array rp (tradeRequestTy S) 48 q (reqs.map (tradeRequestValue S))

/-- **OPQ(Ψ)** (`execute_batch_with_fee_after_tail_validation_not_atomic(long, short, requests)`,
opaque): it may write anything in the market and the two accounts and return `Ok` or `Err`; `Ψ`
relates its starting state, its final state and its result. -/
def opq_after_tail
    (Ψ : HeaderAccount → List (List Nat) → List Nat → List Nat →
      HeaderAccount → List (List Nat) → List Nat → List Nat → Prop) (np : Bool) :
    Property := fun S =>
  ∀ (self hp mp la lap sa sap rp : S.Ptr) (q : Qp) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (reqs : List TradeRequest),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs → (∀ r ∈ reqs, r.WF) →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S la lap lbs ∗
      AccountOwned S sa sap sbs ∗ RequestsShared S rp q reqs ⊢
      callC S np F.afterTail (argsOf S F.afterTail
          [S.ptrVal self, S.ptrVal la, S.ptrVal sa, S.slicePtrVal rp reqs.length])
        (fun r => iprop(RequestsShared S rp q reqs ∗
          After2 S self hp mp la lap sa sap ms
            (fun _ h' ms' _ lbs' sbs' => Ψ h ms lbs sbs h' ms' lbs' sbs') r))

/-- **VS(Θ)** (`validate_shape()` of the mutable view, takes `&self`): writes nothing, and returns
`Ok(())` only if `Θ h ms`. -/
def vs_validate_shape (Θ : HeaderAccount → List (List Nat) → Prop) (np : Bool) :
    Property := fun S =>
  ∀ (self hp mp : S.Ptr) (qv : Qp) (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat),
    MarketOk h ms obs →
    Constants S ∗ ViewCellShared S self hp mp qv h ms obs ⊢
      callC S np F.validateShape (argsOf S F.validateShape [S.ptrVal self])
        (fun r => iprop(ViewCellShared S self hp mp qv h ms obs ∗ ⌊r = okUnit S → Θ h ms⌋))

/-- **VW(Λ)** (`PortfolioV16ViewMut::validate_with_market(&self, &market_view)`): writes nothing,
and returns `Ok(())` only if `Λ bs h ms` for the account bytes `bs`, header `h` and slots `ms` it
is given. If it panics, the borrowed account and market view are given back. -/
def vw_validate_with_market (Λ : List Nat → HeaderAccount → List (List Nat) → Prop)
    (np : Bool) : Property := fun S =>
  ∀ (acc ap mv hp mp : S.Ptr) (qa qm : Qp) (bs : List Nat) (h : HeaderAccount)
    (ms : List (List Nat)),
    AccountBytes bs → h.InRange → (∀ b ∈ ms, SlotBytes b) →
    Constants S ∗ S.pointsToF acc qa (accountViewMutTy S) (S.tupleVal [S.ptrVal ap]) ∗
      S.pointsToF ap qa (accountTy S) (Spec.LayerT.portfolioAccountValue S bs) ∗
      MarketViewShared S mv hp mp qm h ms ⊢
      callCU S np F.validateWithMarket (argsOf S F.validateWithMarket [S.ptrVal acc, S.ptrVal mv])
        iprop(S.pointsToF acc qa (accountViewMutTy S) (S.tupleVal [S.ptrVal ap]) ∗
          S.pointsToF ap qa (accountTy S) (Spec.LayerT.portfolioAccountValue S bs) ∗
          MarketViewShared S mv hp mp qm h ms)
        (fun r => iprop(S.pointsToF acc qa (accountViewMutTy S) (S.tupleVal [S.ptrVal ap]) ∗
          S.pointsToF ap qa (accountTy S) (Spec.LayerT.portfolioAccountValue S bs) ∗
          MarketViewShared S mv hp mp qm h ms ∗ ⌊r = okUnit S → Λ bs h ms⌋))

end Spec.Trade
