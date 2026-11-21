module

public import Spec.Rust.Trade
public import Spec.LayerTRep
public import Spec.Kernel2
public import SepLogic.Public

/-!
# Trade execution: how the market and the two portfolio accounts are stored

The trade entry points are methods of `impl<'a, T> MarketGroupV16ViewMut<'a, T>`, compiled here at
`T = ()` (through the verification harness `harness/trade_exec`, which only calls the public entry
point `execute_batch_with_fee_loss_stale_scoped_not_atomic` and names the types).

* The market header `MarketGroupV16HeaderAccount` (726 bytes) is described field by field by the
  model `Spec.LayerT.HeaderAccount` (unchanged at revision `d1fa39f6`).
* A market slot `Market<()>` is a zero-sized wrapper followed by an `EngineAssetSlotV16Account` of
  1397 bytes (revision `d1fa39f6` added `provider_protection_floor_long_num` and
  `provider_protection_floor_short_num`), described by its raw bytes.
* The view `MarketGroupV16ViewMut<'_, ()>` (152 bytes) holds `header: &mut ..`, `markets: &mut [..]`
  and the private scratch array `protection_observed: [u16; 64]`.
* A `PortfolioV16ViewMut<'_>` holds `header: &mut PortfolioAccountV16Account`; the account (9419
  bytes) is described by its raw bytes, with the readers below for the fields the properties name.
-/

@[expose] public section

namespace Spec.Trade
open Iris Iris.BI Rust2Lean.Public
open Spec.LayerT (HeaderAccount headerValue ByteShape shapeValue assetAccountShape
  sourceCreditShape backingBucketShape insuranceReservationShape portfolioAccountShape
  portfolioAccountValue)

/-! ## Rust types (named through the harness constructors) -/

/-- The Rust type `MarketGroupV16HeaderAccount`. -/
def headerTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::header_type»

/-- The Rust type `Market<()>` of a market slot. -/
def marketTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::market_type»

/-- The Rust type `MarketGroupV16ViewMut<'_, ()>`. -/
def viewMutTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::view_mut_type»

/-- The Rust type `MarketGroupV16View<'_, ()>`: `header: &MarketGroupV16HeaderAccount`,
`markets: &[Market<()>]`. -/
def viewTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::view_type»

/-- The Rust type `PortfolioV16View<'_>`: one field, `header: &PortfolioAccountV16Account`. -/
def accountViewTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::portfolio_view_type»

/-- The Rust type `PortfolioAccountV16Account`. -/
def accountTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::portfolio_account_type»

/-- The Rust type `PortfolioV16ViewMut<'_>`: one field, `header: &mut PortfolioAccountV16Account`. -/
def accountViewMutTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.Trade.Fn.«percolator_trade_exec_harness::portfolio_view_mut_type»

/-! ## Market slots -/

/-- Size in bytes of one `Market<()>` (and of one `EngineAssetSlotV16Account`) at `d1fa39f6`. -/
def marketStride : Nat := 1397

/-- The shape of an `EngineAssetSlotV16Account` (1397 bytes), fields in declaration order: `asset`,
the four domain insurance budgets and spends, the two pending loss barriers, the two source-credit
states, the two backing buckets, the two insurance reservations, the two provider principals and
the two provider-protection floors. -/
def engineSlotShape : ByteShape :=
  .struct [assetAccountShape, .pod 16, .pod 16, .pod 16, .pod 16, .pod 8, .pod 8,
    sourceCreditShape, sourceCreditShape, backingBucketShape, backingBucketShape,
    insuranceReservationShape, insuranceReservationShape, .pod 16, .pod 16, .pod 16, .pod 16]

/-- A `Market<()>` value whose engine slot has these 1397 bytes. -/
def marketValue {PROP : Type} [BI PROP] (S : Vocab PROP) (bs : List Nat) : S.Value :=
  S.tupleVal [S.unitVal, shapeValue S engineSlotShape bs]

/-- The raw contents of a slot: 1397 bytes. -/
def SlotBytes (bs : List Nat) : Prop :=
  bs.length = marketStride ∧ ∀ b ∈ bs, b < 256

/-! ## Little-endian fields of a byte image -/

/-- The unsigned integer whose little-endian bytes are `bs`. -/
def leNat : List Nat → Nat
  | [] => 0
  | b :: bs => b + 256 * leNat bs

/-- The unsigned field of width `w` bytes at byte offset `off`. -/
def fieldNat (bs : List Nat) (off w : Nat) : Nat := leNat ((bs.drop off).take w)

/-- The two's-complement signed field of width `w` bytes at byte offset `off`. -/
def fieldInt (bs : List Nat) (off w : Nat) : Int :=
  let n := fieldNat bs off w
  if n < 2 ^ (8 * w - 1) then (n : Int) else (n : Int) - 2 ^ (8 * w)

/-- Byte images `bs` and `bs'` have the same length and agree outside the byte ranges
`[off, off + len)` listed in `regions`. -/
def SameExcept (regions : List (Nat × Nat)) (bs bs' : List Nat) : Prop :=
  bs'.length = bs.length ∧
    ∀ k, k < bs.length → (∀ r ∈ regions, k < r.1 ∨ r.1 + r.2 ≤ k) → bs'[k]? = bs[k]?

/-! ## Fields of a market slot (offsets in the engine slot) -/

/-- `asset.effective_price` (`V16PodU64` at offset 25). -/
def slotEffectivePrice (bs : List Nat) : Nat := fieldNat bs 25 8
/-- `asset.a_long` (`V16PodU128` at offset 49). -/
def slotALong (bs : List Nat) : Nat := fieldNat bs 49 16
/-- `asset.a_short` (`V16PodU128` at offset 65). -/
def slotAShort (bs : List Nat) : Nat := fieldNat bs 65 16
/-- `asset.oi_eff_long_q` (`V16PodU128` at offset 289). -/
def slotOiEffLong (bs : List Nat) : Nat := fieldNat bs 289 16
/-- `asset.oi_eff_short_q` (`V16PodU128` at offset 305). -/
def slotOiEffShort (bs : List Nat) : Nat := fieldNat bs 305 16

/-- The slot lists `ms` and `ms'` have the same length and agree on every slot except possibly the
slot of index `i`. -/
def SlotsSameExcept (i : Nat) (ms ms' : List (List Nat)) : Prop :=
  ms'.length = ms.length ∧ ∀ k, k ≠ i → ms'[k]? = ms[k]?

/-! ## Fields of a portfolio account (byte offsets in `PortfolioAccountV16Account`) -/

/-- `capital` (`V16PodU128` at offset 132). -/
def accCapital (bs : List Nat) : Nat := fieldNat bs 132 16
/-- `pnl` (`V16PodI128` at offset 148). -/
def accPnl (bs : List Nat) : Int := fieldInt bs 148 16
/-- `fee_credits` (`V16PodI128` at offset 292). -/
def accFeeCredits (bs : List Nat) : Int := fieldInt bs 292 16
/-- `health_cert.certified_equity` (`V16PodI128` at offset 9044). -/
def accCertEquity (bs : List Nat) : Int := fieldInt bs 9044 16
/-- `health_cert.certified_initial_req` (`V16PodU128` at offset 9060). -/
def accCertInitialReq (bs : List Nat) : Nat := fieldNat bs 9060 16
/-- `health_cert.valid` (`u8` at offset 9164; `1` encodes `true`). -/
def accCertValid (bs : List Nat) : Nat := fieldNat bs 9164 1

/-- Byte ranges `(offset, length)` of the account fields named in the frame statements. -/
def rCapital : Nat × Nat := (132, 16)
def rResidualSpentPrincipal : Nat × Nat := (196, 16)
def rResidualReceived : Nat × Nat := (212, 16)
def rActiveBitmap : Nat × Nat := (332, 8)
def rLegs : Nat × Nat := (340, 16 * 152)
def rSourceDomains : Nat × Nat := (2772, 32 * 196)
def rHealthCert : Nat × Nat := (9044, 121)
def rCertValid : Nat × Nat := (9164, 1)
def rLiquidationLock : Nat × Nat := (9168, 1)
def rCloseProgress : Nat × Nat := (9169, 184)

/-- Leg `k` (of 16, 152 bytes each, from offset 340): `active` (`u8` at 0), `asset_index`
(`V16PodU32` at 1), `side` (`u8` at 13; `0` is `Long`), `basis_pos_q` (`V16PodI128` at 14). -/
def legOff (k : Nat) : Nat := 340 + 152 * k
def legActive (bs : List Nat) (k : Nat) : Bool := fieldNat bs (legOff k) 1 == 1
def legAsset (bs : List Nat) (k : Nat) : Nat := fieldNat bs (legOff k + 1) 4
def legSide (bs : List Nat) (k : Nat) : Nat := fieldNat bs (legOff k + 13) 1
def legBasis (bs : List Nat) (k : Nat) : Int := fieldInt bs (legOff k + 14) 16

/-- Leg `k` is active and on asset `i`. -/
def legOn (bs : List Nat) (k i : Nat) : Prop := legActive bs k = true ∧ legAsset bs k = i

/-- `signed_position` of leg `k` (`src/v16.rs`): `0` for an inactive leg, `|basis_pos_q|` for an
active long leg and `-|basis_pos_q|` for an active short one. -/
def legSigned (bs : List Nat) (k : Nat) : Int :=
  if legActive bs k then (if legSide bs k = 0 then (legBasis bs k).natAbs else -((legBasis bs k).natAbs : Int))
  else 0

/-- The account's signed raw position on asset `i`: the sum of `signed_position` over its active
legs on asset `i` (an account with no active leg on `i`, or one whose leg has raw basis 0, has
position 0; the engine keeps at most one active leg per asset). -/
def rawPosition (bs : List Nat) (i : Nat) : Int :=
  ((List.range 16).map fun k => if legActive bs k ∧ legAsset bs k = i then legSigned bs k else 0).sum

/-- The legs of `bs'` on assets other than `i` are those of `bs`: every leg slot is unchanged
unless it holds an active leg on asset `i` before or after. -/
def OtherLegsSame (i : Nat) (bs bs' : List Nat) : Prop :=
  ∀ k, k < 16 → ((bs'.drop (legOff k)).take 152 = (bs.drop (legOff k)).take 152 ∨
    legOn bs k i ∨ legOn bs' k i)

/-- The raw contents of a portfolio account: 9419 bytes. -/
def AccountBytes (bs : List Nat) : Prop :=
  bs.length = 9419 ∧ ∀ b ∈ bs, b < 256

/-! ## Ownership -/

/-- A `[u16; 64]` value. -/
def u16ArrayValue {PROP : Type} [BI PROP] (S : Vocab PROP) (obs : List Nat) : S.Value :=
  S.tupleVal (obs.map fun (x : Nat) => S.intVal (x : Int))

/-- The private scratch memory `protection_observed` holds 64 `u16` values. -/
def ObsOk (obs : List Nat) : Prop := obs.length = 64 ∧ ∀ x ∈ obs, x < 2 ^ 16

/-- We own the market view at `self`: the `MarketGroupV16ViewMut<'_, ()>` cell at `self` holds
`header = hp`, `markets = (mp, ms.length)` and `protection_observed = obs`; we own the header
account at `hp`, whose fields are `h`, and the `ms.length` market slots at `mp`, whose engine slots
hold the bytes `ms`. -/
def ViewOwned {PROP : Type} [BI PROP] (S : Vocab PROP) (self hp mp : S.Ptr) (h : HeaderAccount)
    (ms : List (List Nat)) (obs : List Nat) : PROP :=
  iprop(
    S.pointsTo self (viewMutTy S)
      (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length, u16ArrayValue S obs]) ∗
    S.pointsTo hp (headerTy S) (headerValue S h) ∗
    S.array mp (marketTy S) marketStride 1 (ms.map (marketValue S)))

/-- We own the portfolio view at `acc`: the `PortfolioV16ViewMut<'_>` cell at `acc` holds
`header = ap`, and we own the portfolio account at `ap`, which holds the bytes `bs`. -/
def AccountOwned {PROP : Type} [BI PROP] (S : Vocab PROP) (acc ap : S.Ptr) (bs : List Nat) : PROP :=
  iprop(S.pointsTo acc (accountViewMutTy S) (S.tupleVal [S.ptrVal ap]) ∗
    S.pointsTo ap (accountTy S) (portfolioAccountValue S bs))

/-- A share `q` of a shared portfolio view at `v`: the `PortfolioV16View<'_>` cell at `v` holds
`header = ap`, and the portfolio account at `ap` holds the bytes `bs`. -/
def AccountViewShared {PROP : Type} [BI PROP] (S : Vocab PROP) (v ap : S.Ptr) (q : Qp)
    (bs : List Nat) : PROP :=
  iprop(S.pointsToF v q (accountViewTy S) (S.tupleVal [S.ptrVal ap]) ∗
    S.pointsToF ap q (accountTy S) (portfolioAccountValue S bs))

/-- A share `q` of a shared market view at `v`: the `MarketGroupV16View<'_, ()>` cell at `v`
holds `header = hp`, `markets = (mp, ms.length)`, the header at `hp` has fields `h` and the slots
at `mp` hold `ms`. -/
def MarketViewShared {PROP : Type} [BI PROP] (S : Vocab PROP) (v hp mp : S.Ptr) (q : Qp)
    (h : HeaderAccount) (ms : List (List Nat)) : PROP :=
  iprop(S.pointsToF v q (viewTy S) (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length]) ∗
    S.pointsToF hp q (headerTy S) (headerValue S h) ∗
    S.array mp (marketTy S) marketStride q (ms.map (marketValue S)))

/-- A well-formed starting market: header fields in range, every slot 1397 bytes, scratch memory
of 64 `u16`, and the slice of market slots, like every Rust object, spans fewer than `2^56` bytes
(the standard environment assumption: it fits the x86-64 user address space). -/
def MarketOk (h : HeaderAccount) (ms : List (List Nat)) (obs : List Nat) : Prop :=
  h.InRange ∧ (∀ bs ∈ ms, SlotBytes bs) ∧ ObsOk obs ∧ ms.length * marketStride < 2 ^ 56

/-! ## Values -/

/-- A call outcome predicate: a normal return satisfies `Q`; unwinding (a panic) is not
constrained. -/
def onReturn {PROP : Type} [BI PROP] (S : Vocab PROP) (Q : S.Value → PROP) :
    CallOutcome S.Value → PROP
  | .returned v => Q v
  | .unwound => iprop(True)

/-- A call of `f` with arguments `args`, under any incoming panic-payload stack: if it returns a
value `r`, then `Q r` holds. A panic is allowed and not constrained; nothing says the call
terminates. -/
def callR {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (args : List (S.Value × S.Ty))
    (Q : S.Value → PROP) : PROP :=
  iprop(∀ before : List S.Ptr, S.callUnwind f args before (fun o _ => onReturn S Q o))

/-- The argument list of `f` with argument types from `f`'s signature. -/
def argsOf {PROP : Type} [BI PROP] (S : Vocab PROP) (f : RustFn) (vs : List S.Value) :
    List (S.Value × S.Ty) :=
  (List.range vs.length).zip vs |>.map fun p => (p.2, S.argTy f p.1)

/-- `Ok(x)` of a `V16Result<_>`. -/
def okV {PROP : Type} [BI PROP] (S : Vocab PROP) (x : S.Value) : S.Value := Spec.Kernel.okValue S x

/-- `Ok(())`. -/
def okUnit {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Value := okV S S.unitVal

/-- `Some(x)` / `None` of an `Option<usize>`. -/
def optValue {PROP : Type} [BI PROP] (S : Vocab PROP) : Option Nat → S.Value
  | some x => Spec.Kernel.someValue S (S.intVal x)
  | none => Spec.Kernel.noneValue S

/-- A `SideV16` value (`Long` = 0, `Short` = 1). -/
def sideValue {PROP : Type} [BI PROP] (S : Vocab PROP) (short : Bool) : S.Value :=
  S.variantVal (if short then 1 else 0) S.unitVal

/-- Rust `TradeRequestV16 { asset_index: usize, size_q: i128, exec_price: u64, fee_bps: u64 }`. -/
structure TradeRequest where
  asset_index : Nat
  size_q : Int
  exec_price : Nat
  fee_bps : Nat

def TradeRequest.WF (r : TradeRequest) : Prop :=
  r.asset_index < 2 ^ 64 ∧ -(2 ^ 127 : Int) ≤ r.size_q ∧ r.size_q < 2 ^ 127 ∧
    r.exec_price < 2 ^ 64 ∧ r.fee_bps < 2 ^ 64

def tradeRequestValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : TradeRequest) : S.Value :=
  S.tupleVal [S.intVal r.asset_index, S.intVal r.size_q, S.intVal r.exec_price, S.intVal r.fee_bps]

/-- Rust `PositionDeltaLookupV16` (private): `existing_slot`, `empty_slot: Option<usize>`, the
raw bases `current_q`, `next_q` and the effective quantities `current_effective_q`,
`next_effective_q` (all `i128`). -/
structure Lookup where
  existing_slot : Option Nat
  empty_slot : Option Nat
  current_q : Int
  next_q : Int
  current_effective_q : Int
  next_effective_q : Int

def I128v (x : Int) : Prop := -(2 ^ 127 : Int) ≤ x ∧ x < 2 ^ 127

def Lookup.WF (l : Lookup) : Prop :=
  (∀ x, l.existing_slot = some x → x < 2 ^ 64) ∧ (∀ x, l.empty_slot = some x → x < 2 ^ 64) ∧
  I128v l.current_q ∧ I128v l.next_q ∧ I128v l.current_effective_q ∧ I128v l.next_effective_q

def lookupValue {PROP : Type} [BI PROP] (S : Vocab PROP) (l : Lookup) : S.Value :=
  S.tupleVal [optValue S l.existing_slot, optValue S l.empty_slot, S.intVal l.current_q,
    S.intVal l.next_q, S.intVal l.current_effective_q, S.intVal l.next_effective_q]

/-- Rust `TradePositionPreflightV16` (private), fields in declaration order. -/
structure Preflight where
  risk_increasing : Bool
  long_requires_initial_margin : Bool
  short_requires_initial_margin : Bool
  long_lookup : Lookup
  short_lookup : Lookup
  long_old_abs_q : Nat
  short_old_abs_q : Nat
  long_new_abs_q : Nat
  short_new_abs_q : Nat
  long_has_source_claims : Bool
  short_has_source_claims : Bool

def preflightValue {PROP : Type} [BI PROP] (S : Vocab PROP) (p : Preflight) : S.Value :=
  S.tupleVal [S.boolVal p.risk_increasing, S.boolVal p.long_requires_initial_margin,
    S.boolVal p.short_requires_initial_margin, lookupValue S p.long_lookup,
    lookupValue S p.short_lookup, S.intVal p.long_old_abs_q, S.intVal p.short_old_abs_q,
    S.intVal p.long_new_abs_q, S.intVal p.short_new_abs_q, S.boolVal p.long_has_source_claims,
    S.boolVal p.short_has_source_claims]

/-- Rust `TradeApplyOutcomeV16` (private), fields in declaration order. -/
structure ApplyOutcome where
  fee_a : Nat
  fee_b : Nat
  notional : Nat
  risk_increasing : Bool
  long_requires_initial_margin : Bool
  short_requires_initial_margin : Bool
  long_has_source_claims : Bool
  short_has_source_claims : Bool

def applyOutcomeValue {PROP : Type} [BI PROP] (S : Vocab PROP) (o : ApplyOutcome) : S.Value :=
  S.tupleVal [S.intVal o.fee_a, S.intVal o.fee_b, S.intVal o.notional, S.boolVal o.risk_increasing,
    S.boolVal o.long_requires_initial_margin, S.boolVal o.short_requires_initial_margin,
    S.boolVal o.long_has_source_claims, S.boolVal o.short_has_source_claims]

/-- An `AssetStateV16` value (45 fields, declaration order) whose `effective_price` (field 4),
`a_long` (7), `a_short` (8), `oi_eff_long_q` (23) and `oi_eff_short_q` (24) are the given
numbers; the other fields are arbitrary. -/
def AssetStateHas {PROP : Type} [BI PROP] (S : Vocab PROP) (v : S.Value)
    (effective_price a_long a_short oi_long oi_short : Nat) : Prop :=
  ∃ vs : List S.Value, v = S.tupleVal vs ∧ vs.length = 45 ∧
    vs[4]? = some (S.intVal effective_price) ∧ vs[7]? = some (S.intVal a_long) ∧
    vs[8]? = some (S.intVal a_short) ∧ vs[23]? = some (S.intVal oi_long) ∧
    vs[24]? = some (S.intVal oi_short)

/-! ## Constants and arithmetic models -/

/-- `POS_SCALE = 1_000_000`. -/
def POS_SCALE : Nat := 1000000
/-- `MAX_MARGIN_BPS = 10_000`. -/
def MAX_MARGIN_BPS : Nat := 10000
/-- `MAX_OI_SIDE_Q = 100_000_000_000_000`. -/
def MAX_OI_SIDE_Q : Nat := 100000000000000
/-- `ADL_ONE = 1_000_000_000_000_000`. -/
def ADL_ONE : Nat := 1000000000000000

/-- Ceiling division. -/
def ceilDiv (a b : Nat) : Nat := (a + b - 1) / b

/-- `trade_notional_floor(size_q, exec_price)` on success: `floor(size_q * exec_price / POS_SCALE)`. -/
def tradeNotionalFloor (size_q exec_price : Nat) : Nat := size_q * exec_price / POS_SCALE

/-- `trade_fee_notional_ceil(size_q, exec_price)` on success: `0` if either is `0`, else
`ceil(size_q * exec_price / POS_SCALE)`. -/
def tradeFeeNotionalCeil (size_q exec_price : Nat) : Nat :=
  if size_q = 0 ∨ exec_price = 0 then 0 else ceilDiv (size_q * exec_price) POS_SCALE

/-- `checked_fee_bps(notional, fee_bps)` on success: `0` if either is `0`, else
`ceil(notional * fee_bps / MAX_MARGIN_BPS)`. -/
def checkedFeeBps (notional fee_bps : Nat) : Nat :=
  if notional = 0 ∨ fee_bps = 0 then 0 else ceilDiv (notional * fee_bps) MAX_MARGIN_BPS

/-- What `charge_account_fee_current_not_atomic(account, requested_fee)` charges an account with
`pnl` and `capital`: `0` if the fee is `0` or the PnL is negative, else `min(requested_fee,
capital)`. -/
def chargedFee (requested_fee : Nat) (pnl : Int) (capital : Nat) : Nat :=
  if requested_fee = 0 ∨ pnl < 0 then 0 else min requested_fee capital

end Spec.Trade
