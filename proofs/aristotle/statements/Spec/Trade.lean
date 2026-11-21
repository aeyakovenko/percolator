module

public import Spec.TradeContracts

/-!
# Trade execution: properties of `finish_trade_checks_not_atomic` (Part 1) and
# `execute_batch_with_fee_loss_stale_scoped_not_atomic` (Part 3)

Each property is about one method body: it takes the contracts of the methods that body calls as
explicit hypotheses (`Spec.TradeContracts`), and concludes about every call that returns. A panic
is not constrained, and nothing says the call terminates. All methods are compiled at `T = ()`.

`m` is the market view (header `h`, slots `ms`, scratch `obs`); `L` and `S` are the two portfolio
accounts, with bytes `lbs` and `sbs`. Primed values are after the call.
-/

@[expose] public section

namespace Spec.Trade
open Iris Iris.BI Rust2Lean.Public
open Spec.LayerT (HeaderAccount)

/-! ## Part 1: `finish_trade_checks_not_atomic` -/

/-- The margin conclusion for one account's final bytes: the certificate is valid,
`certified_equity ≥ 0` and `certified_equity ≥ certified_initial_req`. -/
def MarginOk (bs : List Nat) : Prop :=
  accCertValid bs = 1 ∧ accCertEquity bs ≥ 0 ∧ accCertEquity bs ≥ accCertInitialReq bs

/-- The locked-lane conclusion for one account's final bytes: `capital + min(pnl, 0) -
|fee_credits|` is not negative and is at least `certified_initial_req`. -/
def LockedMarginOk (bs : List Nat) : Prop :=
  (accCapital bs : Int) + min (accPnl bs) 0 - (accFeeCredits bs).natAbs ≥ 0 ∧
  (accCapital bs : Int) + min (accPnl bs) 0 - (accFeeCredits bs).natAbs ≥ accCertInitialReq bs

/-- **TF.margin.** Assuming TC9 and the contracts EIM and ELM of `ensure_initial_margin` and
`ensure_no_positive_credit_initial_margin`: for all inputs and
all well-formed starting states, if `finish_trade_checks_not_atomic` returns `Ok(())`, then each
account whose `*_requires_initial_margin` argument is true has, at the return, a valid
certificate with `certified_equity ≥ 0` and `certified_equity ≥ certified_initial_req`. -/
def tf_margin : Property := fun S =>
  tc9_im_lien false S → eim_ensure_initial_margin false S → elm_ensure_locked_margin false S →
  ∀ (self hp mp la lap sa sap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (locked risk_increasing lrim srim lhsc shsc : Bool),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S la lap lbs ∗
      AccountOwned S sa sap sbs ⊢
      callR S F.finishTradeChecks (argsOf S F.finishTradeChecks
          [S.ptrVal self, S.ptrVal la, S.ptrVal sa, S.boolVal locked, S.boolVal risk_increasing,
           S.boolVal lrim, S.boolVal srim, S.boolVal lhsc, S.boolVal shsc])
        (After2 S self hp mp la lap sa sap ms fun r _h' _ms' _obs' lbs' sbs' =>
          r = okUnit S → (lrim = true → MarginOk lbs') ∧ (srim = true → MarginOk sbs'))

/-- **TF.locked.** Assuming TC9 and the contracts EIM and ELM of `ensure_initial_margin` and
`ensure_no_positive_credit_initial_margin`: if `locked` is true and
`finish_trade_checks_not_atomic` returns `Ok(())`, then each account whose
`*_requires_initial_margin` argument is true has, at the return, `capital + min(pnl, 0) -
|fee_credits| ≥ certified_initial_req` with that left side not negative. -/
def tf_locked : Property := fun S =>
  tc9_im_lien false S → eim_ensure_initial_margin false S → elm_ensure_locked_margin false S →
  ∀ (self hp mp la lap sa sap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (locked risk_increasing lrim srim lhsc shsc : Bool),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S la lap lbs ∗
      AccountOwned S sa sap sbs ⊢
      callR S F.finishTradeChecks (argsOf S F.finishTradeChecks
          [S.ptrVal self, S.ptrVal la, S.ptrVal sa, S.boolVal locked, S.boolVal risk_increasing,
           S.boolVal lrim, S.boolVal srim, S.boolVal lhsc, S.boolVal shsc])
        (After2 S self hp mp la lap sa sap ms fun r _h' _ms' _obs' lbs' sbs' =>
          r = okUnit S → locked = true →
            (lrim = true → LockedMarginOk lbs') ∧ (srim = true → LockedMarginOk sbs'))

/-- The header after a call that may write only `backing_provider_earnings_total`, `c_tot`,
`risk_epoch` and the three source totals. -/
def LienHeaderFrame (h h' : HeaderAccount) : Prop :=
  ∃ a b c d e f, h' =
    { h with
      backing_provider_earnings_total := a
      c_tot := b
      risk_epoch := c
      source_claim_bound_total_num := d
      source_fresh_backing_total_num := e
      source_insurance_credit_reserved_total_atoms := f }

/-- **TF.frame.** Assuming TC9 and the contracts EIM and ELM: if `finish_trade_checks_not_atomic`
returns `Ok(())`, then (a) when `risk_increasing` is false or `locked` is true, nothing at all was
written (header, slots, scratch memory, both accounts); (b) in every case neither account has a
written field other than `capital`, `health_cert` and `source_domains`, and the header has no
written field other than `backing_provider_earnings_total`, `c_tot`, `risk_epoch`,
`source_claim_bound_total_num`, `source_fresh_backing_total_num` and
`source_insurance_credit_reserved_total_atoms`. -/
def tf_frame : Property := fun S =>
  tc9_im_lien false S → eim_ensure_initial_margin false S → elm_ensure_locked_margin false S →
  ∀ (self hp mp la lap sa sap : S.Ptr) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (locked risk_increasing lrim srim lhsc shsc : Bool),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S la lap lbs ∗
      AccountOwned S sa sap sbs ⊢
      callR S F.finishTradeChecks (argsOf S F.finishTradeChecks
          [S.ptrVal self, S.ptrVal la, S.ptrVal sa, S.boolVal locked, S.boolVal risk_increasing,
           S.boolVal lrim, S.boolVal srim, S.boolVal lhsc, S.boolVal shsc])
        (After2 S self hp mp la lap sa sap ms fun r h' ms' obs' lbs' sbs' =>
          r = okUnit S →
            ((risk_increasing = false ∨ locked = true) →
              h' = h ∧ ms' = ms ∧ obs' = obs ∧ lbs' = lbs ∧ sbs' = sbs) ∧
            SameExcept [rCapital, rHealthCert, rSourceDomains] lbs lbs' ∧
            SameExcept [rCapital, rHealthCert, rSourceDomains] sbs sbs' ∧
            LienHeaderFrame h h')

/-! ## Part 3: `execute_batch_with_fee_loss_stale_scoped_not_atomic` -/

/-- The value of the local `ignore_unrelated_loss_stale` when the opaque call is made, given the
starting flag `f0` and the requests, for a `can_ignore_unrelated_loss_stale_for_trade` known to
guarantee `Γ`: if it is true, then `f0 = 1`, there is at least one request, and every request's
asset passed `can_ignore_unrelated_loss_stale_for_trade` with `Ok(true)`; if it is false, then
`f0 = 0`, or there is no request, or some request's asset got `Ok(false)` (the loop stops at the
first). -/
def IgnoreChar (Γ : HeaderAccount → List (List Nat) → List Nat → List Nat → Nat → Bool → Prop)
    (h : HeaderAccount) (ms : List (List Nat)) (lbs sbs : List Nat) (reqs : List TradeRequest)
    (ignore : Bool) : Prop :=
  (ignore = true → h.loss_stale_active = 1 ∧ reqs ≠ [] ∧
    ∀ r ∈ reqs, Γ h ms lbs sbs r.asset_index true) ∧
  (ignore = false → h.loss_stale_active = 0 ∨ reqs = [] ∨
    ∃ r ∈ reqs, Γ h ms lbs sbs r.asset_index false)

/-- The validity invariant of a Rust slice reference `&[T]` / `&mut [T]` with data pointer `p`,
`len` elements of `size` bytes and alignment `align`: the pointer is not null, is aligned, and the
slice spans fewer than `2^63` bytes (`isize::MAX`). Every value of a slice-reference type has
these properties, even when the slice is empty. -/
def SliceRefOk {PROP : Type} [BI PROP] (S : Vocab PROP) (p : S.Ptr) (len size align : Nat) :
    Prop :=
  0 < S.address p ∧ S.address p < 2 ^ 64 ∧ S.address p % align = 0 ∧ len * size < 2 ^ 63

/-- For an empty slice at `p` (element type `ty`), a share `q` of its zero-length cell: this
carries no bytes, only that `p` is a valid (dangling but well-formed) pointer for a reborrow, as
every Rust slice reference is. For a non-empty slice it is `emp` (the elements already give it). -/
def EmptySliceCell {PROP : Type} [BI PROP] {α : Type} (S : Vocab PROP) (p : S.Ptr) (ty : S.Ty)
    (q : Qp) (xs : List α) : PROP :=
  if xs = [] then S.pointsToF p q (S.arrayTy ty 0) (S.tupleVal []) else iprop(emp)

/-- The call `execute_batch_with_fee_loss_stale_scoped_not_atomic(long_account, short_account,
requests)` from a well-formed state (including the validity invariant of the slice references
`requests` and `self.markets`, which may be empty: `SliceRefOk` and, for an empty slice,
`EmptySliceCell`), assuming VT, CI(Γ), OPQ(Ψ), VS(Θ) and VW(Λ), with
postcondition `P` on the result and the state it leaves. -/
def ExecuteBatchCall {PROP : Type} [BI PROP] (S : Vocab PROP)
    (P : HeaderAccount → List (List Nat) → List Nat → List Nat → List TradeRequest →
      HeaderAccount → List (List Nat) → List Nat → List Nat → S.Value → Prop) : Prop :=
  ∀ (self hp mp la lap sa sap rp : S.Ptr) (q : Qp) (h : HeaderAccount) (ms : List (List Nat))
    (obs lbs sbs : List Nat) (reqs : List TradeRequest),
    MarketOk h ms obs → AccountBytes lbs → AccountBytes sbs → (∀ r ∈ reqs, r.WF) →
    SliceRefOk S rp reqs.length 48 16 → SliceRefOk S mp ms.length 1397 1 →
    Constants S ∗ ViewOwned S self hp mp h ms obs ∗ AccountOwned S la lap lbs ∗
      AccountOwned S sa sap sbs ∗ RequestsShared S rp q reqs ∗
      EmptySliceCell S rp (tradeRequestTy S) q reqs ∗ EmptySliceCell S mp (marketTy S) 1 ms ⊢
      callR S F.executeBatch (argsOf S F.executeBatch
          [S.ptrVal self, S.ptrVal la, S.ptrVal sa, S.slicePtrVal rp reqs.length])
        (fun r => iprop(RequestsShared S rp q reqs ∗
          EmptySliceCell S rp (tradeRequestTy S) q reqs ∗ EmptySliceCell S mp (marketTy S) 1 ms ∗
          After2 S self hp mp la lap sa sap ms
            (fun r h' ms' _ lbs' sbs' => P h ms lbs sbs reqs h' ms' lbs' sbs' r) r))

/-- **TB.flag.** For every `Γ`, `Ψ`, `Θ`, `Λ` for which the contracts VT, CI(Γ), OPQ(Ψ), VS(Θ) and
VW(Λ) hold: every return of `execute_batch_with_fee_loss_stale_scoped_not_atomic`, `Ok` or `Err`,
is either an `Err` before the opaque call `execute_batch_with_fee_after_tail_validation_not_atomic`
with nothing written, or comes after that call, where for the value `ignore` of
`ignore_unrelated_loss_stale` (characterised by `IgnoreChar`):

* the opaque call started from the starting state, except that `loss_stale_active` was `0` if
  `ignore` is true;
* the state left is the opaque call's final state, except that if `ignore` is true
  `loss_stale_active` is back to its starting value `f0` (so the suspension never leaks);
* if `ignore` is false, the entry point itself wrote nothing (in particular not the flag). -/
def tb_flag
    (Γ : HeaderAccount → List (List Nat) → List Nat → List Nat → Nat → Bool → Prop)
    (Ψ : HeaderAccount → List (List Nat) → List Nat → List Nat →
      HeaderAccount → List (List Nat) → List Nat → List Nat → Prop)
    (Θ : HeaderAccount → List (List Nat) → Prop)
    (Λ : List Nat → HeaderAccount → List (List Nat) → Prop) : Property := fun S =>
  vt_validate_tail false S → ci_can_ignore Γ false S → opq_after_tail Ψ false S →
  vs_validate_shape Θ false S → vw_validate_with_market Λ false S →
  ExecuteBatchCall S fun h ms lbs sbs reqs h' ms' lbs' sbs' r =>
    (r ≠ okUnit S ∧ h' = h ∧ ms' = ms ∧ lbs' = lbs ∧ sbs' = sbs) ∨
    ∃ (ignore : Bool) (hO : HeaderAccount) (msO : List (List Nat)) (lbsO sbsO : List Nat),
      IgnoreChar Γ h ms lbs sbs reqs ignore ∧
      Ψ (if ignore then { h with loss_stale_active := 0 } else h) ms lbs sbs hO msO lbsO sbsO ∧
      h' = (if ignore then { hO with loss_stale_active := h.loss_stale_active } else hO) ∧
      ms' = msO ∧ lbs' = lbsO ∧ sbs' = sbsO

/-- **TB.checked.** For every `Γ`, `Ψ`, `Θ`, `Λ` for which the contracts VT, CI(Γ), OPQ(Ψ), VS(Θ)
and VW(Λ) hold: if `execute_batch_with_fee_loss_stale_scoped_not_atomic` returns `Ok(_)`, then the
final state satisfies what an `Ok` return of `validate_shape()` guarantees (`Θ`) and what an `Ok`
return of `validate_with_market` guarantees for each of the two accounts (`Λ`): those calls are
made on the final state, with no write after them. -/
def tb_checked
    (Γ : HeaderAccount → List (List Nat) → List Nat → List Nat → Nat → Bool → Prop)
    (Ψ : HeaderAccount → List (List Nat) → List Nat → List Nat →
      HeaderAccount → List (List Nat) → List Nat → List Nat → Prop)
    (Θ : HeaderAccount → List (List Nat) → Prop)
    (Λ : List Nat → HeaderAccount → List (List Nat) → Prop) : Property := fun S =>
  vt_validate_tail false S → ci_can_ignore Γ false S → opq_after_tail Ψ false S →
  vs_validate_shape Θ false S → vw_validate_with_market Λ false S →
  ExecuteBatchCall S fun _ _ _ _ _ h' ms' lbs' sbs' r =>
    (∃ v, r = okV S v) → Θ h' ms' ∧ Λ lbs' h' ms' ∧ Λ sbs' h' ms'

end Spec.Trade
