module

/-!
# Independent mathematical model of the market shape check (layer T)

This file is a pure mathematical transcription of the shape check

    MarketGroupV16View::validate_shape(&self) -> V16Result<()>

of `src/v16.rs` (and its callees `validate_dynamic_market_slots_len_static`,
`V16ConfigAccount::try_to_runtime_shape`, `V16ConfigAccount::decode_runtime`,
`V16Config::validate_public_user_fund_shape`, `decode_bool`,
`V16OptionalRecoveryReasonAccount::try_to_runtime`, `decode_recovery_reason`,
`ResolvedPayoutLedgerV16Account::try_to_runtime`, `V16Core::amount_from_bound_num`,
`V16Core::bound_num_from_amount`, `validate_header_aggregate_totals`) in a default
build, i.e. without the `audit-scan` feature, `test` or `kani`.

Conventions:

* Every `V16PodU16` / `V16PodU32` / `V16PodU64` / `V16PodU128` field is modelled by
  the `Nat` it decodes to (`get()`, i.e. `from_le_bytes` of its bytes).
* Every `u8` field is modelled by its raw byte as a `Nat`.
* `market_group_id : [u8; 32]` is modelled by the list of its 32 raw bytes.
* The `InRange` predicates state that every field is representable in its Rust
  width.
* Every Rust error path (`Err(V16Error::InvalidConfig)`, `Err(ArithmeticOverflow)`,
  ...) is a failure of the corresponding predicate; the model does not
  distinguish error values.
* `as usize` / `as u32` casts appearing in the checks are widening (lossless) on
  x86-64, so they disappear in `Nat`.
-/

@[expose] public section

namespace Spec.LayerT

/-! ## Constants -/

/-- `pub const MAX_VAULT_TVL: u128 = 10_000_000_000_000_000;` (`src/lib.rs`), i.e. `10^16`. -/
def MAX_VAULT_TVL : Nat := 10000000000000000

/-- `pub const BOUND_SCALE: u128 = 1_000_000_000_000;` (`src/lib.rs`), i.e. `10^12`. -/
def BOUND_SCALE : Nat := 1000000000000

/-- `pub const V16_MAX_PORTFOLIO_ASSETS_N: usize = 16;` (`src/v16.rs`). -/
def V16_MAX_PORTFOLIO_ASSETS_N : Nat := 16

/-- `pub const MAX_MARGIN_BPS: u64 = 10_000;` (`src/lib.rs`). -/
def MAX_MARGIN_BPS : Nat := 10000

/-- `pub const MAX_PROTOCOL_FEE_ABS: u128 =
1_000_000_000_000_000_000_000_000_000_000_000_000;` (`src/lib.rs`), i.e. `10^36`. -/
def MAX_PROTOCOL_FEE_ABS : Nat := 1000000000000000000000000000000000000

/-- `pub const MAX_RESOLVE_PRICE_DEVIATION_BPS: u64 = 10_000;` (`src/lib.rs`). -/
def MAX_RESOLVE_PRICE_DEVIATION_BPS : Nat := 10000

/-- `pub const MAX_RECOVERY_FALLBACK_DEVIATION_BPS: u64 = MAX_RESOLVE_PRICE_DEVIATION_BPS;`
(`src/lib.rs`), i.e. `10_000`. -/
def MAX_RECOVERY_FALLBACK_DEVIATION_BPS : Nat := MAX_RESOLVE_PRICE_DEVIATION_BPS

/-- `pub const MAX_BACKING_FEE_UTIL_BPS: u64 = 10_000;` (`src/v16.rs`). -/
def MAX_BACKING_FEE_UTIL_BPS : Nat := 10000

/-- `pub const V16_BACKING_BUCKETS_PER_DOMAIN: usize = 1;` (`src/v16.rs`). -/
def V16_BACKING_BUCKETS_PER_DOMAIN : Nat := 1

/-- `pub const MAX_BACKING_FEE_RATE_E9_PER_SLOT: u64 = 1_000_000_000;` (`src/v16.rs`),
i.e. `10^9`. -/
def MAX_BACKING_FEE_RATE_E9_PER_SLOT : Nat := 1000000000

/-- The literal bound `10_000` on `max_abs_funding_e9_per_slot` written inline in
`V16Config::validate_public_user_fund_shape`. -/
def MAX_ABS_FUNDING_E9_PER_SLOT_LITERAL : Nat := 10000

/-! ## Accounts -/

/-- Mirror of `V16ConfigAccount` (field by field, declaration order). Pod integer
fields hold their decoded value, `u8` fields their raw byte. -/
structure ConfigAccount where
  /-- `max_portfolio_assets: V16PodU16` -/
  max_portfolio_assets : Nat
  /-- `max_market_slots: V16PodU32` -/
  max_market_slots : Nat
  /-- `min_nonzero_mm_req: V16PodU128` -/
  min_nonzero_mm_req : Nat
  /-- `min_nonzero_im_req: V16PodU128` -/
  min_nonzero_im_req : Nat
  /-- `h_min: V16PodU64` -/
  h_min : Nat
  /-- `h_max: V16PodU64` -/
  h_max : Nat
  /-- `maintenance_margin_bps: V16PodU64` -/
  maintenance_margin_bps : Nat
  /-- `initial_margin_bps: V16PodU64` -/
  initial_margin_bps : Nat
  /-- `max_trading_fee_bps: V16PodU64` -/
  max_trading_fee_bps : Nat
  /-- `liquidation_fee_bps: V16PodU64` -/
  liquidation_fee_bps : Nat
  /-- `liquidation_fee_cap: V16PodU128` -/
  liquidation_fee_cap : Nat
  /-- `min_liquidation_abs: V16PodU128` -/
  min_liquidation_abs : Nat
  /-- `max_accrual_dt_slots: V16PodU64` -/
  max_accrual_dt_slots : Nat
  /-- `max_abs_funding_e9_per_slot: V16PodU64` -/
  max_abs_funding_e9_per_slot : Nat
  /-- `min_funding_lifetime_slots: V16PodU64` -/
  min_funding_lifetime_slots : Nat
  /-- `max_price_move_bps_per_slot: V16PodU64` -/
  max_price_move_bps_per_slot : Nat
  /-- `max_account_b_settlement_chunks: V16PodU64` -/
  max_account_b_settlement_chunks : Nat
  /-- `max_bankrupt_close_chunks: V16PodU64` -/
  max_bankrupt_close_chunks : Nat
  /-- `max_bankrupt_close_lifetime_slots: V16PodU64` -/
  max_bankrupt_close_lifetime_slots : Nat
  /-- `asset_activation_cooldown_slots: V16PodU64` -/
  asset_activation_cooldown_slots : Nat
  /-- `public_b_chunk_atoms: V16PodU128` -/
  public_b_chunk_atoms : Nat
  /-- `max_recovery_fallback_deviation_bps: V16PodU64` -/
  max_recovery_fallback_deviation_bps : Nat
  /-- `backing_fee_base_rate_e9_per_slot: V16PodU64` -/
  backing_fee_base_rate_e9_per_slot : Nat
  /-- `backing_fee_kink_util_bps: V16PodU64` -/
  backing_fee_kink_util_bps : Nat
  /-- `backing_fee_slope_at_kink_e9_per_slot: V16PodU64` -/
  backing_fee_slope_at_kink_e9_per_slot : Nat
  /-- `backing_fee_slope_above_kink_e9_per_slot: V16PodU64` -/
  backing_fee_slope_above_kink_e9_per_slot : Nat
  /-- `backing_freshness_buckets: u8` (a count, not a flag) -/
  backing_freshness_buckets : Nat
  /-- `margin_mode_realizable_full_shared_cross_margin: u8` (encoded bool) -/
  margin_mode_realizable_full_shared_cross_margin : Nat
  /-- `source_credit_lien_required: u8` (encoded bool) -/
  source_credit_lien_required : Nat
  /-- `insurance_credit_reservation_required: u8` (encoded bool) -/
  insurance_credit_reservation_required : Nat
  /-- `permissionless_recovery_enabled: u8` (encoded bool) -/
  permissionless_recovery_enabled : Nat
  /-- `recovery_fallback_price_enabled: u8` (encoded bool) -/
  recovery_fallback_price_enabled : Nat
  /-- `recovery_fallback_envelope_enabled: u8` (encoded bool) -/
  recovery_fallback_envelope_enabled : Nat
  /-- `credit_lien_revalidation_required: u8` (encoded bool) -/
  credit_lien_revalidation_required : Nat
  /-- `stale_certificate_penalty_enabled: u8` (encoded bool) -/
  stale_certificate_penalty_enabled : Nat
  /-- `full_refresh_required_for_favorable_actions: u8` (encoded bool) -/
  full_refresh_required_for_favorable_actions : Nat
  /-- `public_liveness_profile_crank_forward: u8` (encoded bool) -/
  public_liveness_profile_crank_forward : Nat

/-- Mirror of `ResolvedPayoutLedgerV16Account` (declaration order). -/
structure LedgerAccount where
  /-- `snapshot_residual: V16PodU128` -/
  snapshot_residual : Nat
  /-- `terminal_claim_exact_receipts_num: V16PodU128` -/
  terminal_claim_exact_receipts_num : Nat
  /-- `terminal_claim_bound_unreceipted_num: V16PodU128` -/
  terminal_claim_bound_unreceipted_num : Nat
  /-- `current_payout_rate_num: V16PodU128` -/
  current_payout_rate_num : Nat
  /-- `current_payout_rate_den: V16PodU128` -/
  current_payout_rate_den : Nat
  /-- `snapshot_slot: V16PodU64` -/
  snapshot_slot : Nat
  /-- `payout_halted: u8` (encoded bool) -/
  payout_halted : Nat
  /-- `finalized: u8` (encoded bool) -/
  finalized : Nat

/-- Mirror of `V16OptionalRecoveryReasonAccount` (raw bytes). -/
structure RecoveryReasonAccount where
  /-- `present: u8` -/
  present : Nat
  /-- `value: u8` -/
  value : Nat

/-- Mirror of `MarketGroupV16HeaderAccount` (field by field, declaration order;
39 fields). -/
structure HeaderAccount where
  /-- `market_group_id: [u8; 32]` (raw bytes) -/
  market_group_id : List Nat
  /-- `config: V16ConfigAccount` -/
  config : ConfigAccount
  /-- `asset_slot_capacity: V16PodU32` -/
  asset_slot_capacity : Nat
  /-- `vault: V16PodU128` -/
  vault : Nat
  /-- `insurance: V16PodU128` -/
  insurance : Nat
  /-- `c_tot: V16PodU128` -/
  c_tot : Nat
  /-- `pnl_pos_tot: V16PodU128` -/
  pnl_pos_tot : Nat
  /-- `pnl_pos_bound_tot_num: V16PodU128` -/
  pnl_pos_bound_tot_num : Nat
  /-- `pnl_pos_bound_tot: V16PodU128` -/
  pnl_pos_bound_tot : Nat
  /-- `pnl_matured_pos_tot: V16PodU128` -/
  pnl_matured_pos_tot : Nat
  /-- `backing_provider_earnings_total: V16PodU128` -/
  backing_provider_earnings_total : Nat
  /-- `source_claim_bound_total_num: V16PodU128` -/
  source_claim_bound_total_num : Nat
  /-- `source_fresh_backing_total_num: V16PodU128` -/
  source_fresh_backing_total_num : Nat
  /-- `source_insurance_credit_reserved_total_atoms: V16PodU128` -/
  source_insurance_credit_reserved_total_atoms : Nat
  /-- `insurance_domain_budget_remaining_total: V16PodU128` -/
  insurance_domain_budget_remaining_total : Nat
  /-- `resolved_payout_blocker_count: V16PodU64` -/
  resolved_payout_blocker_count : Nat
  /-- `materialized_portfolio_count: V16PodU64` -/
  materialized_portfolio_count : Nat
  /-- `stale_certificate_count: V16PodU64` -/
  stale_certificate_count : Nat
  /-- `b_stale_account_count: V16PodU64` -/
  b_stale_account_count : Nat
  /-- `negative_pnl_account_count: V16PodU64` -/
  negative_pnl_account_count : Nat
  /-- `risk_epoch: V16PodU64` -/
  risk_epoch : Nat
  /-- `asset_set_epoch: V16PodU64` -/
  asset_set_epoch : Nat
  /-- `asset_activation_count: V16PodU64` -/
  asset_activation_count : Nat
  /-- `last_asset_activation_slot: V16PodU64` -/
  last_asset_activation_slot : Nat
  /-- `next_market_id: V16PodU64` -/
  next_market_id : Nat
  /-- `oracle_epoch: V16PodU64` -/
  oracle_epoch : Nat
  /-- `funding_epoch: V16PodU64` -/
  funding_epoch : Nat
  /-- `slot_last: V16PodU64` -/
  slot_last : Nat
  /-- `current_slot: V16PodU64` -/
  current_slot : Nat
  /-- `bankruptcy_hlock_active: u8` (encoded bool) -/
  bankruptcy_hlock_active : Nat
  /-- `threshold_stress_active: u8` (encoded bool) -/
  threshold_stress_active : Nat
  /-- `loss_stale_active: u8` (encoded bool) -/
  loss_stale_active : Nat
  /-- `recovery_reason: V16OptionalRecoveryReasonAccount` -/
  recovery_reason : RecoveryReasonAccount
  /-- `mode: u8` -/
  mode : Nat
  /-- `resolved_slot: V16PodU64` -/
  resolved_slot : Nat
  /-- `payout_snapshot: V16PodU128` -/
  payout_snapshot : Nat
  /-- `payout_snapshot_pnl_pos_tot: V16PodU128` -/
  payout_snapshot_pnl_pos_tot : Nat
  /-- `payout_snapshot_captured: u8` (encoded bool) -/
  payout_snapshot_captured : Nat
  /-- `resolved_payout_ledger: ResolvedPayoutLedgerV16Account` -/
  resolved_payout_ledger : LedgerAccount

/-! ## Range predicates -/

/-- Every field of the config account fits its Rust width. -/
def ConfigAccount.InRange (c : ConfigAccount) : Prop :=
  c.max_portfolio_assets < 2 ^ 16 ∧
  c.max_market_slots < 2 ^ 32 ∧
  c.min_nonzero_mm_req < 2 ^ 128 ∧
  c.min_nonzero_im_req < 2 ^ 128 ∧
  c.h_min < 2 ^ 64 ∧
  c.h_max < 2 ^ 64 ∧
  c.maintenance_margin_bps < 2 ^ 64 ∧
  c.initial_margin_bps < 2 ^ 64 ∧
  c.max_trading_fee_bps < 2 ^ 64 ∧
  c.liquidation_fee_bps < 2 ^ 64 ∧
  c.liquidation_fee_cap < 2 ^ 128 ∧
  c.min_liquidation_abs < 2 ^ 128 ∧
  c.max_accrual_dt_slots < 2 ^ 64 ∧
  c.max_abs_funding_e9_per_slot < 2 ^ 64 ∧
  c.min_funding_lifetime_slots < 2 ^ 64 ∧
  c.max_price_move_bps_per_slot < 2 ^ 64 ∧
  c.max_account_b_settlement_chunks < 2 ^ 64 ∧
  c.max_bankrupt_close_chunks < 2 ^ 64 ∧
  c.max_bankrupt_close_lifetime_slots < 2 ^ 64 ∧
  c.asset_activation_cooldown_slots < 2 ^ 64 ∧
  c.public_b_chunk_atoms < 2 ^ 128 ∧
  c.max_recovery_fallback_deviation_bps < 2 ^ 64 ∧
  c.backing_fee_base_rate_e9_per_slot < 2 ^ 64 ∧
  c.backing_fee_kink_util_bps < 2 ^ 64 ∧
  c.backing_fee_slope_at_kink_e9_per_slot < 2 ^ 64 ∧
  c.backing_fee_slope_above_kink_e9_per_slot < 2 ^ 64 ∧
  c.backing_freshness_buckets < 2 ^ 8 ∧
  c.margin_mode_realizable_full_shared_cross_margin < 2 ^ 8 ∧
  c.source_credit_lien_required < 2 ^ 8 ∧
  c.insurance_credit_reservation_required < 2 ^ 8 ∧
  c.permissionless_recovery_enabled < 2 ^ 8 ∧
  c.recovery_fallback_price_enabled < 2 ^ 8 ∧
  c.recovery_fallback_envelope_enabled < 2 ^ 8 ∧
  c.credit_lien_revalidation_required < 2 ^ 8 ∧
  c.stale_certificate_penalty_enabled < 2 ^ 8 ∧
  c.full_refresh_required_for_favorable_actions < 2 ^ 8 ∧
  c.public_liveness_profile_crank_forward < 2 ^ 8

/-- Every field of the resolved-payout ledger account fits its Rust width. -/
def LedgerAccount.InRange (l : LedgerAccount) : Prop :=
  l.snapshot_residual < 2 ^ 128 ∧
  l.terminal_claim_exact_receipts_num < 2 ^ 128 ∧
  l.terminal_claim_bound_unreceipted_num < 2 ^ 128 ∧
  l.current_payout_rate_num < 2 ^ 128 ∧
  l.current_payout_rate_den < 2 ^ 128 ∧
  l.snapshot_slot < 2 ^ 64 ∧
  l.payout_halted < 2 ^ 8 ∧
  l.finalized < 2 ^ 8

/-- Both bytes of the optional recovery reason fit in a `u8`. -/
def RecoveryReasonAccount.InRange (r : RecoveryReasonAccount) : Prop :=
  r.present < 2 ^ 8 ∧ r.value < 2 ^ 8

/-- Every field of the header account fits its Rust width. -/
def HeaderAccount.InRange (h : HeaderAccount) : Prop :=
  h.market_group_id.length = 32 ∧ (∀ b ∈ h.market_group_id, b < 2 ^ 8) ∧
  h.config.InRange ∧
  h.asset_slot_capacity < 2 ^ 32 ∧
  h.vault < 2 ^ 128 ∧
  h.insurance < 2 ^ 128 ∧
  h.c_tot < 2 ^ 128 ∧
  h.pnl_pos_tot < 2 ^ 128 ∧
  h.pnl_pos_bound_tot_num < 2 ^ 128 ∧
  h.pnl_pos_bound_tot < 2 ^ 128 ∧
  h.pnl_matured_pos_tot < 2 ^ 128 ∧
  h.backing_provider_earnings_total < 2 ^ 128 ∧
  h.source_claim_bound_total_num < 2 ^ 128 ∧
  h.source_fresh_backing_total_num < 2 ^ 128 ∧
  h.source_insurance_credit_reserved_total_atoms < 2 ^ 128 ∧
  h.insurance_domain_budget_remaining_total < 2 ^ 128 ∧
  h.resolved_payout_blocker_count < 2 ^ 64 ∧
  h.materialized_portfolio_count < 2 ^ 64 ∧
  h.stale_certificate_count < 2 ^ 64 ∧
  h.b_stale_account_count < 2 ^ 64 ∧
  h.negative_pnl_account_count < 2 ^ 64 ∧
  h.risk_epoch < 2 ^ 64 ∧
  h.asset_set_epoch < 2 ^ 64 ∧
  h.asset_activation_count < 2 ^ 64 ∧
  h.last_asset_activation_slot < 2 ^ 64 ∧
  h.next_market_id < 2 ^ 64 ∧
  h.oracle_epoch < 2 ^ 64 ∧
  h.funding_epoch < 2 ^ 64 ∧
  h.slot_last < 2 ^ 64 ∧
  h.current_slot < 2 ^ 64 ∧
  h.bankruptcy_hlock_active < 2 ^ 8 ∧
  h.threshold_stress_active < 2 ^ 8 ∧
  h.loss_stale_active < 2 ^ 8 ∧
  h.recovery_reason.InRange ∧
  h.mode < 2 ^ 8 ∧
  h.resolved_slot < 2 ^ 64 ∧
  h.payout_snapshot < 2 ^ 128 ∧
  h.payout_snapshot_pnl_pos_tot < 2 ^ 128 ∧
  h.payout_snapshot_captured < 2 ^ 8 ∧
  h.resolved_payout_ledger.InRange

/-! ## Decoding -/

/-- `decode_bool(b)` returns `Ok(_)`: the byte is `0` (`false`) or `1` (`true`). -/
def DecodesBool (b : Nat) : Prop := b = 0 ∨ b = 1

/-- `V16ConfigAccount::decode_runtime` returns `Ok(_)`: each of the ten flag bytes
passes `decode_bool`. (`backing_freshness_buckets` is copied as a raw `u8`; all
Pod fields are copied with `get()`, which cannot fail.) -/
def ConfigAccount.DecodeRuntimeOk (c : ConfigAccount) : Prop :=
  DecodesBool c.margin_mode_realizable_full_shared_cross_margin ∧
  DecodesBool c.source_credit_lien_required ∧
  DecodesBool c.insurance_credit_reservation_required ∧
  DecodesBool c.permissionless_recovery_enabled ∧
  DecodesBool c.recovery_fallback_price_enabled ∧
  DecodesBool c.recovery_fallback_envelope_enabled ∧
  DecodesBool c.credit_lien_revalidation_required ∧
  DecodesBool c.stale_certificate_penalty_enabled ∧
  DecodesBool c.full_refresh_required_for_favorable_actions ∧
  DecodesBool c.public_liveness_profile_crank_forward

/-- `V16Config::validate_public_user_fund_shape` returns `Ok(())` on the config
decoded from `c`. Each conjunct is the negation of one disjunct of the Rust
`if ... { return Err(V16Error::InvalidConfig); }` tests, in source order:

```
max_portfolio_assets == 0 || max_portfolio_assets as usize > V16_MAX_PORTFOLIO_ASSETS_N
  || max_market_slots == 0 || max_portfolio_assets as u32 > max_market_slots
h_max == 0 || h_min > h_max
min_nonzero_mm_req == 0 || min_nonzero_mm_req >= min_nonzero_im_req
maintenance_margin_bps > initial_margin_bps || initial_margin_bps > MAX_MARGIN_BPS
  || max_trading_fee_bps > MAX_MARGIN_BPS || liquidation_fee_bps > MAX_MARGIN_BPS
  || min_liquidation_abs > liquidation_fee_cap || liquidation_fee_cap > MAX_PROTOCOL_FEE_ABS
  || max_accrual_dt_slots == 0 || min_funding_lifetime_slots < max_accrual_dt_slots
  || max_abs_funding_e9_per_slot > 10_000 || max_price_move_bps_per_slot == 0
  || max_account_b_settlement_chunks == 0 || max_bankrupt_close_chunks == 0
  || max_bankrupt_close_lifetime_slots == 0 || asset_activation_cooldown_slots == 0
  || public_b_chunk_atoms == 0
  || max_recovery_fallback_deviation_bps > MAX_RECOVERY_FALLBACK_DEVIATION_BPS
  || backing_fee_kink_util_bps == 0 || backing_fee_kink_util_bps >= MAX_BACKING_FEE_UTIL_BPS
  || backing_freshness_buckets == 0
  || backing_freshness_buckets as usize > V16_BACKING_BUCKETS_PER_DOMAIN
backing_fee_base_rate_e9_per_slot.checked_add(backing_fee_slope_at_kink_e9_per_slot)
  .and_then(|v| v.checked_add(backing_fee_slope_above_kink_e9_per_slot))
  .ok_or(InvalidConfig)? > MAX_BACKING_FEE_RATE_E9_PER_SLOT
!margin_mode_realizable_full_shared_cross_margin || ... || !public_liveness_profile_crank_forward
```

The casts `as usize` (from `u16`, `u8`) and `as u32` (from `u16`) are widening.
The `u64` `checked_add` chain followed by `<= MAX_BACKING_FEE_RATE_E9_PER_SLOT` is
stated as the mathematical inequality `base + at + above ≤ MAX_BACKING_FEE_RATE_E9_PER_SLOT`:
since `MAX_BACKING_FEE_RATE_E9_PER_SLOT = 10^9 < 2^64`, for `u64` operands the
inequality holds exactly when neither partial sum overflows and the final sum is
at most the maximum (an overflow means the exact sum is `≥ 2^64 > 10^9`).
The ten flags must decode to `true`, i.e. their byte is `1` (given
`DecodeRuntimeOk`, which makes every flag byte `0` or `1`). -/
def ConfigAccount.ValidatePublicUserFundShapeOk (c : ConfigAccount) : Prop :=
  -- max_portfolio_assets / max_market_slots
  c.max_portfolio_assets ≠ 0 ∧
  c.max_portfolio_assets ≤ V16_MAX_PORTFOLIO_ASSETS_N ∧
  c.max_market_slots ≠ 0 ∧
  c.max_portfolio_assets ≤ c.max_market_slots ∧
  -- h_min / h_max
  c.h_max ≠ 0 ∧
  c.h_min ≤ c.h_max ∧
  -- margin requirements
  c.min_nonzero_mm_req ≠ 0 ∧
  c.min_nonzero_mm_req < c.min_nonzero_im_req ∧
  -- the long disjunction
  c.maintenance_margin_bps ≤ c.initial_margin_bps ∧
  c.initial_margin_bps ≤ MAX_MARGIN_BPS ∧
  c.max_trading_fee_bps ≤ MAX_MARGIN_BPS ∧
  c.liquidation_fee_bps ≤ MAX_MARGIN_BPS ∧
  c.min_liquidation_abs ≤ c.liquidation_fee_cap ∧
  c.liquidation_fee_cap ≤ MAX_PROTOCOL_FEE_ABS ∧
  c.max_accrual_dt_slots ≠ 0 ∧
  c.max_accrual_dt_slots ≤ c.min_funding_lifetime_slots ∧
  c.max_abs_funding_e9_per_slot ≤ MAX_ABS_FUNDING_E9_PER_SLOT_LITERAL ∧
  c.max_price_move_bps_per_slot ≠ 0 ∧
  c.max_account_b_settlement_chunks ≠ 0 ∧
  c.max_bankrupt_close_chunks ≠ 0 ∧
  c.max_bankrupt_close_lifetime_slots ≠ 0 ∧
  c.asset_activation_cooldown_slots ≠ 0 ∧
  c.public_b_chunk_atoms ≠ 0 ∧
  c.max_recovery_fallback_deviation_bps ≤ MAX_RECOVERY_FALLBACK_DEVIATION_BPS ∧
  c.backing_fee_kink_util_bps ≠ 0 ∧
  c.backing_fee_kink_util_bps < MAX_BACKING_FEE_UTIL_BPS ∧
  c.backing_freshness_buckets ≠ 0 ∧
  c.backing_freshness_buckets ≤ V16_BACKING_BUCKETS_PER_DOMAIN ∧
  -- backing fee rate sum (checked_add chain in u64)
  c.backing_fee_base_rate_e9_per_slot + c.backing_fee_slope_at_kink_e9_per_slot +
      c.backing_fee_slope_above_kink_e9_per_slot ≤ MAX_BACKING_FEE_RATE_E9_PER_SLOT ∧
  -- the ten required-true flags
  c.margin_mode_realizable_full_shared_cross_margin = 1 ∧
  c.source_credit_lien_required = 1 ∧
  c.insurance_credit_reservation_required = 1 ∧
  c.permissionless_recovery_enabled = 1 ∧
  c.recovery_fallback_price_enabled = 1 ∧
  c.recovery_fallback_envelope_enabled = 1 ∧
  c.credit_lien_revalidation_required = 1 ∧
  c.stale_certificate_penalty_enabled = 1 ∧
  c.full_refresh_required_for_favorable_actions = 1 ∧
  c.public_liveness_profile_crank_forward = 1

/-- `V16ConfigAccount::try_to_runtime_shape` returns `Ok(_)`:
`self.decode_runtime()?` succeeds and `out.validate_public_user_fund_shape()?`
succeeds on the decoded config. -/
def ConfigAccount.TryToRuntimeShapeOk (c : ConfigAccount) : Prop :=
  c.DecodeRuntimeOk ∧ c.ValidatePublicUserFundShapeOk

/-- `decode_recovery_reason(v)` returns `Ok(_)`: `v` is one of the eight encodings
`0..=7` of `PermissionlessRecoveryReasonV16`. -/
def DecodesRecoveryReason (v : Nat) : Prop := v ≤ 7

/-- `V16OptionalRecoveryReasonAccount::try_to_runtime` returns `Ok(_)`:

```
match self.present {
    0 if self.value == 0 => Ok(None),
    1 => Ok(Some(decode_recovery_reason(self.value)?)),
    _ => Err(V16Error::InvalidConfig),
}
``` -/
def RecoveryReasonAccount.Decodes (r : RecoveryReasonAccount) : Prop :=
  (r.present = 0 ∧ r.value = 0) ∨ (r.present = 1 ∧ DecodesRecoveryReason r.value)

/-- `ResolvedPayoutLedgerV16Account::try_to_runtime` returns `Ok(_)`: the two bool
bytes `payout_halted` and `finalized` pass `decode_bool` (all Pod fields are
copied with `get()`, which cannot fail). -/
def LedgerAccount.Decodes (l : LedgerAccount) : Prop :=
  DecodesBool l.payout_halted ∧ DecodesBool l.finalized

/-- The ledger account decodes to `ResolvedPayoutLedgerV16::EMPTY`, i.e. every
integer field is `0` and both bool bytes decode to `false` (byte `0`).
(`EMPTY` has all five `u128` fields and `snapshot_slot` equal to `0` and
`payout_halted = false`, `finalized = false`; `ResolvedPayoutLedgerV16` derives
`PartialEq`, so `!=` is field-wise.) -/
def LedgerAccount.IsEmpty (l : LedgerAccount) : Prop :=
  l.snapshot_residual = 0 ∧
  l.terminal_claim_exact_receipts_num = 0 ∧
  l.terminal_claim_bound_unreceipted_num = 0 ∧
  l.current_payout_rate_num = 0 ∧
  l.current_payout_rate_den = 0 ∧
  l.snapshot_slot = 0 ∧
  l.payout_halted = 0 ∧
  l.finalized = 0

/-! ## Bound-numerator conversions -/

/-- Value returned by `V16Core::amount_from_bound_num(bound_num)`:

```
let whole = bound_num / BOUND_SCALE;
let rem = bound_num % BOUND_SCALE;
if rem == 0 { Ok(whole) } else { whole.checked_add(1).ok_or(ArithmeticOverflow) }
```

i.e. the ceiling of `bound_num / BOUND_SCALE`. For a `u128` input `whole ≤
(2^128 - 1) / 10^12`, so `whole + 1 < 2^128` and the `checked_add` never
overflows: the function always returns `Ok`. -/
def amountFromBoundNum (n : Nat) : Nat :=
  if n % BOUND_SCALE = 0 then n / BOUND_SCALE else n / BOUND_SCALE + 1

/-- Value of `V16Core::bound_num_from_amount(amount) = amount.checked_mul(BOUND_SCALE)`
when it succeeds (it fails with `ArithmeticOverflow` exactly when
`amount * BOUND_SCALE ≥ 2^128`). -/
def boundNumFromAmount (a : Nat) : Nat := a * BOUND_SCALE

/-! ## The eighteen checks of `validate_shape` -/

/-- **H0** — first statement of `validate_shape`:

```
MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static(
    self.markets.len(),
    self.header.asset_slot_capacity.get() as usize,
    self.header.config.max_market_slots.get() as usize,
)?;
```

which fails iff `supplied_len != capacity || capacity < configured_market_slots`.
Reads `asset_slot_capacity`, `config.max_market_slots` and the length
`marketsLen` of the market-slot slice. -/
def H0 (marketsLen : Nat) (h : HeaderAccount) : Prop :=
  marketsLen = h.asset_slot_capacity ∧ h.config.max_market_slots ≤ h.asset_slot_capacity

/-- **H1** — `self.header.config.try_to_runtime_shape()?;` Reads `config`. -/
def H1 (h : HeaderAccount) : Prop := h.config.TryToRuntimeShapeOk

/-- **H2** — the four flags decode as booleans:

```
decode_bool(self.header.bankruptcy_hlock_active)?;
decode_bool(self.header.threshold_stress_active)?;
decode_bool(self.header.loss_stale_active)?;
decode_bool(self.header.payout_snapshot_captured)?;
```

Reads `bankruptcy_hlock_active`, `threshold_stress_active`, `loss_stale_active`,
`payout_snapshot_captured`. -/
def H2 (h : HeaderAccount) : Prop :=
  DecodesBool h.bankruptcy_hlock_active ∧
  DecodesBool h.threshold_stress_active ∧
  DecodesBool h.loss_stale_active ∧
  DecodesBool h.payout_snapshot_captured

/-- **H3** — `self.header.recovery_reason.try_to_runtime()?;` Reads `recovery_reason`. -/
def H3 (h : HeaderAccount) : Prop := h.recovery_reason.Decodes

/-- **H4** — `let resolved_ledger = self.header.resolved_payout_ledger.try_to_runtime()?;`
Reads `resolved_payout_ledger`. -/
def H4 (h : HeaderAccount) : Prop := h.resolved_payout_ledger.Decodes

/-- **H5** — `if self.header.vault.get() > MAX_VAULT_TVL { return Err(InvalidConfig); }`
Reads `vault`. -/
def H5 (h : HeaderAccount) : Prop := h.vault ≤ MAX_VAULT_TVL

/-- **H6** — `if c_tot > vault || insurance > vault { return Err(InvalidConfig); }`
Reads `c_tot`, `insurance`, `vault`. -/
def H6 (h : HeaderAccount) : Prop := h.c_tot ≤ h.vault ∧ h.insurance ≤ h.vault

/-- **H7** — first two disjuncts of

```
if pnl_matured_pos_tot > pnl_pos_tot || pnl_pos_bound_tot < pnl_pos_tot
    || slot_last > current_slot || next_market_id == 0 { return Err(InvalidConfig); }
```

i.e. `pnl_matured_pos_tot ≤ pnl_pos_tot ≤ pnl_pos_bound_tot`. Reads
`pnl_matured_pos_tot`, `pnl_pos_tot`, `pnl_pos_bound_tot`. -/
def H7 (h : HeaderAccount) : Prop :=
  h.pnl_matured_pos_tot ≤ h.pnl_pos_tot ∧ h.pnl_pos_tot ≤ h.pnl_pos_bound_tot

/-- **H8** — third disjunct of the `if` of H7: `slot_last > current_slot` fails.
Reads `slot_last`, `current_slot`. -/
def H8 (h : HeaderAccount) : Prop := h.slot_last ≤ h.current_slot

/-- **H9** — fourth disjunct of the `if` of H7: `next_market_id == 0` fails.
Reads `next_market_id`. -/
def H9 (h : HeaderAccount) : Prop := h.next_market_id ≠ 0

/-- **H10** —

```
let derived_bound = V16Core::amount_from_bound_num(self.header.pnl_pos_bound_tot_num.get())?;
if self.header.pnl_pos_bound_tot.get() != derived_bound { return Err(InvalidConfig); }
```

(`amount_from_bound_num` never fails on a `u128`). Reads `pnl_pos_bound_tot`,
`pnl_pos_bound_tot_num`. -/
def H10 (h : HeaderAccount) : Prop :=
  h.pnl_pos_bound_tot = amountFromBoundNum h.pnl_pos_bound_tot_num

/-- **H11** —

```
let exact_bound_num = V16Core::bound_num_from_amount(self.header.pnl_pos_tot.get())?;
if self.header.pnl_pos_bound_tot_num.get() < exact_bound_num { return Err(InvalidConfig); }
```

In Rust the `checked_mul` overflow also fails. For in-range values
(`pnl_pos_bound_tot_num < 2^128`) the mathematical inequality below is equivalent:
it forces `pnl_pos_tot * BOUND_SCALE < 2^128` (no overflow), and an overflow
means `pnl_pos_tot * BOUND_SCALE ≥ 2^128 > pnl_pos_bound_tot_num`.
Reads `pnl_pos_bound_tot_num`, `pnl_pos_tot`. -/
def H11 (h : HeaderAccount) : Prop :=
  boundNumFromAmount h.pnl_pos_tot ≤ h.pnl_pos_bound_tot_num

/-- **H12** —

```
if !decode_bool(self.header.payout_snapshot_captured)?
    && resolved_ledger != ResolvedPayoutLedgerV16::EMPTY { return Err(InvalidConfig); }
```

Stated on the raw byte: if `payout_snapshot_captured` is `0` (decodes to `false`)
the ledger must equal `EMPTY`. (The `decode_bool` here cannot fail once H2 holds,
and `resolved_ledger` is the decoded ledger of H4.) Reads
`payout_snapshot_captured`, `resolved_payout_ledger`. -/
def H12 (h : HeaderAccount) : Prop :=
  h.payout_snapshot_captured = 0 → h.resolved_payout_ledger.IsEmpty

/-- **H13** —

```
if asset_activation_count == 0 {
    if last_asset_activation_slot != 0 { return Err(InvalidConfig); }
} else if last_asset_activation_slot > current_slot { return Err(InvalidConfig); }
```

Reads `asset_activation_count`, `last_asset_activation_slot`, `current_slot`. -/
def H13 (h : HeaderAccount) : Prop :=
  (h.asset_activation_count = 0 → h.last_asset_activation_slot = 0) ∧
  (h.asset_activation_count ≠ 0 → h.last_asset_activation_slot ≤ h.current_slot)

/-- **H14** — first check of `validate_header_aggregate_totals`:

```
let senior = c_tot.checked_add(insurance)
    .and_then(|v| v.checked_add(backing_provider_earnings_total))
    .ok_or(ArithmeticOverflow)?;
if senior > vault { return Err(InvalidConfig); }
```

In Rust a `checked_add` overflow also fails; for in-range values (`vault < 2^128`)
the mathematical inequality is equivalent. Reads `c_tot`, `insurance`,
`backing_provider_earnings_total`, `vault`. -/
def H14 (h : HeaderAccount) : Prop :=
  h.c_tot + h.insurance + h.backing_provider_earnings_total ≤ h.vault

/-- **H15** — second check of `validate_header_aggregate_totals`:

```
let senior_with_backing = senior
    .checked_add(source_fresh_backing_total_num / BOUND_SCALE)
    .ok_or(ArithmeticOverflow)?;
if senior_with_backing > vault { return Err(InvalidConfig); }
```

(`/` is floor division.) In Rust a `checked_add` overflow also fails; for
in-range values (`vault < 2^128`) the mathematical inequality is equivalent.
Reads `c_tot`, `insurance`, `backing_provider_earnings_total`,
`source_fresh_backing_total_num`, `vault`. -/
def H15 (h : HeaderAccount) : Prop :=
  h.c_tot + h.insurance + h.backing_provider_earnings_total +
      h.source_fresh_backing_total_num / BOUND_SCALE ≤ h.vault

/-- **H16** — third check of `validate_header_aggregate_totals`:

```
if source_insurance_credit_reserved_total_atoms > insurance
    || insurance_domain_budget_remaining_total > insurance { return Err(InvalidConfig); }
```

Reads `source_insurance_credit_reserved_total_atoms`,
`insurance_domain_budget_remaining_total`, `insurance`. -/
def H16 (h : HeaderAccount) : Prop :=
  h.source_insurance_credit_reserved_total_atoms ≤ h.insurance ∧
  h.insurance_domain_budget_remaining_total ≤ h.insurance

/-- **H17** — last check of `validate_header_aggregate_totals`:

```
if pnl_pos_bound_tot_num < source_claim_bound_total_num { return Err(InvalidConfig); }
```

Reads `pnl_pos_bound_tot_num`, `source_claim_bound_total_num`. -/
def H17 (h : HeaderAccount) : Prop :=
  h.source_claim_bound_total_num ≤ h.pnl_pos_bound_tot_num

/-- `Hh`: every check of `MarketGroupV16View::validate_shape` after its first
statement (`validate_dynamic_market_slots_len_static`) passes on header `h`,
i.e. H1 to H17 (default build, audit scan off). -/
def HeaderShapeOk (h : HeaderAccount) : Prop :=
  H1 h ∧ H2 h ∧ H3 h ∧ H4 h ∧ H5 h ∧ H6 h ∧ H7 h ∧ H8 h ∧ H9 h ∧
  H10 h ∧ H11 h ∧ H12 h ∧ H13 h ∧ H14 h ∧ H15 h ∧ H16 h ∧ H17 h

/-- `H`: `MarketGroupV16View::validate_shape` (equivalently
`MarketGroupV16ViewMut::validate_shape`) returns `Ok(())` in a default build
(audit scan off) on a market whose header is `h` and whose market-slot slice has
length `marketsLen`: H0 and every check of `HeaderShapeOk`. -/
def MarketShapeOk (marketsLen : Nat) (h : HeaderAccount) : Prop :=
  H0 marketsLen h ∧ HeaderShapeOk h

end Spec.LayerT

end
