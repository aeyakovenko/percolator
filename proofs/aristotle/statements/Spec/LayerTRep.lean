module

public import Spec.Rust.LayerT
public import Spec.LayerTModel
public import Spec.Kernel
public import SepLogic.Public

/-!
# How the market header, the market slots and the market view are stored (layer T)

The engine keeps a market in two kinds of zero-copy accounts: one `MarketGroupV16HeaderAccount`
(726 bytes, alignment 1) and a slice of market slots `Market<T>`. The entry points of this layer
are compiled at `T = ()` (one of the wrapper types for which `src/v16.rs` implements
`MarketWrapperPod`), so a slot is a `Market<()>`: a zero-sized wrapper followed by an
`EngineAssetSlotV16Account` of 1365 bytes, alignment 1, no padding.

Every field of these `#[repr(C)]` structs is a `u8`, a `[u8; N]` or a struct of those, so a
stored value is determined by its bytes:

* a `V16PodU16` / `V16PodU32` / `V16PodU64` / `V16PodU128` holding `n` is the struct
  `{ bytes: n.to_le_bytes() }` (`podValue`);
* a `u8` field holding `b` is the integer `b`;
* the header is described field by field by the model `Spec.LayerT.HeaderAccount`
  (`headerValue`), the market slots by their raw bytes (`engineSlotValue`), following the nesting
  of the Rust structs (`engineSlotShape`).

The Rust types themselves are named through the signatures of three constructors of the
verification harness (`header_type`, `market_type`, `view_mut_type`), which only build values of
these types.
-/

@[expose] public section

namespace Spec.LayerT
open Iris Iris.BI Rust2Lean.Public

/-! ## Rust types -/

/-- The Rust type `MarketGroupV16HeaderAccount`. -/
def headerTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::header_type»

/-- The Rust type `Market<()>` of a market slot. -/
def marketTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::market_type»

/-- The Rust type `EngineAssetSlotV16Account` (the engine part of a market slot). -/
def engineSlotTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.argTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::market_type» 1

/-- The Rust type `MarketGroupV16ViewMut<'_, ()>`: a pair of references, `header: &mut
MarketGroupV16HeaderAccount` and `markets: &mut [Market<()>]`. -/
def viewMutTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::view_mut_type»

/-- Size in bytes of one `Market<()>` (and of one `EngineAssetSlotV16Account`). -/
def marketStride : Nat := 1365

/-! ## Byte-level values -/

/-- The `w` little-endian bytes of `n` (`n.to_le_bytes()` for a `w`-byte integer). -/
def leBytes (w n : Nat) : List Nat :=
  (List.range w).map fun i => n / 256 ^ i % 256

/-- A `[u8; N]` value with these bytes. -/
def bytesValue {PROP : Type} [BI PROP] (S : Vocab PROP) (bs : List Nat) : S.Value :=
  S.tupleVal (bs.map fun (b : Nat) => S.intVal (b : Int))

/-- A `V16PodU{8w}` value (`struct { bytes: [u8; w] }`) holding `n`. -/
def podValue {PROP : Type} [BI PROP] (S : Vocab PROP) (w n : Nat) : S.Value :=
  S.tupleVal [bytesValue S (leBytes w n)]

/-- A `V16ConfigAccount` value. -/
def configValue {PROP : Type} [BI PROP] (S : Vocab PROP) (c : ConfigAccount) : S.Value :=
  S.tupleVal [podValue S 2 c.max_portfolio_assets, podValue S 4 c.max_market_slots,
    podValue S 16 c.min_nonzero_mm_req, podValue S 16 c.min_nonzero_im_req,
    podValue S 8 c.h_min, podValue S 8 c.h_max, podValue S 8 c.maintenance_margin_bps,
    podValue S 8 c.initial_margin_bps, podValue S 8 c.max_trading_fee_bps,
    podValue S 8 c.liquidation_fee_bps, podValue S 16 c.liquidation_fee_cap,
    podValue S 16 c.min_liquidation_abs, podValue S 8 c.max_accrual_dt_slots,
    podValue S 8 c.max_abs_funding_e9_per_slot, podValue S 8 c.min_funding_lifetime_slots,
    podValue S 8 c.max_price_move_bps_per_slot, podValue S 8 c.max_account_b_settlement_chunks,
    podValue S 8 c.max_bankrupt_close_chunks, podValue S 8 c.max_bankrupt_close_lifetime_slots,
    podValue S 8 c.asset_activation_cooldown_slots, podValue S 16 c.public_b_chunk_atoms,
    podValue S 8 c.max_recovery_fallback_deviation_bps,
    podValue S 8 c.backing_fee_base_rate_e9_per_slot, podValue S 8 c.backing_fee_kink_util_bps,
    podValue S 8 c.backing_fee_slope_at_kink_e9_per_slot,
    podValue S 8 c.backing_fee_slope_above_kink_e9_per_slot,
    S.intVal c.backing_freshness_buckets,
    S.intVal c.margin_mode_realizable_full_shared_cross_margin,
    S.intVal c.source_credit_lien_required, S.intVal c.insurance_credit_reservation_required,
    S.intVal c.permissionless_recovery_enabled, S.intVal c.recovery_fallback_price_enabled,
    S.intVal c.recovery_fallback_envelope_enabled, S.intVal c.credit_lien_revalidation_required,
    S.intVal c.stale_certificate_penalty_enabled,
    S.intVal c.full_refresh_required_for_favorable_actions,
    S.intVal c.public_liveness_profile_crank_forward]

/-- A `V16OptionalRecoveryReasonAccount` value. -/
def recoveryReasonValue {PROP : Type} [BI PROP] (S : Vocab PROP) (r : RecoveryReasonAccount) :
    S.Value :=
  S.tupleVal [S.intVal r.present, S.intVal r.value]

/-- A `ResolvedPayoutLedgerV16Account` value. -/
def ledgerValue {PROP : Type} [BI PROP] (S : Vocab PROP) (l : LedgerAccount) : S.Value :=
  S.tupleVal [podValue S 16 l.snapshot_residual, podValue S 16 l.terminal_claim_exact_receipts_num,
    podValue S 16 l.terminal_claim_bound_unreceipted_num, podValue S 16 l.current_payout_rate_num,
    podValue S 16 l.current_payout_rate_den, podValue S 8 l.snapshot_slot,
    S.intVal l.payout_halted, S.intVal l.finalized]

/-- A `MarketGroupV16HeaderAccount` value, its 39 fields in declaration order. -/
def headerValue {PROP : Type} [BI PROP] (S : Vocab PROP) (h : HeaderAccount) : S.Value :=
  S.tupleVal [bytesValue S h.market_group_id, configValue S h.config,
    podValue S 4 h.asset_slot_capacity, podValue S 16 h.vault, podValue S 16 h.insurance,
    podValue S 16 h.c_tot, podValue S 16 h.pnl_pos_tot, podValue S 16 h.pnl_pos_bound_tot_num,
    podValue S 16 h.pnl_pos_bound_tot, podValue S 16 h.pnl_matured_pos_tot,
    podValue S 16 h.backing_provider_earnings_total, podValue S 16 h.source_claim_bound_total_num,
    podValue S 16 h.source_fresh_backing_total_num,
    podValue S 16 h.source_insurance_credit_reserved_total_atoms,
    podValue S 16 h.insurance_domain_budget_remaining_total,
    podValue S 8 h.resolved_payout_blocker_count, podValue S 8 h.materialized_portfolio_count,
    podValue S 8 h.stale_certificate_count, podValue S 8 h.b_stale_account_count,
    podValue S 8 h.negative_pnl_account_count, podValue S 8 h.risk_epoch,
    podValue S 8 h.asset_set_epoch, podValue S 8 h.asset_activation_count,
    podValue S 8 h.last_asset_activation_slot, podValue S 8 h.next_market_id,
    podValue S 8 h.oracle_epoch, podValue S 8 h.funding_epoch, podValue S 8 h.slot_last,
    podValue S 8 h.current_slot, S.intVal h.bankruptcy_hlock_active,
    S.intVal h.threshold_stress_active, S.intVal h.loss_stale_active,
    recoveryReasonValue S h.recovery_reason, S.intVal h.mode, podValue S 8 h.resolved_slot,
    podValue S 16 h.payout_snapshot, podValue S 16 h.payout_snapshot_pnl_pos_tot,
    S.intVal h.payout_snapshot_captured, ledgerValue S h.resolved_payout_ledger]

/-! ## Market slots, by their bytes -/

/-- The nesting of a byte-only `#[repr(C)]` struct: a `u8`, a `[u8; n]`, a struct whose
fields have these shapes (in declaration order), or an array `[E; n]` of such elements. -/
inductive ByteShape where
  | byte
  | bytes (n : Nat)
  | struct (fields : List ByteShape)
  | array (n : Nat) (elem : ByteShape)

/-- A `V16PodU{8w}` or `V16PodI{8w}` field: `struct { bytes: [u8; w] }`. -/
def ByteShape.pod (w : Nat) : ByteShape := .struct [.bytes w]

mutual
/-- The number of bytes of a value of this shape. -/
def ByteShape.size : ByteShape → Nat
  | .byte => 1
  | .bytes n => n
  | .struct fs => ByteShape.sizeList fs
  | .array n e => n * e.size

/-- The total size of consecutive fields. -/
def ByteShape.sizeList : List ByteShape → Nat
  | [] => 0
  | f :: fs => f.size + ByteShape.sizeList fs
end

mutual
/-- The value of this shape whose bytes, in memory order, are `bs`. -/
def shapeValue {PROP : Type} [BI PROP] (S : Vocab PROP) : ByteShape → List Nat → S.Value
  | .byte, bs => S.intVal (bs.headD 0)
  | .bytes n, bs => bytesValue S (bs.take n)
  | .struct fs, bs => S.tupleVal (shapeValues S fs bs)
  | .array n e, bs => S.tupleVal (arrayValues S n e bs)

/-- The values of consecutive fields whose bytes, in memory order, are `bs`. -/
def shapeValues {PROP : Type} [BI PROP] (S : Vocab PROP) : List ByteShape → List Nat →
    List S.Value
  | [], _ => []
  | f :: fs, bs => shapeValue S f (bs.take f.size) :: shapeValues S fs (bs.drop f.size)

/-- The values of `n` consecutive elements of shape `e` whose bytes are `bs`. -/
def arrayValues {PROP : Type} [BI PROP] (S : Vocab PROP) : Nat → ByteShape → List Nat →
    List S.Value
  | 0, _, _ => []
  | n + 1, e, bs => shapeValue S e (bs.take e.size) :: arrayValues S n e (bs.drop e.size)
end

/-- The shape of an `AssetStateV16Account` (547 bytes). -/
def assetAccountShape : ByteShape :=
  .struct ([.pod 8, .pod 8, .byte, .pod 8, .pod 8, .pod 8, .pod 8] ++
    List.replicate 6 (.pod 16) ++ [.pod 8, .pod 8] ++ List.replicate 12 (.pod 16) ++
    List.replicate 6 (.pod 8) ++ List.replicate 8 (.pod 16) ++ [.pod 8, .pod 8, .byte, .byte])

/-- The shape of a `SourceCreditStateV16Account` (184 bytes). -/
def sourceCreditShape : ByteShape :=
  .struct (List.replicate 11 (.pod 16) ++ [.pod 8])

/-- The shape of a `BackingBucketV16Account` (97 bytes). -/
def backingBucketShape : ByteShape :=
  .struct ([.pod 8] ++ List.replicate 5 (.pod 16) ++ [.pod 8, .byte])

/-- The shape of an `InsuranceCreditReservationV16Account` (72 bytes). -/
def insuranceReservationShape : ByteShape :=
  .struct (List.replicate 4 (.pod 16) ++ [.pod 8])

/-- The shape of an `EngineAssetSlotV16Account` (1365 bytes), fields in declaration order:
`asset`, the four domain insurance budgets and spends, the two pending loss barriers, the two
source-credit states, the two backing buckets, the two insurance reservations and the two
provider principals. -/
def engineSlotShape : ByteShape :=
  .struct [assetAccountShape, .pod 16, .pod 16, .pod 16, .pod 16, .pod 8, .pod 8,
    sourceCreditShape, sourceCreditShape, backingBucketShape, backingBucketShape,
    insuranceReservationShape, insuranceReservationShape, .pod 16, .pod 16]

/-- An `EngineAssetSlotV16Account` value with these 1365 bytes. -/
def engineSlotValue {PROP : Type} [BI PROP] (S : Vocab PROP) (bs : List Nat) : S.Value :=
  shapeValue S engineSlotShape bs

/-- A `Market<()>` value whose engine slot has these 1365 bytes. -/
def marketValue {PROP : Type} [BI PROP] (S : Vocab PROP) (bs : List Nat) : S.Value :=
  S.tupleVal [S.unitVal, engineSlotValue S bs]

/-- The raw contents of a slot: 1365 bytes. -/
def SlotBytes (bs : List Nat) : Prop :=
  bs.length = marketStride ∧ ∀ b ∈ bs, b < 256


/-! ## Portfolio accounts, by their bytes -/

/-- The shape of a `PortfolioLegV16Account` (186 bytes). -/
def portfolioLegShape : ByteShape :=
  .struct [.byte, .pod 4, .pod 8, .byte, .pod 16, .pod 16, .pod 16, .pod 16, .pod 8, .pod 8,
    .pod 16, .pod 16, .pod 16, .pod 8, .byte, .byte]

/-- The shape of a `PortfolioSourceDomainV16Account` (196 bytes). -/
def portfolioSourceDomainShape : ByteShape :=
  .struct [.pod 4, .pod 8, .pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 8,
    .pod 16, .pod 16, .pod 16, .pod 16]

/-- The shape of a `HealthCertV16Account`. -/
def healthCertShape : ByteShape :=
  .struct [.pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 8, .pod 8, .pod 8, .pod 8,
    .array 1 (.pod 8), .byte]

/-- The shape of a `CloseProgressLedgerV16Account`. -/
def closeProgressShape : ByteShape :=
  .struct [.byte, .byte, .byte, .pod 8, .pod 4, .pod 8, .byte, .pod 16, .pod 8, .pod 8, .pod 16,
    .pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 16, .pod 16]

/-- The shape of a `ResolvedPayoutReceiptV16Account`. -/
def payoutReceiptShape : ByteShape :=
  .struct [.pod 16, .pod 16, .pod 16, .pod 16, .byte, .byte]

/-- The shape of a `ProvenanceHeaderV16Account`. -/
def provenanceHeaderShape : ByteShape :=
  .struct [.bytes 32, .bytes 32, .bytes 32, .pod 2, .pod 2]

/-- The shape of a `PortfolioAccountV16Account` (9419 bytes), fields in declaration order:
`provenance_header`, `owner`, `capital`, `pnl`, `reserved_pnl`, the seven residual and funding
totals, `fee_credits`, `cancel_deposit_escrow`, `last_fee_slot`, `active_bitmap` (one word), the
16 `legs`, the 32 `source_domains`, `health_cert`, the four lock/state bytes, `close_progress` and
`resolved_payout_receipt`. -/
def portfolioAccountShape : ByteShape :=
  .struct ([provenanceHeaderShape, .bytes 32] ++ List.replicate 12 (.pod 16) ++
    [.pod 8, .array 1 (.pod 8), .array 16 portfolioLegShape, .array 32 portfolioSourceDomainShape,
     healthCertShape, .byte, .byte, .byte, .byte, closeProgressShape, payoutReceiptShape])

/-- The Rust type `PortfolioAccountV16Account`. -/
def portfolioAccountTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::portfolio_account_type»

/-- The Rust type `PortfolioV16View<'_>`: one field, `header: &PortfolioAccountV16Account`. -/
def portfolioViewTy {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Ty :=
  S.retTy Mir.LayerT.Fn.«percolator_t_local_frame_harness::portfolio_view_type»

/-- A `PortfolioAccountV16Account` value with these 9419 bytes. -/
def portfolioAccountValue {PROP : Type} [BI PROP] (S : Vocab PROP) (bs : List Nat) : S.Value :=
  shapeValue S portfolioAccountShape bs

/-- The raw contents of a portfolio account: 9419 bytes. -/
def PortfolioBytes (bs : List Nat) : Prop :=
  bs.length = portfolioAccountShape.size ∧ ∀ b ∈ bs, b < 256

/-- A share `q` of a portfolio view at `account`: the `PortfolioV16View` cell at `account` holds
the reference `header = ap`, and the portfolio account at `ap` holds the bytes `bs`. -/
def PortfolioViewOwned {PROP : Type} [BI PROP] (S : Vocab PROP) (account ap : S.Ptr) (q : Qp)
    (bs : List Nat) : PROP :=
  iprop(S.pointsToF account q (portfolioViewTy S) (S.tupleVal [S.ptrVal ap]) ∗
    S.pointsToF ap q (portfolioAccountTy S) (portfolioAccountValue S bs))

/-! ## Ownership -/

/-- We own the market view at `self`: the `MarketGroupV16ViewMut<'_, ()>` cell at `self` holds
the references `header = hp` and `markets = (mp, ms.length)`, we own the header account at `hp`,
whose fields are `h`, and the slice of `ms.length` market slots at `mp`, whose engine slots hold
the bytes `ms`. -/
def ViewOwned {PROP : Type} [BI PROP] (S : Vocab PROP) (self hp mp : S.Ptr) (h : HeaderAccount)
    (ms : List (List Nat)) : PROP :=
  iprop(
    -- the view cell, holding the two references,
    S.pointsTo self (viewMutTy S) (S.tupleVal [S.ptrVal hp, S.slicePtrVal mp ms.length]) ∗
    -- the header account,
    S.pointsTo hp (headerTy S) (headerValue S h) ∗
    -- and the market slots, one after the other.
    S.array mp (marketTy S) marketStride 1 (ms.map (marketValue S)))

/-- `V16Result<()>`'s `Ok(())`. -/
def okUnit {PROP : Type} [BI PROP] (S : Vocab PROP) : S.Value :=
  Spec.Kernel.okValue S S.unitVal

end Spec.LayerT
