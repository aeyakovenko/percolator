//! v16 account-local risk engine.
//!
//! This module implements the v16 slab-free engine surface: authenticated
//! portfolio accounts, bounded per-account refresh, lazy A/K/F/B settlement,
//! source-domain realizable credit, loss-senior fee handling, account-local
//! cranks, residual B booking, dynamic trade fees, liquidation progress checks,
//! and resolved account close.

use crate::wide_math::{
    checked_mul_div_ceil_u256, floor_div_signed_conservative_i128, mul_div_floor_u256_with_rem,
    wide_mul_div_floor_u128, wide_signed_mul_div_floor_from_k_pair, U256,
};
use crate::{
    ADL_ONE, BOUND_SCALE, CREDIT_RATE_SCALE, FUNDING_DEN, MAX_ACCOUNT_NOTIONAL, MAX_MARGIN_BPS,
    MAX_ORACLE_PRICE, MAX_POSITION_ABS_Q, MAX_PROTOCOL_FEE_ABS,
    MAX_RECOVERY_FALLBACK_DEVIATION_BPS, MAX_TRADE_SIZE_Q, MAX_VAULT_TVL, MIN_A_SIDE, POS_SCALE,
    SOCIAL_LOSS_DEN, SOCIAL_WEIGHT_SCALE,
};

pub const V16_MAX_PORTFOLIO_ASSETS_N: usize = 16;
pub const V16_ACTIVE_BITMAP_WORDS: usize = (V16_MAX_PORTFOLIO_ASSETS_N + 63) / 64;
pub type V16ActiveBitmap = [u64; V16_ACTIVE_BITMAP_WORDS];
pub const V16_EMPTY_ACTIVE_BITMAP: V16ActiveBitmap = [0; V16_ACTIVE_BITMAP_WORDS];
pub const V16_BACKING_BUCKETS_PER_DOMAIN: usize = 1;
pub const V16_LAYOUT_DISCRIMINATOR: u16 = 16;
pub const V16_ACCOUNT_VERSION: u16 = 1;
pub const BACKING_FEE_RATE_DEN_E9: u128 = 1_000_000_000;
pub const MAX_BACKING_FEE_RATE_E9_PER_SLOT: u64 = 1_000_000_000;
pub const MAX_BACKING_FEE_UTIL_BPS: u64 = 10_000;

fn apply_backing_utilization_fee_charge(
    account_capital: u128,
    group_c_tot: u128,
    bucket_earnings: u128,
    account_pnl: i128,
    requested_fee: u128,
) -> V16Result<(u128, u128, u128, u128)> {
    if requested_fee == 0 || account_pnl < 0 {
        return Ok((0, account_capital, group_c_tot, bucket_earnings));
    }
    let charged = requested_fee.min(account_capital);
    if charged == 0 {
        return Ok((0, account_capital, group_c_tot, bucket_earnings));
    }
    let next_account_capital = account_capital
        .checked_sub(charged)
        .ok_or(V16Error::CounterUnderflow)?;
    let next_group_c_tot = group_c_tot
        .checked_sub(charged)
        .ok_or(V16Error::CounterUnderflow)?;
    let next_bucket_earnings = bucket_earnings
        .checked_add(charged)
        .ok_or(V16Error::CounterOverflow)?;
    Ok((
        charged,
        next_account_capital,
        next_group_c_tot,
        next_bucket_earnings,
    ))
}

fn apply_backing_provider_earnings_withdraw(
    vault: u128,
    bucket_earnings: u128,
    amount: u128,
) -> V16Result<(u128, u128)> {
    if amount == 0 {
        return Ok((vault, bucket_earnings));
    }
    if bucket_earnings < amount {
        return Err(V16Error::CounterUnderflow);
    }
    let next_vault = vault
        .checked_sub(amount)
        .ok_or(V16Error::CounterUnderflow)?;
    Ok((next_vault, bucket_earnings - amount))
}

#[cfg(kani)]
pub fn kani_apply_backing_utilization_fee_charge(
    account_capital: u128,
    group_c_tot: u128,
    bucket_earnings: u128,
    account_pnl: i128,
    requested_fee: u128,
) -> V16Result<(u128, u128, u128, u128)> {
    apply_backing_utilization_fee_charge(
        account_capital,
        group_c_tot,
        bucket_earnings,
        account_pnl,
        requested_fee,
    )
}

#[cfg(kani)]
pub fn kani_apply_backing_provider_earnings_withdraw(
    vault: u128,
    bucket_earnings: u128,
    amount: u128,
) -> V16Result<(u128, u128)> {
    apply_backing_provider_earnings_withdraw(vault, bucket_earnings, amount)
}

#[inline]
pub fn v16_domain_count_for_market_slots(max_market_slots: u32) -> V16Result<usize> {
    (max_market_slots as usize)
        .checked_mul(2)
        .ok_or(V16Error::ArithmeticOverflow)
}

#[inline]
pub fn v16_domain_pair_for_asset_index(asset_index: usize) -> V16Result<(usize, usize)> {
    let long_domain = asset_index
        .checked_mul(2)
        .ok_or(V16Error::ArithmeticOverflow)?;
    let short_domain = long_domain
        .checked_add(1)
        .ok_or(V16Error::ArithmeticOverflow)?;
    Ok((long_domain, short_domain))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V16Error {
    InvalidConfig,
    ArithmeticOverflow,
    ProvenanceMismatch,
    HiddenLeg,
    InvalidLeg,
    Stale,
    BStale,
    LockActive,
    NonProgress,
    RecoveryRequired,
    CounterOverflow,
    CounterUnderflow,
}

pub type V16Result<T> = core::result::Result<T, V16Error>;

#[inline]
pub const fn active_bitmap_empty() -> V16ActiveBitmap {
    V16_EMPTY_ACTIVE_BITMAP
}

#[inline]
pub fn active_bitmap_is_empty(bitmap: V16ActiveBitmap) -> bool {
    let mut i = 0;
    while i < V16_ACTIVE_BITMAP_WORDS {
        if bitmap[i] != 0 {
            return false;
        }
        i += 1;
    }
    true
}

#[inline]
pub fn active_bitmap_get(bitmap: V16ActiveBitmap, leg_slot_index: usize) -> bool {
    if leg_slot_index >= V16_MAX_PORTFOLIO_ASSETS_N {
        return false;
    }
    let word = leg_slot_index / 64;
    let bit = leg_slot_index % 64;
    ((bitmap[word] >> bit) & 1) != 0
}

#[inline]
pub fn active_bitmap_set(bitmap: &mut V16ActiveBitmap, leg_slot_index: usize) -> V16Result<()> {
    if leg_slot_index >= V16_MAX_PORTFOLIO_ASSETS_N {
        return Err(V16Error::InvalidConfig);
    }
    let word = leg_slot_index / 64;
    let bit = leg_slot_index % 64;
    bitmap[word] |= 1u64 << bit;
    Ok(())
}

#[inline]
pub fn active_bitmap_clear(bitmap: &mut V16ActiveBitmap, leg_slot_index: usize) -> V16Result<()> {
    if leg_slot_index >= V16_MAX_PORTFOLIO_ASSETS_N {
        return Err(V16Error::InvalidConfig);
    }
    let word = leg_slot_index / 64;
    let bit = leg_slot_index % 64;
    bitmap[word] &= !(1u64 << bit);
    Ok(())
}

#[inline]
pub fn active_bitmap_with_cleared(
    mut bitmap: V16ActiveBitmap,
    leg_slot_index: usize,
) -> V16Result<V16ActiveBitmap> {
    active_bitmap_clear(&mut bitmap, leg_slot_index)?;
    Ok(bitmap)
}

#[inline]
fn liquidation_remaining_active_bitmap_after_close(
    active_bitmap: V16ActiveBitmap,
    leg_slot_index: usize,
    close_q: u128,
    leg_abs_q: u128,
) -> V16Result<V16ActiveBitmap> {
    if close_q == leg_abs_q {
        active_bitmap_with_cleared(active_bitmap, leg_slot_index)
    } else {
        Ok(active_bitmap)
    }
}

#[inline]
fn liquidation_uncovered_loss_after_principal(pnl: i128, capital: u128) -> u128 {
    if pnl < 0 {
        pnl.unsigned_abs().saturating_sub(capital)
    } else {
        0
    }
}

#[inline]
fn liquidation_close_would_leave_uncovered_loss_with_open_risk(
    pnl: i128,
    capital: u128,
    active_bitmap: V16ActiveBitmap,
    leg_slot_index: usize,
    close_q: u128,
    leg_abs_q: u128,
) -> V16Result<bool> {
    let uncovered_loss_after_principal = liquidation_uncovered_loss_after_principal(pnl, capital);
    let remaining_active_bitmap = liquidation_remaining_active_bitmap_after_close(
        active_bitmap,
        leg_slot_index,
        close_q,
        leg_abs_q,
    )?;
    Ok(uncovered_loss_after_principal != 0 && !active_bitmap_is_empty(remaining_active_bitmap))
}

#[cfg(kani)]
pub fn kani_liquidation_close_would_leave_uncovered_loss_with_open_risk(
    pnl: i128,
    capital: u128,
    active_bitmap: V16ActiveBitmap,
    leg_slot_index: usize,
    close_q: u128,
    leg_abs_q: u128,
) -> V16Result<bool> {
    liquidation_close_would_leave_uncovered_loss_with_open_risk(
        pnl,
        capital,
        active_bitmap,
        leg_slot_index,
        close_q,
        leg_abs_q,
    )
}

fn add_open_interest_for_new_position(
    asset: &mut AssetStateV16,
    side: SideV16,
    abs_q: u128,
    loss_weight: u128,
) -> V16Result<()> {
    match side {
        SideV16::Long => {
            asset.stored_pos_count_long = asset
                .stored_pos_count_long
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?;
            asset.oi_eff_long_q = asset
                .oi_eff_long_q
                .checked_add(abs_q)
                .ok_or(V16Error::ArithmeticOverflow)?;
            asset.loss_weight_sum_long = asset
                .loss_weight_sum_long
                .checked_add(loss_weight)
                .ok_or(V16Error::ArithmeticOverflow)?;
        }
        SideV16::Short => {
            asset.stored_pos_count_short = asset
                .stored_pos_count_short
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?;
            asset.oi_eff_short_q = asset
                .oi_eff_short_q
                .checked_add(abs_q)
                .ok_or(V16Error::ArithmeticOverflow)?;
            asset.loss_weight_sum_short = asset
                .loss_weight_sum_short
                .checked_add(loss_weight)
                .ok_or(V16Error::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

#[cfg(kani)]
pub fn kani_add_open_interest_for_new_position(
    asset: &mut AssetStateV16,
    side: SideV16,
    abs_q: u128,
    loss_weight: u128,
) -> V16Result<()> {
    add_open_interest_for_new_position(asset, side, abs_q, loss_weight)
}

#[inline]
pub fn active_bitmap_count_ones(bitmap: V16ActiveBitmap) -> u32 {
    let mut total = 0u32;
    let mut i = 0;
    while i < V16_ACTIVE_BITMAP_WORDS {
        total = total.saturating_add(bitmap[i].count_ones());
        i += 1;
    }
    total
}

struct V16Core;

impl V16Core {
    #[inline]
    fn amount_from_bound_num(bound_num: u128) -> V16Result<u128> {
        let whole = bound_num / BOUND_SCALE;
        let rem = bound_num % BOUND_SCALE;
        if rem == 0 {
            Ok(whole)
        } else {
            whole.checked_add(1).ok_or(V16Error::ArithmeticOverflow)
        }
    }

    #[inline]
    fn bound_num_from_amount(amount: u128) -> V16Result<u128> {
        amount
            .checked_mul(BOUND_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)
    }

    #[inline(always)]
    fn source_credit_lien_amounts_for_effective(
        effective_credit: u128,
        credit_rate_num: u128,
    ) -> V16Result<(u128, u128)> {
        if credit_rate_num == 0 {
            return Err(V16Error::LockActive);
        }
        if credit_rate_num > CREDIT_RATE_SCALE {
            return Err(V16Error::InvalidConfig);
        }
        let required_backing_num = Self::bound_num_from_amount(effective_credit)?;
        if credit_rate_num == CREDIT_RATE_SCALE {
            return Ok((required_backing_num, required_backing_num));
        }
        let required_face_num = checked_mul_div_ceil_u256(
            U256::from_u128(required_backing_num),
            U256::from_u128(CREDIT_RATE_SCALE),
            U256::from_u128(credit_rate_num),
        )
        .and_then(|v| v.try_into_u128())
        .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((required_face_num, required_backing_num))
    }

    #[inline]
    fn validate_bound_num_atom_aligned(bound_num: u128) -> V16Result<()> {
        if bound_num == 0 {
            return Ok(());
        }
        if bound_num % BOUND_SCALE != 0 {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    #[inline]
    fn validate_positive_pnl_source_attribution(
        pnl: i128,
        source_claim_sum_num: u128,
    ) -> V16Result<()> {
        if pnl <= 0 {
            return Ok(());
        }
        let required = Self::bound_num_from_amount(pnl as u128)?;
        if source_claim_sum_num < required {
            return Err(V16Error::InvalidLeg);
        }
        Ok(())
    }

    #[inline]
    fn accrual_activity_for_asset_segment(
        old: AssetStateV16,
        segment_dt: u64,
        effective_price: u64,
        funding_rate_e9: i128,
    ) -> AccrueAssetOutcomeV16 {
        let exposed = old.oi_eff_long_q != 0 || old.oi_eff_short_q != 0;
        let balanced_exposure = old.oi_eff_long_q != 0 && old.oi_eff_short_q != 0;
        let price_move_active = effective_price != old.effective_price && exposed;
        let funding_active =
            segment_dt > 0 && funding_rate_e9 != 0 && balanced_exposure && old.fund_px_last > 0;
        AccrueAssetOutcomeV16 {
            dt: segment_dt,
            price_move_active,
            funding_active,
            equity_active: price_move_active || funding_active,
            loss_stale_after: false,
        }
    }

    #[inline]
    fn liquidation_progress_from_scores(before: RiskScoreV16, after: RiskScoreV16) -> bool {
        after.strictly_reduces_from(before)
            || after.certified_liq_deficit < before.certified_liq_deficit
    }

    fn available_backing_num_for_source_credit_state(
        state: SourceCreditStateV16,
    ) -> V16Result<u128> {
        if state.fresh_reserved_backing_num < state.valid_liened_backing_num {
            return Err(V16Error::InvalidConfig);
        }
        let insurance_encumbered = state
            .valid_liened_insurance_num
            .checked_add(state.impaired_liened_insurance_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if state.insurance_credit_reserved_num < insurance_encumbered {
            return Err(V16Error::InvalidConfig);
        }
        (state.fresh_reserved_backing_num - state.valid_liened_backing_num)
            .checked_add(state.insurance_credit_reserved_num - insurance_encumbered)
            .ok_or(V16Error::ArithmeticOverflow)
    }

    fn expected_source_credit_rate_num_for_state(state: SourceCreditStateV16) -> V16Result<u128> {
        Self::validate_source_credit_state_shape_static(state)?;
        if state.positive_claim_bound_num == 0 {
            return Ok(CREDIT_RATE_SCALE);
        }
        let available = Self::available_backing_num_for_source_credit_state(state)?;
        let rate = U256::from_u128(available)
            .checked_mul(U256::from_u128(CREDIT_RATE_SCALE))
            .and_then(|v| v.checked_div(U256::from_u128(state.positive_claim_bound_num)))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(core::cmp::min(rate, CREDIT_RATE_SCALE))
    }

    fn validate_source_credit_state_shape_static(state: SourceCreditStateV16) -> V16Result<()> {
        if state == SourceCreditStateV16::EMPTY {
            return Ok(());
        }
        Self::validate_bound_num_atom_aligned(state.insurance_credit_reserved_num)?;
        Self::validate_bound_num_atom_aligned(state.valid_liened_insurance_num)?;
        Self::validate_bound_num_atom_aligned(state.impaired_liened_insurance_num)?;
        if state.exact_positive_claim_num > state.positive_claim_bound_num
            || state.credit_rate_num > CREDIT_RATE_SCALE
            || state.spent_backing_num < state.provider_receivable_num
        {
            return Err(V16Error::InvalidConfig);
        }
        Self::available_backing_num_for_source_credit_state(state).map(|_| ())
    }

    fn validate_source_credit_state_static(state: SourceCreditStateV16) -> V16Result<()> {
        Self::validate_source_credit_state_shape_static(state)?;
        if state.credit_rate_num != Self::expected_source_credit_rate_num_for_state(state)? {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn validate_backing_bucket_static(bucket: BackingBucketV16) -> V16Result<()> {
        match bucket.status {
            BackingBucketStatusV16::Empty => {
                if !bucket.is_empty_amount_shape() {
                    return Err(V16Error::InvalidConfig);
                }
            }
            BackingBucketStatusV16::Fresh => {
                if bucket.market_id == 0
                    || bucket.expiry_slot == 0
                    || bucket
                        .fresh_unliened_backing_num
                        .checked_add(bucket.valid_liened_backing_num)
                        .ok_or(V16Error::ArithmeticOverflow)?
                        == 0
                {
                    return Err(V16Error::InvalidConfig);
                }
            }
            BackingBucketStatusV16::Expired => {
                if bucket.market_id == 0
                    || bucket.fresh_unliened_backing_num != 0
                    || bucket.valid_liened_backing_num != 0
                    || bucket.impaired_liened_backing_num != 0
                {
                    return Err(V16Error::InvalidConfig);
                }
            }
            BackingBucketStatusV16::Impaired => {
                if bucket.market_id == 0
                    || bucket.fresh_unliened_backing_num != 0
                    || bucket.valid_liened_backing_num != 0
                    || bucket.impaired_liened_backing_num == 0
                {
                    return Err(V16Error::InvalidConfig);
                }
            }
        }
        Ok(())
    }

    fn validate_insurance_reservation_static(
        reservation: InsuranceCreditReservationV16,
    ) -> V16Result<()> {
        if reservation == InsuranceCreditReservationV16::EMPTY {
            return Ok(());
        }
        Self::validate_bound_num_atom_aligned(reservation.insurance_credit_reserved_num)?;
        Self::validate_bound_num_atom_aligned(reservation.valid_liened_insurance_num)?;
        Self::validate_bound_num_atom_aligned(reservation.impaired_liened_insurance_num)?;
        let encumbered = reservation
            .valid_liened_insurance_num
            .checked_add(reservation.impaired_liened_insurance_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if reservation.insurance_credit_reserved_num < encumbered {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn validate_source_domain_ledger_parts(
        expected_market_id: u64,
        source: SourceCreditStateV16,
        bucket: BackingBucketV16,
        reservation: InsuranceCreditReservationV16,
    ) -> V16Result<()> {
        Self::validate_source_credit_state_static(source)?;
        Self::validate_backing_bucket_static(bucket)?;
        Self::validate_insurance_reservation_static(reservation)?;
        if expected_market_id == 0 {
            if source == SourceCreditStateV16::EMPTY
                && bucket == BackingBucketV16::EMPTY
                && reservation == InsuranceCreditReservationV16::EMPTY
            {
                return Ok(());
            }
            return Err(V16Error::InvalidConfig);
        }
        if bucket.market_id != expected_market_id {
            return Err(V16Error::InvalidConfig);
        }
        let fresh_reserved = bucket
            .fresh_unliened_backing_num
            .checked_add(bucket.valid_liened_backing_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if source.fresh_reserved_backing_num != fresh_reserved
            || source.provider_receivable_num != bucket.consumed_liened_backing_num
            || source.valid_liened_backing_num != bucket.valid_liened_backing_num
            || source.impaired_liened_backing_num != bucket.impaired_liened_backing_num
            || source.insurance_credit_reserved_num != reservation.insurance_credit_reserved_num
            || source.valid_liened_insurance_num != reservation.valid_liened_insurance_num
            || source.impaired_liened_insurance_num != reservation.impaired_liened_insurance_num
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn prepare_source_credit_domain_recompute_for_epoch(
        mut source: SourceCreditStateV16,
        risk_epoch: u64,
    ) -> V16Result<(SourceCreditStateV16, u64)> {
        source.credit_rate_num = Self::expected_source_credit_rate_num_for_state(source)?;
        source.credit_epoch = source
            .credit_epoch
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((
            source,
            risk_epoch.checked_add(1).ok_or(V16Error::CounterOverflow)?,
        ))
    }

    fn prepare_source_positive_claim_bound_delta(
        mut source: SourceCreditStateV16,
        claim_bound_num: u128,
        exact_claim_num: u128,
    ) -> V16Result<SourceCreditStateV16> {
        if exact_claim_num > claim_bound_num {
            return Err(V16Error::InvalidConfig);
        }
        source.positive_claim_bound_num = source
            .positive_claim_bound_num
            .checked_add(claim_bound_num)
            .ok_or(V16Error::CounterOverflow)?;
        source.exact_positive_claim_num = source
            .exact_positive_claim_num
            .checked_add(exact_claim_num)
            .ok_or(V16Error::CounterOverflow)?;
        Ok(source)
    }

    fn prepare_counterparty_lien_create_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        current_slot: u64,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.status != BackingBucketStatusV16::Fresh
            || bucket.expiry_slot <= current_slot
            || bucket.fresh_unliened_backing_num < amount
        {
            return Err(V16Error::LockActive);
        }
        bucket.fresh_unliened_backing_num -= amount;
        bucket.valid_liened_backing_num = bucket
            .valid_liened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.valid_liened_backing_num = source
            .valid_liened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((bucket, source))
    }

    fn prepare_counterparty_backing_add_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        amount: u128,
        current_slot: u64,
        expiry_slot: u64,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 || expiry_slot <= current_slot {
            return Err(V16Error::InvalidConfig);
        }
        if source.provider_receivable_num != bucket.consumed_liened_backing_num
            || source.spent_backing_num < source.provider_receivable_num
        {
            return Err(V16Error::InvalidConfig);
        }
        match bucket.status {
            BackingBucketStatusV16::Empty | BackingBucketStatusV16::Expired => {
                bucket.status = BackingBucketStatusV16::Fresh;
                bucket.expiry_slot = expiry_slot;
            }
            BackingBucketStatusV16::Fresh if bucket.expiry_slot == expiry_slot => {}
            _ => return Err(V16Error::LockActive),
        }
        let refill = amount.min(source.provider_receivable_num);
        if refill > bucket.consumed_liened_backing_num {
            return Err(V16Error::CounterUnderflow);
        }
        bucket.consumed_liened_backing_num -= refill;
        source.provider_receivable_num -= refill;
        bucket.fresh_unliened_backing_num = bucket
            .fresh_unliened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.fresh_reserved_backing_num = source
            .fresh_reserved_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((bucket, source))
    }

    fn prepare_counterparty_lien_release_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        current_slot: u64,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.status != BackingBucketStatusV16::Fresh
            || bucket.expiry_slot <= current_slot
            || bucket.valid_liened_backing_num < amount
            || source.valid_liened_backing_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        bucket.valid_liened_backing_num -= amount;
        bucket.fresh_unliened_backing_num = bucket
            .fresh_unliened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.valid_liened_backing_num -= amount;
        Ok((bucket, source))
    }

    fn prepare_counterparty_lien_consume_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.valid_liened_backing_num < amount
            || source.valid_liened_backing_num < amount
            || source.fresh_reserved_backing_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        bucket.valid_liened_backing_num -= amount;
        bucket.consumed_liened_backing_num = bucket
            .consumed_liened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        if bucket.fresh_unliened_backing_num == 0
            && bucket.valid_liened_backing_num == 0
            && bucket.impaired_liened_backing_num == 0
        {
            bucket.status = BackingBucketStatusV16::Expired;
        }
        source.valid_liened_backing_num -= amount;
        source.fresh_reserved_backing_num -= amount;
        source.spent_backing_num = source
            .spent_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.provider_receivable_num = source
            .provider_receivable_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((bucket, source))
    }

    fn prepare_counterparty_lien_impair_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.valid_liened_backing_num < amount
            || source.valid_liened_backing_num < amount
            || source.fresh_reserved_backing_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        bucket.valid_liened_backing_num -= amount;
        bucket.impaired_liened_backing_num = bucket
            .impaired_liened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        if bucket.valid_liened_backing_num == 0 && bucket.fresh_unliened_backing_num == 0 {
            bucket.status = BackingBucketStatusV16::Impaired;
        }
        source.valid_liened_backing_num -= amount;
        source.fresh_reserved_backing_num -= amount;
        source.impaired_liened_backing_num = source
            .impaired_liened_backing_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((bucket, source))
    }

    fn prepare_insurance_lien_consume_delta(
        mut reservation: InsuranceCreditReservationV16,
        mut source: SourceCreditStateV16,
        domain_spent: u128,
        insurance: u128,
        amount: u128,
    ) -> V16Result<(
        InsuranceCreditReservationV16,
        SourceCreditStateV16,
        u128,
        u128,
    )> {
        if amount == 0 {
            return Ok((reservation, source, domain_spent, insurance));
        }
        Self::validate_bound_num_atom_aligned(amount)?;
        let spend_atoms = Self::amount_from_bound_num(amount)?;
        if reservation.valid_liened_insurance_num < amount
            || reservation.insurance_credit_reserved_num < amount
            || source.valid_liened_insurance_num < amount
            || source.insurance_credit_reserved_num < amount
            || insurance < spend_atoms
        {
            return Err(V16Error::CounterUnderflow);
        }
        reservation.valid_liened_insurance_num -= amount;
        reservation.insurance_credit_reserved_num -= amount;
        reservation.consumed_insurance_num = reservation
            .consumed_insurance_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.valid_liened_insurance_num -= amount;
        source.insurance_credit_reserved_num -= amount;
        Ok((
            reservation,
            source,
            domain_spent
                .checked_add(spend_atoms)
                .ok_or(V16Error::CounterOverflow)?,
            insurance - spend_atoms,
        ))
    }

    fn prepare_insurance_lien_create_delta(
        mut reservation: InsuranceCreditReservationV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((reservation, source));
        }
        Self::validate_bound_num_atom_aligned(amount)?;
        let encumbered = reservation
            .valid_liened_insurance_num
            .checked_add(reservation.impaired_liened_insurance_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if reservation
            .insurance_credit_reserved_num
            .checked_sub(encumbered)
            .ok_or(V16Error::CounterUnderflow)?
            < amount
        {
            return Err(V16Error::LockActive);
        }
        reservation.valid_liened_insurance_num = reservation
            .valid_liened_insurance_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.valid_liened_insurance_num = source
            .valid_liened_insurance_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((reservation, source))
    }

    fn prepare_insurance_lien_release_delta(
        mut reservation: InsuranceCreditReservationV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((reservation, source));
        }
        Self::validate_bound_num_atom_aligned(amount)?;
        if reservation.valid_liened_insurance_num < amount
            || source.valid_liened_insurance_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        reservation.valid_liened_insurance_num -= amount;
        source.valid_liened_insurance_num -= amount;
        Ok((reservation, source))
    }

    fn prepare_insurance_lien_impair_delta(
        mut reservation: InsuranceCreditReservationV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((reservation, source));
        }
        Self::validate_bound_num_atom_aligned(amount)?;
        if reservation.valid_liened_insurance_num < amount
            || source.valid_liened_insurance_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        reservation.valid_liened_insurance_num -= amount;
        reservation.impaired_liened_insurance_num = reservation
            .impaired_liened_insurance_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        source.valid_liened_insurance_num -= amount;
        source.impaired_liened_insurance_num = source
            .impaired_liened_insurance_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((reservation, source))
    }

    fn source_credit_state_realizable_support_for_face(
        state: SourceCreditStateV16,
        face_claim: u128,
    ) -> V16Result<u128> {
        if face_claim == 0 || state.positive_claim_bound_num == 0 {
            return Ok(0);
        }
        let credited_num = U256::from_u128(Self::bound_num_from_amount(face_claim)?)
            .checked_mul(U256::from_u128(state.credit_rate_num))
            .and_then(|v| v.checked_div(U256::from_u128(CREDIT_RATE_SCALE)))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((credited_num / BOUND_SCALE)
            .min(Self::available_backing_num_for_source_credit_state(state)? / BOUND_SCALE))
    }

    fn target_effective_lag_loss_penalty(
        abs_pos_q: u128,
        side: SideV16,
        effective_price: u64,
        raw_target_price: u64,
    ) -> V16Result<u128> {
        let adverse_delta =
            Self::target_effective_lag_adverse_delta(side, effective_price, raw_target_price);
        risk_notional_ceil(abs_pos_q, adverse_delta)
    }

    fn target_effective_lag_adverse_delta(
        side: SideV16,
        effective_price: u64,
        raw_target_price: u64,
    ) -> u64 {
        match side {
            SideV16::Long if raw_target_price < effective_price => {
                effective_price - raw_target_price
            }
            SideV16::Short if raw_target_price > effective_price => {
                raw_target_price - effective_price
            }
            _ => 0,
        }
    }

    fn health_requirements_from_notional_and_target_lag(
        config: V16Config,
        risk_notional: u128,
        target_lag_penalty: u128,
    ) -> V16Result<(u128, u128, u128)> {
        let initial_req = margin_requirement(
            risk_notional,
            config.initial_margin_bps,
            config.min_nonzero_im_req,
        )?
        .checked_add(target_lag_penalty)
        .ok_or(V16Error::ArithmeticOverflow)?;
        let maintenance_req = margin_requirement(
            risk_notional,
            config.maintenance_margin_bps,
            config.min_nonzero_mm_req,
        )?
        .checked_add(target_lag_penalty)
        .ok_or(V16Error::ArithmeticOverflow)?;
        let worst_case_loss = risk_notional
            .checked_add(target_lag_penalty)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((initial_req, maintenance_req, worst_case_loss))
    }

    fn backing_utilization_rate_e9_for_source_state(
        config: V16Config,
        source: SourceCreditStateV16,
    ) -> V16Result<u64> {
        config.validate_public_user_fund_shape()?;
        if source.valid_liened_backing_num == 0 {
            return Ok(0);
        }
        if source.fresh_reserved_backing_num == 0
            || source.valid_liened_backing_num > source.fresh_reserved_backing_num
        {
            return Err(V16Error::InvalidConfig);
        }
        let util_bps = U256::from_u128(source.valid_liened_backing_num)
            .checked_mul(U256::from_u64(MAX_BACKING_FEE_UTIL_BPS))
            .and_then(|v| v.checked_div(U256::from_u128(source.fresh_reserved_backing_num)))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)? as u64;
        let kink = config.backing_fee_kink_util_bps;
        let rate = if util_bps <= kink {
            let slope = U256::from_u64(config.backing_fee_slope_at_kink_e9_per_slot)
                .checked_mul(U256::from_u64(util_bps))
                .and_then(|v| v.checked_div(U256::from_u64(kink)))
                .and_then(|v| v.try_into_u128())
                .ok_or(V16Error::ArithmeticOverflow)? as u64;
            config
                .backing_fee_base_rate_e9_per_slot
                .checked_add(slope)
                .ok_or(V16Error::ArithmeticOverflow)?
        } else {
            let above_den = MAX_BACKING_FEE_UTIL_BPS
                .checked_sub(kink)
                .ok_or(V16Error::InvalidConfig)?;
            let above_num = util_bps.checked_sub(kink).ok_or(V16Error::InvalidConfig)?;
            let above_slope = U256::from_u64(config.backing_fee_slope_above_kink_e9_per_slot)
                .checked_mul(U256::from_u64(above_num))
                .and_then(|v| v.checked_div(U256::from_u64(above_den)))
                .and_then(|v| v.try_into_u128())
                .ok_or(V16Error::ArithmeticOverflow)? as u64;
            config
                .backing_fee_base_rate_e9_per_slot
                .checked_add(config.backing_fee_slope_at_kink_e9_per_slot)
                .and_then(|v| v.checked_add(above_slope))
                .ok_or(V16Error::ArithmeticOverflow)?
        };
        if rate > MAX_BACKING_FEE_RATE_E9_PER_SLOT {
            return Err(V16Error::InvalidConfig);
        }
        Ok(rate)
    }

    fn backing_utilization_fee_quote_atoms_for_lien(
        config: V16Config,
        source: SourceCreditStateV16,
        lien_backing_num: u128,
        from_slot: u64,
        to_slot: u64,
    ) -> V16Result<u128> {
        if lien_backing_num == 0 || to_slot <= from_slot {
            return Ok(0);
        }
        let rate = Self::backing_utilization_rate_e9_for_source_state(config, source)?;
        if rate == 0 {
            return Ok(0);
        }
        let den = BACKING_FEE_RATE_DEN_E9
            .checked_mul(BOUND_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        U256::from_u128(lien_backing_num)
            .checked_mul(U256::from_u64(rate))
            .and_then(|v| v.checked_mul(U256::from_u64(to_slot - from_slot)))
            .and_then(|v| v.checked_div(U256::from_u128(den)))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)
    }
}

#[cfg(kani)]
pub fn kani_validate_positive_pnl_source_attribution(
    pnl: i128,
    source_claim_sum_num: u128,
) -> V16Result<()> {
    V16Core::validate_positive_pnl_source_attribution(pnl, source_claim_sum_num)
}

#[cfg(kani)]
pub fn kani_expected_source_credit_rate_num_for_state(
    state: SourceCreditStateV16,
) -> V16Result<u128> {
    V16Core::expected_source_credit_rate_num_for_state(state)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HLockLaneV16 {
    HMin,
    HMax,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideV16 {
    Long,
    Short,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideModeV16 {
    Normal,
    DrainOnly,
    ResetPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetLifecycleV16 {
    Disabled,
    PendingActivation,
    Active,
    DrainOnly,
    Retired,
    Recovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketModeV16 {
    Live,
    Resolved,
    Recovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackingBucketStatusV16 {
    Empty,
    Fresh,
    Expired,
    Impaired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionlessRecoveryReasonV16 {
    BelowProgressFloor,
    BlockedSegmentHeadroomOrRepresentability,
    AccountBSettlementCannotProgress,
    BIndexHeadroomExhausted,
    ActiveBankruptCloseCannotProgress,
    ExplicitLossOrDustAuditOverflow,
    OracleOrTargetUnavailableByAuthenticatedPolicy,
    CounterOrEpochOverflowDeclaredRecovery,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvenanceHeaderV16 {
    pub market_group_id: [u8; 32],
    pub portfolio_account_id: [u8; 32],
    pub owner: [u8; 32],
    pub version: u16,
    pub layout_discriminator: u16,
}

impl ProvenanceHeaderV16 {
    pub const fn new(
        market_group_id: [u8; 32],
        portfolio_account_id: [u8; 32],
        owner: [u8; 32],
    ) -> Self {
        Self {
            market_group_id,
            portfolio_account_id,
            owner,
            version: V16_ACCOUNT_VERSION,
            layout_discriminator: V16_LAYOUT_DISCRIMINATOR,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct V16Config {
    pub max_portfolio_assets: u16,
    pub max_market_slots: u32,
    pub min_nonzero_mm_req: u128,
    pub min_nonzero_im_req: u128,
    pub h_min: u64,
    pub h_max: u64,
    pub maintenance_margin_bps: u64,
    pub initial_margin_bps: u64,
    pub max_trading_fee_bps: u64,
    pub liquidation_fee_bps: u64,
    pub liquidation_fee_cap: u128,
    pub min_liquidation_abs: u128,
    pub max_accrual_dt_slots: u64,
    pub max_abs_funding_e9_per_slot: u64,
    pub min_funding_lifetime_slots: u64,
    pub max_price_move_bps_per_slot: u64,
    pub max_account_b_settlement_chunks: u64,
    pub max_bankrupt_close_chunks: u64,
    pub max_bankrupt_close_lifetime_slots: u64,
    pub asset_activation_cooldown_slots: u64,
    pub public_b_chunk_atoms: u128,
    pub max_recovery_fallback_deviation_bps: u64,
    pub backing_fee_base_rate_e9_per_slot: u64,
    pub backing_fee_kink_util_bps: u64,
    pub backing_fee_slope_at_kink_e9_per_slot: u64,
    pub backing_fee_slope_above_kink_e9_per_slot: u64,
    pub backing_freshness_buckets: u8,
    pub margin_mode_realizable_full_shared_cross_margin: bool,
    pub source_credit_lien_required: bool,
    pub insurance_credit_reservation_required: bool,
    pub permissionless_recovery_enabled: bool,
    pub recovery_fallback_price_enabled: bool,
    pub recovery_fallback_envelope_enabled: bool,
    pub credit_lien_revalidation_required: bool,
    pub stale_certificate_penalty_enabled: bool,
    pub full_refresh_required_for_favorable_actions: bool,
    pub public_liveness_profile_crank_forward: bool,
}

impl V16Config {
    pub const fn public_user_fund(max_portfolio_assets: u16, h_min: u64, h_max: u64) -> Self {
        Self::public_user_fund_with_market_slots(
            max_portfolio_assets,
            max_portfolio_assets as u32,
            h_min,
            h_max,
        )
    }

    pub const fn public_user_fund_with_market_slots(
        max_portfolio_assets: u16,
        max_market_slots: u32,
        h_min: u64,
        h_max: u64,
    ) -> Self {
        Self {
            max_portfolio_assets,
            max_market_slots,
            min_nonzero_mm_req: 1,
            min_nonzero_im_req: 2,
            h_min,
            h_max,
            maintenance_margin_bps: 10_000,
            initial_margin_bps: 10_000,
            max_trading_fee_bps: 0,
            liquidation_fee_bps: 0,
            liquidation_fee_cap: 0,
            min_liquidation_abs: 0,
            max_accrual_dt_slots: 1,
            max_abs_funding_e9_per_slot: 0,
            min_funding_lifetime_slots: 1,
            max_price_move_bps_per_slot: 10_000,
            max_account_b_settlement_chunks: 1,
            max_bankrupt_close_chunks: 1,
            max_bankrupt_close_lifetime_slots: 1,
            asset_activation_cooldown_slots: 1,
            public_b_chunk_atoms: MAX_VAULT_TVL,
            max_recovery_fallback_deviation_bps: MAX_RECOVERY_FALLBACK_DEVIATION_BPS,
            backing_fee_base_rate_e9_per_slot: 0,
            backing_fee_kink_util_bps: 8_000,
            backing_fee_slope_at_kink_e9_per_slot: 0,
            backing_fee_slope_above_kink_e9_per_slot: 0,
            backing_freshness_buckets: V16_BACKING_BUCKETS_PER_DOMAIN as u8,
            margin_mode_realizable_full_shared_cross_margin: true,
            source_credit_lien_required: true,
            insurance_credit_reservation_required: true,
            permissionless_recovery_enabled: true,
            recovery_fallback_price_enabled: true,
            recovery_fallback_envelope_enabled: true,
            credit_lien_revalidation_required: true,
            stale_certificate_penalty_enabled: true,
            full_refresh_required_for_favorable_actions: true,
            public_liveness_profile_crank_forward: true,
        }
    }

    fn ceil_div_u256_to_u128(n: U256, d: U256) -> V16Result<u128> {
        if d.is_zero() {
            return Err(V16Error::InvalidConfig);
        }
        if let (Some(n), Some(d)) = (n.try_into_u128(), d.try_into_u128()) {
            if d == 0 {
                return Err(V16Error::InvalidConfig);
            }
            let q = n / d;
            let r = n % d;
            return q
                .checked_add(u128::from(r != 0))
                .ok_or(V16Error::InvalidConfig);
        }
        let q = n.checked_div(d).ok_or(V16Error::InvalidConfig)?;
        let r = n.checked_rem(d).ok_or(V16Error::InvalidConfig)?;
        let q = if r.is_zero() {
            q
        } else {
            q.checked_add(U256::ONE).ok_or(V16Error::InvalidConfig)?
        };
        q.try_into_u128().ok_or(V16Error::InvalidConfig)
    }

    fn checked_mul_div_ceil_to_u128(a: u128, b: u128, d: u128) -> V16Result<u128> {
        if d == 0 {
            return Err(V16Error::InvalidConfig);
        }
        if let Some(product) = a.checked_mul(b) {
            let q = product / d;
            let r = product % d;
            return q
                .checked_add(u128::from(r != 0))
                .ok_or(V16Error::InvalidConfig);
        }
        checked_mul_div_ceil_u256(U256::from_u128(a), U256::from_u128(b), U256::from_u128(d))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::InvalidConfig)
    }

    fn solvency_envelope_total_for_notional(
        &self,
        n: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> V16Result<u128> {
        let loss = Self::checked_mul_div_ceil_to_u128(n, loss_budget_num, loss_budget_den)?;

        let worst_liq_multiplier = 10_000u128
            .checked_add(price_budget_bps)
            .ok_or(V16Error::InvalidConfig)?;
        let worst_liq_notional =
            Self::checked_mul_div_ceil_to_u128(n, worst_liq_multiplier, 10_000)?;
        let liq_fee_raw = Self::checked_mul_div_ceil_to_u128(
            worst_liq_notional,
            self.liquidation_fee_bps as u128,
            10_000,
        )?;
        let liq_fee = core::cmp::min(
            core::cmp::max(liq_fee_raw, self.min_liquidation_abs),
            self.liquidation_fee_cap,
        );

        loss.checked_add(liq_fee).ok_or(V16Error::InvalidConfig)
    }

    fn maintenance_requirement_for_notional(&self, n: u128) -> V16Result<u128> {
        let mm_prop = if let Some(product) = n.checked_mul(self.maintenance_margin_bps as u128) {
            product / 10_000
        } else {
            U256::from_u128(n)
                .checked_mul(U256::from_u128(self.maintenance_margin_bps as u128))
                .and_then(|v| v.checked_div(U256::from_u128(10_000)))
                .and_then(|v| v.try_into_u128())
                .ok_or(V16Error::InvalidConfig)?
        };
        Ok(core::cmp::max(mm_prop, self.min_nonzero_mm_req))
    }

    fn solvency_envelope_holds_for_notional(
        &self,
        n: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> V16Result<bool> {
        let total = self.solvency_envelope_total_for_notional(
            n,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )?;
        let mm_req = self.maintenance_requirement_for_notional(n)?;
        Ok(total <= mm_req)
    }

    fn solvency_envelope_interval_certifies(
        &self,
        lo: u128,
        hi: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> V16Result<bool> {
        let total_hi = self.solvency_envelope_total_for_notional(
            hi,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )?;
        let mm_lo = self.maintenance_requirement_for_notional(lo)?;
        Ok(total_hi <= mm_lo)
    }

    fn validate_solvency_envelope_range(
        &self,
        lo: u128,
        hi: u128,
        loss_budget_num: u128,
        loss_budget_den: u128,
        price_budget_bps: u128,
    ) -> V16Result<()> {
        if lo > hi {
            return Ok(());
        }

        const MAX_SOLVENCY_INTERVALS: usize = 96;
        const MAX_SOLVENCY_STEPS: usize = 4096;
        const EXACT_CHUNK: u128 = 64;

        let mut stack = [(0u128, 0u128); MAX_SOLVENCY_INTERVALS];
        let mut len = 1usize;
        let mut steps = 0usize;
        stack[0] = (lo, hi);

        while len != 0 {
            steps = steps.checked_add(1).ok_or(V16Error::InvalidConfig)?;
            if steps > MAX_SOLVENCY_STEPS {
                return Err(V16Error::InvalidConfig);
            }

            len -= 1;
            let (range_lo, range_hi) = stack[len];

            if self.solvency_envelope_interval_certifies(
                range_lo,
                range_hi,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            )? {
                continue;
            }

            if range_hi == range_lo || range_hi - range_lo <= EXACT_CHUNK {
                let mut n = range_lo;
                loop {
                    if !self.solvency_envelope_holds_for_notional(
                        n,
                        loss_budget_num,
                        loss_budget_den,
                        price_budget_bps,
                    )? {
                        return Err(V16Error::InvalidConfig);
                    }
                    if n == range_hi {
                        break;
                    }
                    n = n.checked_add(1).ok_or(V16Error::InvalidConfig)?;
                }
                continue;
            }

            let mid = range_lo + (range_hi - range_lo) / 2;
            if len + 2 > MAX_SOLVENCY_INTERVALS {
                return Err(V16Error::InvalidConfig);
            }
            stack[len] = (mid.checked_add(1).ok_or(V16Error::InvalidConfig)?, range_hi);
            stack[len + 1] = (range_lo, mid);
            len += 2;
        }

        Ok(())
    }

    fn validate_funding_headroom(&self, slots: u64) -> V16Result<()> {
        let max_signed = U256::from_u128(i128::MAX as u128);
        let headroom = U256::from_u128(ADL_ONE)
            .checked_mul(U256::from_u128(MAX_ORACLE_PRICE as u128))
            .and_then(|v| v.checked_mul(U256::from_u128(self.max_abs_funding_e9_per_slot as u128)))
            .and_then(|v| v.checked_mul(U256::from_u128(slots as u128)))
            .ok_or(V16Error::InvalidConfig)?;
        if headroom <= max_signed {
            Ok(())
        } else {
            Err(V16Error::InvalidConfig)
        }
    }

    fn validate_exact_solvency_envelope(&self) -> V16Result<()> {
        let price_budget_fast = (self.max_price_move_bps_per_slot as u128)
            .checked_mul(self.max_accrual_dt_slots as u128)
            .ok_or(V16Error::InvalidConfig)?;
        if self.maintenance_margin_bps == 10_000
            && price_budget_fast <= 10_000
            && self.max_abs_funding_e9_per_slot == 0
            && self.liquidation_fee_bps == 0
            && self.min_liquidation_abs == 0
        {
            return Ok(());
        }

        self.validate_funding_headroom(self.max_accrual_dt_slots)?;
        self.validate_funding_headroom(self.min_funding_lifetime_slots)?;

        let move_cap = U256::from_u128(self.max_price_move_bps_per_slot as u128);
        let dt = U256::from_u128(self.max_accrual_dt_slots as u128);
        let rate = U256::from_u128(self.max_abs_funding_e9_per_slot as u128);
        let ten_thousand = U256::from_u128(10_000);
        let funding_den = U256::from_u128(FUNDING_DEN);

        let price_budget_bps = move_cap
            .checked_mul(dt)
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::InvalidConfig)?;
        let funding_budget_num = rate
            .checked_mul(dt)
            .and_then(|v| v.checked_mul(ten_thousand))
            .ok_or(V16Error::InvalidConfig)?;
        let loss_budget_num_wide = U256::from_u128(price_budget_bps)
            .checked_mul(funding_den)
            .and_then(|v| v.checked_add(funding_budget_num))
            .ok_or(V16Error::InvalidConfig)?;
        let loss_budget_den_wide = ten_thousand
            .checked_mul(funding_den)
            .ok_or(V16Error::InvalidConfig)?;

        let funding_budget_bps_ceil = Self::ceil_div_u256_to_u128(funding_budget_num, funding_den)?;
        let loss_budget_bps_ceil = price_budget_bps
            .checked_add(funding_budget_bps_ceil)
            .ok_or(V16Error::InvalidConfig)?;
        let worst_liq_budget_bps_ceil = Self::ceil_div_u256_to_u128(
            U256::from_u128(
                10_000u128
                    .checked_add(price_budget_bps)
                    .ok_or(V16Error::InvalidConfig)?,
            )
            .checked_mul(U256::from_u128(self.liquidation_fee_bps as u128))
            .ok_or(V16Error::InvalidConfig)?,
            ten_thousand,
        )?;
        let linear_budget_bps = loss_budget_bps_ceil
            .checked_add(worst_liq_budget_bps_ceil)
            .ok_or(V16Error::InvalidConfig)?;

        if self.maintenance_margin_bps == 10_000
            && loss_budget_bps_ceil == 10_000
            && worst_liq_budget_bps_ceil == 0
            && self.min_liquidation_abs == 0
        {
            return Ok(());
        }

        let loss_budget_num = loss_budget_num_wide
            .try_into_u128()
            .ok_or(V16Error::InvalidConfig)?;
        let loss_budget_den = loss_budget_den_wide
            .try_into_u128()
            .ok_or(V16Error::InvalidConfig)?;
        let domain_max = MAX_ACCOUNT_NOTIONAL;

        if self.maintenance_margin_bps == 0 {
            if self.solvency_envelope_holds_for_notional(
                domain_max,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            )? {
                return Ok(());
            }
            return Err(V16Error::InvalidConfig);
        }

        let floor_region_max = U256::from_u128(
            self.min_nonzero_mm_req
                .checked_add(1)
                .ok_or(V16Error::InvalidConfig)?,
        )
        .checked_mul(ten_thousand)
        .and_then(|v| v.checked_sub(U256::ONE))
        .and_then(|v| v.checked_div(U256::from_u128(self.maintenance_margin_bps as u128)))
        .and_then(|v| v.try_into_u128())
        .ok_or(V16Error::InvalidConfig)?;
        let floor_region_end = core::cmp::min(floor_region_max, domain_max);
        if floor_region_end != 0
            && !self.solvency_envelope_holds_for_notional(
                floor_region_end,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            )?
        {
            return Err(V16Error::InvalidConfig);
        }
        if floor_region_max >= domain_max {
            return Ok(());
        }

        let exact_start = floor_region_end
            .checked_add(1)
            .ok_or(V16Error::InvalidConfig)?;

        if linear_budget_bps < self.maintenance_margin_bps as u128 {
            let slope_gap = (self.maintenance_margin_bps as u128) - linear_budget_bps;
            let tail_for_linear = Self::ceil_div_u256_to_u128(
                U256::from_u128(3 * 10_000),
                U256::from_u128(slope_gap),
            )?;

            let loss_gap = (self.maintenance_margin_bps as u128)
                .checked_sub(loss_budget_bps_ceil)
                .ok_or(V16Error::InvalidConfig)?;
            let floor_fee_slack = self
                .min_liquidation_abs
                .checked_add(2)
                .ok_or(V16Error::InvalidConfig)?;
            let tail_for_fee_floor = Self::ceil_div_u256_to_u128(
                U256::from_u128(floor_fee_slack)
                    .checked_mul(ten_thousand)
                    .ok_or(V16Error::InvalidConfig)?,
                U256::from_u128(loss_gap),
            )?;

            let exact_tail = core::cmp::max(tail_for_linear, tail_for_fee_floor);
            if exact_tail <= exact_start {
                return Ok(());
            }
            let exact_end = core::cmp::min(exact_tail.saturating_sub(1), domain_max);
            return self.validate_solvency_envelope_range(
                exact_start,
                exact_end,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            );
        }

        if loss_budget_bps_ceil >= self.maintenance_margin_bps as u128 {
            return self.validate_solvency_envelope_range(
                exact_start,
                domain_max,
                loss_budget_num,
                loss_budget_den,
                price_budget_bps,
            );
        }

        let slope_gap = (self.maintenance_margin_bps as u128) - loss_budget_bps_ceil;
        let capped_fee_slack = self
            .liquidation_fee_cap
            .checked_add(3)
            .ok_or(V16Error::InvalidConfig)?;
        let exact_tail = Self::ceil_div_u256_to_u128(
            U256::from_u128(capped_fee_slack)
                .checked_mul(ten_thousand)
                .ok_or(V16Error::InvalidConfig)?,
            U256::from_u128(slope_gap),
        )?;

        if exact_tail <= exact_start {
            return Ok(());
        }

        let exact_end = core::cmp::min(exact_tail.saturating_sub(1), domain_max);
        self.validate_solvency_envelope_range(
            exact_start,
            exact_end,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )
    }

    pub fn validate_public_user_fund_shape(&self) -> V16Result<()> {
        if self.max_portfolio_assets == 0
            || self.max_portfolio_assets as usize > V16_MAX_PORTFOLIO_ASSETS_N
            || self.max_market_slots == 0
            || self.max_portfolio_assets as u32 > self.max_market_slots
        {
            return Err(V16Error::InvalidConfig);
        }
        if self.h_max == 0 || self.h_min > self.h_max {
            return Err(V16Error::InvalidConfig);
        }
        if self.min_nonzero_mm_req == 0 || self.min_nonzero_mm_req >= self.min_nonzero_im_req {
            return Err(V16Error::InvalidConfig);
        }
        if self.maintenance_margin_bps > self.initial_margin_bps
            || self.initial_margin_bps > MAX_MARGIN_BPS
            || self.max_trading_fee_bps > MAX_MARGIN_BPS
            || self.liquidation_fee_bps > MAX_MARGIN_BPS
            || self.min_liquidation_abs > self.liquidation_fee_cap
            || self.liquidation_fee_cap > MAX_PROTOCOL_FEE_ABS
            || self.max_accrual_dt_slots == 0
            || self.min_funding_lifetime_slots < self.max_accrual_dt_slots
            || self.max_abs_funding_e9_per_slot > 10_000
            || self.max_price_move_bps_per_slot == 0
            || self.max_account_b_settlement_chunks == 0
            || self.max_bankrupt_close_chunks == 0
            || self.max_bankrupt_close_lifetime_slots == 0
            || self.asset_activation_cooldown_slots == 0
            || self.public_b_chunk_atoms == 0
            || self.max_recovery_fallback_deviation_bps > MAX_RECOVERY_FALLBACK_DEVIATION_BPS
            || self.backing_fee_kink_util_bps == 0
            || self.backing_fee_kink_util_bps >= MAX_BACKING_FEE_UTIL_BPS
            || self.backing_freshness_buckets == 0
            || self.backing_freshness_buckets as usize > V16_BACKING_BUCKETS_PER_DOMAIN
        {
            return Err(V16Error::InvalidConfig);
        }
        if self
            .backing_fee_base_rate_e9_per_slot
            .checked_add(self.backing_fee_slope_at_kink_e9_per_slot)
            .and_then(|v| v.checked_add(self.backing_fee_slope_above_kink_e9_per_slot))
            .ok_or(V16Error::InvalidConfig)?
            > MAX_BACKING_FEE_RATE_E9_PER_SLOT
        {
            return Err(V16Error::InvalidConfig);
        }
        if !self.margin_mode_realizable_full_shared_cross_margin
            || !self.source_credit_lien_required
            || !self.insurance_credit_reservation_required
            || !self.permissionless_recovery_enabled
            || !self.recovery_fallback_price_enabled
            || !self.recovery_fallback_envelope_enabled
            || !self.credit_lien_revalidation_required
            || !self.stale_certificate_penalty_enabled
            || !self.full_refresh_required_for_favorable_actions
            || !self.public_liveness_profile_crank_forward
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    pub fn validate_public_user_fund(&self) -> V16Result<()> {
        self.validate_public_user_fund_shape()?;
        self.validate_exact_solvency_envelope()
    }

    #[cfg(kani)]
    pub fn kani_solvency_envelope_holds_for_notional(&self, n: u128) -> V16Result<bool> {
        self.validate_funding_headroom(self.max_accrual_dt_slots)?;
        self.validate_funding_headroom(self.min_funding_lifetime_slots)?;
        let price_budget_bps = (self.max_price_move_bps_per_slot as u128)
            .checked_mul(self.max_accrual_dt_slots as u128)
            .ok_or(V16Error::InvalidConfig)?;
        let funding_budget_num = (self.max_abs_funding_e9_per_slot as u128)
            .checked_mul(self.max_accrual_dt_slots as u128)
            .and_then(|v| v.checked_mul(10_000))
            .ok_or(V16Error::InvalidConfig)?;
        let loss_budget_num = price_budget_bps
            .checked_mul(FUNDING_DEN)
            .and_then(|v| v.checked_add(funding_budget_num))
            .ok_or(V16Error::InvalidConfig)?;
        let loss_budget_den = 10_000u128
            .checked_mul(FUNDING_DEN)
            .ok_or(V16Error::InvalidConfig)?;
        self.solvency_envelope_holds_for_notional(
            n,
            loss_budget_num,
            loss_budget_den,
            price_budget_bps,
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetStateV16 {
    pub market_id: u64,
    pub retired_slot: u64,
    pub lifecycle: AssetLifecycleV16,
    pub raw_oracle_target_price: u64,
    pub effective_price: u64,
    pub fund_px_last: u64,
    pub slot_last: u64,
    pub a_long: u128,
    pub a_short: u128,
    pub k_long: i128,
    pub k_short: i128,
    pub f_long_num: i128,
    pub f_short_num: i128,
    pub k_epoch_start_long: i128,
    pub k_epoch_start_short: i128,
    pub f_epoch_start_long_num: i128,
    pub f_epoch_start_short_num: i128,
    pub b_long_num: u128,
    pub b_short_num: u128,
    pub b_epoch_start_long_num: u128,
    pub b_epoch_start_short_num: u128,
    pub oi_eff_long_q: u128,
    pub oi_eff_short_q: u128,
    pub stored_pos_count_long: u64,
    pub stored_pos_count_short: u64,
    pub stale_account_count_long: u64,
    pub stale_account_count_short: u64,
    pub pending_obligation_count_long: u64,
    pub pending_obligation_count_short: u64,
    pub loss_weight_sum_long: u128,
    pub loss_weight_sum_short: u128,
    pub social_loss_remainder_long_num: u128,
    pub social_loss_remainder_short_num: u128,
    pub social_loss_dust_long_num: u128,
    pub social_loss_dust_short_num: u128,
    pub explicit_unallocated_loss_long: u128,
    pub explicit_unallocated_loss_short: u128,
    pub epoch_long: u64,
    pub epoch_short: u64,
    pub mode_long: SideModeV16,
    pub mode_short: SideModeV16,
}

impl Default for AssetStateV16 {
    fn default() -> Self {
        Self {
            market_id: 0,
            retired_slot: 0,
            lifecycle: AssetLifecycleV16::Active,
            raw_oracle_target_price: 1,
            effective_price: 1,
            fund_px_last: 1,
            slot_last: 0,
            a_long: ADL_ONE,
            a_short: ADL_ONE,
            k_long: 0,
            k_short: 0,
            f_long_num: 0,
            f_short_num: 0,
            k_epoch_start_long: 0,
            k_epoch_start_short: 0,
            f_epoch_start_long_num: 0,
            f_epoch_start_short_num: 0,
            b_long_num: 0,
            b_short_num: 0,
            b_epoch_start_long_num: 0,
            b_epoch_start_short_num: 0,
            oi_eff_long_q: 0,
            oi_eff_short_q: 0,
            stored_pos_count_long: 0,
            stored_pos_count_short: 0,
            stale_account_count_long: 0,
            stale_account_count_short: 0,
            pending_obligation_count_long: 0,
            pending_obligation_count_short: 0,
            loss_weight_sum_long: 0,
            loss_weight_sum_short: 0,
            social_loss_remainder_long_num: 0,
            social_loss_remainder_short_num: 0,
            social_loss_dust_long_num: 0,
            social_loss_dust_short_num: 0,
            explicit_unallocated_loss_long: 0,
            explicit_unallocated_loss_short: 0,
            epoch_long: 0,
            epoch_short: 0,
            mode_long: SideModeV16::Normal,
            mode_short: SideModeV16::Normal,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCreditStateV16 {
    pub positive_claim_bound_num: u128,
    pub exact_positive_claim_num: u128,
    pub fresh_reserved_backing_num: u128,
    pub spent_backing_num: u128,
    pub provider_receivable_num: u128,
    pub valid_liened_backing_num: u128,
    pub impaired_liened_backing_num: u128,
    pub insurance_credit_reserved_num: u128,
    pub valid_liened_insurance_num: u128,
    pub impaired_liened_insurance_num: u128,
    pub credit_rate_num: u128,
    pub credit_epoch: u64,
}

impl SourceCreditStateV16 {
    pub const EMPTY: Self = Self {
        positive_claim_bound_num: 0,
        exact_positive_claim_num: 0,
        fresh_reserved_backing_num: 0,
        spent_backing_num: 0,
        provider_receivable_num: 0,
        valid_liened_backing_num: 0,
        impaired_liened_backing_num: 0,
        insurance_credit_reserved_num: 0,
        valid_liened_insurance_num: 0,
        impaired_liened_insurance_num: 0,
        credit_rate_num: CREDIT_RATE_SCALE,
        credit_epoch: 0,
    };
}

impl Default for SourceCreditStateV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackingBucketV16 {
    pub market_id: u64,
    pub fresh_unliened_backing_num: u128,
    pub valid_liened_backing_num: u128,
    pub consumed_liened_backing_num: u128,
    pub impaired_liened_backing_num: u128,
    pub utilization_fee_earnings: u128,
    pub expiry_slot: u64,
    pub status: BackingBucketStatusV16,
}

impl BackingBucketV16 {
    pub const EMPTY: Self = Self {
        market_id: 0,
        fresh_unliened_backing_num: 0,
        valid_liened_backing_num: 0,
        consumed_liened_backing_num: 0,
        impaired_liened_backing_num: 0,
        utilization_fee_earnings: 0,
        expiry_slot: 0,
        status: BackingBucketStatusV16::Empty,
    };

    pub const fn empty_for_market(market_id: u64) -> Self {
        Self {
            market_id,
            fresh_unliened_backing_num: 0,
            valid_liened_backing_num: 0,
            consumed_liened_backing_num: 0,
            impaired_liened_backing_num: 0,
            utilization_fee_earnings: 0,
            expiry_slot: 0,
            status: BackingBucketStatusV16::Empty,
        }
    }

    fn is_empty_amount_shape(self) -> bool {
        self.fresh_unliened_backing_num == 0
            && self.valid_liened_backing_num == 0
            && self.consumed_liened_backing_num == 0
            && self.impaired_liened_backing_num == 0
            && self.utilization_fee_earnings == 0
            && self.expiry_slot == 0
            && self.status == BackingBucketStatusV16::Empty
    }
}

impl Default for BackingBucketV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsuranceCreditReservationV16 {
    pub insurance_credit_reserved_num: u128,
    pub valid_liened_insurance_num: u128,
    pub impaired_liened_insurance_num: u128,
    pub consumed_insurance_num: u128,
    pub source_credit_epoch: u64,
}

impl InsuranceCreditReservationV16 {
    pub const EMPTY: Self = Self {
        insurance_credit_reserved_num: 0,
        valid_liened_insurance_num: 0,
        impaired_liened_insurance_num: 0,
        consumed_insurance_num: 0,
        source_credit_epoch: 0,
    };
}

impl Default for InsuranceCreditReservationV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Wrapper-owned bytes embedded beside the engine market slot.
///
/// # Safety
///
/// Implementors must be valid `bytemuck::Pod` / `Zeroable` values, and
/// `Market<Self>` must not contain inter-field or trailing padding. A wrapper
/// can satisfy this by using an alignment-1 byte wrapper, or by proving its
/// alignment divides `size_of::<EngineAssetSlotV16Account>()`.
#[allow(unsafe_code)]
pub unsafe trait MarketWrapperPod: bytemuck::Pod + bytemuck::Zeroable {}

#[allow(unsafe_code)]
unsafe impl MarketWrapperPod for () {}
#[allow(unsafe_code)]
unsafe impl MarketWrapperPod for u8 {}

macro_rules! impl_market_wrapper_pod_for_byte_arrays {
    ($($n:expr),* $(,)?) => {
        $(
            #[allow(unsafe_code)]
            unsafe impl MarketWrapperPod for [u8; $n] {}
        )*
    };
}

impl_market_wrapper_pod_for_byte_arrays!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 64, 128, 256, 512, 1024,
);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Market<T> {
    pub wrapper: T,
    pub engine: EngineAssetSlotV16Account,
}

#[allow(unsafe_code)]
unsafe impl<T: MarketWrapperPod> bytemuck::Zeroable for Market<T> {}
#[allow(unsafe_code)]
unsafe impl<T: MarketWrapperPod> bytemuck::Pod for Market<T> {}

impl<T> Market<T> {
    pub const fn new(wrapper: T, engine: EngineAssetSlotV16Account) -> Self {
        Self { wrapper, engine }
    }
}

impl<T> MarketSlotV16View for Market<T> {
    fn engine_slot(&self) -> &EngineAssetSlotV16Account {
        &self.engine
    }
}

impl<T> MarketSlotV16ViewMut for Market<T> {
    fn engine_slot_mut(&mut self) -> &mut EngineAssetSlotV16Account {
        &mut self.engine
    }
}

pub struct MarketGroupV16View<'a, T> {
    pub header: &'a MarketGroupV16HeaderAccount,
    pub markets: &'a [Market<T>],
}

pub struct MarketGroupV16ViewMut<'a, T> {
    pub header: &'a mut MarketGroupV16HeaderAccount,
    pub markets: &'a mut [Market<T>],
}

impl<'a, T> MarketGroupV16View<'a, T> {
    pub fn new(header: &'a MarketGroupV16HeaderAccount, markets: &'a [Market<T>]) -> Self {
        Self { header, markets }
    }
}

impl<'a, T> MarketGroupV16ViewMut<'a, T> {
    pub fn new(header: &'a mut MarketGroupV16HeaderAccount, markets: &'a mut [Market<T>]) -> Self {
        Self { header, markets }
    }

    pub fn as_view(&self) -> MarketGroupV16View<'_, T> {
        MarketGroupV16View {
            header: self.header,
            markets: self.markets,
        }
    }
}

pub struct PortfolioV16View<'a> {
    pub header: &'a PortfolioAccountV16Account,
    pub source_domains: &'a [PortfolioSourceDomainV16Account],
}

pub struct PortfolioV16ViewMut<'a> {
    pub header: &'a mut PortfolioAccountV16Account,
    pub source_domains: &'a mut [PortfolioSourceDomainV16Account],
}

impl<'a> PortfolioV16View<'a> {
    pub fn new(
        header: &'a PortfolioAccountV16Account,
        source_domains: &'a [PortfolioSourceDomainV16Account],
    ) -> Self {
        Self {
            header,
            source_domains,
        }
    }
}

impl<'a> PortfolioV16ViewMut<'a> {
    pub fn new(
        header: &'a mut PortfolioAccountV16Account,
        source_domains: &'a mut [PortfolioSourceDomainV16Account],
    ) -> Self {
        Self {
            header,
            source_domains,
        }
    }

    pub fn as_view(&self) -> PortfolioV16View<'_> {
        PortfolioV16View {
            header: self.header,
            source_domains: self.source_domains,
        }
    }
}

impl<'a> PortfolioV16View<'a> {
    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {
        let config = market.header.config.try_to_runtime_shape()?;
        if self.header.provenance_header.market_group_id != market.header.market_group_id
            || self.header.owner != self.header.provenance_header.owner
            || self.header.provenance_header.version.get() != V16_ACCOUNT_VERSION
            || self.header.provenance_header.layout_discriminator.get() != V16_LAYOUT_DISCRIMINATOR
        {
            return Err(V16Error::ProvenanceMismatch);
        }
        let pnl = self.header.pnl.get();
        validate_non_min_i128(pnl)?;
        validate_fee_credits(self.header.fee_credits.get())?;
        if self.header.reserved_pnl.get() > pnl.max(0) as u128 {
            return Err(V16Error::InvalidLeg);
        }
        self.validate_source_credit_shape_with_market(market)?;
        let source_claim_sum_num = self.source_claim_bound_sum_num()?;
        if source_claim_sum_num != 0 {
            V16Core::validate_positive_pnl_source_attribution(pnl, source_claim_sum_num)?;
        }
        Self::validate_resolved_payout_receipt_static(
            self.header.resolved_payout_receipt.try_to_runtime()?,
        )?;
        self.validate_close_progress_ledger_with_market(market)?;

        let active_leg_cap = config.max_portfolio_assets as usize;
        let configured_assets = config.max_market_slots as usize;
        let bitmap = self.header.active_bitmap.map(V16PodU64::get);
        let mut seen_assets = [u32::MAX; V16_MAX_PORTFOLIO_ASSETS_N];
        let mut seen_asset_count = 0usize;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let bit = active_bitmap_get(bitmap, slot);
            let leg = self.header.legs[slot].try_to_runtime()?;
            if slot >= active_leg_cap {
                if bit || !leg.is_empty() {
                    return Err(V16Error::HiddenLeg);
                }
                slot += 1;
                continue;
            }
            if bit != leg.active {
                return Err(V16Error::HiddenLeg);
            }
            if !leg.active {
                if !leg.is_empty() {
                    return Err(V16Error::HiddenLeg);
                }
                slot += 1;
                continue;
            }
            validate_active_leg(leg)?;
            let asset_index = leg.asset_index as usize;
            if asset_index >= configured_assets || asset_index >= market.markets.len() {
                return Err(V16Error::HiddenLeg);
            }
            let asset = market.markets[asset_index].engine.asset.try_to_runtime()?;
            if leg.market_id != asset.market_id
                || !matches!(
                    asset.lifecycle,
                    AssetLifecycleV16::Active
                        | AssetLifecycleV16::DrainOnly
                        | AssetLifecycleV16::Recovery
                )
                || !leg_snapshots_bound_to_asset_side(asset, leg)
            {
                return Err(V16Error::HiddenLeg);
            }
            let mut seen = 0usize;
            while seen < seen_asset_count {
                if seen_assets[seen] == leg.asset_index {
                    return Err(V16Error::HiddenLeg);
                }
                seen += 1;
            }
            seen_assets[seen_asset_count] = leg.asset_index;
            seen_asset_count += 1;
            slot += 1;
        }

        if self.header.close_progress.quantity_adl_applied_q.get() != 0 {
            let i = self.header.close_progress.asset_index.get() as usize;
            if i >= configured_assets || self.active_leg_slot_for_asset(i)?.is_some() {
                return Err(V16Error::InvalidLeg);
            }
        }
        Ok(())
    }

    fn validate_source_credit_shape_with_market<T>(
        &self,
        market: &MarketGroupV16View<'_, T>,
    ) -> V16Result<()> {
        let configured_domains =
            v16_domain_count_for_market_slots(market.header.config.max_market_slots.get())?;
        if self.source_domains.len() < configured_domains {
            return Err(V16Error::HiddenLeg);
        }
        let mut d = 0usize;
        while d < self.source_domains.len() {
            let source = self.source_domains[d];
            let numeric_zero_source_domain = source.source_claim_bound_num.get() == 0
                && source.source_claim_liened_num.get() == 0
                && source.source_claim_counterparty_liened_num.get() == 0
                && source.source_claim_insurance_liened_num.get() == 0
                && source.source_lien_effective_reserved.get() == 0
                && source.source_lien_counterparty_backing_num.get() == 0
                && source.source_lien_insurance_backing_num.get() == 0
                && source.source_lien_fee_last_slot.get() == 0
                && source.source_claim_impaired_num.get() == 0
                && source.source_lien_impaired_effective_reserved.get() == 0;
            if numeric_zero_source_domain {
                if source.source_claim_market_id.get() != 0 {
                    return Err(V16Error::HiddenLeg);
                }
                d += 1;
                continue;
            }
            if d >= configured_domains {
                return Err(V16Error::HiddenLeg);
            }
            let asset_index = d / 2;
            let asset = market.markets[asset_index].engine.asset.try_to_runtime()?;
            if source.source_claim_market_id.get() != asset.market_id {
                return Err(V16Error::HiddenLeg);
            }
            let slot = market.markets[asset_index].engine_slot();
            let domain_credit = if d % 2 == 0 {
                slot.source_credit_long.try_to_runtime()?
            } else {
                slot.source_credit_short.try_to_runtime()?
            };
            if source.source_claim_bound_num.get() > domain_credit.positive_claim_bound_num {
                return Err(V16Error::InvalidLeg);
            }
            let proof = SourceCreditLienAggregateProofV16 {
                domain: u16::try_from(d).map_err(|_| V16Error::ArithmeticOverflow)?,
                source_claim_bound_num: source.source_claim_bound_num.get(),
                face_claim_locked_num: source.source_claim_liened_num.get(),
                counterparty_face_claim_locked_num: source
                    .source_claim_counterparty_liened_num
                    .get(),
                insurance_face_claim_locked_num: source.source_claim_insurance_liened_num.get(),
                effective_credit_reserved: source.source_lien_effective_reserved.get(),
                counterparty_backing_reserved_num: source
                    .source_lien_counterparty_backing_num
                    .get(),
                insurance_backing_reserved_num: source.source_lien_insurance_backing_num.get(),
                impaired_face_claim_num: source.source_claim_impaired_num.get(),
                impaired_effective_credit_reserved: source
                    .source_lien_impaired_effective_reserved
                    .get(),
            };
            proof.validate()?;
            let locked = proof
                .face_claim_locked_num
                .checked_add(proof.impaired_face_claim_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if locked > proof.source_claim_bound_num {
                return Err(V16Error::InvalidLeg);
            }
            let backing_source_claim = proof
                .counterparty_face_claim_locked_num
                .checked_add(proof.insurance_face_claim_locked_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if backing_source_claim != proof.face_claim_locked_num {
                return Err(V16Error::InvalidLeg);
            }
            if proof.effective_credit_reserved
                > V16Core::amount_from_bound_num(proof.face_claim_locked_num)?
            {
                return Err(V16Error::InvalidLeg);
            }
            let lien_backing_num = proof
                .counterparty_backing_reserved_num
                .checked_add(proof.insurance_backing_reserved_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if proof.counterparty_backing_reserved_num % BOUND_SCALE != 0
                || proof.insurance_backing_reserved_num % BOUND_SCALE != 0
                || lien_backing_num
                    != proof
                        .effective_credit_reserved
                        .checked_mul(BOUND_SCALE)
                        .ok_or(V16Error::ArithmeticOverflow)?
            {
                return Err(V16Error::InvalidLeg);
            }
            if proof.impaired_effective_credit_reserved != 0 && proof.impaired_face_claim_num == 0 {
                return Err(V16Error::InvalidLeg);
            }
            if (source.source_lien_counterparty_backing_num.get() == 0
                && source.source_lien_fee_last_slot.get() != 0)
                || source.source_lien_fee_last_slot.get() > market.header.current_slot.get()
            {
                return Err(V16Error::InvalidLeg);
            }
            d += 1;
        }
        Ok(())
    }

    fn source_claim_bound_sum_num(&self) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < self.source_domains.len() {
            sum = sum
                .checked_add(self.source_domains[d].source_claim_bound_num.get())
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        Ok(sum)
    }

    fn validate_resolved_payout_receipt_static(receipt: ResolvedPayoutReceiptV16) -> V16Result<()> {
        validate_resolved_payout_receipt_value(receipt)
    }

    fn validate_close_progress_ledger_with_market<T>(
        &self,
        market: &MarketGroupV16View<'_, T>,
    ) -> V16Result<()> {
        let ledger = self.header.close_progress.try_to_runtime()?;
        let configured_assets = market.header.config.max_market_slots.get() as usize;
        if ledger.canceled {
            if ledger.active
                || ledger.finalized
                || ledger.close_id == 0
                || ledger.asset_index as usize >= configured_assets
                || ledger.drift_reference_slot > ledger.max_close_slot
                || ledger.has_irreversible_progress()
                || ledger.residual_remaining != ledger.gross_loss_at_close_start
            {
                return Err(V16Error::InvalidLeg);
            }
            return Ok(());
        }
        if !ledger.active {
            if !ledger.is_empty() {
                return Err(V16Error::InvalidLeg);
            }
            return Ok(());
        }
        let asset_index = ledger.asset_index as usize;
        if ledger.close_id == 0
            || asset_index >= configured_assets
            || asset_index >= market.markets.len()
            || ledger.market_id != market.markets[asset_index].engine.asset.market_id.get()
            || ledger.drift_reference_slot > ledger.max_close_slot
            || ledger.max_close_slot < ledger.drift_reference_slot
            || ledger.support_consumed > ledger.junior_face_burned
            || ledger.canceled
        {
            return Err(V16Error::InvalidLeg);
        }
        let progress = ledger
            .support_consumed
            .checked_add(ledger.insurance_spent)
            .and_then(|v| v.checked_add(ledger.b_loss_booked))
            .and_then(|v| v.checked_add(ledger.explicit_loss_assigned))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let total_loss = ledger
            .gross_loss_at_close_start
            .checked_add(ledger.drift_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if progress > total_loss || ledger.residual_remaining != total_loss - progress {
            return Err(V16Error::InvalidLeg);
        }
        if ledger.finalized && ledger.residual_remaining != 0 {
            return Err(V16Error::InvalidLeg);
        }
        if ledger.quantity_adl_applied_q != 0
            && (!ledger.finalized || ledger.residual_remaining != 0)
        {
            return Err(V16Error::InvalidLeg);
        }
        if ledger.active {
            if let Some(slot) = self.active_leg_slot_for_asset(asset_index)? {
                let leg = self.header.legs[slot].try_to_runtime()?;
                if leg.active && ledger.domain_side != opposite_side(leg.side) {
                    return Err(V16Error::InvalidLeg);
                }
            }
        }
        Ok(())
    }

    fn active_leg_slot_for_asset(&self, asset_index: usize) -> V16Result<Option<usize>> {
        let mut found = None;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = self.header.legs[slot].try_to_runtime()?;
            if leg.active && leg.asset_index as usize == asset_index {
                if found.is_some() {
                    return Err(V16Error::HiddenLeg);
                }
                found = Some(slot);
            }
            slot += 1;
        }
        Ok(found)
    }
}

impl<'a> PortfolioV16ViewMut<'a> {
    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {
        self.as_view().validate_with_market(market)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortfolioLegV16 {
    pub active: bool,
    pub asset_index: u32,
    pub market_id: u64,
    pub side: SideV16,
    pub basis_pos_q: i128,
    pub a_basis: u128,
    pub k_snap: i128,
    pub f_snap: i128,
    pub epoch_snap: u64,
    pub loss_weight: u128,
    pub b_snap: u128,
    pub b_rem: u128,
    pub b_epoch_snap: u64,
    pub b_stale: bool,
    pub stale: bool,
}

impl PortfolioLegV16 {
    pub const EMPTY: Self = Self {
        active: false,
        asset_index: 0,
        market_id: 0,
        side: SideV16::Long,
        basis_pos_q: 0,
        a_basis: ADL_ONE,
        k_snap: 0,
        f_snap: 0,
        epoch_snap: 0,
        loss_weight: 0,
        b_snap: 0,
        b_rem: 0,
        b_epoch_snap: 0,
        b_stale: false,
        stale: false,
    };

    pub fn is_empty(self) -> bool {
        !self.active
            && self.asset_index == 0
            && self.market_id == 0
            && matches!(self.side, SideV16::Long)
            && self.basis_pos_q == 0
            && self.a_basis == ADL_ONE
            && self.k_snap == 0
            && self.f_snap == 0
            && self.epoch_snap == 0
            && self.loss_weight == 0
            && self.b_snap == 0
            && self.b_rem == 0
            && self.b_epoch_snap == 0
            && !self.b_stale
            && !self.stale
    }
}

impl Default for PortfolioLegV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HealthCertV16 {
    pub certified_equity: i128,
    pub certified_initial_req: u128,
    pub certified_maintenance_req: u128,
    pub certified_liq_deficit: u128,
    pub certified_worst_case_loss: u128,
    pub cert_oracle_epoch: u64,
    pub cert_funding_epoch: u64,
    pub cert_risk_epoch: u64,
    pub cert_asset_set_epoch: u64,
    pub active_bitmap_at_cert: V16ActiveBitmap,
    pub valid: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseProgressLedgerV16 {
    pub active: bool,
    pub finalized: bool,
    pub canceled: bool,
    pub close_id: u64,
    pub asset_index: u32,
    pub market_id: u64,
    pub domain_side: SideV16,
    pub gross_loss_at_close_start: u128,
    pub drift_reference_slot: u64,
    pub max_close_slot: u64,
    pub support_consumed: u128,
    pub junior_face_burned: u128,
    pub insurance_spent: u128,
    pub b_loss_booked: u128,
    pub explicit_loss_assigned: u128,
    pub quantity_adl_applied_q: u128,
    pub drift_consumed: u128,
    pub residual_remaining: u128,
}

impl CloseProgressLedgerV16 {
    pub const EMPTY: Self = Self {
        active: false,
        finalized: false,
        canceled: false,
        close_id: 0,
        asset_index: 0,
        market_id: 0,
        domain_side: SideV16::Long,
        gross_loss_at_close_start: 0,
        drift_reference_slot: 0,
        max_close_slot: 0,
        support_consumed: 0,
        junior_face_burned: 0,
        insurance_spent: 0,
        b_loss_booked: 0,
        explicit_loss_assigned: 0,
        quantity_adl_applied_q: 0,
        drift_consumed: 0,
        residual_remaining: 0,
    };

    pub fn has_pending_residual(self) -> bool {
        self.active && !self.finalized && !self.canceled && self.residual_remaining != 0
    }

    pub fn has_irreversible_progress(self) -> bool {
        self.support_consumed != 0
            || self.junior_face_burned != 0
            || self.insurance_spent != 0
            || self.b_loss_booked != 0
            || self.explicit_loss_assigned != 0
            || self.quantity_adl_applied_q != 0
            || self.drift_consumed != 0
    }

    pub fn is_empty(self) -> bool {
        !self.active
            && !self.finalized
            && !self.canceled
            && self.close_id == 0
            && self.asset_index == 0
            && self.market_id == 0
            && matches!(self.domain_side, SideV16::Long)
            && self.gross_loss_at_close_start == 0
            && self.drift_reference_slot == 0
            && self.max_close_slot == 0
            && self.support_consumed == 0
            && self.junior_face_burned == 0
            && self.insurance_spent == 0
            && self.b_loss_booked == 0
            && self.explicit_loss_assigned == 0
            && self.quantity_adl_applied_q == 0
            && self.drift_consumed == 0
            && self.residual_remaining == 0
    }
}

impl Default for CloseProgressLedgerV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPayoutLedgerV16 {
    pub snapshot_residual: u128,
    pub terminal_claim_exact_receipts_num: u128,
    pub terminal_claim_bound_unreceipted_num: u128,
    pub current_payout_rate_num: u128,
    pub current_payout_rate_den: u128,
    pub snapshot_slot: u64,
    pub payout_halted: bool,
    pub finalized: bool,
}

impl ResolvedPayoutLedgerV16 {
    pub const EMPTY: Self = Self {
        snapshot_residual: 0,
        terminal_claim_exact_receipts_num: 0,
        terminal_claim_bound_unreceipted_num: 0,
        current_payout_rate_num: 0,
        current_payout_rate_den: 0,
        snapshot_slot: 0,
        payout_halted: false,
        finalized: false,
    };
}

impl Default for ResolvedPayoutLedgerV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPayoutReceiptV16 {
    pub present: bool,
    pub prior_bound_contribution_num: u128,
    pub live_released_face_at_receipt: u128,
    pub terminal_positive_claim_face: u128,
    pub paid_effective: u128,
    pub finalized: bool,
}

impl ResolvedPayoutReceiptV16 {
    pub const EMPTY: Self = Self {
        present: false,
        prior_bound_contribution_num: 0,
        live_released_face_at_receipt: 0,
        terminal_positive_claim_face: 0,
        paid_effective: 0,
        finalized: false,
    };

    pub fn is_empty(self) -> bool {
        !self.present
            && self.prior_bound_contribution_num == 0
            && self.live_released_face_at_receipt == 0
            && self.terminal_positive_claim_face == 0
            && self.paid_effective == 0
            && !self.finalized
    }
}

impl Default for ResolvedPayoutReceiptV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

fn validate_resolved_payout_receipt_value(receipt: ResolvedPayoutReceiptV16) -> V16Result<()> {
    if !receipt.present {
        if !receipt.is_empty() {
            return Err(V16Error::InvalidLeg);
        }
        return Ok(());
    }
    let exact_num = receipt
        .terminal_positive_claim_face
        .checked_mul(BOUND_SCALE)
        .ok_or(V16Error::ArithmeticOverflow)?;
    if exact_num > receipt.prior_bound_contribution_num
        || receipt.paid_effective > receipt.terminal_positive_claim_face
        || receipt.finalized != (receipt.paid_effective == receipt.terminal_positive_claim_face)
    {
        return Err(V16Error::InvalidLeg);
    }
    Ok(())
}

fn apply_resolved_payout_receipt_payment(
    mut receipt: ResolvedPayoutReceiptV16,
    actual_resolved_paid: u128,
) -> V16Result<ResolvedPayoutReceiptV16> {
    validate_resolved_payout_receipt_value(receipt)?;
    if actual_resolved_paid == 0 {
        return Ok(receipt);
    }
    if !receipt.present {
        return Err(V16Error::InvalidLeg);
    }
    let remaining = receipt
        .terminal_positive_claim_face
        .checked_sub(receipt.paid_effective)
        .ok_or(V16Error::InvalidLeg)?;
    if actual_resolved_paid > remaining {
        return Err(V16Error::InvalidLeg);
    }
    receipt.paid_effective = receipt
        .paid_effective
        .checked_add(actual_resolved_paid)
        .ok_or(V16Error::ArithmeticOverflow)?;
    receipt.finalized = receipt.paid_effective == receipt.terminal_positive_claim_face;
    validate_resolved_payout_receipt_value(receipt)?;
    Ok(receipt)
}

#[cfg(kani)]
pub fn kani_apply_resolved_payout_receipt_payment(
    receipt: ResolvedPayoutReceiptV16,
    actual_resolved_paid: u128,
) -> V16Result<ResolvedPayoutReceiptV16> {
    apply_resolved_payout_receipt_payment(receipt, actual_resolved_paid)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccrueAssetOutcomeV16 {
    pub dt: u64,
    pub price_move_active: bool,
    pub funding_active: bool,
    pub equity_active: bool,
    pub loss_stale_after: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeRequestV16 {
    pub asset_index: usize,
    pub size_q: u128,
    pub exec_price: u64,
    pub fee_bps: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeOutcomeV16 {
    pub fee_a: u128,
    pub fee_b: u128,
    pub notional: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TradePositionPreflightV16 {
    risk_increasing: bool,
    long_lookup: PositionDeltaLookupV16,
    short_lookup: PositionDeltaLookupV16,
    long_old_abs_q: u128,
    short_old_abs_q: u128,
    long_new_abs_q: u128,
    short_new_abs_q: u128,
    long_has_source_claims: bool,
    short_has_source_claims: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PositionDeltaLookupV16 {
    existing_slot: Option<usize>,
    empty_slot: Option<usize>,
    current_q: i128,
    next_q: i128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountRefreshCertOutcomeV16 {
    Certified(HealthCertV16),
    BChunk(AccountBSettlementChunkV16),
}

pub const V16_TOKEN_VALUE_CLASS_COUNT: usize = 17;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenValueClassV16 {
    TokenVault = 0,
    SeniorCapital = 1,
    InsuranceCapital = 2,
    AccountCapital = 3,
    CloseSupportConsumed = 4,
    CloseInsuranceSpent = 5,
    CloseCounterpartyCreditConsumed = 6,
    BResidualBooked = 7,
    PendingObligationEscrow = 8,
    PendingObligationCredit = 9,
    ExplicitBackedLoss = 10,
    SettlementRoundingResidue = 11,
    CancelDepositEscrow = 12,
    ResolvedPayoutPaid = 13,
    ProtocolFeePaid = 14,
    ExternalQuote = 15,
    UnallocatedProtocolSurplus = 16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenValueFlowProofV16 {
    pub debits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    pub credits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    pub external_quote_in: u128,
    pub external_quote_out: u128,
    pub vault_before: u128,
    pub vault_after: u128,
}

impl TokenValueFlowProofV16 {
    pub const fn empty(vault_before: u128, vault_after: u128) -> Self {
        Self {
            debits: [0; V16_TOKEN_VALUE_CLASS_COUNT],
            credits: [0; V16_TOKEN_VALUE_CLASS_COUNT],
            external_quote_in: 0,
            external_quote_out: 0,
            vault_before,
            vault_after,
        }
    }

    pub fn external_in_to_account_capital(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_in = amount;
        proof.credit(TokenValueClassV16::ExternalQuote, amount)?;
        proof.debit(TokenValueClassV16::AccountCapital, amount)?;
        Ok(proof)
    }

    pub fn account_capital_to_external_out(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_out = amount;
        proof.debit(TokenValueClassV16::AccountCapital, amount)?;
        proof.credit(TokenValueClassV16::ExternalQuote, amount)?;
        Ok(proof)
    }

    pub fn close_cure_to_account_capital(
        optional_external_deposit: u128,
        cancel_deposit_escrow: u128,
        capital_credit: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let expected_credit = optional_external_deposit
            .checked_add(cancel_deposit_escrow)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if expected_credit != capital_credit {
            return Err(V16Error::InvalidConfig);
        }
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_in = optional_external_deposit;
        proof.credit(TokenValueClassV16::ExternalQuote, optional_external_deposit)?;
        proof.credit(
            TokenValueClassV16::CancelDepositEscrow,
            cancel_deposit_escrow,
        )?;
        proof.debit(TokenValueClassV16::AccountCapital, capital_credit)?;
        Ok(proof)
    }

    pub fn account_capital_to_insurance(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.debit(TokenValueClassV16::AccountCapital, amount)?;
        proof.credit(TokenValueClassV16::InsuranceCapital, amount)?;
        Ok(proof)
    }

    pub fn account_capital_to_realized_loss(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.debit(TokenValueClassV16::AccountCapital, amount)?;
        proof.credit(TokenValueClassV16::ExplicitBackedLoss, amount)?;
        Ok(proof)
    }

    pub fn insurance_to_close_insurance_spent(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.debit(TokenValueClassV16::InsuranceCapital, amount)?;
        proof.credit(TokenValueClassV16::CloseInsuranceSpent, amount)?;
        Ok(proof)
    }

    pub fn validate_insurance_to_close_insurance_spent(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<()> {
        Self::insurance_to_close_insurance_spent(amount, vault_before, vault_after)?.validate()
    }

    pub fn support_to_account_capital(
        account_capital_credit: u128,
        counterparty_credit_consumed: u128,
        insurance_credit_consumed: u128,
        protocol_surplus_consumed: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let source_total = counterparty_credit_consumed
            .checked_add(insurance_credit_consumed)
            .and_then(|v| v.checked_add(protocol_surplus_consumed))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if source_total != account_capital_credit {
            return Err(V16Error::InvalidConfig);
        }
        let mut proof = Self::empty(vault_before, vault_after);
        proof.credit(
            TokenValueClassV16::CloseCounterpartyCreditConsumed,
            counterparty_credit_consumed,
        )?;
        proof.credit(
            TokenValueClassV16::CloseInsuranceSpent,
            insurance_credit_consumed,
        )?;
        proof.credit(
            TokenValueClassV16::UnallocatedProtocolSurplus,
            protocol_surplus_consumed,
        )?;
        proof.debit(TokenValueClassV16::AccountCapital, account_capital_credit)?;
        Ok(proof)
    }

    pub fn capital_and_resolved_payout_to_external_out(
        capital_paid: u128,
        resolved_payout_paid: u128,
        total_external_out: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let total_source = capital_paid
            .checked_add(resolved_payout_paid)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if total_source != total_external_out {
            return Err(V16Error::InvalidConfig);
        }
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_out = total_external_out;
        proof.debit(TokenValueClassV16::AccountCapital, capital_paid)?;
        proof.debit(TokenValueClassV16::ResolvedPayoutPaid, resolved_payout_paid)?;
        proof.credit(TokenValueClassV16::ExternalQuote, total_external_out)?;
        Ok(proof)
    }

    pub fn debit(&mut self, class: TokenValueClassV16, amount: u128) -> V16Result<()> {
        let idx = class as usize;
        self.debits[idx] = self.debits[idx]
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn credit(&mut self, class: TokenValueClassV16, amount: u128) -> V16Result<()> {
        let idx = class as usize;
        self.credits[idx] = self.credits[idx]
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn validate(&self) -> V16Result<()> {
        let mut total_debits = 0u128;
        let mut total_credits = 0u128;
        let mut i = 0;
        while i < V16_TOKEN_VALUE_CLASS_COUNT {
            total_debits = total_debits
                .checked_add(self.debits[i])
                .ok_or(V16Error::ArithmeticOverflow)?;
            total_credits = total_credits
                .checked_add(self.credits[i])
                .ok_or(V16Error::ArithmeticOverflow)?;
            i += 1;
        }
        if total_debits != total_credits {
            return Err(V16Error::InvalidConfig);
        }

        if self.vault_after >= self.vault_before {
            let vault_delta = self.vault_after - self.vault_before;
            if self.external_quote_in < self.external_quote_out
                || self.external_quote_in - self.external_quote_out != vault_delta
            {
                return Err(V16Error::InvalidConfig);
            }
        } else {
            let vault_delta = self.vault_before - self.vault_after;
            if self.external_quote_out < self.external_quote_in
                || self.external_quote_out - self.external_quote_in != vault_delta
            {
                return Err(V16Error::InvalidConfig);
            }
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReservationEncumbranceProofV16 {
    pub domain: u16,
    pub exact_positive_claim_num: u128,
    pub positive_claim_bound_num: u128,
    pub source_fresh_reserved_backing_num: u128,
    pub source_spent_backing_num: u128,
    pub source_provider_receivable_num: u128,
    pub bucket_fresh_unliened_backing_num: u128,
    pub bucket_valid_liened_backing_num: u128,
    pub bucket_consumed_liened_backing_num: u128,
    pub source_valid_liened_backing_num: u128,
    pub source_impaired_liened_backing_num: u128,
    pub bucket_impaired_liened_backing_num: u128,
    pub source_insurance_credit_reserved_num: u128,
    pub reservation_insurance_credit_reserved_num: u128,
    pub source_valid_liened_insurance_num: u128,
    pub reservation_valid_liened_insurance_num: u128,
    pub source_impaired_liened_insurance_num: u128,
    pub reservation_impaired_liened_insurance_num: u128,
    pub source_credit_rate_num: u128,
}

impl ReservationEncumbranceProofV16 {
    pub fn validate(&self) -> V16Result<()> {
        let fresh_reserved = self
            .bucket_fresh_unliened_backing_num
            .checked_add(self.bucket_valid_liened_backing_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if self.source_fresh_reserved_backing_num != fresh_reserved
            || self.source_provider_receivable_num != self.bucket_consumed_liened_backing_num
            || self.source_spent_backing_num < self.source_provider_receivable_num
            || self.source_valid_liened_backing_num != self.bucket_valid_liened_backing_num
            || self.source_impaired_liened_backing_num != self.bucket_impaired_liened_backing_num
            || self.source_insurance_credit_reserved_num
                != self.reservation_insurance_credit_reserved_num
            || self.source_valid_liened_insurance_num != self.reservation_valid_liened_insurance_num
            || self.source_impaired_liened_insurance_num
                != self.reservation_impaired_liened_insurance_num
        {
            return Err(V16Error::InvalidConfig);
        }
        let insurance_encumbered = self
            .reservation_valid_liened_insurance_num
            .checked_add(self.reservation_impaired_liened_insurance_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if self.reservation_insurance_credit_reserved_num < insurance_encumbered {
            return Err(V16Error::InvalidConfig);
        }
        let source = SourceCreditStateV16 {
            exact_positive_claim_num: self.exact_positive_claim_num,
            positive_claim_bound_num: self.positive_claim_bound_num,
            fresh_reserved_backing_num: self.source_fresh_reserved_backing_num,
            valid_liened_backing_num: self.source_valid_liened_backing_num,
            impaired_liened_backing_num: self.source_impaired_liened_backing_num,
            spent_backing_num: self.source_spent_backing_num,
            provider_receivable_num: self.source_provider_receivable_num,
            insurance_credit_reserved_num: self.source_insurance_credit_reserved_num,
            valid_liened_insurance_num: self.source_valid_liened_insurance_num,
            impaired_liened_insurance_num: self.source_impaired_liened_insurance_num,
            credit_rate_num: self.source_credit_rate_num,
            credit_epoch: 0,
        };
        V16Core::validate_source_credit_state_static(source)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StockReconciliationProofV16 {
    pub token_vault: u128,
    pub senior_capital_total: u128,
    pub insurance_capital: u128,
    pub backing_provider_earnings: u128,
    pub settlement_rounding_residue_total: u128,
    pub unallocated_protocol_surplus: u128,
}

impl StockReconciliationProofV16 {
    pub fn validate(&self) -> V16Result<()> {
        let accounted = self
            .senior_capital_total
            .checked_add(self.insurance_capital)
            .and_then(|v| v.checked_add(self.backing_provider_earnings))
            .and_then(|v| v.checked_add(self.settlement_rounding_residue_total))
            .and_then(|v| v.checked_add(self.unallocated_protocol_surplus))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if accounted != self.token_vault {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCreditLienAggregateProofV16 {
    pub domain: u16,
    pub source_claim_bound_num: u128,
    pub face_claim_locked_num: u128,
    pub counterparty_face_claim_locked_num: u128,
    pub insurance_face_claim_locked_num: u128,
    pub effective_credit_reserved: u128,
    pub counterparty_backing_reserved_num: u128,
    pub insurance_backing_reserved_num: u128,
    pub impaired_face_claim_num: u128,
    pub impaired_effective_credit_reserved: u128,
}

impl SourceCreditLienAggregateProofV16 {
    pub fn validate(&self) -> V16Result<()> {
        let backing_face = self
            .counterparty_face_claim_locked_num
            .checked_add(self.insurance_face_claim_locked_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if backing_face != self.face_claim_locked_num {
            return Err(V16Error::InvalidLeg);
        }
        let locked_or_impaired = self
            .face_claim_locked_num
            .checked_add(self.impaired_face_claim_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if locked_or_impaired > self.source_claim_bound_num {
            return Err(V16Error::InvalidLeg);
        }
        if self.effective_credit_reserved
            > V16Core::amount_from_bound_num(self.face_claim_locked_num)?
        {
            return Err(V16Error::InvalidLeg);
        }
        if self.counterparty_backing_reserved_num % BOUND_SCALE != 0
            || self.insurance_backing_reserved_num % BOUND_SCALE != 0
        {
            return Err(V16Error::InvalidLeg);
        }
        let backing_num = self
            .counterparty_backing_reserved_num
            .checked_add(self.insurance_backing_reserved_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let expected_backing_num = self
            .effective_credit_reserved
            .checked_mul(BOUND_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if backing_num != expected_backing_num {
            return Err(V16Error::InvalidLeg);
        }
        if self.impaired_effective_credit_reserved != 0 && self.impaired_face_claim_num == 0 {
            return Err(V16Error::InvalidLeg);
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidationRequestV16 {
    pub asset_index: usize,
    pub close_q: u128,
    pub fee_bps: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidationOutcomeV16 {
    pub closed_q: u128,
    pub insurance_used: u128,
    pub residual_booked: u128,
    pub explicit_loss: u128,
    pub fee_charged: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadLegForfeitOutcomeV16 {
    pub detached: bool,
    pub positive_pnl_forfeited: u128,
    pub loss_settled: u128,
    pub support_consumed: u128,
    pub junior_face_burned: u128,
    pub principal_used: u128,
    pub insurance_used: u128,
    pub residual_booked: u128,
    pub explicit_loss: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SupportLossApplicationV16 {
    support_consumed: u128,
    junior_face_burned: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceCreditConsumptionV16 {
    face_burn: u128,
    counterparty_credit_consumed: u128,
    insurance_credit_consumed: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceCreditBackingSourceV16 {
    Counterparty,
    Insurance,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebalanceRequestV16 {
    pub asset_index: usize,
    pub reduce_q: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebalanceOutcomeV16 {
    pub reduced_q: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BResidualBookingOutcomeV16 {
    pub booked_loss: u128,
    pub explicit_loss: u128,
    pub delta_b: u128,
    pub remaining_after: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuantityAdlOutcomeV16 {
    pub closed_q: u128,
    pub opposite_a_after: u128,
    pub reset_started: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionlessCrankActionV16 {
    Refresh,
    SettleB { asset_index: usize },
    Liquidate(LiquidationRequestV16),
    Recover(PermissionlessRecoveryReasonV16),
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionlessCrankRequestV16 {
    pub now_slot: u64,
    pub asset_index: usize,
    pub effective_price: u64,
    pub funding_rate_e9: i128,
    pub action: PermissionlessCrankActionV16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedCloseOutcomeV16 {
    ProgressOnly,
    Closed { payout: u128 },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16PodU16 {
    pub bytes: [u8; 2],
}

impl V16PodU16 {
    pub fn new(value: u16) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    pub fn get(self) -> u16 {
        u16::from_le_bytes(self.bytes)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16PodU32 {
    pub bytes: [u8; 4],
}

impl V16PodU32 {
    pub fn new(value: u32) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    pub fn get(self) -> u32 {
        u32::from_le_bytes(self.bytes)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16PodU64 {
    pub bytes: [u8; 8],
}

impl V16PodU64 {
    pub fn new(value: u64) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    pub fn get(self) -> u64 {
        u64::from_le_bytes(self.bytes)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16PodU128 {
    pub bytes: [u8; 16],
}

impl V16PodU128 {
    pub fn new(value: u128) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    pub fn get(self) -> u128 {
        u128::from_le_bytes(self.bytes)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16PodI128 {
    pub bytes: [u8; 16],
}

impl V16PodI128 {
    pub fn new(value: i128) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    pub fn get(self) -> i128 {
        i128::from_le_bytes(self.bytes)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16OptionalRecoveryReasonAccount {
    pub present: u8,
    pub value: u8,
}

impl V16OptionalRecoveryReasonAccount {
    pub fn from_runtime(value: Option<PermissionlessRecoveryReasonV16>) -> Self {
        match value {
            Some(reason) => Self {
                present: 1,
                value: encode_recovery_reason(reason),
            },
            None => Self {
                present: 0,
                value: 0,
            },
        }
    }

    pub fn try_to_runtime(self) -> V16Result<Option<PermissionlessRecoveryReasonV16>> {
        match self.present {
            0 if self.value == 0 => Ok(None),
            1 => Ok(Some(decode_recovery_reason(self.value)?)),
            _ => Err(V16Error::InvalidConfig),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ProvenanceHeaderV16Account {
    pub market_group_id: [u8; 32],
    pub portfolio_account_id: [u8; 32],
    pub owner: [u8; 32],
    pub version: V16PodU16,
    pub layout_discriminator: V16PodU16,
}

impl ProvenanceHeaderV16Account {
    pub fn from_runtime(value: &ProvenanceHeaderV16) -> Self {
        Self {
            market_group_id: value.market_group_id,
            portfolio_account_id: value.portfolio_account_id,
            owner: value.owner,
            version: V16PodU16::new(value.version),
            layout_discriminator: V16PodU16::new(value.layout_discriminator),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<ProvenanceHeaderV16> {
        let out = ProvenanceHeaderV16 {
            market_group_id: self.market_group_id,
            portfolio_account_id: self.portfolio_account_id,
            owner: self.owner,
            version: self.version.get(),
            layout_discriminator: self.layout_discriminator.get(),
        };
        if out.version != V16_ACCOUNT_VERSION
            || out.layout_discriminator != V16_LAYOUT_DISCRIMINATOR
        {
            return Err(V16Error::ProvenanceMismatch);
        }
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct V16ConfigAccount {
    pub max_portfolio_assets: V16PodU16,
    pub max_market_slots: V16PodU32,
    pub min_nonzero_mm_req: V16PodU128,
    pub min_nonzero_im_req: V16PodU128,
    pub h_min: V16PodU64,
    pub h_max: V16PodU64,
    pub maintenance_margin_bps: V16PodU64,
    pub initial_margin_bps: V16PodU64,
    pub max_trading_fee_bps: V16PodU64,
    pub liquidation_fee_bps: V16PodU64,
    pub liquidation_fee_cap: V16PodU128,
    pub min_liquidation_abs: V16PodU128,
    pub max_accrual_dt_slots: V16PodU64,
    pub max_abs_funding_e9_per_slot: V16PodU64,
    pub min_funding_lifetime_slots: V16PodU64,
    pub max_price_move_bps_per_slot: V16PodU64,
    pub max_account_b_settlement_chunks: V16PodU64,
    pub max_bankrupt_close_chunks: V16PodU64,
    pub max_bankrupt_close_lifetime_slots: V16PodU64,
    pub asset_activation_cooldown_slots: V16PodU64,
    pub public_b_chunk_atoms: V16PodU128,
    pub max_recovery_fallback_deviation_bps: V16PodU64,
    pub backing_fee_base_rate_e9_per_slot: V16PodU64,
    pub backing_fee_kink_util_bps: V16PodU64,
    pub backing_fee_slope_at_kink_e9_per_slot: V16PodU64,
    pub backing_fee_slope_above_kink_e9_per_slot: V16PodU64,
    pub backing_freshness_buckets: u8,
    pub margin_mode_realizable_full_shared_cross_margin: u8,
    pub source_credit_lien_required: u8,
    pub insurance_credit_reservation_required: u8,
    pub permissionless_recovery_enabled: u8,
    pub recovery_fallback_price_enabled: u8,
    pub recovery_fallback_envelope_enabled: u8,
    pub credit_lien_revalidation_required: u8,
    pub stale_certificate_penalty_enabled: u8,
    pub full_refresh_required_for_favorable_actions: u8,
    pub public_liveness_profile_crank_forward: u8,
}

impl V16ConfigAccount {
    pub fn from_runtime(value: &V16Config) -> Self {
        Self {
            max_portfolio_assets: V16PodU16::new(value.max_portfolio_assets),
            max_market_slots: V16PodU32::new(value.max_market_slots),
            min_nonzero_mm_req: V16PodU128::new(value.min_nonzero_mm_req),
            min_nonzero_im_req: V16PodU128::new(value.min_nonzero_im_req),
            h_min: V16PodU64::new(value.h_min),
            h_max: V16PodU64::new(value.h_max),
            maintenance_margin_bps: V16PodU64::new(value.maintenance_margin_bps),
            initial_margin_bps: V16PodU64::new(value.initial_margin_bps),
            max_trading_fee_bps: V16PodU64::new(value.max_trading_fee_bps),
            liquidation_fee_bps: V16PodU64::new(value.liquidation_fee_bps),
            liquidation_fee_cap: V16PodU128::new(value.liquidation_fee_cap),
            min_liquidation_abs: V16PodU128::new(value.min_liquidation_abs),
            max_accrual_dt_slots: V16PodU64::new(value.max_accrual_dt_slots),
            max_abs_funding_e9_per_slot: V16PodU64::new(value.max_abs_funding_e9_per_slot),
            min_funding_lifetime_slots: V16PodU64::new(value.min_funding_lifetime_slots),
            max_price_move_bps_per_slot: V16PodU64::new(value.max_price_move_bps_per_slot),
            max_account_b_settlement_chunks: V16PodU64::new(value.max_account_b_settlement_chunks),
            max_bankrupt_close_chunks: V16PodU64::new(value.max_bankrupt_close_chunks),
            max_bankrupt_close_lifetime_slots: V16PodU64::new(
                value.max_bankrupt_close_lifetime_slots,
            ),
            asset_activation_cooldown_slots: V16PodU64::new(value.asset_activation_cooldown_slots),
            public_b_chunk_atoms: V16PodU128::new(value.public_b_chunk_atoms),
            max_recovery_fallback_deviation_bps: V16PodU64::new(
                value.max_recovery_fallback_deviation_bps,
            ),
            backing_fee_base_rate_e9_per_slot: V16PodU64::new(
                value.backing_fee_base_rate_e9_per_slot,
            ),
            backing_fee_kink_util_bps: V16PodU64::new(value.backing_fee_kink_util_bps),
            backing_fee_slope_at_kink_e9_per_slot: V16PodU64::new(
                value.backing_fee_slope_at_kink_e9_per_slot,
            ),
            backing_fee_slope_above_kink_e9_per_slot: V16PodU64::new(
                value.backing_fee_slope_above_kink_e9_per_slot,
            ),
            backing_freshness_buckets: value.backing_freshness_buckets,
            margin_mode_realizable_full_shared_cross_margin: encode_bool(
                value.margin_mode_realizable_full_shared_cross_margin,
            ),
            source_credit_lien_required: encode_bool(value.source_credit_lien_required),
            insurance_credit_reservation_required: encode_bool(
                value.insurance_credit_reservation_required,
            ),
            permissionless_recovery_enabled: encode_bool(value.permissionless_recovery_enabled),
            recovery_fallback_price_enabled: encode_bool(value.recovery_fallback_price_enabled),
            recovery_fallback_envelope_enabled: encode_bool(
                value.recovery_fallback_envelope_enabled,
            ),
            credit_lien_revalidation_required: encode_bool(value.credit_lien_revalidation_required),
            stale_certificate_penalty_enabled: encode_bool(value.stale_certificate_penalty_enabled),
            full_refresh_required_for_favorable_actions: encode_bool(
                value.full_refresh_required_for_favorable_actions,
            ),
            public_liveness_profile_crank_forward: encode_bool(
                value.public_liveness_profile_crank_forward,
            ),
        }
    }

    fn decode_runtime(&self) -> V16Result<V16Config> {
        let out = V16Config {
            max_portfolio_assets: self.max_portfolio_assets.get(),
            max_market_slots: self.max_market_slots.get(),
            min_nonzero_mm_req: self.min_nonzero_mm_req.get(),
            min_nonzero_im_req: self.min_nonzero_im_req.get(),
            h_min: self.h_min.get(),
            h_max: self.h_max.get(),
            maintenance_margin_bps: self.maintenance_margin_bps.get(),
            initial_margin_bps: self.initial_margin_bps.get(),
            max_trading_fee_bps: self.max_trading_fee_bps.get(),
            liquidation_fee_bps: self.liquidation_fee_bps.get(),
            liquidation_fee_cap: self.liquidation_fee_cap.get(),
            min_liquidation_abs: self.min_liquidation_abs.get(),
            max_accrual_dt_slots: self.max_accrual_dt_slots.get(),
            max_abs_funding_e9_per_slot: self.max_abs_funding_e9_per_slot.get(),
            min_funding_lifetime_slots: self.min_funding_lifetime_slots.get(),
            max_price_move_bps_per_slot: self.max_price_move_bps_per_slot.get(),
            max_account_b_settlement_chunks: self.max_account_b_settlement_chunks.get(),
            max_bankrupt_close_chunks: self.max_bankrupt_close_chunks.get(),
            max_bankrupt_close_lifetime_slots: self.max_bankrupt_close_lifetime_slots.get(),
            asset_activation_cooldown_slots: self.asset_activation_cooldown_slots.get(),
            public_b_chunk_atoms: self.public_b_chunk_atoms.get(),
            max_recovery_fallback_deviation_bps: self.max_recovery_fallback_deviation_bps.get(),
            backing_fee_base_rate_e9_per_slot: self.backing_fee_base_rate_e9_per_slot.get(),
            backing_fee_kink_util_bps: self.backing_fee_kink_util_bps.get(),
            backing_fee_slope_at_kink_e9_per_slot: self.backing_fee_slope_at_kink_e9_per_slot.get(),
            backing_fee_slope_above_kink_e9_per_slot: self
                .backing_fee_slope_above_kink_e9_per_slot
                .get(),
            backing_freshness_buckets: self.backing_freshness_buckets,
            margin_mode_realizable_full_shared_cross_margin: decode_bool(
                self.margin_mode_realizable_full_shared_cross_margin,
            )?,
            source_credit_lien_required: decode_bool(self.source_credit_lien_required)?,
            insurance_credit_reservation_required: decode_bool(
                self.insurance_credit_reservation_required,
            )?,
            permissionless_recovery_enabled: decode_bool(self.permissionless_recovery_enabled)?,
            recovery_fallback_price_enabled: decode_bool(self.recovery_fallback_price_enabled)?,
            recovery_fallback_envelope_enabled: decode_bool(
                self.recovery_fallback_envelope_enabled,
            )?,
            credit_lien_revalidation_required: decode_bool(self.credit_lien_revalidation_required)?,
            stale_certificate_penalty_enabled: decode_bool(self.stale_certificate_penalty_enabled)?,
            full_refresh_required_for_favorable_actions: decode_bool(
                self.full_refresh_required_for_favorable_actions,
            )?,
            public_liveness_profile_crank_forward: decode_bool(
                self.public_liveness_profile_crank_forward,
            )?,
        };
        Ok(out)
    }

    pub fn try_to_runtime_shape(&self) -> V16Result<V16Config> {
        let out = self.decode_runtime()?;
        out.validate_public_user_fund_shape()?;
        Ok(out)
    }

    pub fn try_to_runtime(&self) -> V16Result<V16Config> {
        let out = self.decode_runtime()?;
        out.validate_public_user_fund()?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct SourceCreditStateV16Account {
    pub positive_claim_bound_num: V16PodU128,
    pub exact_positive_claim_num: V16PodU128,
    pub fresh_reserved_backing_num: V16PodU128,
    pub spent_backing_num: V16PodU128,
    pub provider_receivable_num: V16PodU128,
    pub valid_liened_backing_num: V16PodU128,
    pub impaired_liened_backing_num: V16PodU128,
    pub insurance_credit_reserved_num: V16PodU128,
    pub valid_liened_insurance_num: V16PodU128,
    pub impaired_liened_insurance_num: V16PodU128,
    pub credit_rate_num: V16PodU128,
    pub credit_epoch: V16PodU64,
}

impl SourceCreditStateV16Account {
    pub fn from_runtime(value: &SourceCreditStateV16) -> Self {
        Self {
            positive_claim_bound_num: V16PodU128::new(value.positive_claim_bound_num),
            exact_positive_claim_num: V16PodU128::new(value.exact_positive_claim_num),
            fresh_reserved_backing_num: V16PodU128::new(value.fresh_reserved_backing_num),
            spent_backing_num: V16PodU128::new(value.spent_backing_num),
            provider_receivable_num: V16PodU128::new(value.provider_receivable_num),
            valid_liened_backing_num: V16PodU128::new(value.valid_liened_backing_num),
            impaired_liened_backing_num: V16PodU128::new(value.impaired_liened_backing_num),
            insurance_credit_reserved_num: V16PodU128::new(value.insurance_credit_reserved_num),
            valid_liened_insurance_num: V16PodU128::new(value.valid_liened_insurance_num),
            impaired_liened_insurance_num: V16PodU128::new(value.impaired_liened_insurance_num),
            credit_rate_num: V16PodU128::new(value.credit_rate_num),
            credit_epoch: V16PodU64::new(value.credit_epoch),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<SourceCreditStateV16> {
        let out = SourceCreditStateV16 {
            positive_claim_bound_num: self.positive_claim_bound_num.get(),
            exact_positive_claim_num: self.exact_positive_claim_num.get(),
            fresh_reserved_backing_num: self.fresh_reserved_backing_num.get(),
            spent_backing_num: self.spent_backing_num.get(),
            provider_receivable_num: self.provider_receivable_num.get(),
            valid_liened_backing_num: self.valid_liened_backing_num.get(),
            impaired_liened_backing_num: self.impaired_liened_backing_num.get(),
            insurance_credit_reserved_num: self.insurance_credit_reserved_num.get(),
            valid_liened_insurance_num: self.valid_liened_insurance_num.get(),
            impaired_liened_insurance_num: self.impaired_liened_insurance_num.get(),
            credit_rate_num: self.credit_rate_num.get(),
            credit_epoch: self.credit_epoch.get(),
        };
        V16Core::validate_source_credit_state_static(out)?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct BackingBucketV16Account {
    pub market_id: V16PodU64,
    pub fresh_unliened_backing_num: V16PodU128,
    pub valid_liened_backing_num: V16PodU128,
    pub consumed_liened_backing_num: V16PodU128,
    pub impaired_liened_backing_num: V16PodU128,
    pub utilization_fee_earnings: V16PodU128,
    pub expiry_slot: V16PodU64,
    pub status: u8,
}

impl BackingBucketV16Account {
    pub fn from_runtime(value: &BackingBucketV16) -> Self {
        Self {
            market_id: V16PodU64::new(value.market_id),
            fresh_unliened_backing_num: V16PodU128::new(value.fresh_unliened_backing_num),
            valid_liened_backing_num: V16PodU128::new(value.valid_liened_backing_num),
            consumed_liened_backing_num: V16PodU128::new(value.consumed_liened_backing_num),
            impaired_liened_backing_num: V16PodU128::new(value.impaired_liened_backing_num),
            utilization_fee_earnings: V16PodU128::new(value.utilization_fee_earnings),
            expiry_slot: V16PodU64::new(value.expiry_slot),
            status: encode_backing_bucket_status(value.status),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<BackingBucketV16> {
        let out = BackingBucketV16 {
            market_id: self.market_id.get(),
            fresh_unliened_backing_num: self.fresh_unliened_backing_num.get(),
            valid_liened_backing_num: self.valid_liened_backing_num.get(),
            consumed_liened_backing_num: self.consumed_liened_backing_num.get(),
            impaired_liened_backing_num: self.impaired_liened_backing_num.get(),
            utilization_fee_earnings: self.utilization_fee_earnings.get(),
            expiry_slot: self.expiry_slot.get(),
            status: decode_backing_bucket_status(self.status)?,
        };
        V16Core::validate_backing_bucket_static(out)?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct InsuranceCreditReservationV16Account {
    pub insurance_credit_reserved_num: V16PodU128,
    pub valid_liened_insurance_num: V16PodU128,
    pub impaired_liened_insurance_num: V16PodU128,
    pub consumed_insurance_num: V16PodU128,
    pub source_credit_epoch: V16PodU64,
}

impl InsuranceCreditReservationV16Account {
    pub fn from_runtime(value: &InsuranceCreditReservationV16) -> Self {
        Self {
            insurance_credit_reserved_num: V16PodU128::new(value.insurance_credit_reserved_num),
            valid_liened_insurance_num: V16PodU128::new(value.valid_liened_insurance_num),
            impaired_liened_insurance_num: V16PodU128::new(value.impaired_liened_insurance_num),
            consumed_insurance_num: V16PodU128::new(value.consumed_insurance_num),
            source_credit_epoch: V16PodU64::new(value.source_credit_epoch),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<InsuranceCreditReservationV16> {
        let out = InsuranceCreditReservationV16 {
            insurance_credit_reserved_num: self.insurance_credit_reserved_num.get(),
            valid_liened_insurance_num: self.valid_liened_insurance_num.get(),
            impaired_liened_insurance_num: self.impaired_liened_insurance_num.get(),
            consumed_insurance_num: self.consumed_insurance_num.get(),
            source_credit_epoch: self.source_credit_epoch.get(),
        };
        V16Core::validate_insurance_reservation_static(out)?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct AssetStateV16Account {
    pub market_id: V16PodU64,
    pub retired_slot: V16PodU64,
    pub lifecycle: u8,
    pub raw_oracle_target_price: V16PodU64,
    pub effective_price: V16PodU64,
    pub fund_px_last: V16PodU64,
    pub slot_last: V16PodU64,
    pub a_long: V16PodU128,
    pub a_short: V16PodU128,
    pub k_long: V16PodI128,
    pub k_short: V16PodI128,
    pub f_long_num: V16PodI128,
    pub f_short_num: V16PodI128,
    pub k_epoch_start_long: V16PodI128,
    pub k_epoch_start_short: V16PodI128,
    pub f_epoch_start_long_num: V16PodI128,
    pub f_epoch_start_short_num: V16PodI128,
    pub b_long_num: V16PodU128,
    pub b_short_num: V16PodU128,
    pub b_epoch_start_long_num: V16PodU128,
    pub b_epoch_start_short_num: V16PodU128,
    pub oi_eff_long_q: V16PodU128,
    pub oi_eff_short_q: V16PodU128,
    pub stored_pos_count_long: V16PodU64,
    pub stored_pos_count_short: V16PodU64,
    pub stale_account_count_long: V16PodU64,
    pub stale_account_count_short: V16PodU64,
    pub pending_obligation_count_long: V16PodU64,
    pub pending_obligation_count_short: V16PodU64,
    pub loss_weight_sum_long: V16PodU128,
    pub loss_weight_sum_short: V16PodU128,
    pub social_loss_remainder_long_num: V16PodU128,
    pub social_loss_remainder_short_num: V16PodU128,
    pub social_loss_dust_long_num: V16PodU128,
    pub social_loss_dust_short_num: V16PodU128,
    pub explicit_unallocated_loss_long: V16PodU128,
    pub explicit_unallocated_loss_short: V16PodU128,
    pub epoch_long: V16PodU64,
    pub epoch_short: V16PodU64,
    pub mode_long: u8,
    pub mode_short: u8,
}

impl AssetStateV16Account {
    pub fn from_runtime(value: &AssetStateV16) -> Self {
        Self {
            market_id: V16PodU64::new(value.market_id),
            retired_slot: V16PodU64::new(value.retired_slot),
            lifecycle: encode_asset_lifecycle(value.lifecycle),
            raw_oracle_target_price: V16PodU64::new(value.raw_oracle_target_price),
            effective_price: V16PodU64::new(value.effective_price),
            fund_px_last: V16PodU64::new(value.fund_px_last),
            slot_last: V16PodU64::new(value.slot_last),
            a_long: V16PodU128::new(value.a_long),
            a_short: V16PodU128::new(value.a_short),
            k_long: V16PodI128::new(value.k_long),
            k_short: V16PodI128::new(value.k_short),
            f_long_num: V16PodI128::new(value.f_long_num),
            f_short_num: V16PodI128::new(value.f_short_num),
            k_epoch_start_long: V16PodI128::new(value.k_epoch_start_long),
            k_epoch_start_short: V16PodI128::new(value.k_epoch_start_short),
            f_epoch_start_long_num: V16PodI128::new(value.f_epoch_start_long_num),
            f_epoch_start_short_num: V16PodI128::new(value.f_epoch_start_short_num),
            b_long_num: V16PodU128::new(value.b_long_num),
            b_short_num: V16PodU128::new(value.b_short_num),
            b_epoch_start_long_num: V16PodU128::new(value.b_epoch_start_long_num),
            b_epoch_start_short_num: V16PodU128::new(value.b_epoch_start_short_num),
            oi_eff_long_q: V16PodU128::new(value.oi_eff_long_q),
            oi_eff_short_q: V16PodU128::new(value.oi_eff_short_q),
            stored_pos_count_long: V16PodU64::new(value.stored_pos_count_long),
            stored_pos_count_short: V16PodU64::new(value.stored_pos_count_short),
            stale_account_count_long: V16PodU64::new(value.stale_account_count_long),
            stale_account_count_short: V16PodU64::new(value.stale_account_count_short),
            pending_obligation_count_long: V16PodU64::new(value.pending_obligation_count_long),
            pending_obligation_count_short: V16PodU64::new(value.pending_obligation_count_short),
            loss_weight_sum_long: V16PodU128::new(value.loss_weight_sum_long),
            loss_weight_sum_short: V16PodU128::new(value.loss_weight_sum_short),
            social_loss_remainder_long_num: V16PodU128::new(value.social_loss_remainder_long_num),
            social_loss_remainder_short_num: V16PodU128::new(value.social_loss_remainder_short_num),
            social_loss_dust_long_num: V16PodU128::new(value.social_loss_dust_long_num),
            social_loss_dust_short_num: V16PodU128::new(value.social_loss_dust_short_num),
            explicit_unallocated_loss_long: V16PodU128::new(value.explicit_unallocated_loss_long),
            explicit_unallocated_loss_short: V16PodU128::new(value.explicit_unallocated_loss_short),
            epoch_long: V16PodU64::new(value.epoch_long),
            epoch_short: V16PodU64::new(value.epoch_short),
            mode_long: encode_side_mode(value.mode_long),
            mode_short: encode_side_mode(value.mode_short),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<AssetStateV16> {
        let out = AssetStateV16 {
            market_id: self.market_id.get(),
            retired_slot: self.retired_slot.get(),
            lifecycle: decode_asset_lifecycle(self.lifecycle)?,
            raw_oracle_target_price: self.raw_oracle_target_price.get(),
            effective_price: self.effective_price.get(),
            fund_px_last: self.fund_px_last.get(),
            slot_last: self.slot_last.get(),
            a_long: self.a_long.get(),
            a_short: self.a_short.get(),
            k_long: self.k_long.get(),
            k_short: self.k_short.get(),
            f_long_num: self.f_long_num.get(),
            f_short_num: self.f_short_num.get(),
            k_epoch_start_long: self.k_epoch_start_long.get(),
            k_epoch_start_short: self.k_epoch_start_short.get(),
            f_epoch_start_long_num: self.f_epoch_start_long_num.get(),
            f_epoch_start_short_num: self.f_epoch_start_short_num.get(),
            b_long_num: self.b_long_num.get(),
            b_short_num: self.b_short_num.get(),
            b_epoch_start_long_num: self.b_epoch_start_long_num.get(),
            b_epoch_start_short_num: self.b_epoch_start_short_num.get(),
            oi_eff_long_q: self.oi_eff_long_q.get(),
            oi_eff_short_q: self.oi_eff_short_q.get(),
            stored_pos_count_long: self.stored_pos_count_long.get(),
            stored_pos_count_short: self.stored_pos_count_short.get(),
            stale_account_count_long: self.stale_account_count_long.get(),
            stale_account_count_short: self.stale_account_count_short.get(),
            pending_obligation_count_long: self.pending_obligation_count_long.get(),
            pending_obligation_count_short: self.pending_obligation_count_short.get(),
            loss_weight_sum_long: self.loss_weight_sum_long.get(),
            loss_weight_sum_short: self.loss_weight_sum_short.get(),
            social_loss_remainder_long_num: self.social_loss_remainder_long_num.get(),
            social_loss_remainder_short_num: self.social_loss_remainder_short_num.get(),
            social_loss_dust_long_num: self.social_loss_dust_long_num.get(),
            social_loss_dust_short_num: self.social_loss_dust_short_num.get(),
            explicit_unallocated_loss_long: self.explicit_unallocated_loss_long.get(),
            explicit_unallocated_loss_short: self.explicit_unallocated_loss_short.get(),
            epoch_long: self.epoch_long.get(),
            epoch_short: self.epoch_short.get(),
            mode_long: decode_side_mode(self.mode_long)?,
            mode_short: decode_side_mode(self.mode_short)?,
        };
        validate_non_min_i128(out.k_long)?;
        validate_non_min_i128(out.k_short)?;
        validate_non_min_i128(out.f_long_num)?;
        validate_non_min_i128(out.f_short_num)?;
        validate_non_min_i128(out.k_epoch_start_long)?;
        validate_non_min_i128(out.k_epoch_start_short)?;
        validate_non_min_i128(out.f_epoch_start_long_num)?;
        validate_non_min_i128(out.f_epoch_start_short_num)?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct EngineAssetSlotV16Account {
    pub asset: AssetStateV16Account,
    pub insurance_domain_budget_long: V16PodU128,
    pub insurance_domain_budget_short: V16PodU128,
    pub insurance_domain_spent_long: V16PodU128,
    pub insurance_domain_spent_short: V16PodU128,
    pub pending_domain_loss_barrier_long: V16PodU64,
    pub pending_domain_loss_barrier_short: V16PodU64,
    pub source_credit_long: SourceCreditStateV16Account,
    pub source_credit_short: SourceCreditStateV16Account,
    pub backing_long: BackingBucketV16Account,
    pub backing_short: BackingBucketV16Account,
    pub insurance_reservation_long: InsuranceCreditReservationV16Account,
    pub insurance_reservation_short: InsuranceCreditReservationV16Account,
}

fn asset_contributes_to_loss_stale_summary(asset: AssetStateV16) -> bool {
    matches!(
        asset.lifecycle,
        AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly
    ) && (asset.oi_eff_long_q != 0
        || asset.oi_eff_short_q != 0
        || asset.stored_pos_count_long != 0
        || asset.stored_pos_count_short != 0
        || asset.stale_account_count_long != 0
        || asset.stale_account_count_short != 0
        || asset.pending_obligation_count_long != 0
        || asset.pending_obligation_count_short != 0
        || asset.loss_weight_sum_long != 0
        || asset.loss_weight_sum_short != 0)
}

pub trait MarketSlotV16View {
    fn engine_slot(&self) -> &EngineAssetSlotV16Account;
}

pub trait MarketSlotV16ViewMut: MarketSlotV16View {
    fn engine_slot_mut(&mut self) -> &mut EngineAssetSlotV16Account;
}

impl MarketSlotV16View for EngineAssetSlotV16Account {
    fn engine_slot(&self) -> &EngineAssetSlotV16Account {
        self
    }
}

impl MarketSlotV16ViewMut for EngineAssetSlotV16Account {
    fn engine_slot_mut(&mut self) -> &mut EngineAssetSlotV16Account {
        self
    }
}

impl EngineAssetSlotV16Account {
    fn is_zero_fill_or_canonical_disabled_slot(&self) -> V16Result<bool> {
        if !Self::asset_account_is_empty_for_activation(self.asset) {
            return Ok(false);
        }
        Ok((self.insurance_domain_budget_long.get() == 0
            || self.insurance_domain_budget_long.get() == MAX_VAULT_TVL)
            && (self.insurance_domain_budget_short.get() == 0
                || self.insurance_domain_budget_short.get() == MAX_VAULT_TVL)
            && self.insurance_domain_spent_long.get() == 0
            && self.insurance_domain_spent_short.get() == 0
            && self.pending_domain_loss_barrier_long.get() == 0
            && self.pending_domain_loss_barrier_short.get() == 0
            && Self::source_credit_account_is_empty_for_activation(self.source_credit_long)
            && Self::source_credit_account_is_empty_for_activation(self.source_credit_short)
            && Self::backing_bucket_account_is_empty_for_activation(self.backing_long)
            && Self::backing_bucket_account_is_empty_for_activation(self.backing_short)
            && Self::insurance_reservation_account_is_empty_for_activation(
                self.insurance_reservation_long,
            )
            && Self::insurance_reservation_account_is_empty_for_activation(
                self.insurance_reservation_short,
            ))
    }

    pub fn validate_market_id_binding(&self) -> V16Result<()> {
        let asset = self.asset.try_to_runtime()?;
        let long_bucket = self.backing_long.try_to_runtime()?;
        let short_bucket = self.backing_short.try_to_runtime()?;
        if long_bucket.market_id != asset.market_id || short_bucket.market_id != asset.market_id {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    pub fn empty_for_market(market_id: u64) -> Self {
        let backing = BackingBucketV16::empty_for_market(market_id);
        Self {
            asset: AssetStateV16Account::default(),
            insurance_domain_budget_long: V16PodU128::default(),
            insurance_domain_budget_short: V16PodU128::default(),
            insurance_domain_spent_long: V16PodU128::default(),
            insurance_domain_spent_short: V16PodU128::default(),
            pending_domain_loss_barrier_long: V16PodU64::default(),
            pending_domain_loss_barrier_short: V16PodU64::default(),
            source_credit_long: SourceCreditStateV16Account::from_runtime(
                &SourceCreditStateV16::EMPTY,
            ),
            source_credit_short: SourceCreditStateV16Account::from_runtime(
                &SourceCreditStateV16::EMPTY,
            ),
            backing_long: BackingBucketV16Account::from_runtime(&backing),
            backing_short: BackingBucketV16Account::from_runtime(&backing),
            insurance_reservation_long: InsuranceCreditReservationV16Account::from_runtime(
                &InsuranceCreditReservationV16::EMPTY,
            ),
            insurance_reservation_short: InsuranceCreditReservationV16Account::from_runtime(
                &InsuranceCreditReservationV16::EMPTY,
            ),
        }
    }

    fn source_credit_account_is_empty_for_activation(state: SourceCreditStateV16Account) -> bool {
        let all_non_rate_fields_empty = state.positive_claim_bound_num.get() == 0
            && state.exact_positive_claim_num.get() == 0
            && state.fresh_reserved_backing_num.get() == 0
            && state.spent_backing_num.get() == 0
            && state.provider_receivable_num.get() == 0
            && state.valid_liened_backing_num.get() == 0
            && state.impaired_liened_backing_num.get() == 0
            && state.insurance_credit_reserved_num.get() == 0
            && state.valid_liened_insurance_num.get() == 0
            && state.impaired_liened_insurance_num.get() == 0
            && state.credit_epoch.get() == 0;
        all_non_rate_fields_empty
            && (state.credit_rate_num.get() == 0
                || state.credit_rate_num.get() == CREDIT_RATE_SCALE)
    }

    fn backing_bucket_account_is_empty_for_activation(state: BackingBucketV16Account) -> bool {
        state.market_id.get() == 0
            && state.fresh_unliened_backing_num.get() == 0
            && state.valid_liened_backing_num.get() == 0
            && state.consumed_liened_backing_num.get() == 0
            && state.impaired_liened_backing_num.get() == 0
            && state.expiry_slot.get() == 0
            && state.status == 0
    }

    fn insurance_reservation_account_is_empty_for_activation(
        state: InsuranceCreditReservationV16Account,
    ) -> bool {
        state.insurance_credit_reserved_num.get() == 0
            && state.valid_liened_insurance_num.get() == 0
            && state.impaired_liened_insurance_num.get() == 0
            && state.consumed_insurance_num.get() == 0
            && state.source_credit_epoch.get() == 0
    }

    fn asset_account_is_empty_for_activation(asset: AssetStateV16Account) -> bool {
        let a_shape = (asset.a_long.get() == 0 && asset.a_short.get() == 0)
            || (asset.a_long.get() == ADL_ONE && asset.a_short.get() == ADL_ONE);
        asset.lifecycle == 0
            && asset.market_id.get() == 0
            && a_shape
            && asset.k_long.get() == 0
            && asset.k_short.get() == 0
            && asset.f_long_num.get() == 0
            && asset.f_short_num.get() == 0
            && asset.k_epoch_start_long.get() == 0
            && asset.k_epoch_start_short.get() == 0
            && asset.f_epoch_start_long_num.get() == 0
            && asset.f_epoch_start_short_num.get() == 0
            && asset.b_long_num.get() == 0
            && asset.b_short_num.get() == 0
            && asset.b_epoch_start_long_num.get() == 0
            && asset.b_epoch_start_short_num.get() == 0
            && asset.oi_eff_long_q.get() == 0
            && asset.oi_eff_short_q.get() == 0
            && asset.stored_pos_count_long.get() == 0
            && asset.stored_pos_count_short.get() == 0
            && asset.stale_account_count_long.get() == 0
            && asset.stale_account_count_short.get() == 0
            && asset.pending_obligation_count_long.get() == 0
            && asset.pending_obligation_count_short.get() == 0
            && asset.loss_weight_sum_long.get() == 0
            && asset.loss_weight_sum_short.get() == 0
            && asset.social_loss_remainder_long_num.get() == 0
            && asset.social_loss_remainder_short_num.get() == 0
            && asset.social_loss_dust_long_num.get() == 0
            && asset.social_loss_dust_short_num.get() == 0
            && asset.explicit_unallocated_loss_long.get() == 0
            && asset.explicit_unallocated_loss_short.get() == 0
            && asset.retired_slot.get() == 0
            && asset.raw_oracle_target_price.get() == 0
            && asset.effective_price.get() == 0
            && asset.fund_px_last.get() == 0
            && asset.slot_last.get() == 0
            && asset.epoch_long.get() == 0
            && asset.epoch_short.get() == 0
            && asset.mode_long == 0
            && asset.mode_short == 0
    }

    fn asset_state_is_empty_for_activation(asset: AssetStateV16) -> bool {
        let a_shape = (asset.a_long == 0 && asset.a_short == 0)
            || (asset.a_long == ADL_ONE && asset.a_short == ADL_ONE);
        a_shape
            && asset.k_long == 0
            && asset.k_short == 0
            && asset.f_long_num == 0
            && asset.f_short_num == 0
            && asset.k_epoch_start_long == 0
            && asset.k_epoch_start_short == 0
            && asset.f_epoch_start_long_num == 0
            && asset.f_epoch_start_short_num == 0
            && asset.b_long_num == 0
            && asset.b_short_num == 0
            && asset.b_epoch_start_long_num == 0
            && asset.b_epoch_start_short_num == 0
            && asset.oi_eff_long_q == 0
            && asset.oi_eff_short_q == 0
            && asset.stored_pos_count_long == 0
            && asset.stored_pos_count_short == 0
            && asset.stale_account_count_long == 0
            && asset.stale_account_count_short == 0
            && asset.pending_obligation_count_long == 0
            && asset.pending_obligation_count_short == 0
            && asset.loss_weight_sum_long == 0
            && asset.loss_weight_sum_short == 0
            && asset.social_loss_remainder_long_num == 0
            && asset.social_loss_remainder_short_num == 0
            && asset.social_loss_dust_long_num == 0
            && asset.social_loss_dust_short_num == 0
            && asset.explicit_unallocated_loss_long == 0
            && asset.explicit_unallocated_loss_short == 0
            && asset.mode_long == SideModeV16::Normal
            && asset.mode_short == SideModeV16::Normal
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct MarketGroupV16HeaderAccount {
    pub market_group_id: [u8; 32],
    pub config: V16ConfigAccount,
    pub asset_slot_capacity: V16PodU32,
    pub vault: V16PodU128,
    pub insurance: V16PodU128,
    pub c_tot: V16PodU128,
    pub pnl_pos_tot: V16PodU128,
    pub pnl_pos_bound_tot_num: V16PodU128,
    pub pnl_pos_bound_tot: V16PodU128,
    pub pnl_matured_pos_tot: V16PodU128,
    pub materialized_portfolio_count: V16PodU64,
    pub stale_certificate_count: V16PodU64,
    pub b_stale_account_count: V16PodU64,
    pub negative_pnl_account_count: V16PodU64,
    pub risk_epoch: V16PodU64,
    pub asset_set_epoch: V16PodU64,
    pub asset_activation_count: V16PodU64,
    pub last_asset_activation_slot: V16PodU64,
    pub next_market_id: V16PodU64,
    pub oracle_epoch: V16PodU64,
    pub funding_epoch: V16PodU64,
    pub slot_last: V16PodU64,
    pub current_slot: V16PodU64,
    pub bankruptcy_hlock_active: u8,
    pub threshold_stress_active: u8,
    pub loss_stale_active: u8,
    pub recovery_reason: V16OptionalRecoveryReasonAccount,
    pub mode: u8,
    pub resolved_slot: V16PodU64,
    pub payout_snapshot: V16PodU128,
    pub payout_snapshot_pnl_pos_tot: V16PodU128,
    pub payout_snapshot_captured: u8,
    pub resolved_payout_ledger: ResolvedPayoutLedgerV16Account,
}

impl Default for MarketGroupV16HeaderAccount {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

impl MarketGroupV16HeaderAccount {
    pub fn dynamic_asset_slot_stride<T: MarketWrapperPod>() -> usize {
        core::mem::size_of::<Market<T>>()
    }

    pub fn dynamic_market_group_account_len<T: MarketWrapperPod>(
        asset_slot_capacity: usize,
    ) -> V16Result<usize> {
        let slot_len = Self::dynamic_asset_slot_stride::<T>();
        core::mem::size_of::<Self>()
            .checked_add(
                asset_slot_capacity
                    .checked_mul(slot_len)
                    .ok_or(V16Error::ArithmeticOverflow)?,
            )
            .ok_or(V16Error::ArithmeticOverflow)
    }

    pub fn dynamic_asset_slot_offset<T: MarketWrapperPod>(asset_index: usize) -> V16Result<usize> {
        let slot_len = Self::dynamic_asset_slot_stride::<T>();
        core::mem::size_of::<Self>()
            .checked_add(
                asset_index
                    .checked_mul(slot_len)
                    .ok_or(V16Error::ArithmeticOverflow)?,
            )
            .ok_or(V16Error::ArithmeticOverflow)
    }

    pub fn dynamic_asset_slot_capacity_from_account_len<T: MarketWrapperPod>(
        account_len: usize,
    ) -> V16Result<usize> {
        let header_len = core::mem::size_of::<Self>();
        if account_len < header_len {
            return Err(V16Error::InvalidConfig);
        }
        let slot_len = Self::dynamic_asset_slot_stride::<T>();
        let trailing_len = account_len - header_len;
        if slot_len == 0 || trailing_len % slot_len != 0 {
            return Err(V16Error::InvalidConfig);
        }
        Ok(trailing_len / slot_len)
    }

    pub fn validate_dynamic_market_group_account_len<T: MarketWrapperPod>(
        account_len: usize,
        asset_slot_capacity: usize,
    ) -> V16Result<()> {
        let expected_len = Self::dynamic_market_group_account_len::<T>(asset_slot_capacity)?;
        if account_len != expected_len {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    pub fn new_dynamic(
        market_group_id: [u8; 32],
        config: V16Config,
        asset_slot_capacity: u32,
        init_slot: u64,
    ) -> V16Result<Self> {
        if asset_slot_capacity < config.max_market_slots {
            return Err(V16Error::InvalidConfig);
        }
        config.validate_public_user_fund()?;
        Ok(Self {
            market_group_id,
            config: V16ConfigAccount::from_runtime(&config),
            asset_slot_capacity: V16PodU32::new(asset_slot_capacity),
            vault: V16PodU128::default(),
            insurance: V16PodU128::default(),
            c_tot: V16PodU128::default(),
            pnl_pos_tot: V16PodU128::default(),
            pnl_pos_bound_tot_num: V16PodU128::default(),
            pnl_pos_bound_tot: V16PodU128::default(),
            pnl_matured_pos_tot: V16PodU128::default(),
            materialized_portfolio_count: V16PodU64::default(),
            stale_certificate_count: V16PodU64::default(),
            b_stale_account_count: V16PodU64::default(),
            negative_pnl_account_count: V16PodU64::default(),
            risk_epoch: V16PodU64::default(),
            asset_set_epoch: V16PodU64::default(),
            asset_activation_count: V16PodU64::default(),
            last_asset_activation_slot: V16PodU64::default(),
            next_market_id: V16PodU64::new(1),
            oracle_epoch: V16PodU64::default(),
            funding_epoch: V16PodU64::default(),
            slot_last: V16PodU64::new(init_slot),
            current_slot: V16PodU64::new(init_slot),
            bankruptcy_hlock_active: 0,
            threshold_stress_active: 0,
            loss_stale_active: 0,
            recovery_reason: V16OptionalRecoveryReasonAccount::default(),
            mode: encode_market_mode(MarketModeV16::Live),
            resolved_slot: V16PodU64::default(),
            payout_snapshot: V16PodU128::default(),
            payout_snapshot_pnl_pos_tot: V16PodU128::default(),
            payout_snapshot_captured: 0,
            resolved_payout_ledger: ResolvedPayoutLedgerV16Account::from_runtime(
                &ResolvedPayoutLedgerV16::EMPTY,
            ),
        })
    }

    pub fn grow_asset_slot_capacity_not_atomic(
        &mut self,
        new_asset_slot_capacity: u32,
        new_max_market_slots: u32,
    ) -> V16Result<()> {
        let mut config = self.config.try_to_runtime_shape()?;
        let old_capacity = self.asset_slot_capacity.get();
        if decode_market_mode(self.mode)? != MarketModeV16::Live
            || new_asset_slot_capacity < old_capacity
            || new_max_market_slots < config.max_market_slots
            || new_max_market_slots > new_asset_slot_capacity
        {
            return Err(V16Error::InvalidConfig);
        }
        config.max_market_slots = new_max_market_slots;
        config.validate_public_user_fund_shape()?;
        let next_asset_set_epoch = self
            .asset_set_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        let next_risk_epoch = self
            .risk_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        self.asset_slot_capacity = V16PodU32::new(new_asset_slot_capacity);
        self.config = V16ConfigAccount::from_runtime(&config);
        self.asset_set_epoch = V16PodU64::new(next_asset_set_epoch);
        self.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    fn validate_dynamic_market_slots_shape<S: MarketSlotV16View>(
        &self,
        slots: &[S],
    ) -> V16Result<()> {
        let configured_market_slots = self.config.max_market_slots.get() as usize;
        let capacity = self.asset_slot_capacity.get() as usize;
        Self::validate_dynamic_market_slots_len_static(
            slots.len(),
            capacity,
            configured_market_slots,
        )?;
        if capacity == configured_market_slots {
            return Ok(());
        }
        let mut slot_index = configured_market_slots;
        while slot_index < capacity {
            self.validate_dynamic_market_slot_shape_at(slot_index, &slots[slot_index])?;
            slot_index += 1;
        }
        Ok(())
    }

    fn validate_dynamic_market_slots_len_static(
        supplied_len: usize,
        capacity: usize,
        configured_market_slots: usize,
    ) -> V16Result<()> {
        if supplied_len != capacity || capacity < configured_market_slots {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    #[cfg(kani)]
    pub fn kani_validate_dynamic_market_slots_len(
        supplied_len: usize,
        capacity: usize,
        configured_market_slots: usize,
    ) -> V16Result<()> {
        Self::validate_dynamic_market_slots_len_static(
            supplied_len,
            capacity,
            configured_market_slots,
        )
    }

    fn validate_dynamic_market_slot_shape_at<S: MarketSlotV16View>(
        &self,
        slot_index: usize,
        slot: &S,
    ) -> V16Result<()> {
        let configured_market_slots = self.config.max_market_slots.get() as usize;
        let capacity = self.asset_slot_capacity.get() as usize;
        if slot_index >= capacity || capacity < configured_market_slots {
            return Err(V16Error::InvalidConfig);
        }
        if slot_index >= configured_market_slots
            && !slot
                .engine_slot()
                .is_zero_fill_or_canonical_disabled_slot()?
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    #[cfg(kani)]
    pub fn kani_validate_dynamic_market_slot_shape_at<S: MarketSlotV16View>(
        &self,
        slot_index: usize,
        slot: &S,
    ) -> V16Result<()> {
        self.validate_dynamic_market_slot_shape_at(slot_index, slot)
    }

    pub fn activate_empty_market_slot_not_atomic<S: MarketSlotV16ViewMut>(
        &mut self,
        asset_index: u32,
        slot: &mut S,
        authenticated_price: u64,
        now_slot: u64,
    ) -> V16Result<()> {
        let config = self.config.try_to_runtime_shape()?;
        let capacity = self.asset_slot_capacity.get();
        if asset_index >= capacity || asset_index as usize >= config.max_market_slots as usize {
            return Err(V16Error::InvalidLeg);
        }
        if decode_market_mode(self.mode)? != MarketModeV16::Live
            || authenticated_price == 0
            || authenticated_price > MAX_ORACLE_PRICE
            || now_slot < self.current_slot.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        config.validate_public_user_fund_shape()?;
        if self.asset_activation_count.get() != 0 {
            let elapsed = now_slot
                .checked_sub(self.last_asset_activation_slot.get())
                .ok_or(V16Error::Stale)?;
            if elapsed < config.asset_activation_cooldown_slots {
                return Err(V16Error::LockActive);
            }
        }
        let slot = slot.engine_slot_mut();
        let previous_asset = slot.asset.try_to_runtime()?;
        if !EngineAssetSlotV16Account::asset_state_is_empty_for_activation(previous_asset) {
            return Err(V16Error::LockActive);
        }
        match previous_asset.lifecycle {
            AssetLifecycleV16::Disabled => {
                if previous_asset.market_id != 0 {
                    return Err(V16Error::LockActive);
                }
            }
            AssetLifecycleV16::Retired => {
                if previous_asset.market_id == 0 || previous_asset.retired_slot == 0 {
                    return Err(V16Error::LockActive);
                }
                let elapsed = now_slot
                    .checked_sub(previous_asset.retired_slot)
                    .ok_or(V16Error::Stale)?;
                if elapsed < config.asset_activation_cooldown_slots {
                    return Err(V16Error::LockActive);
                }
                slot.validate_market_id_binding()?;
            }
            _ => return Err(V16Error::LockActive),
        }
        if slot.insurance_domain_spent_long.get() != 0
            || slot.insurance_domain_spent_short.get() != 0
            || slot.pending_domain_loss_barrier_long.get() != 0
            || slot.pending_domain_loss_barrier_short.get() != 0
            || !EngineAssetSlotV16Account::source_credit_account_is_empty_for_activation(
                slot.source_credit_long,
            )
            || !EngineAssetSlotV16Account::source_credit_account_is_empty_for_activation(
                slot.source_credit_short,
            )
            || !slot.backing_long.try_to_runtime()?.is_empty_amount_shape()
            || !slot.backing_short.try_to_runtime()?.is_empty_amount_shape()
            || slot.insurance_reservation_long.try_to_runtime()?
                != InsuranceCreditReservationV16::EMPTY
            || slot.insurance_reservation_short.try_to_runtime()?
                != InsuranceCreditReservationV16::EMPTY
        {
            return Err(V16Error::LockActive);
        }

        let market_id = self.next_market_id.get();
        if market_id == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let next_market_id = market_id.checked_add(1).ok_or(V16Error::CounterOverflow)?;
        let next_activation_count = self
            .asset_activation_count
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        let next_asset_set_epoch = self
            .asset_set_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        let next_risk_epoch = self
            .risk_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        let mut asset = AssetStateV16::default();
        asset.market_id = market_id;
        asset.lifecycle = AssetLifecycleV16::Active;
        asset.raw_oracle_target_price = authenticated_price;
        asset.effective_price = authenticated_price;
        asset.fund_px_last = authenticated_price;
        asset.slot_last = now_slot;
        *slot = EngineAssetSlotV16Account {
            asset: AssetStateV16Account::from_runtime(&asset),
            insurance_domain_budget_long: V16PodU128::default(),
            insurance_domain_budget_short: V16PodU128::default(),
            insurance_domain_spent_long: V16PodU128::default(),
            insurance_domain_spent_short: V16PodU128::default(),
            pending_domain_loss_barrier_long: V16PodU64::default(),
            pending_domain_loss_barrier_short: V16PodU64::default(),
            source_credit_long: SourceCreditStateV16Account::from_runtime(
                &SourceCreditStateV16::EMPTY,
            ),
            source_credit_short: SourceCreditStateV16Account::from_runtime(
                &SourceCreditStateV16::EMPTY,
            ),
            backing_long: BackingBucketV16Account::from_runtime(
                &BackingBucketV16::empty_for_market(market_id),
            ),
            backing_short: BackingBucketV16Account::from_runtime(
                &BackingBucketV16::empty_for_market(market_id),
            ),
            insurance_reservation_long: InsuranceCreditReservationV16Account::from_runtime(
                &InsuranceCreditReservationV16::EMPTY,
            ),
            insurance_reservation_short: InsuranceCreditReservationV16Account::from_runtime(
                &InsuranceCreditReservationV16::EMPTY,
            ),
        };
        self.next_market_id = V16PodU64::new(next_market_id);
        self.current_slot = V16PodU64::new(now_slot);
        self.asset_activation_count = V16PodU64::new(next_activation_count);
        self.last_asset_activation_slot = V16PodU64::new(now_slot);
        self.asset_set_epoch = V16PodU64::new(next_asset_set_epoch);
        self.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn activate_empty_asset_slot_not_atomic(
        &mut self,
        asset_index: u32,
        slot: &mut EngineAssetSlotV16Account,
        authenticated_price: u64,
        now_slot: u64,
    ) -> V16Result<()> {
        self.activate_empty_market_slot_not_atomic(asset_index, slot, authenticated_price, now_slot)
    }
}

impl<'a, T> MarketGroupV16View<'a, T> {
    pub fn validate_shape(&self) -> V16Result<()> {
        self.header
            .validate_dynamic_market_slots_shape(self.markets)?;
        self.header.config.try_to_runtime_shape()?;
        decode_bool(self.header.bankruptcy_hlock_active)?;
        decode_bool(self.header.threshold_stress_active)?;
        decode_bool(self.header.loss_stale_active)?;
        decode_bool(self.header.payout_snapshot_captured)?;
        self.header.recovery_reason.try_to_runtime()?;
        let mode = decode_market_mode(self.header.mode)?;
        let resolved_ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        if self.header.vault.get() > MAX_VAULT_TVL {
            return Err(V16Error::InvalidConfig);
        }
        if self.header.c_tot.get() > self.header.vault.get()
            || self.header.insurance.get() > self.header.vault.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        if self.header.pnl_matured_pos_tot.get() > self.header.pnl_pos_tot.get()
            || self.header.pnl_pos_bound_tot.get() < self.header.pnl_pos_tot.get()
            || self.header.slot_last.get() > self.header.current_slot.get()
            || self.header.next_market_id.get() == 0
        {
            return Err(V16Error::InvalidConfig);
        }
        let derived_bound =
            V16Core::amount_from_bound_num(self.header.pnl_pos_bound_tot_num.get())?;
        if self.header.pnl_pos_bound_tot.get() != derived_bound {
            return Err(V16Error::InvalidConfig);
        }
        let exact_bound_num = V16Core::bound_num_from_amount(self.header.pnl_pos_tot.get())?;
        if self.header.pnl_pos_bound_tot_num.get() < exact_bound_num {
            return Err(V16Error::InvalidConfig);
        }
        if !decode_bool(self.header.payout_snapshot_captured)?
            && resolved_ledger != ResolvedPayoutLedgerV16::EMPTY
        {
            return Err(V16Error::InvalidConfig);
        }
        if self.header.asset_activation_count.get() == 0 {
            if self.header.last_asset_activation_slot.get() != 0 {
                return Err(V16Error::InvalidConfig);
            }
        } else if self.header.last_asset_activation_slot.get() > self.header.current_slot.get() {
            return Err(V16Error::InvalidConfig);
        }

        let configured_assets = self.header.config.max_market_slots.get() as usize;
        let mut backing_provider_earnings = 0u128;
        let mut live_source_credit_insurance_atoms = 0u128;
        let mut live_domain_budget_remaining_atoms = 0u128;
        let mut i = 0usize;
        while i < self.markets.len() {
            let slot = self.markets[i].engine_slot();
            backing_provider_earnings = backing_provider_earnings
                .checked_add(slot.backing_long.utilization_fee_earnings.get())
                .and_then(|v| v.checked_add(slot.backing_short.utilization_fee_earnings.get()))
                .ok_or(V16Error::ArithmeticOverflow)?;
            let asset = slot.asset.try_to_runtime()?;
            if i >= configured_assets {
                if asset.lifecycle != AssetLifecycleV16::Disabled
                    || asset.market_id != 0
                    || !EngineAssetSlotV16Account::asset_state_is_empty_for_activation(asset)
                    || !slot.is_zero_fill_or_canonical_disabled_slot()?
                {
                    return Err(V16Error::InvalidConfig);
                }
                i += 1;
                continue;
            }
            if asset.lifecycle == AssetLifecycleV16::Disabled
                && asset.market_id == 0
                && slot.is_zero_fill_or_canonical_disabled_slot()?
            {
                i += 1;
                continue;
            }
            Self::validate_asset_shape_for_view(
                asset,
                mode,
                self.header.current_slot.get(),
                self.header.next_market_id.get(),
            )?;
            Self::validate_domain_shape_for_view(
                asset.market_id,
                slot.source_credit_long.try_to_runtime()?,
                slot.backing_long.try_to_runtime()?,
                slot.insurance_reservation_long.try_to_runtime()?,
                slot.insurance_domain_budget_long.get(),
                slot.insurance_domain_spent_long.get(),
                slot.pending_domain_loss_barrier_long.get(),
                self.header.current_slot.get(),
                &mut live_source_credit_insurance_atoms,
            )?;
            live_domain_budget_remaining_atoms = live_domain_budget_remaining_atoms
                .checked_add(
                    slot.insurance_domain_budget_long
                        .get()
                        .checked_sub(slot.insurance_domain_spent_long.get())
                        .ok_or(V16Error::InvalidConfig)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            Self::validate_domain_shape_for_view(
                asset.market_id,
                slot.source_credit_short.try_to_runtime()?,
                slot.backing_short.try_to_runtime()?,
                slot.insurance_reservation_short.try_to_runtime()?,
                slot.insurance_domain_budget_short.get(),
                slot.insurance_domain_spent_short.get(),
                slot.pending_domain_loss_barrier_short.get(),
                self.header.current_slot.get(),
                &mut live_source_credit_insurance_atoms,
            )?;
            live_domain_budget_remaining_atoms = live_domain_budget_remaining_atoms
                .checked_add(
                    slot.insurance_domain_budget_short
                        .get()
                        .checked_sub(slot.insurance_domain_spent_short.get())
                        .ok_or(V16Error::InvalidConfig)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            i += 1;
        }
        let senior = self
            .header
            .c_tot
            .get()
            .checked_add(self.header.insurance.get())
            .and_then(|v| v.checked_add(backing_provider_earnings))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if senior > self.header.vault.get() {
            return Err(V16Error::InvalidConfig);
        }
        if live_source_credit_insurance_atoms > self.header.insurance.get()
            || live_domain_budget_remaining_atoms > self.header.insurance.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn validate_asset_shape_for_view(
        asset: AssetStateV16,
        mode: MarketModeV16,
        current_slot: u64,
        next_market_id: u64,
    ) -> V16Result<()> {
        if matches!(asset.lifecycle, AssetLifecycleV16::Disabled) {
            if asset.market_id != 0 {
                return Err(V16Error::InvalidConfig);
            }
        } else if asset.market_id == 0 || asset.market_id >= next_market_id {
            return Err(V16Error::InvalidConfig);
        }
        let requires_price = matches!(
            asset.lifecycle,
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly | AssetLifecycleV16::Recovery
        );
        if (requires_price
            && (asset.effective_price == 0
                || asset.effective_price > MAX_ORACLE_PRICE
                || asset.raw_oracle_target_price == 0
                || asset.raw_oracle_target_price > MAX_ORACLE_PRICE
                || asset.fund_px_last == 0
                || asset.fund_px_last > MAX_ORACLE_PRICE))
            || asset.slot_last > current_slot
            || asset.k_long == i128::MIN
            || asset.k_short == i128::MIN
            || asset.f_long_num == i128::MIN
            || asset.f_short_num == i128::MIN
            || asset.k_epoch_start_long == i128::MIN
            || asset.k_epoch_start_short == i128::MIN
            || asset.f_epoch_start_long_num == i128::MIN
            || asset.f_epoch_start_short_num == i128::MIN
            || asset.oi_eff_long_q > crate::MAX_OI_SIDE_Q
            || asset.oi_eff_short_q > crate::MAX_OI_SIDE_Q
            || (mode == MarketModeV16::Live && asset.oi_eff_long_q != asset.oi_eff_short_q)
            || asset.loss_weight_sum_long > SOCIAL_LOSS_DEN
            || asset.loss_weight_sum_short > SOCIAL_LOSS_DEN
            || (asset.oi_eff_long_q != 0 && asset.loss_weight_sum_long == 0)
            || (asset.oi_eff_short_q != 0 && asset.loss_weight_sum_short == 0)
            || (asset.loss_weight_sum_long != 0 && asset.stored_pos_count_long == 0)
            || (asset.loss_weight_sum_short != 0 && asset.stored_pos_count_short == 0)
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn validate_domain_shape_for_view(
        market_id: u64,
        source: SourceCreditStateV16,
        bucket: BackingBucketV16,
        reservation: InsuranceCreditReservationV16,
        budget: u128,
        spent: u128,
        pending_barrier: u64,
        _current_slot: u64,
        live_source_credit_insurance_atoms: &mut u128,
    ) -> V16Result<()> {
        if spent > budget || pending_barrier > 1 {
            return Err(V16Error::InvalidConfig);
        }
        V16Core::validate_source_domain_ledger_parts(market_id, source, bucket, reservation)?;
        let reserved_atoms = V16Core::amount_from_bound_num(source.insurance_credit_reserved_num)?;
        *live_source_credit_insurance_atoms = live_source_credit_insurance_atoms
            .checked_add(reserved_atoms)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if spent
            .checked_add(reserved_atoms)
            .ok_or(V16Error::ArithmeticOverflow)?
            > budget
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }
}

impl<'a, T> MarketGroupV16ViewMut<'a, T> {
    pub fn validate_shape(&self) -> V16Result<()> {
        self.as_view().validate_shape()
    }

    #[inline]
    fn validate_shape_audit_scan(&self) -> V16Result<()> {
        #[cfg(feature = "audit-scan")]
        {
            self.validate_shape()
        }
        #[cfg(not(feature = "audit-scan"))]
        {
            Ok(())
        }
    }

    #[inline]
    fn validate_account_audit_scan(&self, _account: &PortfolioV16View<'_>) -> V16Result<()> {
        #[cfg(feature = "audit-scan")]
        {
            _account.validate_with_market(&self.as_view())
        }
        #[cfg(not(feature = "audit-scan"))]
        {
            Ok(())
        }
    }

    fn validate_account_scalar_preflight(&self, account: &PortfolioV16View<'_>) -> V16Result<()> {
        if account.header.provenance_header.market_group_id != self.header.market_group_id
            || account.header.owner != account.header.provenance_header.owner
            || account.header.provenance_header.version.get() != V16_ACCOUNT_VERSION
            || account.header.provenance_header.layout_discriminator.get()
                != V16_LAYOUT_DISCRIMINATOR
        {
            return Err(V16Error::ProvenanceMismatch);
        }
        let pnl = account.header.pnl.get();
        validate_non_min_i128(pnl)?;
        validate_fee_credits(account.header.fee_credits.get())?;
        if account.header.reserved_pnl.get() > pnl.max(0) as u128 {
            return Err(V16Error::InvalidLeg);
        }
        PortfolioV16View::validate_resolved_payout_receipt_static(
            account.header.resolved_payout_receipt.try_to_runtime()?,
        )?;
        if account.header.close_progress.try_to_runtime()? != CloseProgressLedgerV16::EMPTY {
            account.validate_close_progress_ledger_with_market(&self.as_view())?;
        }
        Ok(())
    }

    fn configured_domain_count(&self) -> V16Result<usize> {
        v16_domain_count_for_market_slots(self.header.config.max_market_slots.get())
    }

    fn residual(&self) -> u128 {
        self.header.vault.get().saturating_sub(
            self.header
                .c_tot
                .get()
                .saturating_add(self.header.insurance.get()),
        )
    }

    fn junior_claim_bound(&self) -> u128 {
        self.header.pnl_pos_bound_tot.get()
    }

    fn domain_asset_side(&self, domain: usize) -> V16Result<(usize, SideV16)> {
        let configured_domains = self.configured_domain_count()?;
        if domain >= configured_domains {
            return Err(V16Error::InvalidLeg);
        }
        let asset_index = domain / 2;
        let side = if domain % 2 == 0 {
            SideV16::Long
        } else {
            SideV16::Short
        };
        if asset_index >= self.markets.len() {
            return Err(V16Error::InvalidLeg);
        }
        Ok((asset_index, side))
    }

    fn insurance_domain_index(&self, asset_index: usize, side: SideV16) -> V16Result<usize> {
        if asset_index >= self.header.config.max_market_slots.get() as usize {
            return Err(V16Error::InvalidLeg);
        }
        let domain = asset_index
            .checked_mul(2)
            .and_then(|v| v.checked_add(encode_side(side) as usize))
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.domain_asset_side(domain)?;
        Ok(domain)
    }

    fn source_credit_for_domain(&self, domain: usize) -> V16Result<SourceCreditStateV16> {
        let source = self.source_credit_for_domain_shape(domain)?;
        V16Core::validate_source_credit_state_static(source)?;
        Ok(source)
    }

    fn source_credit_for_domain_shape(&self, domain: usize) -> V16Result<SourceCreditStateV16> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        let source = match side {
            SideV16::Long => slot.source_credit_long,
            SideV16::Short => slot.source_credit_short,
        };
        let out = SourceCreditStateV16 {
            positive_claim_bound_num: source.positive_claim_bound_num.get(),
            exact_positive_claim_num: source.exact_positive_claim_num.get(),
            fresh_reserved_backing_num: source.fresh_reserved_backing_num.get(),
            spent_backing_num: source.spent_backing_num.get(),
            provider_receivable_num: source.provider_receivable_num.get(),
            valid_liened_backing_num: source.valid_liened_backing_num.get(),
            impaired_liened_backing_num: source.impaired_liened_backing_num.get(),
            insurance_credit_reserved_num: source.insurance_credit_reserved_num.get(),
            valid_liened_insurance_num: source.valid_liened_insurance_num.get(),
            impaired_liened_insurance_num: source.impaired_liened_insurance_num.get(),
            credit_rate_num: source.credit_rate_num.get(),
            credit_epoch: source.credit_epoch.get(),
        };
        V16Core::validate_source_credit_state_shape_static(out)?;
        Ok(out)
    }

    fn set_source_credit_for_domain(
        &mut self,
        domain: usize,
        source: SourceCreditStateV16,
    ) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => {
                slot.source_credit_long = SourceCreditStateV16Account::from_runtime(&source)
            }
            SideV16::Short => {
                slot.source_credit_short = SourceCreditStateV16Account::from_runtime(&source)
            }
        }
        Ok(())
    }

    fn backing_bucket_for_domain(&self, domain: usize) -> V16Result<BackingBucketV16> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        match side {
            SideV16::Long => slot.backing_long.try_to_runtime(),
            SideV16::Short => slot.backing_short.try_to_runtime(),
        }
    }

    fn set_backing_bucket_for_domain(
        &mut self,
        domain: usize,
        bucket: BackingBucketV16,
    ) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.backing_long = BackingBucketV16Account::from_runtime(&bucket),
            SideV16::Short => slot.backing_short = BackingBucketV16Account::from_runtime(&bucket),
        }
        Ok(())
    }

    fn insurance_reservation_for_domain(
        &self,
        domain: usize,
    ) -> V16Result<InsuranceCreditReservationV16> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        match side {
            SideV16::Long => slot.insurance_reservation_long.try_to_runtime(),
            SideV16::Short => slot.insurance_reservation_short.try_to_runtime(),
        }
    }

    fn validate_source_domain_ledger(&self, domain: usize) -> V16Result<()> {
        let source = self.source_credit_for_domain_shape(domain)?;
        let bucket = self.backing_bucket_for_domain(domain)?;
        let reservation = self.insurance_reservation_for_domain(domain)?;
        let (asset_index, _) = self.domain_asset_side(domain)?;
        let market_id = self.markets[asset_index].engine.asset.market_id.get();
        V16Core::validate_source_domain_ledger_parts(market_id, source, bucket, reservation)
    }

    fn validate_source_domain_ledger_current(&self, domain: usize) -> V16Result<()> {
        self.validate_source_domain_ledger(domain)?;
        let bucket = self.backing_bucket_for_domain(domain)?;
        if bucket.status == BackingBucketStatusV16::Fresh
            && bucket.expiry_slot <= self.header.current_slot.get()
        {
            return Err(V16Error::Stale);
        }
        Ok(())
    }

    fn recompute_source_credit_domain_after_mutation(&mut self, domain: usize) -> V16Result<()> {
        let source = self.source_credit_for_domain_shape(domain)?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    fn refresh_source_credit_domain_after_mutation(&mut self, domain: usize) -> V16Result<()> {
        self.recompute_source_credit_domain_after_mutation(domain)?;
        self.reservation_encumbrance_proof_for_domain(domain)?
            .validate()?;
        self.validate_shape()
    }

    pub fn recompute_source_credit_rate_not_atomic(&mut self, domain: usize) -> V16Result<u128> {
        self.recompute_source_credit_domain_after_mutation(domain)?;
        let rate = self.source_credit_for_domain(domain)?.credit_rate_num;
        self.reservation_encumbrance_proof_for_domain(domain)?
            .validate()?;
        self.validate_shape()?;
        Ok(rate)
    }

    pub fn add_source_positive_claim_bound_not_atomic(
        &mut self,
        domain: usize,
        claim_bound_num: u128,
        exact_claim_num: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if exact_claim_num > claim_bound_num {
            return Err(V16Error::InvalidConfig);
        }
        let source = V16Core::prepare_source_positive_claim_bound_delta(
            self.source_credit_for_domain(domain)?,
            claim_bound_num,
            exact_claim_num,
        )?;
        self.set_source_credit_for_domain(domain, source)?;
        self.refresh_source_credit_domain_after_mutation(domain)
    }

    pub fn reservation_encumbrance_proof_for_domain(
        &self,
        domain: usize,
    ) -> V16Result<ReservationEncumbranceProofV16> {
        let source = self.source_credit_for_domain(domain)?;
        let bucket = self.backing_bucket_for_domain(domain)?;
        let reservation = self.insurance_reservation_for_domain(domain)?;
        Ok(ReservationEncumbranceProofV16 {
            domain: u16::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?,
            exact_positive_claim_num: source.exact_positive_claim_num,
            positive_claim_bound_num: source.positive_claim_bound_num,
            source_fresh_reserved_backing_num: source.fresh_reserved_backing_num,
            source_spent_backing_num: source.spent_backing_num,
            source_provider_receivable_num: source.provider_receivable_num,
            bucket_fresh_unliened_backing_num: bucket.fresh_unliened_backing_num,
            bucket_valid_liened_backing_num: bucket.valid_liened_backing_num,
            bucket_consumed_liened_backing_num: bucket.consumed_liened_backing_num,
            source_valid_liened_backing_num: source.valid_liened_backing_num,
            source_impaired_liened_backing_num: source.impaired_liened_backing_num,
            bucket_impaired_liened_backing_num: bucket.impaired_liened_backing_num,
            source_insurance_credit_reserved_num: source.insurance_credit_reserved_num,
            reservation_insurance_credit_reserved_num: reservation.insurance_credit_reserved_num,
            source_valid_liened_insurance_num: source.valid_liened_insurance_num,
            reservation_valid_liened_insurance_num: reservation.valid_liened_insurance_num,
            source_impaired_liened_insurance_num: source.impaired_liened_insurance_num,
            reservation_impaired_liened_insurance_num: reservation.impaired_liened_insurance_num,
            source_credit_rate_num: source.credit_rate_num,
        })
    }

    pub fn source_credit_lien_proof_for_account_domain(
        &self,
        account: &PortfolioV16View<'_>,
        domain: usize,
    ) -> V16Result<SourceCreditLienAggregateProofV16> {
        self.domain_asset_side(domain)?;
        let source = account
            .source_domains
            .get(domain)
            .ok_or(V16Error::InvalidLeg)?;
        Ok(SourceCreditLienAggregateProofV16 {
            domain: u16::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?,
            source_claim_bound_num: source.source_claim_bound_num.get(),
            face_claim_locked_num: source.source_claim_liened_num.get(),
            counterparty_face_claim_locked_num: source.source_claim_counterparty_liened_num.get(),
            insurance_face_claim_locked_num: source.source_claim_insurance_liened_num.get(),
            effective_credit_reserved: source.source_lien_effective_reserved.get(),
            counterparty_backing_reserved_num: source.source_lien_counterparty_backing_num.get(),
            insurance_backing_reserved_num: source.source_lien_insurance_backing_num.get(),
            impaired_face_claim_num: source.source_claim_impaired_num.get(),
            impaired_effective_credit_reserved: source
                .source_lien_impaired_effective_reserved
                .get(),
        })
    }

    fn reservation_encumbrance_proof_for_domain_parts(
        &self,
        domain: usize,
        source: SourceCreditStateV16,
        bucket: BackingBucketV16,
        reservation: InsuranceCreditReservationV16,
    ) -> V16Result<ReservationEncumbranceProofV16> {
        self.domain_asset_side(domain)?;
        Ok(ReservationEncumbranceProofV16 {
            domain: u16::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?,
            exact_positive_claim_num: source.exact_positive_claim_num,
            positive_claim_bound_num: source.positive_claim_bound_num,
            source_fresh_reserved_backing_num: source.fresh_reserved_backing_num,
            source_spent_backing_num: source.spent_backing_num,
            source_provider_receivable_num: source.provider_receivable_num,
            bucket_fresh_unliened_backing_num: bucket.fresh_unliened_backing_num,
            bucket_valid_liened_backing_num: bucket.valid_liened_backing_num,
            bucket_consumed_liened_backing_num: bucket.consumed_liened_backing_num,
            source_valid_liened_backing_num: source.valid_liened_backing_num,
            source_impaired_liened_backing_num: source.impaired_liened_backing_num,
            bucket_impaired_liened_backing_num: bucket.impaired_liened_backing_num,
            source_insurance_credit_reserved_num: source.insurance_credit_reserved_num,
            reservation_insurance_credit_reserved_num: reservation.insurance_credit_reserved_num,
            source_valid_liened_insurance_num: source.valid_liened_insurance_num,
            reservation_valid_liened_insurance_num: reservation.valid_liened_insurance_num,
            source_impaired_liened_insurance_num: source.impaired_liened_insurance_num,
            reservation_impaired_liened_insurance_num: reservation.impaired_liened_insurance_num,
            source_credit_rate_num: source.credit_rate_num,
        })
    }

    fn fresh_counterparty_backing_expiry_slot(&self, domain: usize) -> V16Result<u64> {
        self.domain_asset_side(domain)?;
        let bucket = self.backing_bucket_for_domain(domain)?;
        if bucket.status == BackingBucketStatusV16::Fresh
            && bucket.expiry_slot > self.header.current_slot.get()
        {
            return Ok(bucket.expiry_slot);
        }
        let config = self.header.config.try_to_runtime_shape()?;
        let freshness_horizon = config
            .max_accrual_dt_slots
            .max(config.h_max)
            .max(config.max_bankrupt_close_lifetime_slots)
            .max(1);
        self.header
            .current_slot
            .get()
            .checked_add(freshness_horizon)
            .ok_or(V16Error::CounterOverflow)
    }

    fn add_fresh_counterparty_backing_unchecked(
        &mut self,
        domain: usize,
        amount: u128,
        expiry_slot: u64,
    ) -> V16Result<()> {
        let bucket = self.backing_bucket_for_domain(domain)?;
        let source = self.source_credit_for_domain(domain)?;
        let (bucket, source) = V16Core::prepare_counterparty_backing_add_delta(
            bucket,
            source,
            amount,
            self.header.current_slot.get(),
            expiry_slot,
        )?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.recompute_source_credit_domain_after_mutation(domain)?;
        self.reservation_encumbrance_proof_for_domain(domain)?
            .validate()
    }

    pub fn add_fresh_counterparty_backing_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
        expiry_slot: u64,
    ) -> V16Result<()> {
        self.add_fresh_counterparty_backing_unchecked(domain, amount, expiry_slot)?;
        self.validate_shape()
    }

    pub fn expire_source_backing_bucket_not_atomic(
        &mut self,
        domain: usize,
        now_slot: u64,
    ) -> V16Result<()> {
        let mut bucket = self.backing_bucket_for_domain(domain)?;
        if bucket.status != BackingBucketStatusV16::Fresh || now_slot < bucket.expiry_slot {
            return Err(V16Error::Stale);
        }
        let mut source = self.source_credit_for_domain(domain)?;
        let expired_unliened = bucket.fresh_unliened_backing_num;
        let expired_liened = bucket.valid_liened_backing_num;
        let expired_total = expired_unliened
            .checked_add(expired_liened)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if source.fresh_reserved_backing_num < expired_total
            || source.valid_liened_backing_num < expired_liened
        {
            return Err(V16Error::CounterUnderflow);
        }
        source.fresh_reserved_backing_num -= expired_total;
        source.valid_liened_backing_num -= expired_liened;
        source.impaired_liened_backing_num = source
            .impaired_liened_backing_num
            .checked_add(expired_liened)
            .ok_or(V16Error::CounterOverflow)?;
        bucket.fresh_unliened_backing_num = 0;
        bucket.valid_liened_backing_num = 0;
        bucket.impaired_liened_backing_num = bucket
            .impaired_liened_backing_num
            .checked_add(expired_liened)
            .ok_or(V16Error::CounterOverflow)?;
        bucket.status = if expired_liened == 0 && bucket.impaired_liened_backing_num == 0 {
            BackingBucketStatusV16::Expired
        } else {
            BackingBucketStatusV16::Impaired
        };
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.refresh_source_credit_domain_after_mutation(domain)
    }

    pub fn create_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.create_source_credit_lien_from_counterparty_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    fn create_source_credit_lien_from_counterparty_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_create_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            self.header.current_slot.get(),
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            bucket,
            self.insurance_reservation_for_domain(domain)?,
        )?
        .validate()?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn release_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_release_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            self.header.current_slot.get(),
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            bucket,
            self.insurance_reservation_for_domain(domain)?,
        )?
        .validate()?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    fn consume_source_credit_lien_from_counterparty_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_consume_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            bucket,
            self.insurance_reservation_for_domain(domain)?,
        )?
        .validate()?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn consume_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.consume_source_credit_lien_from_counterparty_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    fn impair_source_credit_lien_from_counterparty_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_impair_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            bucket,
            self.insurance_reservation_for_domain(domain)?,
        )?
        .validate()?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn impair_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.impair_source_credit_lien_from_counterparty_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    pub fn reserve_insurance_credit_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        V16Core::validate_bound_num_atom_aligned(amount)?;
        let current_reservation = self.insurance_reservation_for_domain(domain)?;
        let new_reserved = current_reservation
            .insurance_credit_reserved_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        let configured_domains = self.configured_domain_count()?;
        let mut live_source_credit_insurance_atoms = 0u128;
        let mut d = 0usize;
        while d < configured_domains {
            let reserved_num = if d == domain {
                new_reserved
            } else {
                self.insurance_reservation_for_domain(d)?
                    .insurance_credit_reserved_num
            };
            let reserved_atoms = V16Core::amount_from_bound_num(reserved_num)?;
            live_source_credit_insurance_atoms = live_source_credit_insurance_atoms
                .checked_add(reserved_atoms)
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        let domain_reserved_atoms = V16Core::amount_from_bound_num(new_reserved)?;
        let (budget, spent) = self.domain_insurance_budget_spent(domain)?;
        if live_source_credit_insurance_atoms > self.header.insurance.get()
            || spent
                .checked_add(domain_reserved_atoms)
                .ok_or(V16Error::ArithmeticOverflow)?
                > budget
        {
            return Err(V16Error::LockActive);
        }
        let mut reservation = current_reservation;
        let mut source = self.source_credit_for_domain(domain)?;
        reservation.insurance_credit_reserved_num = new_reserved;
        reservation.source_credit_epoch = source.credit_epoch;
        source.insurance_credit_reserved_num = source
            .insurance_credit_reserved_num
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    pub fn create_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.create_source_credit_lien_from_insurance_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    fn create_source_credit_lien_from_insurance_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source) = V16Core::prepare_insurance_lien_create_delta(
            self.insurance_reservation_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn release_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source) = V16Core::prepare_insurance_lien_release_delta(
            self.insurance_reservation_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    pub fn consume_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source, next_domain_spent, next_insurance) =
            V16Core::prepare_insurance_lien_consume_delta(
                self.insurance_reservation_for_domain(domain)?,
                self.source_credit_for_domain(domain)?,
                self.domain_insurance_budget_spent(domain)?.1,
                self.header.insurance.get(),
                amount,
            )?;
        let spend_atoms = self
            .header
            .insurance
            .get()
            .checked_sub(next_insurance)
            .ok_or(V16Error::CounterUnderflow)?;
        let vault_before = self.header.vault.get();
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        TokenValueFlowProofV16::validate_insurance_to_close_insurance_spent(
            spend_atoms,
            vault_before,
            self.header.vault.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.insurance = V16PodU128::new(next_insurance);
        self.set_domain_insurance_spent(domain, next_domain_spent)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    fn impair_source_credit_lien_from_insurance_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source) = V16Core::prepare_insurance_lien_impair_delta(
            self.insurance_reservation_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    pub fn impair_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.impair_source_credit_lien_from_insurance_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    pub fn withdraw_backing_provider_earnings_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let mut bucket = self.backing_bucket_for_domain(domain)?;
        let (next_vault, next_earnings) = apply_backing_provider_earnings_withdraw(
            self.header.vault.get(),
            bucket.utilization_fee_earnings,
            amount,
        )?;
        bucket.utilization_fee_earnings = next_earnings;
        self.header.vault = V16PodU128::new(next_vault);
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    fn account_source_claim_bound_sum_num(account: &PortfolioV16View<'_>) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < account.source_domains.len() {
            sum = sum
                .checked_add(account.source_domains[d].source_claim_bound_num.get())
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        Ok(sum)
    }

    fn account_has_source_claims(account: &PortfolioV16View<'_>) -> V16Result<bool> {
        Ok(Self::account_source_claim_bound_sum_num(account)? != 0)
    }

    fn source_claim_unliened_num(account: &PortfolioV16View<'_>, domain: usize) -> V16Result<u128> {
        if domain >= account.source_domains.len() {
            return Ok(0);
        }
        let source = account.source_domains[domain];
        let locked = source
            .source_claim_liened_num
            .get()
            .checked_add(source.source_claim_impaired_num.get())
            .ok_or(V16Error::ArithmeticOverflow)?;
        source
            .source_claim_bound_num
            .get()
            .checked_sub(locked)
            .ok_or(V16Error::CounterUnderflow)
    }

    fn clear_account_source_claim_market_id_if_empty(
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
    ) {
        if domain >= account.source_domains.len() {
            return;
        }
        let source = &mut account.source_domains[domain];
        if source.source_claim_bound_num.get() == 0
            && source.source_claim_liened_num.get() == 0
            && source.source_claim_counterparty_liened_num.get() == 0
            && source.source_claim_insurance_liened_num.get() == 0
            && source.source_lien_effective_reserved.get() == 0
            && source.source_lien_counterparty_backing_num.get() == 0
            && source.source_lien_insurance_backing_num.get() == 0
            && source.source_lien_fee_last_slot.get() == 0
            && source.source_claim_impaired_num.get() == 0
            && source.source_lien_impaired_effective_reserved.get() == 0
        {
            source.source_claim_market_id = V16PodU64::new(0);
        }
    }

    fn impair_account_source_credit_insurance_lien_fields(
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        face: u128,
        effective: u128,
    ) -> V16Result<u128> {
        let source = account
            .source_domains
            .get_mut(domain)
            .ok_or(V16Error::InvalidLeg)?;
        source.source_claim_insurance_liened_num = V16PodU128::new(0);
        source.source_claim_liened_num = V16PodU128::new(
            source
                .source_claim_liened_num
                .get()
                .checked_sub(face)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        source.source_claim_impaired_num = V16PodU128::new(
            source
                .source_claim_impaired_num
                .get()
                .checked_add(face)
                .ok_or(V16Error::CounterOverflow)?,
        );
        source.source_lien_insurance_backing_num = V16PodU128::new(0);
        source.source_lien_effective_reserved = V16PodU128::new(
            source
                .source_lien_effective_reserved
                .get()
                .checked_sub(effective)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        source.source_lien_impaired_effective_reserved = V16PodU128::new(
            source
                .source_lien_impaired_effective_reserved
                .get()
                .checked_add(effective)
                .ok_or(V16Error::CounterOverflow)?,
        );
        account.header.health_cert.valid = 0;
        Ok(effective)
    }

    fn burn_account_source_claim_bound_num(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        mut burn_num: u128,
    ) -> V16Result<()> {
        if burn_num == 0 {
            return Ok(());
        }
        let account_claim_sum = Self::account_source_claim_bound_sum_num(&account.as_view())?;
        if account_claim_sum == 0 {
            return Ok(());
        }
        if account_claim_sum < burn_num {
            return Err(V16Error::CounterUnderflow);
        }
        let domain_count = self.configured_domain_count()?;
        let mut d = 0usize;
        while d < domain_count && burn_num != 0 {
            let burnable = Self::source_claim_unliened_num(&account.as_view(), d)?;
            let burn = burnable.min(burn_num);
            if burn != 0 {
                account.source_domains[d].source_claim_bound_num = V16PodU128::new(
                    account.source_domains[d]
                        .source_claim_bound_num
                        .get()
                        .checked_sub(burn)
                        .ok_or(V16Error::CounterUnderflow)?,
                );
                let mut source_credit = self.source_credit_for_domain(d)?;
                source_credit.positive_claim_bound_num = source_credit
                    .positive_claim_bound_num
                    .checked_sub(burn)
                    .ok_or(V16Error::CounterUnderflow)?;
                source_credit.exact_positive_claim_num = source_credit
                    .exact_positive_claim_num
                    .checked_sub(burn.min(source_credit.exact_positive_claim_num))
                    .ok_or(V16Error::CounterUnderflow)?;
                self.set_source_credit_for_domain(d, source_credit)?;
                burn_num -= burn;
                Self::clear_account_source_claim_market_id_if_empty(account, d);
                self.recompute_source_credit_domain_after_mutation(d)?;
            }
            if burn_num != 0 {
                let impaired_burn = account.source_domains[d]
                    .source_claim_impaired_num
                    .get()
                    .min(burn_num);
                if impaired_burn != 0 {
                    let old_impaired = account.source_domains[d].source_claim_impaired_num.get();
                    let next_impaired = old_impaired
                        .checked_sub(impaired_burn)
                        .ok_or(V16Error::CounterUnderflow)?;
                    account.source_domains[d].source_claim_bound_num = V16PodU128::new(
                        account.source_domains[d]
                            .source_claim_bound_num
                            .get()
                            .checked_sub(impaired_burn)
                            .ok_or(V16Error::CounterUnderflow)?,
                    );
                    account.source_domains[d].source_claim_impaired_num =
                        V16PodU128::new(next_impaired);
                    let impaired_effective_burn = if next_impaired == 0 {
                        account.source_domains[d]
                            .source_lien_impaired_effective_reserved
                            .get()
                    } else {
                        V16Core::amount_from_bound_num(impaired_burn)?.min(
                            account.source_domains[d]
                                .source_lien_impaired_effective_reserved
                                .get(),
                        )
                    };
                    account.source_domains[d].source_lien_impaired_effective_reserved =
                        V16PodU128::new(
                            account.source_domains[d]
                                .source_lien_impaired_effective_reserved
                                .get()
                                .checked_sub(impaired_effective_burn)
                                .ok_or(V16Error::CounterUnderflow)?,
                        );
                    let mut source_credit = self.source_credit_for_domain(d)?;
                    source_credit.positive_claim_bound_num = source_credit
                        .positive_claim_bound_num
                        .checked_sub(impaired_burn)
                        .ok_or(V16Error::CounterUnderflow)?;
                    source_credit.exact_positive_claim_num = source_credit
                        .exact_positive_claim_num
                        .checked_sub(impaired_burn.min(source_credit.exact_positive_claim_num))
                        .ok_or(V16Error::CounterUnderflow)?;
                    burn_num -= impaired_burn;
                    self.set_source_credit_for_domain(d, source_credit)?;
                    Self::clear_account_source_claim_market_id_if_empty(account, d);
                    self.recompute_source_credit_domain_after_mutation(d)?;
                }
            }
            d += 1;
        }
        if burn_num != 0 {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn source_domain_realizable_support_for_face(
        &self,
        domain: usize,
        face_claim: u128,
    ) -> V16Result<u128> {
        if face_claim == 0 {
            return Ok(0);
        }
        self.validate_source_domain_ledger_current(domain)?;
        V16Core::source_credit_state_realizable_support_for_face(
            self.source_credit_for_domain(domain)?,
            face_claim,
        )
    }

    fn account_source_realizable_support(
        &self,
        account: &PortfolioV16View<'_>,
        face_claim: u128,
    ) -> V16Result<u128> {
        if face_claim == 0 {
            return Ok(0);
        }
        let configured_domains = self.configured_domain_count()?;
        if account.source_domains.len() < configured_domains {
            return Err(V16Error::InvalidLeg);
        }
        let mut remaining_num = V16Core::bound_num_from_amount(face_claim)?;
        let mut support_num = U256::ZERO;
        let mut d = 0usize;
        while d < configured_domains && remaining_num != 0 {
            let source = account.source_domains[d];
            let locked = source
                .source_claim_liened_num
                .get()
                .checked_add(source.source_claim_impaired_num.get())
                .ok_or(V16Error::ArithmeticOverflow)?;
            if locked > source.source_claim_bound_num.get() {
                return Err(V16Error::InvalidLeg);
            }
            let valid_lien_effective_num = source
                .source_lien_effective_reserved
                .get()
                .checked_mul(BOUND_SCALE)
                .ok_or(V16Error::ArithmeticOverflow)?
                .min(remaining_num);
            if valid_lien_effective_num != 0 {
                support_num = support_num
                    .checked_add(U256::from_u128(valid_lien_effective_num))
                    .ok_or(V16Error::ArithmeticOverflow)?;
                remaining_num -= valid_lien_effective_num;
            }
            let claim_num = source
                .source_claim_bound_num
                .get()
                .checked_sub(locked)
                .ok_or(V16Error::CounterUnderflow)?
                .min(remaining_num);
            if claim_num != 0 {
                self.validate_source_domain_ledger_current(d)?;
                let credited_num = U256::from_u128(claim_num)
                    .checked_mul(U256::from_u128(
                        self.source_credit_for_domain(d)?.credit_rate_num,
                    ))
                    .and_then(|v| v.checked_div(U256::from_u128(CREDIT_RATE_SCALE)))
                    .ok_or(V16Error::ArithmeticOverflow)?;
                support_num = support_num
                    .checked_add(credited_num)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                remaining_num -= claim_num;
            }
            d += 1;
        }
        support_num
            .checked_div(U256::from_u128(BOUND_SCALE))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)
    }

    fn account_unliened_source_realizable_support(
        &self,
        account: &PortfolioV16View<'_>,
        face_claim: u128,
    ) -> V16Result<u128> {
        if face_claim == 0 {
            return Ok(0);
        }
        let configured_domains = self.configured_domain_count()?;
        if account.source_domains.len() < configured_domains {
            return Err(V16Error::InvalidLeg);
        }
        let mut remaining_num = V16Core::bound_num_from_amount(face_claim)?;
        let mut support_num = U256::ZERO;
        let mut d = 0usize;
        while d < configured_domains && remaining_num != 0 {
            let claim_num = Self::source_claim_unliened_num(account, d)?.min(remaining_num);
            if claim_num != 0 {
                self.validate_source_domain_ledger_current(d)?;
                let credited_num = U256::from_u128(claim_num)
                    .checked_mul(U256::from_u128(
                        self.source_credit_for_domain(d)?.credit_rate_num,
                    ))
                    .and_then(|v| v.checked_div(U256::from_u128(CREDIT_RATE_SCALE)))
                    .ok_or(V16Error::ArithmeticOverflow)?;
                support_num = support_num
                    .checked_add(credited_num)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                remaining_num -= claim_num;
            }
            d += 1;
        }
        support_num
            .checked_div(U256::from_u128(BOUND_SCALE))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)
    }

    pub fn source_credit_available_backing_num(&self, domain: usize) -> V16Result<u128> {
        V16Core::available_backing_num_for_source_credit_state(
            self.source_credit_for_domain(domain)?,
        )
    }

    fn valid_source_lien_effective_reserved_sum(account: &PortfolioV16View<'_>) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < account.source_domains.len() {
            sum = sum
                .checked_add(
                    account.source_domains[d]
                        .source_lien_effective_reserved
                        .get(),
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        Ok(sum)
    }

    fn incremental_initial_margin_source_credit_needed(
        account: &PortfolioV16View<'_>,
        no_positive_equity: i128,
    ) -> V16Result<u128> {
        let cert = account.header.health_cert.try_to_runtime()?;
        if !cert.valid {
            return Err(V16Error::Stale);
        }
        let existing_lien = Self::valid_source_lien_effective_reserved_sum(account)?;
        if no_positive_equity >= 0 {
            let covered = (no_positive_equity as u128)
                .checked_add(existing_lien)
                .ok_or(V16Error::ArithmeticOverflow)?;
            return Ok(cert.certified_initial_req.saturating_sub(covered));
        }
        let need_before_lien = cert
            .certified_initial_req
            .checked_add(no_positive_equity.unsigned_abs())
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(need_before_lien.saturating_sub(existing_lien))
    }

    fn set_insurance_reservation_for_domain(
        &mut self,
        domain: usize,
        reservation: InsuranceCreditReservationV16,
    ) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => {
                slot.insurance_reservation_long =
                    InsuranceCreditReservationV16Account::from_runtime(&reservation)
            }
            SideV16::Short => {
                slot.insurance_reservation_short =
                    InsuranceCreditReservationV16Account::from_runtime(&reservation)
            }
        }
        Ok(())
    }

    fn domain_insurance_budget_spent(&self, domain: usize) -> V16Result<(u128, u128)> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        Ok(match side {
            SideV16::Long => (
                slot.insurance_domain_budget_long.get(),
                slot.insurance_domain_spent_long.get(),
            ),
            SideV16::Short => (
                slot.insurance_domain_budget_short.get(),
                slot.insurance_domain_spent_short.get(),
            ),
        })
    }

    fn set_domain_insurance_spent(&mut self, domain: usize, spent: u128) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.insurance_domain_spent_long = V16PodU128::new(spent),
            SideV16::Short => slot.insurance_domain_spent_short = V16PodU128::new(spent),
        }
        Ok(())
    }

    fn available_domain_insurance(&self, domain: usize) -> V16Result<u128> {
        let (budget, spent) = self.domain_insurance_budget_spent(domain)?;
        let configured_domains = self.configured_domain_count()?;
        let mut total_reserved_atoms = 0u128;
        let mut domain_reserved_atoms = 0u128;
        let mut d = 0usize;
        while d < configured_domains {
            let reserved_atoms = V16Core::amount_from_bound_num(
                self.insurance_reservation_for_domain(d)?
                    .insurance_credit_reserved_num,
            )?;
            total_reserved_atoms = total_reserved_atoms
                .checked_add(reserved_atoms)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if d == domain {
                domain_reserved_atoms = reserved_atoms;
            }
            d += 1;
        }
        let global_available = self
            .header
            .insurance
            .get()
            .saturating_sub(total_reserved_atoms);
        let budget_remaining = budget
            .saturating_sub(spent)
            .saturating_sub(domain_reserved_atoms);
        Ok(global_available.min(budget_remaining))
    }

    fn consume_domain_insurance_for_negative_pnl(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let domain = self.insurance_domain_index(asset_index, opposite_side(bankrupt_side))?;
        if account.header.pnl.get() >= 0 {
            return Ok(0);
        }
        self.header.bankruptcy_hlock_active = 1;
        let residual = account.header.pnl.get().unsigned_abs();
        let domain_available = self.available_domain_insurance(domain)?;
        let used = residual.min(domain_available);
        if used == 0 {
            return Ok(0);
        }
        let vault_before = self.header.vault.get();
        self.header.insurance = V16PodU128::new(
            self.header
                .insurance
                .get()
                .checked_sub(used)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        let (_, spent_before) = self.domain_insurance_budget_spent(domain)?;
        self.set_domain_insurance_spent(
            domain,
            spent_before
                .checked_add(used)
                .ok_or(V16Error::ArithmeticOverflow)?,
        )?;
        let used_i128 = i128::try_from(used).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = account
            .header
            .pnl
            .get()
            .checked_add(used_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_account_pnl(account, new_pnl)?;
        TokenValueFlowProofV16::validate_insurance_to_close_insurance_spent(
            used,
            vault_before,
            self.header.vault.get(),
        )?;
        account.header.health_cert.valid = 0;
        Ok(used)
    }

    #[cfg(kani)]
    pub fn kani_consume_domain_insurance_for_negative_pnl(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        self.consume_domain_insurance_for_negative_pnl(asset_index, bankrupt_side, account)
    }

    fn preflight_liquidation_residual_durability(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        let domain = self.insurance_domain_index(asset_index, opposite_side(bankrupt_side))?;
        let residual_after_principal_and_insurance = if account.header.pnl.get() < 0 {
            account
                .header
                .pnl
                .get()
                .unsigned_abs()
                .saturating_sub(account.header.capital.get())
                .saturating_sub(self.available_domain_insurance(domain)?)
        } else {
            0
        };
        if residual_after_principal_and_insurance == 0 {
            return Ok(());
        }
        let capacity = self.bankruptcy_residual_single_step_capacity(
            asset_index,
            bankrupt_side,
            residual_after_principal_and_insurance,
        )?;
        if capacity < residual_after_principal_and_insurance {
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        Ok(())
    }

    #[cfg(kani)]
    pub fn kani_preflight_liquidation_residual_durability(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        self.preflight_liquidation_residual_durability(asset_index, bankrupt_side, account)
    }

    fn create_and_consume_source_credit_from_counterparty_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_create_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            self.header.current_slot.get(),
            amount,
        )?;
        let (bucket, source) =
            V16Core::prepare_counterparty_lien_consume_delta(bucket, source, amount)?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            bucket,
            self.insurance_reservation_for_domain(domain)?,
        )?
        .validate()?;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    fn create_and_consume_source_credit_from_insurance_core_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source) = V16Core::prepare_insurance_lien_create_delta(
            self.insurance_reservation_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            amount,
        )?;
        let (_, spent_before) = self.domain_insurance_budget_spent(domain)?;
        let insurance_before = self.header.insurance.get();
        let (reservation, source, next_domain_spent, next_insurance) =
            V16Core::prepare_insurance_lien_consume_delta(
                reservation,
                source,
                spent_before,
                insurance_before,
                amount,
            )?;
        let spend_atoms = insurance_before
            .checked_sub(next_insurance)
            .ok_or(V16Error::CounterUnderflow)?;
        let vault_before = self.header.vault.get();
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        TokenValueFlowProofV16::validate_insurance_to_close_insurance_spent(
            spend_atoms,
            vault_before,
            self.header.vault.get(),
        )?;
        self.reservation_encumbrance_proof_for_domain_parts(
            domain,
            source,
            self.backing_bucket_for_domain(domain)?,
            reservation,
        )?
        .validate()?;
        self.set_insurance_reservation_for_domain(domain, reservation)?;
        self.set_source_credit_for_domain(domain, source)?;
        self.header.insurance = V16PodU128::new(next_insurance);
        self.set_domain_insurance_spent(domain, next_domain_spent)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        Ok(())
    }

    fn consume_source_domain_credit_for_effective_not_atomic(
        &mut self,
        domain: usize,
        effective_credit: u128,
    ) -> V16Result<SourceCreditConsumptionV16> {
        self.domain_asset_side(domain)?;
        if effective_credit == 0 {
            return Ok(SourceCreditConsumptionV16 {
                face_burn: 0,
                counterparty_credit_consumed: 0,
                insurance_credit_consumed: 0,
            });
        }
        self.validate_source_domain_ledger_current(domain)?;
        let rate = self.source_credit_for_domain(domain)?.credit_rate_num;
        let (required_face_num, backing_num) =
            V16Core::source_credit_lien_amounts_for_effective(effective_credit, rate)?;
        if self.source_credit_available_backing_num(domain)? < backing_num {
            return Err(V16Error::LockActive);
        }
        let bucket = self.backing_bucket_for_domain(domain)?;
        let mut counterparty_credit_consumed = 0;
        let mut insurance_credit_consumed = 0;
        if bucket.status == BackingBucketStatusV16::Fresh
            && bucket.expiry_slot > self.header.current_slot.get()
            && bucket.fresh_unliened_backing_num >= backing_num
        {
            self.create_and_consume_source_credit_from_counterparty_core_not_atomic(
                domain,
                backing_num,
            )?;
            counterparty_credit_consumed = effective_credit;
        } else {
            self.create_and_consume_source_credit_from_insurance_core_not_atomic(
                domain,
                backing_num,
            )?;
            insurance_credit_consumed = effective_credit;
        }
        Ok(SourceCreditConsumptionV16 {
            face_burn: V16Core::amount_from_bound_num(required_face_num)?,
            counterparty_credit_consumed,
            insurance_credit_consumed,
        })
    }

    fn create_and_consume_account_source_credit_for_effective_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        effective_credit: u128,
    ) -> V16Result<SourceCreditConsumptionV16> {
        account.validate_with_market(&self.as_view())?;
        if effective_credit == 0 {
            return Ok(SourceCreditConsumptionV16 {
                face_burn: 0,
                counterparty_credit_consumed: 0,
                insurance_credit_consumed: 0,
            });
        }
        let mut remaining = effective_credit;
        let mut face_burn_num = 0u128;
        let mut counterparty_credit_consumed = 0u128;
        let mut insurance_credit_consumed = 0u128;
        let domain_count = self.configured_domain_count()?;
        let mut d = 0usize;
        while d < domain_count && remaining != 0 {
            let rate = self.source_credit_for_domain(d)?.credit_rate_num;
            let unliened = Self::source_claim_unliened_num(&account.as_view(), d)?;
            if rate != 0 && unliened != 0 {
                self.validate_source_domain_ledger_current(d)?;
                let soft_num = U256::from_u128(unliened)
                    .checked_mul(U256::from_u128(rate))
                    .and_then(|v| v.checked_div(U256::from_u128(CREDIT_RATE_SCALE)))
                    .and_then(|v| v.try_into_u128())
                    .ok_or(V16Error::ArithmeticOverflow)?;
                let by_claim = soft_num / BOUND_SCALE;
                let by_backing = self.source_credit_available_backing_num(d)? / BOUND_SCALE;
                let take = remaining.min(by_claim).min(by_backing);
                if take != 0 {
                    let (face_num, backing_num) =
                        V16Core::source_credit_lien_amounts_for_effective(take, rate)?;
                    let bucket = self.backing_bucket_for_domain(d)?;
                    if bucket.status == BackingBucketStatusV16::Fresh
                        && bucket.expiry_slot > self.header.current_slot.get()
                        && bucket.fresh_unliened_backing_num >= backing_num
                    {
                        self.create_and_consume_source_credit_from_counterparty_core_not_atomic(
                            d,
                            backing_num,
                        )?;
                        counterparty_credit_consumed = counterparty_credit_consumed
                            .checked_add(take)
                            .ok_or(V16Error::ArithmeticOverflow)?;
                    } else {
                        self.create_and_consume_source_credit_from_insurance_core_not_atomic(
                            d,
                            backing_num,
                        )?;
                        insurance_credit_consumed = insurance_credit_consumed
                            .checked_add(take)
                            .ok_or(V16Error::ArithmeticOverflow)?;
                    }
                    face_burn_num = face_burn_num
                        .checked_add(face_num)
                        .ok_or(V16Error::ArithmeticOverflow)?;
                    remaining -= take;
                }
            }
            d += 1;
        }
        if remaining != 0 {
            return Err(V16Error::LockActive);
        }
        Ok(SourceCreditConsumptionV16 {
            face_burn: V16Core::amount_from_bound_num(face_burn_num)?,
            counterparty_credit_consumed,
            insurance_credit_consumed,
        })
    }

    fn create_source_credit_lien_backing_not_atomic(
        &mut self,
        domain: usize,
        backing_num: u128,
    ) -> V16Result<SourceCreditBackingSourceV16> {
        self.domain_asset_side(domain)?;
        if backing_num == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let bucket = self.backing_bucket_for_domain(domain)?;
        if bucket.status == BackingBucketStatusV16::Fresh
            && bucket.expiry_slot > self.header.current_slot.get()
            && bucket.fresh_unliened_backing_num >= backing_num
        {
            self.create_source_credit_lien_from_counterparty_core_not_atomic(domain, backing_num)?;
            return Ok(SourceCreditBackingSourceV16::Counterparty);
        }
        let reservation = self.insurance_reservation_for_domain(domain)?;
        let encumbered = reservation
            .valid_liened_insurance_num
            .checked_add(reservation.impaired_liened_insurance_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if reservation
            .insurance_credit_reserved_num
            .checked_sub(encumbered)
            .ok_or(V16Error::CounterUnderflow)?
            >= backing_num
        {
            self.create_source_credit_lien_from_insurance_core_not_atomic(domain, backing_num)?;
            return Ok(SourceCreditBackingSourceV16::Insurance);
        }
        Err(V16Error::LockActive)
    }

    fn apply_account_source_credit_lien_delta(
        source: &mut PortfolioSourceDomainV16Account,
        backing_source: SourceCreditBackingSourceV16,
        required_face_num: u128,
        required_backing_num: u128,
        effective_credit: u128,
        current_slot: u64,
    ) -> V16Result<()> {
        let prior_counterparty_backing = source.source_lien_counterparty_backing_num.get();
        source.source_claim_liened_num = V16PodU128::new(
            source
                .source_claim_liened_num
                .get()
                .checked_add(required_face_num)
                .ok_or(V16Error::ArithmeticOverflow)?,
        );
        source.source_lien_effective_reserved = V16PodU128::new(
            source
                .source_lien_effective_reserved
                .get()
                .checked_add(effective_credit)
                .ok_or(V16Error::ArithmeticOverflow)?,
        );
        match backing_source {
            SourceCreditBackingSourceV16::Counterparty => {
                source.source_claim_counterparty_liened_num = V16PodU128::new(
                    source
                        .source_claim_counterparty_liened_num
                        .get()
                        .checked_add(required_face_num)
                        .ok_or(V16Error::ArithmeticOverflow)?,
                );
                source.source_lien_counterparty_backing_num = V16PodU128::new(
                    source
                        .source_lien_counterparty_backing_num
                        .get()
                        .checked_add(required_backing_num)
                        .ok_or(V16Error::ArithmeticOverflow)?,
                );
                if prior_counterparty_backing == 0 {
                    source.source_lien_fee_last_slot = V16PodU64::new(current_slot);
                }
            }
            SourceCreditBackingSourceV16::Insurance => {
                source.source_claim_insurance_liened_num = V16PodU128::new(
                    source
                        .source_claim_insurance_liened_num
                        .get()
                        .checked_add(required_face_num)
                        .ok_or(V16Error::ArithmeticOverflow)?,
                );
                source.source_lien_insurance_backing_num = V16PodU128::new(
                    source
                        .source_lien_insurance_backing_num
                        .get()
                        .checked_add(required_backing_num)
                        .ok_or(V16Error::ArithmeticOverflow)?,
                );
            }
        }
        Ok(())
    }

    #[cfg(kani)]
    pub fn kani_apply_counterparty_source_credit_lien_delta(
        source: &mut PortfolioSourceDomainV16Account,
        required_face_num: u128,
        required_backing_num: u128,
        effective_credit: u128,
        current_slot: u64,
    ) -> V16Result<()> {
        Self::apply_account_source_credit_lien_delta(
            source,
            SourceCreditBackingSourceV16::Counterparty,
            required_face_num,
            required_backing_num,
            effective_credit,
            current_slot,
        )
    }

    #[cfg(kani)]
    pub fn kani_prepare_counterparty_lien_create_delta(
        bucket: BackingBucketV16,
        source: SourceCreditStateV16,
        current_slot: u64,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        V16Core::prepare_counterparty_lien_create_delta(bucket, source, current_slot, amount)
    }

    #[cfg(kani)]
    pub fn kani_prepare_counterparty_lien_consume_delta(
        bucket: BackingBucketV16,
        source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        V16Core::prepare_counterparty_lien_consume_delta(bucket, source, amount)
    }

    #[cfg(kani)]
    pub fn kani_prepare_counterparty_backing_add_delta(
        bucket: BackingBucketV16,
        source: SourceCreditStateV16,
        amount: u128,
        current_slot: u64,
        expiry_slot: u64,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        V16Core::prepare_counterparty_backing_add_delta(
            bucket,
            source,
            amount,
            current_slot,
            expiry_slot,
        )
    }

    #[cfg(kani)]
    pub fn kani_source_credit_lien_amounts_for_effective(
        effective_credit: u128,
        credit_rate_num: u128,
    ) -> V16Result<(u128, u128)> {
        V16Core::source_credit_lien_amounts_for_effective(effective_credit, credit_rate_num)
    }

    #[cfg(kani)]
    pub fn kani_counterparty_cure_atoms_from_scaled_backing(amount: u128) -> V16Result<u128> {
        V16Core::validate_bound_num_atom_aligned(amount)?;
        Ok(amount / BOUND_SCALE)
    }

    #[cfg(kani)]
    pub fn kani_prepare_insurance_lien_consume_delta(
        reservation: InsuranceCreditReservationV16,
        source: SourceCreditStateV16,
        domain_spent: u128,
        insurance: u128,
        amount: u128,
    ) -> V16Result<(
        InsuranceCreditReservationV16,
        SourceCreditStateV16,
        u128,
        u128,
    )> {
        V16Core::prepare_insurance_lien_consume_delta(
            reservation,
            source,
            domain_spent,
            insurance,
            amount,
        )
    }

    fn create_account_source_credit_lien_for_effective_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        effective_credit: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        self.domain_asset_side(domain)?;
        if domain >= account.source_domains.len() {
            return Err(V16Error::InvalidLeg);
        }
        if effective_credit == 0 {
            return Ok(());
        }
        self.validate_source_domain_ledger_current(domain)?;
        let rate = self.source_credit_for_domain(domain)?.credit_rate_num;
        let (required_face_num, required_backing_num) =
            V16Core::source_credit_lien_amounts_for_effective(effective_credit, rate)?;
        if Self::source_claim_unliened_num(&account.as_view(), domain)? < required_face_num {
            return Err(V16Error::LockActive);
        }
        self.collect_account_backing_utilization_fee_for_domain_not_atomic(account, domain)?;
        let backing_source =
            self.create_source_credit_lien_backing_not_atomic(domain, required_backing_num)?;
        Self::apply_account_source_credit_lien_delta(
            &mut account.source_domains[domain],
            backing_source,
            required_face_num,
            required_backing_num,
            effective_credit,
            self.header.current_slot.get(),
        )?;
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())
    }

    fn create_account_source_credit_lien_for_effective_any_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        effective_credit: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        let mut remaining = effective_credit;
        let domain_count = self.configured_domain_count()?;
        let mut d = 0usize;
        while d < domain_count && remaining != 0 {
            let rate = self.source_credit_for_domain(d)?.credit_rate_num;
            let unliened = Self::source_claim_unliened_num(&account.as_view(), d)?;
            if rate != 0 && unliened != 0 {
                self.validate_source_domain_ledger_current(d)?;
                let soft_num = U256::from_u128(unliened)
                    .checked_mul(U256::from_u128(rate))
                    .and_then(|v| v.checked_div(U256::from_u128(CREDIT_RATE_SCALE)))
                    .and_then(|v| v.try_into_u128())
                    .ok_or(V16Error::ArithmeticOverflow)?;
                let by_claim = soft_num / BOUND_SCALE;
                let by_backing = self.source_credit_available_backing_num(d)? / BOUND_SCALE;
                let take = remaining.min(by_claim).min(by_backing);
                if take != 0 {
                    self.create_account_source_credit_lien_for_effective_not_atomic(
                        account, d, take,
                    )?;
                    remaining -= take;
                }
            }
            d += 1;
        }
        if remaining != 0 {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn create_initial_margin_source_lien_if_needed(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        let mut attempts = 0u8;
        while attempts < 2 {
            if !decode_bool(account.header.health_cert.valid)? {
                return Err(V16Error::Stale);
            }
            let no_positive = Self::account_no_positive_credit_equity(&account.as_view())?;
            let required_credit = Self::incremental_initial_margin_source_credit_needed(
                &account.as_view(),
                no_positive,
            )?;
            if required_credit == 0 {
                return Ok(());
            }
            self.create_account_source_credit_lien_for_effective_any_not_atomic(
                account,
                required_credit,
            )?;
            self.recertify_account_after_source_lien_change(account)?;
            attempts += 1;
        }
        Err(V16Error::LockActive)
    }

    #[cfg(kani)]
    pub fn kani_create_initial_margin_source_lien_if_needed(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        self.create_initial_margin_source_lien_if_needed(account)
    }

    fn haircut_effective_support(
        &self,
        face_claim: u128,
        residual: u128,
        junior_bound: u128,
    ) -> V16Result<u128> {
        if face_claim == 0 || residual == 0 || junior_bound == 0 {
            return Ok(0);
        }
        if residual >= junior_bound {
            return Ok(face_claim);
        }
        Ok(wide_mul_div_floor_u128(face_claim, residual, junior_bound))
    }

    fn account_haircut_equity(&self, account: &PortfolioV16View<'_>) -> V16Result<i128> {
        validate_non_min_i128(account.header.pnl.get())?;
        validate_fee_credits(account.header.fee_credits.get())?;
        let capital = i128::try_from(account.header.capital.get())
            .map_err(|_| V16Error::ArithmeticOverflow)?;
        let fee_debt = i128::try_from(account.header.fee_credits.get().unsigned_abs())
            .map_err(|_| V16Error::ArithmeticOverflow)?;
        if account.header.pnl.get() <= 0 {
            return capital
                .checked_add(account.header.pnl.get())
                .and_then(|v| v.checked_sub(fee_debt))
                .ok_or(V16Error::ArithmeticOverflow);
        }
        let positive_support = if Self::account_has_source_claims(account)? {
            self.account_source_realizable_support(account, account.header.pnl.get() as u128)?
        } else {
            0
        };
        let positive_support_i128 =
            i128::try_from(positive_support).map_err(|_| V16Error::ArithmeticOverflow)?;
        capital
            .checked_add(positive_support_i128)
            .and_then(|v| v.checked_sub(fee_debt))
            .ok_or(V16Error::ArithmeticOverflow)
    }

    fn ensure_account_source_claim_market_id(
        &self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
    ) -> V16Result<()> {
        if domain >= account.source_domains.len() {
            return Err(V16Error::InvalidLeg);
        }
        let (asset_index, _) = self.domain_asset_side(domain)?;
        let market_id = self.markets[asset_index].engine.asset.market_id.get();
        if market_id == 0 {
            return Err(V16Error::InvalidLeg);
        }
        let source = &mut account.source_domains[domain];
        if source.source_claim_market_id.get() == 0 {
            source.source_claim_market_id = V16PodU64::new(market_id);
            return Ok(());
        }
        if source.source_claim_market_id.get() != market_id {
            return Err(V16Error::HiddenLeg);
        }
        Ok(())
    }

    fn set_account_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        new_pnl: i128,
    ) -> V16Result<()> {
        self.set_account_pnl_inner(account, new_pnl, None)
    }

    fn set_account_pnl_with_source(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        new_pnl: i128,
        source_domain: usize,
    ) -> V16Result<()> {
        self.domain_asset_side(source_domain)?;
        self.set_account_pnl_inner(account, new_pnl, Some(source_domain))
    }

    fn set_account_pnl_inner(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        new_pnl: i128,
        source_domain: Option<usize>,
    ) -> V16Result<()> {
        validate_non_min_i128(new_pnl)?;
        let old_pos = account.header.pnl.get().max(0) as u128;
        let new_pos = new_pnl.max(0) as u128;
        if new_pos >= old_pos {
            let increase = new_pos - old_pos;
            let increase_num = V16Core::bound_num_from_amount(increase)?;
            let increase_domain = if increase_num != 0 {
                if source_domain.is_none()
                    && decode_market_mode(self.header.mode)? == MarketModeV16::Live
                {
                    return Err(V16Error::InvalidLeg);
                }
                source_domain
            } else {
                None
            };
            self.header.pnl_pos_tot = V16PodU128::new(
                self.header
                    .pnl_pos_tot
                    .get()
                    .checked_add(increase)
                    .ok_or(V16Error::ArithmeticOverflow)?,
            );
            self.header.pnl_pos_bound_tot_num = V16PodU128::new(
                self.header
                    .pnl_pos_bound_tot_num
                    .get()
                    .checked_add(increase_num)
                    .ok_or(V16Error::ArithmeticOverflow)?,
            );
            if let Some(domain) = increase_domain {
                self.ensure_account_source_claim_market_id(account, domain)?;
                let source = &mut account.source_domains[domain];
                source.source_claim_bound_num = V16PodU128::new(
                    source
                        .source_claim_bound_num
                        .get()
                        .checked_add(increase_num)
                        .ok_or(V16Error::ArithmeticOverflow)?,
                );
                let mut source_credit = self.source_credit_for_domain(domain)?;
                source_credit.positive_claim_bound_num = source_credit
                    .positive_claim_bound_num
                    .checked_add(increase_num)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                source_credit.exact_positive_claim_num = source_credit
                    .exact_positive_claim_num
                    .checked_add(increase_num)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                let (source_credit, next_risk_epoch) =
                    V16Core::prepare_source_credit_domain_recompute_for_epoch(
                        source_credit,
                        self.header.risk_epoch.get(),
                    )?;
                self.set_source_credit_for_domain(domain, source_credit)?;
                self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
            }
        } else {
            let decrease = old_pos - new_pos;
            let decrease_num = V16Core::bound_num_from_amount(decrease)?;
            self.burn_account_source_claim_bound_num(account, decrease_num)?;
            self.header.pnl_pos_tot = V16PodU128::new(
                self.header
                    .pnl_pos_tot
                    .get()
                    .checked_sub(decrease)
                    .ok_or(V16Error::CounterUnderflow)?,
            );
            let next_bound_num = self
                .header
                .pnl_pos_bound_tot_num
                .get()
                .saturating_sub(decrease_num);
            let exact_min_num = V16Core::bound_num_from_amount(self.header.pnl_pos_tot.get())?;
            self.header.pnl_pos_bound_tot_num = V16PodU128::new(next_bound_num.max(exact_min_num));
            self.header.pnl_matured_pos_tot = V16PodU128::new(
                self.header
                    .pnl_matured_pos_tot
                    .get()
                    .min(self.header.pnl_pos_tot.get()),
            );
        }
        self.header.pnl_pos_bound_tot = V16PodU128::new(V16Core::amount_from_bound_num(
            self.header.pnl_pos_bound_tot_num.get(),
        )?);

        let old_negative = account.header.pnl.get() < 0;
        let new_negative = new_pnl < 0;
        match (old_negative, new_negative) {
            (false, true) => {
                self.header.negative_pnl_account_count = V16PodU64::new(
                    self.header
                        .negative_pnl_account_count
                        .get()
                        .checked_add(1)
                        .ok_or(V16Error::CounterOverflow)?,
                );
            }
            (true, false) => {
                self.header.negative_pnl_account_count = V16PodU64::new(
                    self.header
                        .negative_pnl_account_count
                        .get()
                        .checked_sub(1)
                        .ok_or(V16Error::CounterUnderflow)?,
                );
            }
            _ => {}
        }
        account.header.pnl = V16PodI128::new(new_pnl);
        account.header.health_cert.valid = 0;
        Ok(())
    }

    fn face_claim_to_burn_for_support(
        &self,
        effective_support: u128,
        residual: u128,
        junior_bound: u128,
    ) -> V16Result<u128> {
        if effective_support == 0 {
            return Ok(0);
        }
        if residual == 0 || junior_bound == 0 {
            return Err(V16Error::LockActive);
        }
        if residual >= junior_bound {
            return Ok(effective_support);
        }
        checked_mul_div_ceil_u256(
            U256::from_u128(effective_support),
            U256::from_u128(junior_bound),
            U256::from_u128(residual),
        )
        .and_then(|v| v.try_into_u128())
        .ok_or(V16Error::ArithmeticOverflow)
    }

    fn apply_haircut_bounded_close_loss_to_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        loss_abs: u128,
    ) -> V16Result<SupportLossApplicationV16> {
        if loss_abs == 0 {
            return Ok(SupportLossApplicationV16 {
                support_consumed: 0,
                junior_face_burned: 0,
            });
        }
        let old_positive_face = account.header.pnl.get().max(0) as u128;
        if old_positive_face == 0 {
            let loss_i128 = i128::try_from(loss_abs).map_err(|_| V16Error::ArithmeticOverflow)?;
            let new_pnl = account
                .header
                .pnl
                .get()
                .checked_sub(loss_i128)
                .ok_or(V16Error::ArithmeticOverflow)?;
            self.set_account_pnl(account, new_pnl)?;
            return Ok(SupportLossApplicationV16 {
                support_consumed: 0,
                junior_face_burned: 0,
            });
        }
        let has_source_claims = Self::account_has_source_claims(&account.as_view())?;
        let effective_available = if has_source_claims {
            self.account_unliened_source_realizable_support(&account.as_view(), old_positive_face)?
        } else if decode_market_mode(self.header.mode)? == MarketModeV16::Live {
            0
        } else {
            self.haircut_effective_support(
                old_positive_face,
                self.residual(),
                self.junior_claim_bound(),
            )?
        };
        let support_consumed = effective_available.min(loss_abs);
        let remaining_loss = loss_abs
            .checked_sub(support_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let mut junior_face_burned = if has_source_claims {
            self.create_and_consume_account_source_credit_for_effective_not_atomic(
                account,
                support_consumed,
            )?
            .face_burn
            .min(old_positive_face)
        } else if support_consumed == 0 {
            0
        } else {
            let residual = self.residual();
            let junior_bound = self.junior_claim_bound();
            self.face_claim_to_burn_for_support(support_consumed, residual, junior_bound)?
        };
        if remaining_loss != 0 {
            junior_face_burned = old_positive_face;
        }
        if junior_face_burned > old_positive_face {
            return Err(V16Error::ArithmeticOverflow);
        }
        let retained_face = old_positive_face
            .checked_sub(junior_face_burned)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let retained_i128 =
            i128::try_from(retained_face).map_err(|_| V16Error::ArithmeticOverflow)?;
        let remaining_i128 =
            i128::try_from(remaining_loss).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = retained_i128
            .checked_sub(remaining_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        account.header.reserved_pnl = V16PodU128::new(
            account
                .header
                .reserved_pnl
                .get()
                .min(new_pnl.max(0) as u128),
        );
        self.set_account_pnl(account, new_pnl)?;
        Ok(SupportLossApplicationV16 {
            support_consumed,
            junior_face_burned,
        })
    }

    fn apply_signed_kf_delta_to_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        delta: i128,
        source_domain: Option<usize>,
    ) -> V16Result<SupportLossApplicationV16> {
        validate_non_min_i128(delta)?;
        if delta == 0 {
            return Ok(SupportLossApplicationV16 {
                support_consumed: 0,
                junior_face_burned: 0,
            });
        }
        if delta < 0 {
            return self.apply_haircut_bounded_close_loss_to_pnl(account, delta.unsigned_abs());
        }
        if account.header.pnl.get() >= 0 {
            if source_domain.is_none()
                && decode_market_mode(self.header.mode)? == MarketModeV16::Live
            {
                return Err(V16Error::InvalidLeg);
            }
            let new_pnl = account
                .header
                .pnl
                .get()
                .checked_add(delta)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if let Some(domain) = source_domain {
                self.set_account_pnl_with_source(account, new_pnl, domain)?;
            } else {
                self.set_account_pnl(account, new_pnl)?;
            }
            return Ok(SupportLossApplicationV16 {
                support_consumed: 0,
                junior_face_burned: 0,
            });
        }

        let old_loss = account.header.pnl.get().unsigned_abs();
        let new_face_support = delta as u128;
        let (effective_available, source_support_domain) = if let Some(domain) = source_domain {
            (
                self.source_domain_realizable_support_for_face(domain, new_face_support)?,
                Some(domain),
            )
        } else if decode_market_mode(self.header.mode)? == MarketModeV16::Live {
            (0, None)
        } else {
            let residual = self.residual();
            let junior_bound = self
                .junior_claim_bound()
                .checked_add(new_face_support)
                .ok_or(V16Error::ArithmeticOverflow)?;
            (
                self.haircut_effective_support(new_face_support, residual, junior_bound)?,
                None,
            )
        };
        let support_consumed = effective_available.min(old_loss);
        let remaining_loss = old_loss
            .checked_sub(support_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let mut junior_face_burned = if let Some(domain) = source_support_domain {
            self.consume_source_domain_credit_for_effective_not_atomic(domain, support_consumed)?
                .face_burn
                .min(new_face_support)
        } else if support_consumed == 0 {
            0
        } else {
            let residual = self.residual();
            let junior_bound = self
                .junior_claim_bound()
                .checked_add(new_face_support)
                .ok_or(V16Error::ArithmeticOverflow)?;
            self.face_claim_to_burn_for_support(support_consumed, residual, junior_bound)?
        };
        if (source_support_domain.is_none()
            && decode_market_mode(self.header.mode)? == MarketModeV16::Live)
            || remaining_loss != 0
        {
            junior_face_burned = new_face_support;
        }
        if junior_face_burned > new_face_support {
            return Err(V16Error::ArithmeticOverflow);
        }
        let retained_face = new_face_support
            .checked_sub(junior_face_burned)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let retained_i128 =
            i128::try_from(retained_face).map_err(|_| V16Error::ArithmeticOverflow)?;
        let remaining_i128 =
            i128::try_from(remaining_loss).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = retained_i128
            .checked_sub(remaining_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if new_pnl > 0 {
            if let Some(domain) = source_domain {
                self.set_account_pnl_with_source(account, new_pnl, domain)?;
            } else {
                self.set_account_pnl(account, new_pnl)?;
            }
        } else {
            self.set_account_pnl(account, new_pnl)?;
        }
        Ok(SupportLossApplicationV16 {
            support_consumed,
            junior_face_burned,
        })
    }

    #[cfg(kani)]
    pub fn kani_apply_signed_kf_delta_to_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        delta: i128,
        source_domain: Option<usize>,
    ) -> V16Result<(u128, u128)> {
        let out = self.apply_signed_kf_delta_to_pnl(account, delta, source_domain)?;
        Ok((out.support_consumed, out.junior_face_burned))
    }

    #[cfg(kani)]
    pub fn kani_account_unliened_source_realizable_support(
        &self,
        account: &PortfolioV16View<'_>,
        face_claim: u128,
    ) -> V16Result<u128> {
        self.account_unliened_source_realizable_support(account, face_claim)
    }

    fn reserve_new_capital_backed_loss_for_source_domain_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        negative_before: u128,
        negative_after: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        let new_negative_loss = negative_after.saturating_sub(negative_before);
        if new_negative_loss == 0 {
            return Ok(());
        }
        let capital_not_already_encumbered =
            account.header.capital.get().saturating_sub(negative_before);
        let backing = new_negative_loss.min(capital_not_already_encumbered);
        if backing == 0 {
            return Ok(());
        }
        let backing_num = backing
            .checked_mul(BOUND_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let vault_before = self.header.vault.get();
        account.header.capital = V16PodU128::new(
            account
                .header
                .capital
                .get()
                .checked_sub(backing)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        self.header.c_tot = V16PodU128::new(
            self.header
                .c_tot
                .get()
                .checked_sub(backing)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        let backing_i128 = i128::try_from(backing).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = account
            .header
            .pnl
            .get()
            .checked_add(backing_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_account_pnl(account, new_pnl)?;
        TokenValueFlowProofV16::account_capital_to_realized_loss(
            backing,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        let expiry_slot = self.fresh_counterparty_backing_expiry_slot(domain)?;
        self.add_fresh_counterparty_backing_unchecked(domain, backing_num, expiry_slot)?;
        account.header.health_cert.valid = 0;
        Ok(())
    }

    fn kf_target_for_leg(
        &self,
        asset_index: usize,
        leg: PortfolioLegV16,
    ) -> V16Result<(i128, i128)> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
        {
            return Err(V16Error::InvalidLeg);
        }
        let asset = self.markets[asset_index].engine.asset.try_to_runtime()?;
        let (current_k, current_f, epoch_start_k, epoch_start_f, side_epoch, mode) = match leg.side
        {
            SideV16::Long => (
                asset.k_long,
                asset.f_long_num,
                asset.k_epoch_start_long,
                asset.f_epoch_start_long_num,
                asset.epoch_long,
                asset.mode_long,
            ),
            SideV16::Short => (
                asset.k_short,
                asset.f_short_num,
                asset.k_epoch_start_short,
                asset.f_epoch_start_short_num,
                asset.epoch_short,
                asset.mode_short,
            ),
        };
        if leg.epoch_snap == side_epoch {
            Ok((current_k, current_f))
        } else if mode == SideModeV16::ResetPending
            && leg.epoch_snap.checked_add(1) == Some(side_epoch)
        {
            Ok((epoch_start_k, epoch_start_f))
        } else {
            Err(V16Error::InvalidLeg)
        }
    }

    fn b_target_for_leg(&self, asset_index: usize, leg: PortfolioLegV16) -> V16Result<u128> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
        {
            return Err(V16Error::InvalidLeg);
        }
        let asset = self.markets[asset_index].engine.asset.try_to_runtime()?;
        let (current_b, epoch_start_b, side_epoch, mode) = match leg.side {
            SideV16::Long => (
                asset.b_long_num,
                asset.b_epoch_start_long_num,
                asset.epoch_long,
                asset.mode_long,
            ),
            SideV16::Short => (
                asset.b_short_num,
                asset.b_epoch_start_short_num,
                asset.epoch_short,
                asset.mode_short,
            ),
        };
        if leg.b_epoch_snap == side_epoch {
            Ok(current_b)
        } else if mode == SideModeV16::ResetPending
            && leg.b_epoch_snap.checked_add(1) == Some(side_epoch)
        {
            Ok(epoch_start_b)
        } else {
            Err(V16Error::InvalidLeg)
        }
    }

    #[inline(always)]
    fn leg_kf_delta_for_settlement(&self, leg: PortfolioLegV16) -> V16Result<(i128, i128, i128)> {
        let asset_index = leg.asset_index as usize;
        let (k_now, f_now) = self.kf_target_for_leg(asset_index, leg)?;
        let den = leg
            .a_basis
            .checked_mul(POS_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let k_delta = scaled_adl_delta_fast(
            leg.basis_pos_q.unsigned_abs(),
            leg.a_basis,
            leg.k_snap,
            k_now,
        )
        .unwrap_or_else(|| {
            wide_signed_mul_div_floor_from_k_pair(
                leg.basis_pos_q.unsigned_abs(),
                leg.k_snap,
                k_now,
                den,
            )
        });
        let f_delta = scaled_adl_delta_fast(
            leg.basis_pos_q.unsigned_abs(),
            leg.a_basis,
            leg.f_snap,
            f_now,
        )
        .unwrap_or_else(|| {
            wide_signed_mul_div_floor_from_k_pair(
                leg.basis_pos_q.unsigned_abs(),
                leg.f_snap,
                f_now,
                den,
            )
        });
        let net = k_delta
            .checked_add(f_delta)
            .ok_or(V16Error::ArithmeticOverflow)?;
        validate_non_min_i128(net)?;
        Ok((k_now, f_now, net))
    }

    #[cfg(kani)]
    pub fn kani_leg_kf_delta_for_settlement(
        &self,
        leg: PortfolioLegV16,
    ) -> V16Result<(i128, i128, i128)> {
        self.leg_kf_delta_for_settlement(leg)
    }

    fn settle_leg_kf_effects_at_slot(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        leg_slot: usize,
    ) -> V16Result<()> {
        if leg_slot >= V16_MAX_PORTFOLIO_ASSETS_N {
            return Err(V16Error::InvalidLeg);
        }
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
        if !leg.active {
            return Ok(());
        }
        let asset_index = leg.asset_index as usize;
        let (k_now, f_now, net) = self.leg_kf_delta_for_settlement(leg)?;
        if net != 0 {
            if net > 0 {
                let source_domain =
                    Some(self.insurance_domain_index(asset_index, opposite_side(leg.side))?);
                self.apply_signed_kf_delta_to_pnl(account, net, source_domain)?;
            } else {
                let negative_before = account.header.pnl.get().min(0).unsigned_abs();
                self.apply_signed_kf_delta_to_pnl(account, net, None)?;
                let negative_after = account.header.pnl.get().min(0).unsigned_abs();
                let loss_source_domain = self.insurance_domain_index(asset_index, leg.side)?;
                self.reserve_new_capital_backed_loss_for_source_domain_not_atomic(
                    account,
                    loss_source_domain,
                    negative_before,
                    negative_after,
                )?;
            }
        }
        leg.k_snap = k_now;
        leg.f_snap = f_now;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&leg);
        account.header.health_cert.valid = 0;
        Ok(())
    }

    fn clear_account_stale(&mut self, account: &mut PortfolioV16ViewMut<'_>) -> V16Result<()> {
        if decode_bool(account.header.stale_state)? {
            account.header.stale_state = 0;
            self.header.stale_certificate_count = V16PodU64::new(
                self.header
                    .stale_certificate_count
                    .get()
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?,
            );
        }
        Ok(())
    }

    fn compute_account_health_cert_with_price_override(
        &self,
        account: &PortfolioV16View<'_>,
        require_b_current: bool,
        price_override: Option<(usize, u64)>,
    ) -> V16Result<HealthCertV16> {
        let config = self.header.config.try_to_runtime_shape()?;
        let mut initial_req = 0u128;
        let mut maintenance_req = 0u128;
        let mut worst_case_loss = 0u128;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if !leg.active {
                slot += 1;
                continue;
            }
            let asset_index = leg.asset_index as usize;
            if require_b_current && self.b_target_for_leg(asset_index, leg)? > leg.b_snap {
                return Err(V16Error::BStale);
            }
            let price = if let Some((override_asset, override_price)) = price_override {
                if override_asset == asset_index {
                    override_price
                } else {
                    self.markets[asset_index].engine.asset.effective_price.get()
                }
            } else {
                self.markets[asset_index].engine.asset.effective_price.get()
            };
            let risk_notional = risk_notional_ceil(leg.basis_pos_q.unsigned_abs(), price)?;
            let target_lag_penalty = V16Core::target_effective_lag_loss_penalty(
                leg.basis_pos_q.unsigned_abs(),
                leg.side,
                price,
                self.markets[asset_index]
                    .engine
                    .asset
                    .raw_oracle_target_price
                    .get(),
            )?;
            let (leg_initial, leg_maintenance, leg_worst_case_loss) =
                V16Core::health_requirements_from_notional_and_target_lag(
                    config,
                    risk_notional,
                    target_lag_penalty,
                )?;
            initial_req = initial_req
                .checked_add(leg_initial)
                .ok_or(V16Error::ArithmeticOverflow)?;
            maintenance_req = maintenance_req
                .checked_add(leg_maintenance)
                .ok_or(V16Error::ArithmeticOverflow)?;
            worst_case_loss = worst_case_loss
                .checked_add(leg_worst_case_loss)
                .ok_or(V16Error::ArithmeticOverflow)?;
            slot += 1;
        }
        let equity = self.account_haircut_equity(account)?;
        Ok(HealthCertV16 {
            certified_equity: equity,
            certified_initial_req: initial_req,
            certified_maintenance_req: maintenance_req,
            certified_liq_deficit: if equity < 0 {
                equity.unsigned_abs()
            } else {
                maintenance_req.saturating_sub(equity as u128)
            },
            certified_worst_case_loss: worst_case_loss,
            cert_oracle_epoch: self.header.oracle_epoch.get(),
            cert_funding_epoch: self.header.funding_epoch.get(),
            cert_risk_epoch: self.header.risk_epoch.get(),
            cert_asset_set_epoch: self.header.asset_set_epoch.get(),
            active_bitmap_at_cert: account.header.active_bitmap.map(V16PodU64::get),
            valid: true,
        })
    }

    pub fn full_account_refresh_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<HealthCertV16> {
        match self.refresh_account_and_certify_not_atomic(account, None, 0, false)? {
            AccountRefreshCertOutcomeV16::Certified(cert) => Ok(cert),
            AccountRefreshCertOutcomeV16::BChunk(_) => Err(V16Error::BStale),
        }
    }

    fn refresh_account_and_certify_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        price_override: Option<(usize, u64)>,
        b_delta_budget: u128,
        allow_b_chunk: bool,
    ) -> V16Result<AccountRefreshCertOutcomeV16> {
        self.validate_account_scalar_preflight(&account.as_view())?;
        account
            .as_view()
            .validate_source_credit_shape_with_market(&self.as_view())?;
        let source_claim_sum_num = account.as_view().source_claim_bound_sum_num()?;
        if source_claim_sum_num != 0 {
            V16Core::validate_positive_pnl_source_attribution(
                account.header.pnl.get(),
                source_claim_sum_num,
            )?;
        }
        if decode_bool(account.header.b_stale_state)? && !allow_b_chunk {
            return Err(V16Error::BStale);
        }
        let config = self.header.config.try_to_runtime_shape()?;
        let mut initial_req = 0u128;
        let mut maintenance_req = 0u128;
        let mut worst_case_loss = 0u128;
        let active_leg_cap = config.max_portfolio_assets as usize;
        let configured_assets = config.max_market_slots as usize;
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut seen_assets = [u32::MAX; V16_MAX_PORTFOLIO_ASSETS_N];
        let mut seen_asset_count = 0usize;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            let bit = active_bitmap_get(bitmap, slot);
            if slot >= active_leg_cap {
                if bit || !leg.is_empty() {
                    return Err(V16Error::HiddenLeg);
                }
                slot += 1;
                continue;
            }
            if bit != leg.active {
                return Err(V16Error::HiddenLeg);
            }
            if !leg.active {
                if !leg.is_empty() {
                    return Err(V16Error::HiddenLeg);
                }
                slot += 1;
                continue;
            }
            validate_active_leg(leg)?;
            let asset_index = leg.asset_index as usize;
            if asset_index >= configured_assets || asset_index >= self.markets.len() {
                return Err(V16Error::HiddenLeg);
            }
            let asset = self.markets[asset_index].engine.asset.try_to_runtime()?;
            if leg.market_id != asset.market_id
                || !matches!(
                    asset.lifecycle,
                    AssetLifecycleV16::Active
                        | AssetLifecycleV16::DrainOnly
                        | AssetLifecycleV16::Recovery
                )
                || !leg_snapshots_bound_to_asset_side(asset, leg)
            {
                return Err(V16Error::HiddenLeg);
            }
            let mut seen = 0usize;
            while seen < seen_asset_count {
                if seen_assets[seen] == leg.asset_index {
                    return Err(V16Error::HiddenLeg);
                }
                seen += 1;
            }
            seen_assets[seen_asset_count] = leg.asset_index;
            seen_asset_count += 1;
            self.settle_leg_kf_effects_at_slot(account, slot)?;
            let mut refreshed = account.header.legs[slot].try_to_runtime()?;
            let target = self.b_target_for_leg(asset_index, refreshed)?;
            if target > refreshed.b_snap {
                self.mark_leg_b_stale(account, asset_index)?;
                if allow_b_chunk {
                    let chunk =
                        self.settle_account_b_chunk(account, asset_index, b_delta_budget)?;
                    if chunk.remaining_after != 0 {
                        return Ok(AccountRefreshCertOutcomeV16::BChunk(chunk));
                    }
                    refreshed = account.header.legs[slot].try_to_runtime()?;
                } else {
                    return Err(V16Error::BStale);
                }
            }
            if refreshed.b_stale {
                if allow_b_chunk {
                    return Ok(AccountRefreshCertOutcomeV16::BChunk(
                        AccountBSettlementChunkV16 {
                            delta_b: 0,
                            loss: 0,
                            new_remainder: refreshed.b_rem,
                            remaining_after: target.saturating_sub(refreshed.b_snap),
                        },
                    ));
                }
                return Err(V16Error::BStale);
            }
            let price = if let Some((override_asset, override_price)) = price_override {
                if override_asset == asset_index {
                    override_price
                } else {
                    self.markets[asset_index].engine.asset.effective_price.get()
                }
            } else {
                self.markets[asset_index].engine.asset.effective_price.get()
            };
            let risk_notional = risk_notional_ceil(refreshed.basis_pos_q.unsigned_abs(), price)?;
            let target_lag_penalty = V16Core::target_effective_lag_loss_penalty(
                refreshed.basis_pos_q.unsigned_abs(),
                refreshed.side,
                price,
                asset.raw_oracle_target_price,
            )?;
            let (leg_initial, leg_maintenance, leg_worst_case_loss) =
                V16Core::health_requirements_from_notional_and_target_lag(
                    config,
                    risk_notional,
                    target_lag_penalty,
                )?;
            initial_req = initial_req
                .checked_add(leg_initial)
                .ok_or(V16Error::ArithmeticOverflow)?;
            maintenance_req = maintenance_req
                .checked_add(leg_maintenance)
                .ok_or(V16Error::ArithmeticOverflow)?;
            worst_case_loss = worst_case_loss
                .checked_add(leg_worst_case_loss)
                .ok_or(V16Error::ArithmeticOverflow)?;
            slot += 1;
        }
        self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        self.collect_account_backing_utilization_fees_not_atomic(account)?;
        if decode_bool(account.header.b_stale_state)? || Self::has_b_stale_leg(&account.as_view())?
        {
            return Err(V16Error::BStale);
        }
        if decode_bool(account.header.stale_state)? {
            self.clear_account_stale(account)?;
        }
        let equity = self.account_haircut_equity(&account.as_view())?;
        let cert = HealthCertV16 {
            certified_equity: equity,
            certified_initial_req: initial_req,
            certified_maintenance_req: maintenance_req,
            certified_liq_deficit: if equity < 0 {
                equity.unsigned_abs()
            } else {
                maintenance_req.saturating_sub(equity as u128)
            },
            certified_worst_case_loss: worst_case_loss,
            cert_oracle_epoch: self.header.oracle_epoch.get(),
            cert_funding_epoch: self.header.funding_epoch.get(),
            cert_risk_epoch: self.header.risk_epoch.get(),
            cert_asset_set_epoch: self.header.asset_set_epoch.get(),
            active_bitmap_at_cert: account.header.active_bitmap.map(V16PodU64::get),
            valid: true,
        };
        account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
        self.validate_account_audit_scan(&account.as_view())?;
        self.validate_shape_audit_scan()?;
        Ok(AccountRefreshCertOutcomeV16::Certified(cert))
    }

    fn has_b_stale_leg(account: &PortfolioV16View<'_>) -> V16Result<bool> {
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active && leg.b_stale {
                return Ok(true);
            }
            slot += 1;
        }
        Ok(false)
    }

    fn mark_account_b_stale(&mut self, account: &mut PortfolioV16ViewMut<'_>) -> V16Result<()> {
        if !decode_bool(account.header.b_stale_state)? {
            account.header.b_stale_state = 1;
            account.header.health_cert.valid = 0;
            self.header.b_stale_account_count = V16PodU64::new(
                self.header
                    .b_stale_account_count
                    .get()
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?,
            );
        }
        Ok(())
    }

    fn clear_account_b_stale(&mut self, account: &mut PortfolioV16ViewMut<'_>) -> V16Result<()> {
        if Self::has_b_stale_leg(&account.as_view())? {
            return Err(V16Error::BStale);
        }
        if decode_bool(account.header.b_stale_state)? {
            account.header.b_stale_state = 0;
            self.header.b_stale_account_count = V16PodU64::new(
                self.header
                    .b_stale_account_count
                    .get()
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?,
            );
        }
        Ok(())
    }

    fn collect_account_backing_utilization_fees_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let mut total_charged = 0u128;
        let domain_count = self
            .configured_domain_count()?
            .min(account.source_domains.len());
        let mut d = 0usize;
        while d < domain_count {
            total_charged = total_charged
                .checked_add(
                    self.collect_account_backing_utilization_fee_for_domain_not_atomic(account, d)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        self.validate_account_audit_scan(&account.as_view())?;
        self.validate_shape_audit_scan()?;
        Ok(total_charged)
    }

    fn collect_account_backing_utilization_fee_for_domain_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
    ) -> V16Result<u128> {
        self.domain_asset_side(domain)?;
        if domain >= account.source_domains.len() {
            return Err(V16Error::InvalidLeg);
        }
        let lien_backing_num = account.source_domains[domain]
            .source_lien_counterparty_backing_num
            .get();
        if lien_backing_num == 0 {
            account.source_domains[domain].source_lien_fee_last_slot = V16PodU64::new(0);
            return Ok(0);
        }
        let last_slot = account.source_domains[domain]
            .source_lien_fee_last_slot
            .get();
        if last_slot == 0 {
            account.source_domains[domain].source_lien_fee_last_slot =
                V16PodU64::new(self.header.current_slot.get());
            return Ok(0);
        }
        let fee = V16Core::backing_utilization_fee_quote_atoms_for_lien(
            self.header.config.try_to_runtime_shape()?,
            self.source_credit_for_domain(domain)?,
            lien_backing_num,
            last_slot,
            self.header.current_slot.get(),
        )?;
        account.source_domains[domain].source_lien_fee_last_slot =
            V16PodU64::new(self.header.current_slot.get());
        let mut bucket = self.backing_bucket_for_domain(domain)?;
        let (charged, next_capital, next_c_tot, next_earnings) =
            apply_backing_utilization_fee_charge(
                account.header.capital.get(),
                self.header.c_tot.get(),
                bucket.utilization_fee_earnings,
                account.header.pnl.get(),
                fee,
            )?;
        if charged == 0 {
            return Ok(0);
        }
        account.header.capital = V16PodU128::new(next_capital);
        self.header.c_tot = V16PodU128::new(next_c_tot);
        bucket.utilization_fee_earnings = next_earnings;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        account.header.health_cert.valid = 0;
        Ok(charged)
    }

    fn mark_leg_b_stale(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
    ) -> V16Result<()> {
        let leg_slot = Self::require_active_leg_slot_for_asset(&account.as_view(), asset_index)?;
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
        leg.b_stale = true;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&leg);
        self.mark_account_b_stale(account)
    }

    fn account_b_settlement_chunk_from_leg(
        &self,
        leg: PortfolioLegV16,
        target: u128,
        endpoint_delta_budget: u128,
    ) -> V16Result<AccountBSettlementChunkV16> {
        if target < leg.b_snap {
            return Err(V16Error::RecoveryRequired);
        }
        let b_remaining = target - leg.b_snap;
        if b_remaining == 0 {
            return Ok(AccountBSettlementChunkV16 {
                delta_b: 0,
                loss: 0,
                new_remainder: leg.b_rem,
                remaining_after: 0,
            });
        }
        if leg.loss_weight == 0 || endpoint_delta_budget == 0 {
            return Err(V16Error::RecoveryRequired);
        }
        let limit = self.header.config.public_b_chunk_atoms.get();
        let max_num = limit
            .checked_add(1)
            .and_then(|v| v.checked_mul(SOCIAL_LOSS_DEN))
            .and_then(|v| v.checked_sub(1))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if leg.b_rem > max_num {
            return Err(V16Error::RecoveryRequired);
        }
        let max_delta_by_loss = (max_num - leg.b_rem) / leg.loss_weight;
        let delta_b = b_remaining
            .min(max_delta_by_loss)
            .min(endpoint_delta_budget);
        if delta_b == 0 {
            return Err(V16Error::RecoveryRequired);
        }
        let num = leg
            .loss_weight
            .checked_mul(delta_b)
            .and_then(|v| v.checked_add(leg.b_rem))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let loss = num / SOCIAL_LOSS_DEN;
        let new_remainder = num % SOCIAL_LOSS_DEN;
        Ok(AccountBSettlementChunkV16 {
            delta_b,
            loss,
            new_remainder,
            remaining_after: b_remaining - delta_b,
        })
    }

    fn account_b_settlement_chunk(
        &self,
        account: &PortfolioV16View<'_>,
        asset_index: usize,
        endpoint_delta_budget: u128,
    ) -> V16Result<AccountBSettlementChunkV16> {
        account.validate_with_market(&self.as_view())?;
        let leg = Self::active_leg_for_asset(account, asset_index)?;
        if !leg.active {
            return Err(V16Error::InvalidLeg);
        }
        let target = self.b_target_for_leg(asset_index, leg)?;
        if target < leg.b_snap {
            return Err(V16Error::RecoveryRequired);
        }
        self.account_b_settlement_chunk_from_leg(leg, target, endpoint_delta_budget)
    }

    pub fn settle_account_b_chunk(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        endpoint_delta_budget: u128,
    ) -> V16Result<AccountBSettlementChunkV16> {
        let chunk = self.account_b_settlement_chunk(
            &account.as_view(),
            asset_index,
            endpoint_delta_budget,
        )?;
        if chunk.delta_b == 0 {
            if !Self::has_b_stale_leg(&account.as_view())? {
                self.clear_account_b_stale(account)?;
            }
            return Ok(chunk);
        }
        let old_pnl = account.header.pnl.get();
        let loss_i128 = i128::try_from(chunk.loss).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = old_pnl
            .checked_sub(loss_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let leg_slot = Self::require_active_leg_slot_for_asset(&account.as_view(), asset_index)?;
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
        leg.b_snap = leg
            .b_snap
            .checked_add(chunk.delta_b)
            .ok_or(V16Error::ArithmeticOverflow)?;
        leg.b_rem = chunk.new_remainder;
        leg.b_stale = chunk.remaining_after != 0;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&leg);
        self.set_account_pnl(account, new_pnl)?;
        if chunk.remaining_after != 0 {
            self.mark_account_b_stale(account)?;
        } else if !Self::has_b_stale_leg(&account.as_view())? {
            self.clear_account_b_stale(account)?;
        }
        account.header.health_cert.valid = 0;
        self.validate_account_audit_scan(&account.as_view())?;
        Ok(chunk)
    }

    fn settle_account_side_effects_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        b_delta_budget: u128,
    ) -> V16Result<PermissionlessProgressOutcomeV16> {
        account.validate_with_market(&self.as_view())?;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active {
                let asset_index = leg.asset_index as usize;
                self.settle_leg_kf_effects_at_slot(account, slot)?;
                let refreshed = account.header.legs[slot].try_to_runtime()?;
                let target = self.b_target_for_leg(asset_index, refreshed)?;
                if target > refreshed.b_snap {
                    self.mark_leg_b_stale(account, asset_index)?;
                    let chunk =
                        self.settle_account_b_chunk(account, asset_index, b_delta_budget)?;
                    if chunk.remaining_after != 0 {
                        return Ok(PermissionlessProgressOutcomeV16::AccountBChunk(chunk));
                    }
                }
            }
            slot += 1;
        }
        self.settle_negative_pnl_from_principal_not_atomic(account)?;
        account.header.health_cert.valid = 0;
        Ok(PermissionlessProgressOutcomeV16::AccountCurrent)
    }

    fn certify_account_after_local_settlement_with_price_override(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        price_override: Option<(usize, u64)>,
    ) -> V16Result<HealthCertV16> {
        self.collect_account_backing_utilization_fees_not_atomic(account)?;
        if decode_bool(account.header.b_stale_state)? || Self::has_b_stale_leg(&account.as_view())?
        {
            return Err(V16Error::BStale);
        }
        if decode_bool(account.header.stale_state)? {
            self.clear_account_stale(account)?;
        }
        let cert = self.compute_account_health_cert_with_price_override(
            &account.as_view(),
            true,
            price_override,
        )?;
        account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
        Ok(cert)
    }

    fn require_asset_accruable(&self, asset_index: usize) -> V16Result<()> {
        match self.asset_state(asset_index)?.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => Ok(()),
            _ => Err(V16Error::LockActive),
        }
    }

    fn accruable_asset_slot_summary(
        &self,
        config: &V16Config,
        now_slot: u64,
    ) -> V16Result<(u64, bool)> {
        let configured = (config.max_market_slots as usize).min(self.markets.len());
        let mut anchor = now_slot;
        let mut saw_accruable = false;
        let mut i = 0usize;
        while i < configured {
            let asset = self.markets[i].engine.asset.try_to_runtime()?;
            if asset_contributes_to_loss_stale_summary(asset) {
                if asset.slot_last > now_slot {
                    return Err(V16Error::InvalidConfig);
                }
                saw_accruable = true;
                anchor = anchor.min(asset.slot_last);
            }
            i += 1;
        }
        Ok((anchor, saw_accruable && anchor < now_slot))
    }

    pub fn accrue_asset_to_not_atomic(
        &mut self,
        asset_index: usize,
        now_slot: u64,
        effective_price: u64,
        funding_rate_e9: i128,
        protective_progress_committed: bool,
    ) -> V16Result<AccrueAssetOutcomeV16> {
        let config = self.header.config.try_to_runtime_shape()?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        if asset_index >= config.max_market_slots as usize
            || asset_index >= self.markets.len()
            || effective_price == 0
            || effective_price > MAX_ORACLE_PRICE
            || funding_rate_e9.unsigned_abs() > config.max_abs_funding_e9_per_slot as u128
            || now_slot < self.header.current_slot.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        self.require_asset_accruable(asset_index)?;
        let old = self.asset_state(asset_index)?;
        if now_slot < old.slot_last {
            return Err(V16Error::InvalidConfig);
        }
        let dt_total = now_slot - old.slot_last;
        let segment_dt = if dt_total > config.max_accrual_dt_slots {
            config.max_accrual_dt_slots
        } else {
            dt_total
        };
        let activity = V16Core::accrual_activity_for_asset_segment(
            old,
            segment_dt,
            effective_price,
            funding_rate_e9,
        );
        if activity.equity_active {
            if segment_dt == 0 {
                return Err(V16Error::NonProgress);
            }
            let price_diff = effective_price.abs_diff(old.effective_price) as u128;
            let lhs = price_diff
                .checked_mul(MAX_MARGIN_BPS as u128)
                .ok_or(V16Error::ArithmeticOverflow)?;
            let rhs = (config.max_price_move_bps_per_slot as u128)
                .checked_mul(segment_dt as u128)
                .and_then(|v| v.checked_mul(old.effective_price as u128))
                .ok_or(V16Error::ArithmeticOverflow)?;
            if lhs > rhs {
                return Err(V16Error::RecoveryRequired);
            }
            if !protective_progress_committed {
                return Err(V16Error::NonProgress);
            }
        }

        let price_delta = effective_price as i128 - old.effective_price as i128;
        let k_delta = checked_i128_mul(price_delta, ADL_ONE as i128)?;
        let funding_delta = if activity.funding_active {
            let n = funding_rate_e9
                .checked_mul(segment_dt as i128)
                .and_then(|v| v.checked_mul(effective_price as i128))
                .ok_or(V16Error::ArithmeticOverflow)?;
            floor_div_signed_conservative_i128(n, FUNDING_DEN)
                .checked_mul(ADL_ONE as i128)
                .ok_or(V16Error::ArithmeticOverflow)?
        } else {
            0
        };

        let mut asset = old;
        asset.k_long = add_non_min_i128(asset.k_long, k_delta)?;
        asset.k_short = add_non_min_i128(asset.k_short, -k_delta)?;
        asset.f_long_num = add_non_min_i128(asset.f_long_num, -funding_delta)?;
        asset.f_short_num = add_non_min_i128(asset.f_short_num, funding_delta)?;
        asset.effective_price = effective_price;
        asset.fund_px_last = effective_price;
        asset.slot_last = asset
            .slot_last
            .checked_add(segment_dt)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_asset_state(asset_index, asset)?;
        self.header.current_slot = V16PodU64::new(now_slot);
        let (group_slot_last, group_loss_stale) =
            self.accruable_asset_slot_summary(&config, now_slot)?;
        self.header.slot_last = V16PodU64::new(group_slot_last);
        self.header.loss_stale_active = encode_bool(group_loss_stale);
        if activity.price_move_active {
            self.header.oracle_epoch = V16PodU64::new(
                self.header
                    .oracle_epoch
                    .get()
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?,
            );
        }
        if activity.funding_active {
            self.header.funding_epoch = V16PodU64::new(
                self.header
                    .funding_epoch
                    .get()
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?,
            );
        }
        self.validate_shape_audit_scan()?;
        Ok(AccrueAssetOutcomeV16 {
            dt: segment_dt,
            price_move_active: activity.price_move_active,
            funding_active: activity.funding_active,
            equity_active: activity.equity_active,
            loss_stale_after: asset.slot_last < now_slot,
        })
    }

    pub fn declare_permissionless_recovery(
        &mut self,
        reason: PermissionlessRecoveryReasonV16,
    ) -> V16Result<PermissionlessProgressOutcomeV16> {
        if !decode_bool(self.header.config.permissionless_recovery_enabled)? {
            return Err(V16Error::InvalidConfig);
        }
        if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
            return Err(V16Error::LockActive);
        }
        if let Some(existing_reason) = self.header.recovery_reason.try_to_runtime()? {
            return Ok(PermissionlessProgressOutcomeV16::RecoveryDeclared(
                existing_reason,
            ));
        }
        self.header.mode = encode_market_mode(MarketModeV16::Recovery);
        self.header.recovery_reason = V16OptionalRecoveryReasonAccount::from_runtime(Some(reason));
        self.validate_shape()?;
        Ok(PermissionlessProgressOutcomeV16::RecoveryDeclared(reason))
    }

    pub fn declare_explicit_loss_or_dust_audit_overflow_not_atomic(
        &mut self,
    ) -> V16Result<PermissionlessProgressOutcomeV16> {
        self.declare_permissionless_recovery(
            PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow,
        )
    }

    pub fn permissionless_crank_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        request: PermissionlessCrankRequestV16,
    ) -> V16Result<PermissionlessProgressOutcomeV16> {
        self.validate_unconfigured_market_tail()?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            && !matches!(request.action, PermissionlessCrankActionV16::Recover(_))
        {
            return Err(V16Error::LockActive);
        }
        let protective_progress = match request.action {
            PermissionlessCrankActionV16::Refresh => {
                let touches_accrued_asset = request.asset_index
                    < self.header.config.max_market_slots.get() as usize
                    && Self::active_leg_slot_for_asset(&account.as_view(), request.asset_index)?
                        .is_some();
                match self.refresh_account_and_certify_not_atomic(
                    account,
                    Some((request.asset_index, request.effective_price)),
                    self.header.config.public_b_chunk_atoms.get(),
                    true,
                )? {
                    AccountRefreshCertOutcomeV16::Certified(_) => {}
                    AccountRefreshCertOutcomeV16::BChunk(out) => {
                        self.validate_shape_audit_scan()?;
                        return Ok(PermissionlessProgressOutcomeV16::AccountBChunk(out));
                    }
                }
                touches_accrued_asset
            }
            PermissionlessCrankActionV16::SettleB { asset_index } => {
                let out = self.settle_account_b_chunk(
                    account,
                    asset_index,
                    self.header.config.public_b_chunk_atoms.get(),
                )?;
                return Ok(PermissionlessProgressOutcomeV16::AccountBChunk(out));
            }
            PermissionlessCrankActionV16::Liquidate(_) => {
                if let PermissionlessCrankActionV16::Liquidate(liq) = request.action {
                    let liquidated_asset_index = liq.asset_index;
                    self.liquidate_account_not_atomic(account, liq)?;
                    liquidated_asset_index == request.asset_index
                } else {
                    unreachable!()
                }
            }
            PermissionlessCrankActionV16::Recover(reason) => {
                return self.declare_permissionless_recovery(reason);
            }
        };
        self.accrue_asset_to_not_atomic(
            request.asset_index,
            request.now_slot,
            request.effective_price,
            request.funding_rate_e9,
            protective_progress,
        )?;
        Ok(PermissionlessProgressOutcomeV16::AccountCurrent)
    }

    fn active_leg_slot_for_asset(
        account: &PortfolioV16View<'_>,
        asset_index: usize,
    ) -> V16Result<Option<usize>> {
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut found = None;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            if active_bitmap_get(bitmap, slot) {
                let leg = account.header.legs[slot].try_to_runtime()?;
                if !leg.active {
                    return Err(V16Error::HiddenLeg);
                }
                if leg.asset_index as usize == asset_index {
                    if found.is_some() {
                        return Err(V16Error::HiddenLeg);
                    }
                    found = Some(slot);
                }
            }
            slot += 1;
        }
        Ok(found)
    }

    fn require_active_leg_slot_for_asset(
        account: &PortfolioV16View<'_>,
        asset_index: usize,
    ) -> V16Result<usize> {
        Self::active_leg_slot_for_asset(account, asset_index)?.ok_or(V16Error::InvalidLeg)
    }

    fn active_leg_for_asset(
        account: &PortfolioV16View<'_>,
        asset_index: usize,
    ) -> V16Result<PortfolioLegV16> {
        if let Some(slot) = Self::active_leg_slot_for_asset(account, asset_index)? {
            account.header.legs[slot].try_to_runtime()
        } else {
            Ok(PortfolioLegV16::EMPTY)
        }
    }

    fn empty_leg_slot(account: &PortfolioV16View<'_>) -> V16Result<usize> {
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if !active_bitmap_get(bitmap, slot) && !leg.active {
                if !leg.is_empty() {
                    return Err(V16Error::HiddenLeg);
                }
                return Ok(slot);
            }
            slot += 1;
        }
        Err(V16Error::InvalidLeg)
    }

    fn asset_state(&self, asset_index: usize) -> V16Result<AssetStateV16> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
        {
            return Err(V16Error::InvalidLeg);
        }
        self.markets[asset_index].engine.asset.try_to_runtime()
    }

    fn set_asset_state(&mut self, asset_index: usize, asset: AssetStateV16) -> V16Result<()> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
        {
            return Err(V16Error::InvalidLeg);
        }
        self.markets[asset_index].engine.asset = AssetStateV16Account::from_runtime(&asset);
        Ok(())
    }

    fn require_asset_active_for_risk_increase(&self, asset_index: usize) -> V16Result<()> {
        let asset = self.asset_state(asset_index)?;
        if asset.lifecycle != AssetLifecycleV16::Active {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn validate_configured_asset_index(&self, asset_index: usize) -> V16Result<()> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
        {
            return Err(V16Error::InvalidLeg);
        }
        Ok(())
    }

    #[inline]
    fn validate_unconfigured_market_tail(&self) -> V16Result<()> {
        self.header
            .validate_dynamic_market_slots_shape(self.markets)
    }

    fn checked_asset_set_epoch_bump(&self) -> V16Result<(u64, u64)> {
        let next_asset_set_epoch = self
            .header
            .asset_set_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        let next_risk_epoch = self
            .header
            .risk_epoch
            .get()
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((next_asset_set_epoch, next_risk_epoch))
    }

    fn commit_asset_set_epoch_bump(&mut self, next_asset_set_epoch: u64, next_risk_epoch: u64) {
        self.header.asset_set_epoch = V16PodU64::new(next_asset_set_epoch);
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
    }

    fn require_asset_live_reducible(&self, asset_index: usize) -> V16Result<()> {
        let asset = self.asset_state(asset_index)?;
        match asset.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => Ok(()),
            _ => Err(V16Error::LockActive),
        }
    }

    fn require_empty_asset_lifecycle_state(&self, asset_index: usize) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let asset = self.asset_state(asset_index)?;
        let long_domain = self.insurance_domain_index(asset_index, SideV16::Long)?;
        let short_domain = self.insurance_domain_index(asset_index, SideV16::Short)?;
        let long_bucket = self.backing_bucket_for_domain(long_domain)?;
        let short_bucket = self.backing_bucket_for_domain(short_domain)?;
        let long_source = self.source_credit_for_domain_shape(long_domain)?;
        let short_source = self.source_credit_for_domain_shape(short_domain)?;
        let long_reservation = self.insurance_reservation_for_domain(long_domain)?;
        let short_reservation = self.insurance_reservation_for_domain(short_domain)?;
        let slot = self.markets[asset_index].engine_slot();

        if slot.pending_domain_loss_barrier_long.get() != 0
            || slot.pending_domain_loss_barrier_short.get() != 0
            || asset.mode_long != SideModeV16::Normal
            || asset.mode_short != SideModeV16::Normal
            || !((asset.a_long == ADL_ONE && asset.a_short == ADL_ONE)
                || (asset.a_long == 0 && asset.a_short == 0))
            || asset.k_long != 0
            || asset.k_short != 0
            || asset.f_long_num != 0
            || asset.f_short_num != 0
            || asset.k_epoch_start_long != 0
            || asset.k_epoch_start_short != 0
            || asset.f_epoch_start_long_num != 0
            || asset.f_epoch_start_short_num != 0
            || asset.b_long_num != 0
            || asset.b_short_num != 0
            || asset.b_epoch_start_long_num != 0
            || asset.b_epoch_start_short_num != 0
            || asset.oi_eff_long_q != 0
            || asset.oi_eff_short_q != 0
            || asset.stored_pos_count_long != 0
            || asset.stored_pos_count_short != 0
            || asset.stale_account_count_long != 0
            || asset.stale_account_count_short != 0
            || asset.pending_obligation_count_long != 0
            || asset.pending_obligation_count_short != 0
            || asset.loss_weight_sum_long != 0
            || asset.loss_weight_sum_short != 0
            || asset.social_loss_remainder_long_num != 0
            || asset.social_loss_remainder_short_num != 0
            || asset.social_loss_dust_long_num != 0
            || asset.social_loss_dust_short_num != 0
            || asset.explicit_unallocated_loss_long != 0
            || asset.explicit_unallocated_loss_short != 0
            || slot.insurance_domain_spent_long.get() != 0
            || slot.insurance_domain_spent_short.get() != 0
            || long_source != SourceCreditStateV16::EMPTY
            || short_source != SourceCreditStateV16::EMPTY
            || !long_bucket.is_empty_amount_shape()
            || !short_bucket.is_empty_amount_shape()
            || long_bucket.market_id != asset.market_id
            || short_bucket.market_id != asset.market_id
            || long_reservation != InsuranceCreditReservationV16::EMPTY
            || short_reservation != InsuranceCreditReservationV16::EMPTY
        {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn side_mode_for(&self, asset_index: usize, side: SideV16) -> V16Result<SideModeV16> {
        let asset = self.asset_state(asset_index)?;
        Ok(match side {
            SideV16::Long => asset.mode_long,
            SideV16::Short => asset.mode_short,
        })
    }

    fn account_b_loss_bound(account: &PortfolioV16View<'_>) -> V16Result<u128> {
        let mut bound = 0u128;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active && leg.b_stale {
                bound = bound
                    .checked_add(leg.loss_weight)
                    .ok_or(V16Error::ArithmeticOverflow)?;
            }
            slot += 1;
        }
        Ok(bound)
    }

    fn risk_score_unchecked(&self, account: &PortfolioV16View<'_>) -> V16Result<RiskScoreV16> {
        let cert = account.header.health_cert.try_to_runtime()?;
        if !cert.valid {
            return Err(V16Error::Stale);
        }
        Ok(RiskScoreV16 {
            certified_liq_deficit: cert.certified_liq_deficit,
            unsettled_b_loss_bound: Self::account_b_loss_bound(account)?,
            stale_loss_bound: if decode_bool(account.header.stale_state)? {
                1
            } else {
                0
            },
            gross_risk_notional: cert.certified_worst_case_loss,
            active_leg_count: active_bitmap_count_ones(
                account.header.active_bitmap.map(V16PodU64::get),
            ),
        })
    }

    #[inline(never)]
    fn validate_liquidation_progress_from_score(
        &self,
        before_score: RiskScoreV16,
        after: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        let after_score = self.risk_score_unchecked(after)?;
        if V16Core::liquidation_progress_from_scores(before_score, after_score) {
            Ok(())
        } else {
            Err(V16Error::NonProgress)
        }
    }

    fn ensure_favorable_action_current_certificate(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        let cert = account.header.health_cert.try_to_runtime()?;
        if !cert.valid
            || cert.cert_oracle_epoch != self.header.oracle_epoch.get()
            || cert.cert_funding_epoch != self.header.funding_epoch.get()
            || cert.cert_risk_epoch != self.header.risk_epoch.get()
            || cert.cert_asset_set_epoch != self.header.asset_set_epoch.get()
            || cert.active_bitmap_at_cert != account.header.active_bitmap.map(V16PodU64::get)
        {
            return Err(V16Error::Stale);
        }
        Ok(())
    }

    fn account_has_target_effective_lag(&self, account: &PortfolioV16View<'_>) -> V16Result<bool> {
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active && self.asset_has_target_effective_lag(leg.asset_index as usize)? {
                return Ok(true);
            }
            slot += 1;
        }
        Ok(false)
    }

    fn ensure_favorable_action_allowed(&self, account: &PortfolioV16View<'_>) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if self.h_lock_lane(Some(account), false)? == HLockLaneV16::HMax {
            return Err(V16Error::LockActive);
        }
        self.ensure_favorable_action_current_certificate(account)?;
        if self.account_has_target_effective_lag(account)? {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn account_has_active_exposure_for_source_domain(
        &self,
        account: &PortfolioV16View<'_>,
        domain: usize,
    ) -> V16Result<bool> {
        let (asset_index, source_side) = self.domain_asset_side(domain)?;
        let leg = Self::active_leg_for_asset(account, asset_index)?;
        Ok(leg.active && opposite_side(leg.side) == source_side)
    }

    fn account_has_active_source_claim_exposure(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<bool> {
        let configured_domains = self.configured_domain_count()?;
        if account.source_domains.len() < configured_domains {
            return Err(V16Error::InvalidLeg);
        }
        let mut d = 0usize;
        while d < configured_domains {
            if account.source_domains[d].source_claim_bound_num.get() != 0
                && self.account_has_active_exposure_for_source_domain(account, d)?
            {
                return Ok(true);
            }
            d += 1;
        }
        Ok(false)
    }

    fn pending_domain_loss_barrier_count(
        &self,
        asset_index: usize,
        side: SideV16,
    ) -> V16Result<u64> {
        let domain = self.insurance_domain_index(asset_index, side)?;
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        Ok(match side {
            SideV16::Long => slot.pending_domain_loss_barrier_long.get(),
            SideV16::Short => slot.pending_domain_loss_barrier_short.get(),
        })
    }

    fn has_pending_domain_loss_barrier(
        &self,
        asset_index: usize,
        side: SideV16,
    ) -> V16Result<bool> {
        Ok(self.pending_domain_loss_barrier_count(asset_index, side)? != 0)
    }

    fn account_touches_pending_domain_loss_barrier(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<bool> {
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active
                && self.has_pending_domain_loss_barrier(leg.asset_index as usize, leg.side)?
            {
                return Ok(true);
            }
            slot += 1;
        }
        Ok(false)
    }

    fn position_change_touches_pending_domain_loss_barrier(
        &self,
        asset_index: usize,
        current: i128,
        next: i128,
    ) -> V16Result<bool> {
        if current != 0 {
            let current_side = if current > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            if self.has_pending_domain_loss_barrier(asset_index, current_side)? {
                return Ok(true);
            }
        }
        if next != 0 {
            let next_side = if next > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            if self.has_pending_domain_loss_barrier(asset_index, next_side)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(kani)]
    pub fn kani_position_change_touches_pending_domain_loss_barrier(
        &self,
        asset_index: usize,
        current: i128,
        next: i128,
    ) -> V16Result<bool> {
        self.position_change_touches_pending_domain_loss_barrier(asset_index, current, next)
    }

    fn position_delta_touches_pending_domain_loss_barrier(
        &self,
        account: &PortfolioV16View<'_>,
        asset_index: usize,
        delta_q: i128,
    ) -> V16Result<bool> {
        if delta_q == 0 {
            return Ok(false);
        }
        let current = signed_position(Self::active_leg_for_asset(account, asset_index)?);
        let next = current
            .checked_add(delta_q)
            .ok_or(V16Error::ArithmeticOverflow)?;
        validate_basis_or_zero(next)?;
        self.position_change_touches_pending_domain_loss_barrier(asset_index, current, next)
    }

    fn position_delta_blocked_by_pending_domain_loss_barrier(
        &self,
        account: &PortfolioV16View<'_>,
        asset_index: usize,
        delta_q: i128,
    ) -> V16Result<bool> {
        let current = signed_position(Self::active_leg_for_asset(account, asset_index)?);
        let next = current
            .checked_add(delta_q)
            .ok_or(V16Error::ArithmeticOverflow)?;
        validate_basis_or_zero(next)?;
        if !self.position_change_touches_pending_domain_loss_barrier(asset_index, current, next)? {
            return Ok(false);
        }
        Ok(!same_side_risk_reduction_or_flat_obligation(current, next))
    }

    fn h_lock_lane(
        &self,
        account: Option<&PortfolioV16View<'_>>,
        instruction_bankruptcy_candidate: bool,
    ) -> V16Result<HLockLaneV16> {
        if let Some(account) = account {
            if decode_bool(account.header.stale_state)?
                || decode_bool(account.header.b_stale_state)?
            {
                return Ok(HLockLaneV16::HMax);
            }
            if account
                .header
                .close_progress
                .try_to_runtime()?
                .has_pending_residual()
            {
                return Ok(HLockLaneV16::HMax);
            }
            if self.account_touches_pending_domain_loss_barrier(account)? {
                return Ok(HLockLaneV16::HMax);
            }
        }
        if decode_bool(self.header.threshold_stress_active)?
            || decode_bool(self.header.bankruptcy_hlock_active)?
            || decode_market_mode(self.header.mode)? == MarketModeV16::Recovery
            || instruction_bankruptcy_candidate
            || decode_bool(self.header.loss_stale_active)?
        {
            return Ok(HLockLaneV16::HMax);
        }
        Ok(HLockLaneV16::HMin)
    }

    #[cfg(kani)]
    pub fn kani_h_lock_lane(
        &self,
        account: Option<&PortfolioV16View<'_>>,
        instruction_bankruptcy_candidate: bool,
    ) -> V16Result<HLockLaneV16> {
        self.h_lock_lane(account, instruction_bankruptcy_candidate)
    }

    fn asset_has_target_effective_lag(&self, asset_index: usize) -> V16Result<bool> {
        let asset = self.asset_state(asset_index)?;
        Ok(asset.raw_oracle_target_price != asset.effective_price)
    }

    fn validate_trade_request(&self, request: TradeRequestV16) -> V16Result<()> {
        let config = self.header.config.try_to_runtime_shape()?;
        if request.asset_index >= config.max_market_slots as usize
            || request.size_q == 0
            || request.size_q > MAX_TRADE_SIZE_Q
            || request.exec_price == 0
            || request.exec_price > MAX_ORACLE_PRICE
            || request.fee_bps > config.max_trading_fee_bps
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn validate_trade_position_preflight(
        &self,
        long_account: &PortfolioV16View<'_>,
        short_account: &PortfolioV16View<'_>,
        request: TradeRequestV16,
    ) -> V16Result<TradePositionPreflightV16> {
        let long_delta =
            i128::try_from(request.size_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let short_delta = long_delta
            .checked_neg()
            .ok_or(V16Error::ArithmeticOverflow)?;
        let long_lookup =
            Self::position_delta_lookup_for_asset(long_account, request.asset_index, long_delta)?;
        let short_lookup =
            Self::position_delta_lookup_for_asset(short_account, request.asset_index, short_delta)?;
        let risk_increasing = position_delta_increases_risk(long_lookup.current_q, long_delta)?
            || position_delta_increases_risk(short_lookup.current_q, short_delta)?;
        let target_effective_lag = self.asset_has_target_effective_lag(request.asset_index)?;
        let touches_pending_domain_barrier =
            (self.position_change_touches_pending_domain_loss_barrier(
                request.asset_index,
                long_lookup.current_q,
                long_lookup.next_q,
            )? && !same_side_risk_reduction_or_flat_obligation(
                long_lookup.current_q,
                long_lookup.next_q,
            )) || (self.position_change_touches_pending_domain_loss_barrier(
                request.asset_index,
                short_lookup.current_q,
                short_lookup.next_q,
            )? && !same_side_risk_reduction_or_flat_obligation(
                short_lookup.current_q,
                short_lookup.next_q,
            ));
        if touches_pending_domain_barrier {
            return Err(V16Error::LockActive);
        }
        if risk_increasing && (decode_bool(self.header.loss_stale_active)? || target_effective_lag)
        {
            return Err(V16Error::LockActive);
        }
        Ok(TradePositionPreflightV16 {
            risk_increasing,
            long_lookup,
            short_lookup,
            long_old_abs_q: long_lookup.current_q.unsigned_abs(),
            short_old_abs_q: short_lookup.current_q.unsigned_abs(),
            long_new_abs_q: long_lookup.next_q.unsigned_abs(),
            short_new_abs_q: short_lookup.next_q.unsigned_abs(),
            long_has_source_claims: Self::account_has_source_claims(long_account)?,
            short_has_source_claims: Self::account_has_source_claims(short_account)?,
        })
    }

    fn position_delta_lookup_for_asset(
        account: &PortfolioV16View<'_>,
        asset_index: usize,
        delta_q: i128,
    ) -> V16Result<PositionDeltaLookupV16> {
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut existing_slot = None;
        let mut empty_slot = None;
        let mut current_q = 0i128;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let in_bitmap = active_bitmap_get(bitmap, slot);
            let leg = account.header.legs[slot].try_to_runtime()?;
            if in_bitmap {
                if !leg.active {
                    return Err(V16Error::HiddenLeg);
                }
                if leg.asset_index as usize == asset_index {
                    if existing_slot.is_some() {
                        return Err(V16Error::HiddenLeg);
                    }
                    existing_slot = Some(slot);
                    current_q = signed_position(leg);
                }
            } else if leg.active || !leg.is_empty() {
                return Err(V16Error::HiddenLeg);
            } else if empty_slot.is_none() {
                empty_slot = Some(slot);
            }
            slot += 1;
        }
        let next_q = current_q
            .checked_add(delta_q)
            .ok_or(V16Error::ArithmeticOverflow)?;
        validate_basis_or_zero(next_q)?;
        Ok(PositionDeltaLookupV16 {
            existing_slot,
            empty_slot,
            current_q,
            next_q,
        })
    }

    fn attach_leg(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        side: SideV16,
        basis_pos_q: i128,
    ) -> V16Result<()> {
        if Self::active_leg_slot_for_asset(&account.as_view(), asset_index)?.is_some() {
            return Err(V16Error::InvalidLeg);
        }
        let leg_slot = Self::empty_leg_slot(&account.as_view())?;
        self.attach_leg_at_slot(account, asset_index, side, basis_pos_q, leg_slot)
    }

    fn attach_leg_at_slot(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        side: SideV16,
        basis_pos_q: i128,
        leg_slot: usize,
    ) -> V16Result<()> {
        if leg_slot >= V16_MAX_PORTFOLIO_ASSETS_N {
            return Err(V16Error::InvalidLeg);
        }
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let existing = account.header.legs[leg_slot].try_to_runtime()?;
        if active_bitmap_get(bitmap, leg_slot) || existing.active || !existing.is_empty() {
            return Err(V16Error::HiddenLeg);
        }
        if self.has_pending_domain_loss_barrier(asset_index, side)? {
            return Err(V16Error::LockActive);
        }
        validate_basis(basis_pos_q)?;
        let mut asset = self.asset_state(asset_index)?;
        self.require_asset_active_for_risk_increase(asset_index)?;
        let (a_basis, k_snap, f_snap, b_snap, epoch_snap) = match side {
            SideV16::Long => (
                asset.a_long,
                asset.k_long,
                asset.f_long_num,
                asset.b_long_num,
                asset.epoch_long,
            ),
            SideV16::Short => (
                asset.a_short,
                asset.k_short,
                asset.f_short_num,
                asset.b_short_num,
                asset.epoch_short,
            ),
        };
        if !(MIN_A_SIDE..=ADL_ONE).contains(&a_basis) {
            return Err(V16Error::InvalidLeg);
        }
        let loss_weight = loss_weight_for_basis(basis_pos_q.unsigned_abs(), a_basis)?;
        if loss_weight == 0 {
            return Err(V16Error::InvalidLeg);
        }
        add_open_interest_for_new_position(
            &mut asset,
            side,
            basis_pos_q.unsigned_abs(),
            loss_weight,
        )?;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&PortfolioLegV16 {
            active: true,
            asset_index: asset_index as u32,
            market_id: asset.market_id,
            side,
            basis_pos_q,
            a_basis,
            k_snap,
            f_snap,
            epoch_snap,
            loss_weight,
            b_snap,
            b_rem: 0,
            b_epoch_snap: epoch_snap,
            b_stale: false,
            stale: false,
        });
        let mut bitmap = account.header.active_bitmap.map(V16PodU64::get);
        active_bitmap_set(&mut bitmap, leg_slot)?;
        account.header.active_bitmap = bitmap.map(V16PodU64::new);
        account.header.health_cert.valid = 0;
        self.set_asset_state(asset_index, asset)?;
        Ok(())
    }

    fn clear_leg(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
    ) -> V16Result<()> {
        let leg_slot = Self::require_active_leg_slot_for_asset(&account.as_view(), asset_index)?;
        let leg = account.header.legs[leg_slot].try_to_runtime()?;
        if !leg.active || leg.b_stale || leg.stale {
            return Err(V16Error::InvalidLeg);
        }
        if account
            .header
            .close_progress
            .try_to_runtime()?
            .has_pending_residual()
        {
            return Err(V16Error::LockActive);
        }
        if self.has_pending_domain_loss_barrier(asset_index, leg.side)? {
            return Err(V16Error::LockActive);
        }
        let (k_target, f_target) = self.kf_target_for_leg(asset_index, leg)?;
        if k_target != leg.k_snap || f_target != leg.f_snap {
            return Err(V16Error::Stale);
        }
        if self.b_target_for_leg(asset_index, leg)? != leg.b_snap {
            return Err(V16Error::Stale);
        }
        let mut asset = self.asset_state(asset_index)?;
        let prior_reset_epoch = match leg.side {
            SideV16::Long => {
                asset.mode_long == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_long)
            }
            SideV16::Short => {
                asset.mode_short == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_short)
            }
        };
        let dust_after_clear = if !prior_reset_epoch && leg.b_rem != 0 {
            let current_dust = match leg.side {
                SideV16::Long => asset.social_loss_dust_long_num,
                SideV16::Short => asset.social_loss_dust_short_num,
            };
            let new_dust = current_dust
                .checked_add(leg.b_rem)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if new_dust >= SOCIAL_LOSS_DEN {
                return Err(V16Error::RecoveryRequired);
            }
            Some(new_dust)
        } else {
            None
        };
        match leg.side {
            SideV16::Long => {
                asset.stored_pos_count_long = asset
                    .stored_pos_count_long
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?;
                if leg.basis_pos_q == 0 && leg.loss_weight != 0 {
                    asset.pending_obligation_count_long = asset
                        .pending_obligation_count_long
                        .checked_sub(1)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
                if !prior_reset_epoch {
                    if let Some(new_dust) = dust_after_clear {
                        asset.social_loss_dust_long_num = new_dust;
                    }
                    asset.oi_eff_long_q = asset
                        .oi_eff_long_q
                        .checked_sub(leg.basis_pos_q.unsigned_abs())
                        .ok_or(V16Error::CounterUnderflow)?;
                    asset.loss_weight_sum_long = asset
                        .loss_weight_sum_long
                        .checked_sub(leg.loss_weight)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
            }
            SideV16::Short => {
                asset.stored_pos_count_short = asset
                    .stored_pos_count_short
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?;
                if leg.basis_pos_q == 0 && leg.loss_weight != 0 {
                    asset.pending_obligation_count_short = asset
                        .pending_obligation_count_short
                        .checked_sub(1)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
                if !prior_reset_epoch {
                    if let Some(new_dust) = dust_after_clear {
                        asset.social_loss_dust_short_num = new_dust;
                    }
                    asset.oi_eff_short_q = asset
                        .oi_eff_short_q
                        .checked_sub(leg.basis_pos_q.unsigned_abs())
                        .ok_or(V16Error::CounterUnderflow)?;
                    asset.loss_weight_sum_short = asset
                        .loss_weight_sum_short
                        .checked_sub(leg.loss_weight)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
            }
        }
        account.header.legs[leg_slot] =
            PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        let mut bitmap = account.header.active_bitmap.map(V16PodU64::get);
        active_bitmap_clear(&mut bitmap, leg_slot)?;
        account.header.active_bitmap = bitmap.map(V16PodU64::new);
        account.header.health_cert.valid = 0;
        self.set_asset_state(asset_index, asset)?;
        Ok(())
    }

    fn apply_position_delta(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        delta_q: i128,
    ) -> V16Result<()> {
        if delta_q == 0 {
            return Ok(());
        }
        let lookup =
            Self::position_delta_lookup_for_asset(&account.as_view(), asset_index, delta_q)?;
        self.apply_position_delta_with_lookup(account, asset_index, delta_q, lookup)
    }

    fn apply_position_delta_with_lookup(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        delta_q: i128,
        lookup: PositionDeltaLookupV16,
    ) -> V16Result<()> {
        self.apply_position_delta_with_lookup_inner(account, asset_index, delta_q, lookup, true)
    }

    fn apply_current_position_delta_with_lookup(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        delta_q: i128,
        lookup: PositionDeltaLookupV16,
    ) -> V16Result<()> {
        self.apply_position_delta_with_lookup_inner(account, asset_index, delta_q, lookup, false)
    }

    fn apply_position_delta_with_lookup_inner(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        delta_q: i128,
        lookup: PositionDeltaLookupV16,
        settle_existing: bool,
    ) -> V16Result<()> {
        if delta_q == 0 {
            return Ok(());
        }
        let expected_next = lookup
            .current_q
            .checked_add(delta_q)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if expected_next != lookup.next_q {
            return Err(V16Error::InvalidLeg);
        }
        validate_basis_or_zero(lookup.next_q)?;
        let existing_slot = lookup.existing_slot;
        if settle_existing {
            if let Some(existing_slot) = existing_slot {
                self.settle_leg_kf_effects_at_slot(account, existing_slot)?;
            }
        }
        let current_leg = if let Some(existing_slot) = existing_slot {
            let leg = account.header.legs[existing_slot].try_to_runtime()?;
            if !leg.active || leg.asset_index as usize != asset_index {
                return Err(V16Error::HiddenLeg);
            }
            leg
        } else {
            PortfolioLegV16::EMPTY
        };
        let current = signed_position(current_leg);
        if current != lookup.current_q {
            return Err(V16Error::HiddenLeg);
        }
        let new = lookup.next_q;
        if self.position_change_touches_pending_domain_loss_barrier(asset_index, current, new)?
            && !same_side_risk_reduction_or_flat_obligation(current, new)
        {
            return Err(V16Error::LockActive);
        }
        if current == 0 {
            let side = if new > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            let leg_slot = lookup.empty_slot.ok_or(V16Error::InvalidLeg)?;
            return self.attach_leg_at_slot(account, asset_index, side, new, leg_slot);
        }
        let leg_slot = existing_slot.ok_or(V16Error::InvalidLeg)?;
        if new == 0 {
            let leg = current_leg;
            if leg.active && self.has_pending_domain_loss_barrier(asset_index, leg.side)? {
                let old_abs = leg.basis_pos_q.unsigned_abs();
                let mut asset = self.asset_state(asset_index)?;
                match leg.side {
                    SideV16::Long => {
                        asset.oi_eff_long_q = asset
                            .oi_eff_long_q
                            .checked_sub(old_abs)
                            .ok_or(V16Error::CounterUnderflow)?;
                        asset.pending_obligation_count_long = asset
                            .pending_obligation_count_long
                            .checked_add(1)
                            .ok_or(V16Error::CounterOverflow)?;
                    }
                    SideV16::Short => {
                        asset.oi_eff_short_q = asset
                            .oi_eff_short_q
                            .checked_sub(old_abs)
                            .ok_or(V16Error::CounterUnderflow)?;
                        asset.pending_obligation_count_short = asset
                            .pending_obligation_count_short
                            .checked_add(1)
                            .ok_or(V16Error::CounterOverflow)?;
                    }
                }
                let mut zero_basis_leg = leg;
                zero_basis_leg.basis_pos_q = 0;
                account.header.legs[leg_slot] =
                    PortfolioLegV16Account::from_runtime(&zero_basis_leg);
                account.header.health_cert.valid = 0;
                self.set_asset_state(asset_index, asset)?;
                return Ok(());
            }
            return self.clear_leg(account, asset_index);
        }
        if current.signum() != new.signum() {
            self.require_asset_active_for_risk_increase(asset_index)?;
            self.clear_leg(account, asset_index)?;
            let side = if new > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            return self.attach_leg(account, asset_index, side, new);
        }
        if new.unsigned_abs() > current.unsigned_abs() {
            self.require_asset_active_for_risk_increase(asset_index)?;
        }
        let mut old_leg = account.header.legs[leg_slot].try_to_runtime()?;
        let old_abs = old_leg.basis_pos_q.unsigned_abs();
        let new_abs = new.unsigned_abs();
        let new_weight = loss_weight_for_basis(new_abs, old_leg.a_basis)?;
        let preserve_pending_obligation_weight =
            same_side_risk_reduction_or_flat_obligation(current, new)
                && self.has_pending_domain_loss_barrier(asset_index, old_leg.side)?;
        let mut asset = self.asset_state(asset_index)?;
        match old_leg.side {
            SideV16::Long => {
                asset.oi_eff_long_q = adjust_u128(asset.oi_eff_long_q, old_abs, new_abs)?;
                if !preserve_pending_obligation_weight {
                    asset.loss_weight_sum_long =
                        adjust_u128(asset.loss_weight_sum_long, old_leg.loss_weight, new_weight)?;
                }
            }
            SideV16::Short => {
                asset.oi_eff_short_q = adjust_u128(asset.oi_eff_short_q, old_abs, new_abs)?;
                if !preserve_pending_obligation_weight {
                    asset.loss_weight_sum_short =
                        adjust_u128(asset.loss_weight_sum_short, old_leg.loss_weight, new_weight)?;
                }
            }
        }
        old_leg.basis_pos_q = new;
        if !preserve_pending_obligation_weight {
            old_leg.loss_weight = new_weight;
        }
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&old_leg);
        account.header.health_cert.valid = 0;
        self.set_asset_state(asset_index, asset)?;
        Ok(())
    }

    fn set_pending_domain_loss_barrier_count(
        &mut self,
        asset_index: usize,
        side: SideV16,
        value: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.pending_domain_loss_barrier_long = V16PodU64::new(value),
            SideV16::Short => slot.pending_domain_loss_barrier_short = V16PodU64::new(value),
        }
        Ok(())
    }

    fn begin_close_progress_ledger(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        domain_side: SideV16,
        gross_loss: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if gross_loss == 0 {
            return Ok(());
        }
        if account.header.close_progress.try_to_runtime()?.active {
            return Err(V16Error::LockActive);
        }
        let domain = self.insurance_domain_index(asset_index, domain_side)?;
        if self.pending_domain_loss_barrier_count(asset_index, domain_side)? != 0 {
            return Err(V16Error::LockActive);
        }
        let current = account.header.close_progress.try_to_runtime()?;
        let close_id = current.close_id.saturating_add(1).max(1);
        let asset = self.asset_state(asset_index)?;
        let ledger = CloseProgressLedgerV16 {
            active: true,
            finalized: false,
            close_id,
            asset_index: u32::try_from(asset_index).map_err(|_| V16Error::InvalidLeg)?,
            market_id: asset.market_id,
            domain_side,
            gross_loss_at_close_start: gross_loss,
            drift_reference_slot: self.header.current_slot.get(),
            max_close_slot: self
                .header
                .current_slot
                .get()
                .checked_add(self.header.config.max_bankrupt_close_lifetime_slots.get())
                .ok_or(V16Error::ArithmeticOverflow)?,
            residual_remaining: gross_loss,
            ..CloseProgressLedgerV16::EMPTY
        };
        account.header.close_progress = CloseProgressLedgerV16Account::from_runtime(&ledger);
        let count = self.pending_domain_loss_barrier_count(asset_index, domain_side)?;
        self.set_pending_domain_loss_barrier_count(
            asset_index,
            domain_side,
            count.checked_add(1).ok_or(V16Error::CounterOverflow)?,
        )?;
        self.domain_asset_side(domain)?;
        self.validate_account_audit_scan(&account.as_view())
    }

    fn ensure_close_progress_not_expired(
        &mut self,
        ledger: CloseProgressLedgerV16,
    ) -> V16Result<()> {
        if ledger.active && self.header.current_slot.get() > ledger.max_close_slot {
            if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
                return Ok(());
            }
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        Ok(())
    }

    #[cfg(kani)]
    pub fn kani_ensure_close_progress_not_expired(
        &mut self,
        ledger: CloseProgressLedgerV16,
    ) -> V16Result<()> {
        self.ensure_close_progress_not_expired(ledger)
    }

    fn ensure_open_close_snapshot_current_or_recovery(
        &mut self,
        account: &PortfolioV16View<'_>,
        ledger: CloseProgressLedgerV16,
    ) -> V16Result<()> {
        if !ledger.active {
            return Ok(());
        }
        let asset_index = ledger.asset_index as usize;
        if asset_index < self.header.config.max_market_slots.get() as usize
            && Self::active_leg_slot_for_asset(account, asset_index)?.is_some()
            && self.header.current_slot.get() > ledger.drift_reference_slot
        {
            if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
                return Ok(());
            }
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        Ok(())
    }

    fn advance_close_progress_ledger(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        support_consumed: u128,
        junior_face_burned: u128,
        insurance_spent: u128,
        b_loss_booked: u128,
        explicit_loss_assigned: u128,
    ) -> V16Result<()> {
        if support_consumed == 0
            && junior_face_burned == 0
            && insurance_spent == 0
            && b_loss_booked == 0
            && explicit_loss_assigned == 0
        {
            return Ok(());
        }
        let mut ledger = account.header.close_progress.try_to_runtime()?;
        self.ensure_close_progress_not_expired(ledger)?;
        let was_pending = ledger.has_pending_residual();
        let domain_side = ledger.domain_side;
        let asset_index = ledger.asset_index as usize;
        if !ledger.active || ledger.finalized {
            return Err(V16Error::LockActive);
        }
        ledger.support_consumed = ledger
            .support_consumed
            .checked_add(support_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        ledger.junior_face_burned = ledger
            .junior_face_burned
            .checked_add(junior_face_burned)
            .ok_or(V16Error::ArithmeticOverflow)?;
        ledger.insurance_spent = ledger
            .insurance_spent
            .checked_add(insurance_spent)
            .ok_or(V16Error::ArithmeticOverflow)?;
        ledger.b_loss_booked = ledger
            .b_loss_booked
            .checked_add(b_loss_booked)
            .ok_or(V16Error::ArithmeticOverflow)?;
        ledger.explicit_loss_assigned = ledger
            .explicit_loss_assigned
            .checked_add(explicit_loss_assigned)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let total_loss = ledger
            .gross_loss_at_close_start
            .checked_add(ledger.drift_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let progress = ledger
            .support_consumed
            .checked_add(ledger.insurance_spent)
            .and_then(|v| v.checked_add(ledger.b_loss_booked))
            .and_then(|v| v.checked_add(ledger.explicit_loss_assigned))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if progress > total_loss {
            return Err(V16Error::ArithmeticOverflow);
        }
        ledger.residual_remaining = total_loss - progress;
        if ledger.residual_remaining == 0 {
            ledger.finalized = true;
        }
        if was_pending && !ledger.has_pending_residual() {
            let count = self.pending_domain_loss_barrier_count(asset_index, domain_side)?;
            self.set_pending_domain_loss_barrier_count(
                asset_index,
                domain_side,
                count.checked_sub(1).ok_or(V16Error::CounterUnderflow)?,
            )?;
        }
        account.header.close_progress = CloseProgressLedgerV16Account::from_runtime(&ledger);
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())
    }

    fn bankruptcy_residual_single_step_capacity(
        &self,
        asset_index: usize,
        bankrupt_side: SideV16,
        residual_remaining: u128,
    ) -> V16Result<u128> {
        self.validate_configured_asset_index(asset_index)?;
        if residual_remaining == 0 {
            return Ok(0);
        }
        let opp = opposite_side(bankrupt_side);
        let asset = self.asset_state(asset_index)?;
        let (b_now, weight_sum, rem) = match opp {
            SideV16::Long => (
                asset.b_long_num,
                asset.loss_weight_sum_long,
                asset.social_loss_remainder_long_num,
            ),
            SideV16::Short => (
                asset.b_short_num,
                asset.loss_weight_sum_short,
                asset.social_loss_remainder_short_num,
            ),
        };
        if weight_sum == 0 {
            return Ok(0);
        }
        let candidate = residual_remaining.min(self.header.config.public_b_chunk_atoms.get());
        if candidate != 0 {
            if let Some(delta_b) = candidate
                .checked_mul(SOCIAL_LOSS_DEN)
                .and_then(|v| v.checked_add(rem))
                .map(|v| v / weight_sum)
            {
                if delta_b != 0 && b_now.checked_add(delta_b).is_some() {
                    return Ok(candidate);
                }
            }
        }
        let headroom_plus_one = U256::from_u128(u128::MAX - b_now)
            .checked_add(U256::ONE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let max_scaled = headroom_plus_one
            .checked_mul(U256::from_u128(weight_sum))
            .and_then(|v| v.checked_sub(U256::ONE))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if U256::from_u128(rem) > max_scaled {
            return Ok(0);
        }
        let max_chunk_by_b_wide = max_scaled
            .checked_sub(U256::from_u128(rem))
            .and_then(|v| v.checked_div(U256::from_u128(SOCIAL_LOSS_DEN)))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let max_chunk_by_b = max_chunk_by_b_wide
            .try_into_u128()
            .unwrap_or(residual_remaining);
        Ok(residual_remaining
            .min(max_chunk_by_b)
            .min(self.header.config.public_b_chunk_atoms.get()))
    }

    #[cfg(kani)]
    pub fn kani_bankruptcy_residual_single_step_capacity(
        &self,
        asset_index: usize,
        bankrupt_side: SideV16,
        residual_remaining: u128,
    ) -> V16Result<u128> {
        self.bankruptcy_residual_single_step_capacity(
            asset_index,
            bankrupt_side,
            residual_remaining,
        )
    }

    fn book_bankruptcy_residual_chunk_internal(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        residual_remaining: u128,
    ) -> V16Result<BResidualBookingOutcomeV16> {
        self.validate_configured_asset_index(asset_index)?;
        if residual_remaining == 0 {
            return Ok(BResidualBookingOutcomeV16 {
                booked_loss: 0,
                explicit_loss: 0,
                delta_b: 0,
                remaining_after: 0,
            });
        }
        let opp = opposite_side(bankrupt_side);
        let asset = self.asset_state(asset_index)?;
        let weight_sum = match opp {
            SideV16::Long => asset.loss_weight_sum_long,
            SideV16::Short => asset.loss_weight_sum_short,
        };
        if weight_sum == 0 {
            if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
                self.header.bankruptcy_hlock_active = 1;
                return Ok(BResidualBookingOutcomeV16 {
                    booked_loss: 0,
                    explicit_loss: residual_remaining,
                    delta_b: 0,
                    remaining_after: 0,
                });
            }
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        let engine_chunk = self.bankruptcy_residual_single_step_capacity(
            asset_index,
            bankrupt_side,
            residual_remaining,
        )?;
        if engine_chunk == 0 {
            if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
                self.header.bankruptcy_hlock_active = 1;
                return Ok(BResidualBookingOutcomeV16 {
                    booked_loss: 0,
                    explicit_loss: residual_remaining,
                    delta_b: 0,
                    remaining_after: 0,
                });
            }
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        let mut asset = asset;
        if let Some(outcome) = Self::apply_bankruptcy_residual_chunk_to_loss_side(
            &mut asset,
            opp,
            engine_chunk,
            residual_remaining,
        )? {
            self.set_asset_state(asset_index, asset)?;
            self.header.bankruptcy_hlock_active = 1;
            return Ok(outcome);
        }
        if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
            self.header.bankruptcy_hlock_active = 1;
            return Ok(BResidualBookingOutcomeV16 {
                booked_loss: 0,
                explicit_loss: residual_remaining,
                delta_b: 0,
                remaining_after: 0,
            });
        }
        self.declare_permissionless_recovery(
            PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
        )?;
        Err(V16Error::RecoveryRequired)
    }

    fn apply_bankruptcy_residual_chunk_to_loss_side(
        asset: &mut AssetStateV16,
        opp: SideV16,
        engine_chunk: u128,
        residual_remaining: u128,
    ) -> V16Result<Option<BResidualBookingOutcomeV16>> {
        if engine_chunk == 0 || engine_chunk > residual_remaining {
            return Ok(None);
        }
        let (b_now, weight_sum, rem) = match opp {
            SideV16::Long => (
                asset.b_long_num,
                asset.loss_weight_sum_long,
                asset.social_loss_remainder_long_num,
            ),
            SideV16::Short => (
                asset.b_short_num,
                asset.loss_weight_sum_short,
                asset.social_loss_remainder_short_num,
            ),
        };
        if weight_sum == 0 {
            return Ok(None);
        }
        let numerator = engine_chunk
            .checked_mul(SOCIAL_LOSS_DEN)
            .and_then(|v| v.checked_add(rem))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let delta_b = numerator / weight_sum;
        let new_rem = numerator % weight_sum;
        if delta_b == 0 || b_now.checked_add(delta_b).is_none() {
            return Ok(None);
        }
        match opp {
            SideV16::Long => {
                asset.b_long_num = asset
                    .b_long_num
                    .checked_add(delta_b)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                asset.social_loss_remainder_long_num = new_rem;
            }
            SideV16::Short => {
                asset.b_short_num = asset
                    .b_short_num
                    .checked_add(delta_b)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                asset.social_loss_remainder_short_num = new_rem;
            }
        }
        Ok(Some(BResidualBookingOutcomeV16 {
            booked_loss: engine_chunk,
            explicit_loss: 0,
            delta_b,
            remaining_after: residual_remaining - engine_chunk,
        }))
    }

    #[cfg(kani)]
    pub fn kani_book_bankruptcy_residual_chunk_internal(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        residual_remaining: u128,
    ) -> V16Result<BResidualBookingOutcomeV16> {
        self.book_bankruptcy_residual_chunk_internal(asset_index, bankrupt_side, residual_remaining)
    }

    #[cfg(kani)]
    pub fn kani_apply_bankruptcy_residual_chunk_to_loss_side(
        asset: &mut AssetStateV16,
        opp: SideV16,
        engine_chunk: u128,
        residual_remaining: u128,
    ) -> V16Result<Option<BResidualBookingOutcomeV16>> {
        Self::apply_bankruptcy_residual_chunk_to_loss_side(
            asset,
            opp,
            engine_chunk,
            residual_remaining,
        )
    }

    fn book_bankruptcy_residual_chunk_for_account_core(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        bankrupt_side: SideV16,
        residual_remaining: u128,
    ) -> V16Result<BResidualBookingOutcomeV16> {
        if residual_remaining == 0 {
            return Ok(BResidualBookingOutcomeV16 {
                booked_loss: 0,
                explicit_loss: 0,
                delta_b: 0,
                remaining_after: 0,
            });
        }
        let domain_side = opposite_side(bankrupt_side);
        if !account.header.close_progress.try_to_runtime()?.active {
            if self.bankruptcy_residual_single_step_capacity(
                asset_index,
                bankrupt_side,
                residual_remaining,
            )? == 0
            {
                self.declare_permissionless_recovery(
                    PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
                )?;
                return Err(V16Error::RecoveryRequired);
            }
            self.begin_close_progress_ledger(
                account,
                asset_index,
                domain_side,
                residual_remaining,
            )?;
        }
        let ledger = account.header.close_progress.try_to_runtime()?;
        self.ensure_close_progress_not_expired(ledger)?;
        if ledger.asset_index as usize != asset_index || ledger.domain_side != domain_side {
            return Err(V16Error::LockActive);
        }
        self.ensure_open_close_snapshot_current_or_recovery(&account.as_view(), ledger)?;
        let outcome = self.book_bankruptcy_residual_chunk_internal(
            asset_index,
            bankrupt_side,
            ledger.residual_remaining,
        )?;
        self.advance_close_progress_ledger(
            account,
            0,
            0,
            0,
            outcome.booked_loss,
            outcome.explicit_loss,
        )?;
        Ok(outcome)
    }

    pub fn apply_quantity_adl_after_residual_for_account_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        bankrupt_side: SideV16,
        close_q: u128,
    ) -> V16Result<QuantityAdlOutcomeV16> {
        account.validate_with_market(&self.as_view())?;
        self.validate_configured_asset_index(asset_index)?;
        let ledger = account.header.close_progress.try_to_runtime()?;
        let leg = Self::active_leg_for_asset(&account.as_view(), asset_index)?;
        if !ledger.active
            || !ledger.finalized
            || ledger.residual_remaining != 0
            || ledger.asset_index as usize != asset_index
            || ledger.domain_side != opposite_side(bankrupt_side)
        {
            return Err(V16Error::LockActive);
        }
        if !leg.active
            || leg.stale
            || leg.b_stale
            || leg.side != bankrupt_side
            || close_q != leg.basis_pos_q.unsigned_abs()
        {
            return Err(V16Error::InvalidLeg);
        }
        self.ensure_close_progress_not_expired(ledger)?;
        self.ensure_open_close_snapshot_current_or_recovery(&account.as_view(), ledger)?;
        let out =
            self.apply_quantity_adl_after_residual_internal(asset_index, bankrupt_side, close_q)?;
        self.advance_close_progress_quantity_adl(account, out.closed_q)?;
        self.clear_leg_after_quantity_adl(account, asset_index, leg)?;
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())?;
        Ok(out)
    }

    fn advance_close_progress_quantity_adl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        quantity_adl_applied_q: u128,
    ) -> V16Result<()> {
        if quantity_adl_applied_q == 0 {
            return Err(V16Error::NonProgress);
        }
        let mut ledger = account.header.close_progress.try_to_runtime()?;
        self.ensure_close_progress_not_expired(ledger)?;
        if !ledger.active || !ledger.finalized || ledger.residual_remaining != 0 {
            return Err(V16Error::LockActive);
        }
        if ledger.quantity_adl_applied_q != 0 {
            return Err(V16Error::LockActive);
        }
        ledger.quantity_adl_applied_q = quantity_adl_applied_q;
        account.header.close_progress = CloseProgressLedgerV16Account::from_runtime(&ledger);
        account.header.health_cert.valid = 0;
        Ok(())
    }

    fn clear_leg_after_quantity_adl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        leg: PortfolioLegV16,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let leg_slot = Self::require_active_leg_slot_for_asset(&account.as_view(), asset_index)?;
        if !leg.active
            || leg.stale
            || leg.b_stale
            || account.header.legs[leg_slot].try_to_runtime()? != leg
        {
            return Err(V16Error::InvalidLeg);
        }

        let mut asset = self.asset_state(asset_index)?;
        let prior_reset_epoch = match leg.side {
            SideV16::Long => {
                asset.mode_long == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_long)
            }
            SideV16::Short => {
                asset.mode_short == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_short)
            }
        };
        match leg.side {
            SideV16::Long => {
                asset.stored_pos_count_long = asset
                    .stored_pos_count_long
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?;
                if !prior_reset_epoch {
                    asset.loss_weight_sum_long = asset
                        .loss_weight_sum_long
                        .checked_sub(leg.loss_weight)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
            }
            SideV16::Short => {
                asset.stored_pos_count_short = asset
                    .stored_pos_count_short
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?;
                if !prior_reset_epoch {
                    asset.loss_weight_sum_short = asset
                        .loss_weight_sum_short
                        .checked_sub(leg.loss_weight)
                        .ok_or(V16Error::CounterUnderflow)?;
                }
            }
        }
        self.set_asset_state(asset_index, asset)?;
        account.header.legs[leg_slot] =
            PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        let mut bitmap = account.header.active_bitmap.map(V16PodU64::get);
        active_bitmap_clear(&mut bitmap, leg_slot)?;
        account.header.active_bitmap = bitmap.map(V16PodU64::new);
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())
    }

    fn apply_quantity_adl_after_residual_internal(
        &mut self,
        asset_index: usize,
        bankrupt_side: SideV16,
        close_q: u128,
    ) -> V16Result<QuantityAdlOutcomeV16> {
        self.validate_configured_asset_index(asset_index)?;
        if close_q == 0 {
            return Err(V16Error::InvalidLeg);
        }
        let opp = opposite_side(bankrupt_side);
        let asset = self.asset_state(asset_index)?;
        let (liq_oi_before, opp_oi_before, opp_a_before) = match (bankrupt_side, opp) {
            (SideV16::Long, SideV16::Short) => {
                (asset.oi_eff_long_q, asset.oi_eff_short_q, asset.a_short)
            }
            (SideV16::Short, SideV16::Long) => {
                (asset.oi_eff_short_q, asset.oi_eff_long_q, asset.a_long)
            }
            _ => unreachable!(),
        };
        if close_q > liq_oi_before || close_q > opp_oi_before {
            return Err(V16Error::InvalidLeg);
        }
        let liq_oi_after = liq_oi_before - close_q;
        let opp_oi_after = opp_oi_before - close_q;
        let mut reset_started = false;
        let mut opposite_a_after = if opp_oi_after == 0 {
            ADL_ONE
        } else {
            wide_mul_div_floor_u128(opp_a_before, opp_oi_after, opp_oi_before)
        };

        let force_full_reset = opp_oi_after != 0 && opposite_a_after == 0;
        let final_liq_oi_after = if force_full_reset { 0 } else { liq_oi_after };
        let final_opp_oi_after = if force_full_reset { 0 } else { opp_oi_after };
        if force_full_reset {
            opposite_a_after = ADL_ONE;
        }

        let mut asset = asset;
        match bankrupt_side {
            SideV16::Long => asset.oi_eff_long_q = final_liq_oi_after,
            SideV16::Short => asset.oi_eff_short_q = final_liq_oi_after,
        }
        match opp {
            SideV16::Long => {
                asset.oi_eff_long_q = final_opp_oi_after;
                asset.a_long =
                    opposite_a_after.max(if final_opp_oi_after == 0 { ADL_ONE } else { 1 });
                if final_opp_oi_after != 0 && asset.a_long < MIN_A_SIDE {
                    asset.mode_long = SideModeV16::DrainOnly;
                }
            }
            SideV16::Short => {
                asset.oi_eff_short_q = final_opp_oi_after;
                asset.a_short =
                    opposite_a_after.max(if final_opp_oi_after == 0 { ADL_ONE } else { 1 });
                if final_opp_oi_after != 0 && asset.a_short < MIN_A_SIDE {
                    asset.mode_short = SideModeV16::DrainOnly;
                }
            }
        }
        self.set_asset_state(asset_index, asset)?;

        if final_liq_oi_after == 0 {
            self.begin_full_drain_reset_inner(asset_index, bankrupt_side)?;
            reset_started = true;
        }
        if final_opp_oi_after == 0 {
            self.begin_full_drain_reset_inner(asset_index, opp)?;
            reset_started = true;
        }
        Ok(QuantityAdlOutcomeV16 {
            closed_q: close_q,
            opposite_a_after,
            reset_started,
        })
    }

    fn begin_full_drain_reset_inner(&mut self, asset_index: usize, side: SideV16) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if self.has_pending_domain_loss_barrier(asset_index, side)? {
            return Err(V16Error::LockActive);
        }
        let mut asset = self.asset_state(asset_index)?;
        match side {
            SideV16::Long => {
                if asset.mode_long == SideModeV16::ResetPending {
                    return Err(V16Error::LockActive);
                }
                if asset.oi_eff_long_q != 0 || asset.pending_obligation_count_long != 0 {
                    return Err(V16Error::LockActive);
                }
                quarantine_remainder(
                    &mut asset.social_loss_remainder_long_num,
                    &mut asset.social_loss_dust_long_num,
                )?;
                asset.k_epoch_start_long = asset.k_long;
                asset.f_epoch_start_long_num = asset.f_long_num;
                asset.b_epoch_start_long_num = asset.b_long_num;
                asset.k_long = 0;
                asset.f_long_num = 0;
                asset.b_long_num = 0;
                asset.loss_weight_sum_long = 0;
                asset.a_long = ADL_ONE;
                asset.epoch_long = asset
                    .epoch_long
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?;
                asset.mode_long = SideModeV16::ResetPending;
            }
            SideV16::Short => {
                if asset.mode_short == SideModeV16::ResetPending {
                    return Err(V16Error::LockActive);
                }
                if asset.oi_eff_short_q != 0 || asset.pending_obligation_count_short != 0 {
                    return Err(V16Error::LockActive);
                }
                quarantine_remainder(
                    &mut asset.social_loss_remainder_short_num,
                    &mut asset.social_loss_dust_short_num,
                )?;
                asset.k_epoch_start_short = asset.k_short;
                asset.f_epoch_start_short_num = asset.f_short_num;
                asset.b_epoch_start_short_num = asset.b_short_num;
                asset.k_short = 0;
                asset.f_short_num = 0;
                asset.b_short_num = 0;
                asset.loss_weight_sum_short = 0;
                asset.a_short = ADL_ONE;
                asset.epoch_short = asset
                    .epoch_short
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?;
                asset.mode_short = SideModeV16::ResetPending;
            }
        }
        self.set_asset_state(asset_index, asset)?;
        self.header.risk_epoch = V16PodU64::new(
            self.header
                .risk_epoch
                .get()
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
        );
        Ok(())
    }

    fn reduce_matching_open_interest_for_unilateral_close(
        &mut self,
        asset_index: usize,
        closed_side: SideV16,
        close_q: u128,
    ) -> V16Result<()> {
        if close_q == 0 {
            return Ok(());
        }
        let opp = opposite_side(closed_side);
        let mut asset = self.asset_state(asset_index)?;
        let (opp_oi_before, opp_a_before) = match opp {
            SideV16::Long => (asset.oi_eff_long_q, asset.a_long),
            SideV16::Short => (asset.oi_eff_short_q, asset.a_short),
        };
        if close_q > opp_oi_before {
            return Err(V16Error::InvalidLeg);
        }
        let opp_oi_after = opp_oi_before - close_q;
        let opp_a_after = if opp_oi_after == 0 {
            ADL_ONE
        } else {
            let candidate = wide_mul_div_floor_u128(opp_a_before, opp_oi_after, opp_oi_before);
            if candidate == 0 {
                self.declare_permissionless_recovery(
                    PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
                )?;
                return Err(V16Error::RecoveryRequired);
            }
            candidate
        };
        match opp {
            SideV16::Long => {
                asset.oi_eff_long_q = opp_oi_after;
                asset.a_long = opp_a_after;
                if opp_oi_after != 0 && asset.a_long < MIN_A_SIDE {
                    asset.mode_long = SideModeV16::DrainOnly;
                }
            }
            SideV16::Short => {
                asset.oi_eff_short_q = opp_oi_after;
                asset.a_short = opp_a_after;
                if opp_oi_after != 0 && asset.a_short < MIN_A_SIDE {
                    asset.mode_short = SideModeV16::DrainOnly;
                }
            }
        }
        self.set_asset_state(asset_index, asset)?;
        if opp_oi_after == 0 {
            self.begin_full_drain_reset_inner(asset_index, opp)?;
        }
        Ok(())
    }

    fn reduce_position(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        close_q: u128,
    ) -> V16Result<()> {
        if close_q == 0 {
            return Ok(());
        }
        let leg = Self::active_leg_for_asset(&account.as_view(), asset_index)?;
        if !leg.active {
            return Err(V16Error::InvalidLeg);
        }
        let close_i128 = i128::try_from(close_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let delta = match leg.side {
            SideV16::Long => close_i128
                .checked_neg()
                .ok_or(V16Error::ArithmeticOverflow)?,
            SideV16::Short => close_i128,
        };
        self.apply_position_delta(account, asset_index, delta)?;
        self.reduce_matching_open_interest_for_unilateral_close(asset_index, leg.side, close_q)
    }

    pub fn liquidate_account_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        request: LiquidationRequestV16,
    ) -> V16Result<LiquidationOutcomeV16> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        let config = self.header.config.try_to_runtime_shape()?;
        if request.asset_index >= config.max_market_slots as usize
            || request.close_q == 0
            || request.fee_bps > config.liquidation_fee_bps.max(config.max_trading_fee_bps)
        {
            return Err(V16Error::InvalidConfig);
        }
        self.require_asset_live_reducible(request.asset_index)?;
        self.validate_account_scalar_preflight(&account.as_view())?;
        Self::require_active_leg_slot_for_asset(&account.as_view(), request.asset_index)?;
        match self.refresh_account_and_certify_not_atomic(
            account,
            None,
            self.header.config.public_b_chunk_atoms.get(),
            false,
        )? {
            AccountRefreshCertOutcomeV16::Certified(_) => {}
            AccountRefreshCertOutcomeV16::BChunk(_) => return Err(V16Error::BStale),
        }
        let cert = account.header.health_cert.try_to_runtime()?;
        if cert.certified_liq_deficit == 0 {
            return Err(V16Error::NonProgress);
        }
        let before_score = self.risk_score_unchecked(&account.as_view())?;
        let leg_slot =
            Self::require_active_leg_slot_for_asset(&account.as_view(), request.asset_index)?;
        let leg = account.header.legs[leg_slot].try_to_runtime()?;
        if !leg.active {
            return Err(V16Error::InvalidLeg);
        }
        let close_q = request.close_q.min(leg.basis_pos_q.unsigned_abs());
        let close_i128 = i128::try_from(close_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let close_delta = match leg.side {
            SideV16::Long => close_i128
                .checked_neg()
                .ok_or(V16Error::ArithmeticOverflow)?,
            SideV16::Short => close_i128,
        };
        if self.position_delta_touches_pending_domain_loss_barrier(
            &account.as_view(),
            request.asset_index,
            close_delta,
        )? {
            return Err(V16Error::LockActive);
        }
        if liquidation_close_would_leave_uncovered_loss_with_open_risk(
            account.header.pnl.get(),
            account.header.capital.get(),
            account.header.active_bitmap.map(V16PodU64::get),
            leg_slot,
            close_q,
            leg.basis_pos_q.unsigned_abs(),
        )? {
            self.declare_permissionless_recovery(
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )?;
            return Err(V16Error::RecoveryRequired);
        }
        self.preflight_liquidation_residual_durability(
            request.asset_index,
            leg.side,
            &account.as_view(),
        )?;
        let fee_notional = risk_notional_ceil(
            close_q,
            self.asset_state(request.asset_index)?.effective_price,
        )?;
        let fee = checked_fee_bps(fee_notional, request.fee_bps)?
            .max(config.min_liquidation_abs)
            .min(config.liquidation_fee_cap);
        let charged_fee = self.charge_account_fee_not_atomic(account, fee)?;
        self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        let gross_bankruptcy_residual = if account.header.pnl.get() < 0 {
            account.header.pnl.get().unsigned_abs()
        } else {
            0
        };
        if gross_bankruptcy_residual != 0 {
            self.begin_close_progress_ledger(
                account,
                request.asset_index,
                opposite_side(leg.side),
                gross_bankruptcy_residual,
            )?;
        }
        let insurance_used =
            self.consume_domain_insurance_for_negative_pnl(request.asset_index, leg.side, account)?;
        if insurance_used != 0 {
            self.advance_close_progress_ledger(account, 0, 0, insurance_used, 0, 0)?;
        }
        let residual = if account.header.pnl.get() < 0 {
            account.header.pnl.get().unsigned_abs()
        } else {
            0
        };
        let mut booked = 0u128;
        let mut explicit = 0u128;
        if residual != 0 {
            let outcome = self.book_bankruptcy_residual_chunk_for_account_core(
                account,
                request.asset_index,
                leg.side,
                residual,
            )?;
            booked = outcome.booked_loss;
            explicit = outcome.explicit_loss;
            let cleared = booked
                .checked_add(explicit)
                .ok_or(V16Error::ArithmeticOverflow)?
                .min(residual);
            let cleared_i128 = i128::try_from(cleared).map_err(|_| V16Error::ArithmeticOverflow)?;
            let new_pnl = account
                .header
                .pnl
                .get()
                .checked_add(cleared_i128)
                .ok_or(V16Error::ArithmeticOverflow)?;
            self.set_account_pnl(account, new_pnl)?;
            self.header.bankruptcy_hlock_active = 1;
        }
        self.reduce_position(account, request.asset_index, close_q)?;
        self.certify_account_after_local_settlement_with_price_override(account, None)?;
        self.validate_liquidation_progress_from_score(before_score, &account.as_view())?;
        self.validate_shape_audit_scan()?;
        self.validate_account_audit_scan(&account.as_view())?;
        Ok(LiquidationOutcomeV16 {
            closed_q: close_q,
            insurance_used,
            residual_booked: booked,
            explicit_loss: explicit,
            fee_charged: charged_fee,
        })
    }

    pub fn rebalance_reduce_position_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        request: RebalanceRequestV16,
    ) -> V16Result<RebalanceOutcomeV16> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        if request.asset_index >= self.header.config.max_market_slots.get() as usize
            || request.reduce_q == 0
        {
            return Err(V16Error::InvalidConfig);
        }
        self.require_asset_live_reducible(request.asset_index)?;
        self.settle_account_side_effects_not_atomic(
            account,
            self.header.config.public_b_chunk_atoms.get(),
        )?;
        self.certify_account_after_local_settlement_with_price_override(account, None)?;
        let before_score = self.risk_score_unchecked(&account.as_view())?;
        let leg = Self::active_leg_for_asset(&account.as_view(), request.asset_index)?;
        if !leg.active {
            return Err(V16Error::InvalidLeg);
        }
        let reduce_q = request.reduce_q.min(leg.basis_pos_q.unsigned_abs());
        if reduce_q == 0 {
            return Err(V16Error::NonProgress);
        }
        let reduce_i128 = i128::try_from(reduce_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let reduce_delta = match leg.side {
            SideV16::Long => reduce_i128
                .checked_neg()
                .ok_or(V16Error::ArithmeticOverflow)?,
            SideV16::Short => reduce_i128,
        };
        if self.position_delta_blocked_by_pending_domain_loss_barrier(
            &account.as_view(),
            request.asset_index,
            reduce_delta,
        )? {
            return Err(V16Error::LockActive);
        }
        self.reduce_position(account, request.asset_index, reduce_q)?;
        self.settle_negative_pnl_from_principal_not_atomic(account)?;
        self.certify_account_after_local_settlement_with_price_override(account, None)?;
        self.validate_liquidation_progress_from_score(before_score, &account.as_view())?;
        self.validate_shape_audit_scan()?;
        self.validate_account_audit_scan(&account.as_view())?;
        Ok(RebalanceOutcomeV16 {
            reduced_q: reduce_q,
        })
    }

    fn settle_account_for_position_action_and_refresh_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<HealthCertV16> {
        self.validate_account_scalar_preflight(&account.as_view())?;
        if !decode_bool(account.header.stale_state)?
            && !decode_bool(account.header.b_stale_state)?
            && !Self::has_b_stale_leg(&account.as_view())?
            && self
                .ensure_favorable_action_current_certificate(&account.as_view())
                .is_ok()
        {
            return account.header.health_cert.try_to_runtime();
        }
        match self.refresh_account_and_certify_not_atomic(account, None, 0, false)? {
            AccountRefreshCertOutcomeV16::Certified(cert) => Ok(cert),
            AccountRefreshCertOutcomeV16::BChunk(_) => Err(V16Error::BStale),
        }
    }

    fn ensure_initial_margin(account: &PortfolioV16View<'_>) -> V16Result<()> {
        let cert = account.header.health_cert.try_to_runtime()?;
        if !cert.valid {
            return Err(V16Error::Stale);
        }
        let equity = cert.certified_equity;
        if equity < 0 || (equity as u128) < cert.certified_initial_req {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    fn account_no_positive_credit_equity(account: &PortfolioV16View<'_>) -> V16Result<i128> {
        validate_non_min_i128(account.header.pnl.get())?;
        validate_fee_credits(account.header.fee_credits.get())?;
        let capital = i128::try_from(account.header.capital.get())
            .map_err(|_| V16Error::ArithmeticOverflow)?;
        let fee_debt = i128::try_from(account.header.fee_credits.get().unsigned_abs())
            .map_err(|_| V16Error::ArithmeticOverflow)?;
        capital
            .checked_add(account.header.pnl.get().min(0))
            .and_then(|v| v.checked_sub(fee_debt))
            .ok_or(V16Error::ArithmeticOverflow)
    }

    fn ensure_no_positive_credit_initial_margin(account: &PortfolioV16View<'_>) -> V16Result<()> {
        let equity = Self::account_no_positive_credit_equity(account)?;
        let cert = account.header.health_cert.try_to_runtime()?;
        if equity < 0 || (equity as u128) < cert.certified_initial_req {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    fn recertify_account_after_source_lien_change(
        &self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<HealthCertV16> {
        let existing = account.header.health_cert.try_to_runtime()?;
        if existing.active_bitmap_at_cert != account.header.active_bitmap.map(V16PodU64::get) {
            return Err(V16Error::Stale);
        }
        let equity = self.account_haircut_equity(&account.as_view())?;
        let certified_liq_deficit = if equity < 0 {
            equity.unsigned_abs()
        } else {
            existing
                .certified_maintenance_req
                .saturating_sub(equity as u128)
        };
        let cert = HealthCertV16 {
            certified_equity: equity,
            certified_initial_req: existing.certified_initial_req,
            certified_maintenance_req: existing.certified_maintenance_req,
            certified_liq_deficit,
            certified_worst_case_loss: existing.certified_worst_case_loss,
            cert_oracle_epoch: self.header.oracle_epoch.get(),
            cert_funding_epoch: self.header.funding_epoch.get(),
            cert_risk_epoch: self.header.risk_epoch.get(),
            cert_asset_set_epoch: self.header.asset_set_epoch.get(),
            active_bitmap_at_cert: account.header.active_bitmap.map(V16PodU64::get),
            valid: true,
        };
        account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
        Ok(cert)
    }

    fn recertify_account_after_trade_delta(
        &self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        old_abs_q: u128,
        new_abs_q: u128,
        price: u64,
    ) -> V16Result<HealthCertV16> {
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || price == 0
            || price > MAX_ORACLE_PRICE
        {
            return Err(V16Error::InvalidConfig);
        }
        let asset = self.asset_state(asset_index)?;
        if asset.raw_oracle_target_price != asset.effective_price {
            let cert = self.compute_account_health_cert_with_price_override(
                &account.as_view(),
                false,
                None,
            )?;
            account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
            return Ok(cert);
        }
        let existing = account.header.health_cert.try_to_runtime()?;
        let old_notional = risk_notional_ceil(old_abs_q, price)?;
        let new_notional = risk_notional_ceil(new_abs_q, price)?;
        let config = self.header.config.try_to_runtime_shape()?;
        let old_initial = margin_requirement(
            old_notional,
            config.initial_margin_bps,
            config.min_nonzero_im_req,
        )?;
        let old_maintenance = margin_requirement(
            old_notional,
            config.maintenance_margin_bps,
            config.min_nonzero_mm_req,
        )?;
        let new_initial = margin_requirement(
            new_notional,
            config.initial_margin_bps,
            config.min_nonzero_im_req,
        )?;
        let new_maintenance = margin_requirement(
            new_notional,
            config.maintenance_margin_bps,
            config.min_nonzero_mm_req,
        )?;
        let initial_req = existing
            .certified_initial_req
            .checked_sub(old_initial)
            .and_then(|v| v.checked_add(new_initial))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let maintenance_req = existing
            .certified_maintenance_req
            .checked_sub(old_maintenance)
            .and_then(|v| v.checked_add(new_maintenance))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let worst_case_loss = existing
            .certified_worst_case_loss
            .checked_sub(old_notional)
            .and_then(|v| v.checked_add(new_notional))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let equity = self.account_haircut_equity(&account.as_view())?;
        let certified_liq_deficit = if equity < 0 {
            equity.unsigned_abs()
        } else {
            maintenance_req.saturating_sub(equity as u128)
        };
        let cert = HealthCertV16 {
            certified_equity: equity,
            certified_initial_req: initial_req,
            certified_maintenance_req: maintenance_req,
            certified_liq_deficit,
            certified_worst_case_loss: worst_case_loss,
            cert_oracle_epoch: self.header.oracle_epoch.get(),
            cert_funding_epoch: self.header.funding_epoch.get(),
            cert_risk_epoch: self.header.risk_epoch.get(),
            cert_asset_set_epoch: self.header.asset_set_epoch.get(),
            active_bitmap_at_cert: account.header.active_bitmap.map(V16PodU64::get),
            valid: true,
        };
        account.header.health_cert = HealthCertV16Account::from_runtime(&cert);
        Ok(cert)
    }

    pub fn execute_trade_with_fee_in_place_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        request: TradeRequestV16,
    ) -> V16Result<TradeOutcomeV16> {
        self.validate_trade_request(request)?;
        self.validate_unconfigured_market_tail()?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        self.settle_account_for_position_action_and_refresh_not_atomic(long_account)?;
        self.settle_account_for_position_action_and_refresh_not_atomic(short_account)?;

        let long_delta =
            i128::try_from(request.size_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let short_delta = long_delta
            .checked_neg()
            .ok_or(V16Error::ArithmeticOverflow)?;
        let locked = self.h_lock_lane(Some(&long_account.as_view()), false)? == HLockLaneV16::HMax
            || self.h_lock_lane(Some(&short_account.as_view()), false)? == HLockLaneV16::HMax;
        let trade_preflight = self.validate_trade_position_preflight(
            &long_account.as_view(),
            &short_account.as_view(),
            request,
        )?;
        let risk_increasing = trade_preflight.risk_increasing;
        if risk_increasing {
            self.require_asset_active_for_risk_increase(request.asset_index)?;
        }
        let notional = trade_notional_floor(request.size_q, request.exec_price)?;
        let fee = checked_fee_bps(notional, request.fee_bps)?;
        let price = self.asset_state(request.asset_index)?.effective_price;
        let fee_a = self.charge_account_fee_current_not_atomic(long_account, fee)?;
        let fee_b = self.charge_account_fee_current_not_atomic(short_account, fee)?;
        self.apply_current_position_delta_with_lookup(
            long_account,
            request.asset_index,
            long_delta,
            trade_preflight.long_lookup,
        )?;
        self.apply_current_position_delta_with_lookup(
            short_account,
            request.asset_index,
            short_delta,
            trade_preflight.short_lookup,
        )?;
        self.recertify_account_after_trade_delta(
            long_account,
            request.asset_index,
            trade_preflight.long_old_abs_q,
            trade_preflight.long_new_abs_q,
            price,
        )?;
        self.recertify_account_after_trade_delta(
            short_account,
            request.asset_index,
            trade_preflight.short_old_abs_q,
            trade_preflight.short_new_abs_q,
            price,
        )?;

        if risk_increasing && !locked {
            if trade_preflight.long_has_source_claims {
                self.create_initial_margin_source_lien_if_needed(long_account)?;
            }
            if trade_preflight.short_has_source_claims {
                self.create_initial_margin_source_lien_if_needed(short_account)?;
            }
        }
        Self::ensure_initial_margin(&long_account.as_view())?;
        Self::ensure_initial_margin(&short_account.as_view())?;
        if locked {
            Self::ensure_no_positive_credit_initial_margin(&long_account.as_view())?;
            Self::ensure_no_positive_credit_initial_margin(&short_account.as_view())?;
        }
        self.validate_shape_audit_scan()?;
        self.validate_account_audit_scan(&long_account.as_view())?;
        self.validate_account_audit_scan(&short_account.as_view())?;
        Ok(TradeOutcomeV16 {
            fee_a,
            fee_b,
            notional,
        })
    }

    fn set_account_pnl_after_principal_settlement(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        new_pnl: i128,
    ) -> V16Result<()> {
        validate_non_min_i128(new_pnl)?;
        let old_pnl = account.header.pnl.get();
        if old_pnl >= 0 || new_pnl > 0 || new_pnl < old_pnl {
            return Err(V16Error::InvalidConfig);
        }
        if old_pnl < 0 && new_pnl == 0 {
            self.header.negative_pnl_account_count = V16PodU64::new(
                self.header
                    .negative_pnl_account_count
                    .get()
                    .checked_sub(1)
                    .ok_or(V16Error::CounterUnderflow)?,
            );
        }
        account.header.pnl = V16PodI128::new(new_pnl);
        Ok(())
    }

    fn resolved_bankruptcy_attribution(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<Option<(usize, SideV16)>> {
        let ledger = account.header.close_progress.try_to_runtime()?;
        if ledger.active && !ledger.canceled && !ledger.finalized && ledger.residual_remaining != 0
        {
            let asset_index = ledger.asset_index as usize;
            self.validate_configured_asset_index(asset_index)?;
            return Ok(Some((asset_index, opposite_side(ledger.domain_side))));
        }

        let mut out = None;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active && !leg.stale && !leg.b_stale {
                let candidate = (leg.asset_index as usize, leg.side);
                self.validate_configured_asset_index(candidate.0)?;
                if out.replace(candidate).is_some() {
                    return Ok(None);
                }
            }
            slot += 1;
        }
        Ok(out)
    }

    fn clear_resolved_unattributed_negative_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        if account.header.pnl.get() >= 0 {
            return Ok(());
        }
        Err(V16Error::RecoveryRequired)
    }

    fn resolved_unattributed_insolvent_negative_pnl_requires_recovery(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<bool> {
        Ok(
            decode_market_mode(self.header.mode)? == MarketModeV16::Resolved
                && account.header.pnl.get() < 0
                && account.header.pnl.get().unsigned_abs() > account.header.capital.get()
                && self.resolved_bankruptcy_attribution(account)?.is_none(),
        )
    }

    fn settle_resolved_bankruptcy_negative_pnl(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        if account.header.pnl.get() >= 0 {
            return Ok(());
        }
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved {
            return Err(V16Error::LockActive);
        }
        let Some((asset_index, bankrupt_side)) =
            self.resolved_bankruptcy_attribution(&account.as_view())?
        else {
            return self.clear_resolved_unattributed_negative_pnl(account);
        };

        self.header.bankruptcy_hlock_active = 1;
        let gross_residual = account.header.pnl.get().unsigned_abs();
        if !account.header.close_progress.try_to_runtime()?.active {
            self.begin_close_progress_ledger(
                account,
                asset_index,
                opposite_side(bankrupt_side),
                gross_residual,
            )?;
        }

        let insurance_used =
            self.consume_domain_insurance_for_negative_pnl(asset_index, bankrupt_side, account)?;
        if insurance_used != 0 {
            self.advance_close_progress_ledger(account, 0, 0, insurance_used, 0, 0)?;
        }

        let residual = if account.header.pnl.get() < 0 {
            account.header.pnl.get().unsigned_abs()
        } else {
            0
        };
        if residual == 0 {
            account.header.health_cert.valid = 0;
            return Ok(());
        }

        let outcome = self.book_bankruptcy_residual_chunk_for_account_core(
            account,
            asset_index,
            bankrupt_side,
            residual,
        )?;
        let cleared = outcome
            .booked_loss
            .checked_add(outcome.explicit_loss)
            .ok_or(V16Error::ArithmeticOverflow)?
            .min(residual);
        let cleared_i128 = i128::try_from(cleared).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = account
            .header
            .pnl
            .get()
            .checked_add(cleared_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_account_pnl(account, new_pnl)?;
        account.header.health_cert.valid = 0;
        Ok(())
    }

    pub fn settle_negative_pnl_from_principal_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        account.validate_with_market(&self.as_view())?;
        let paid = self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        self.validate_account_audit_scan(&account.as_view())?;
        self.validate_shape_audit_scan()?;
        Ok(paid)
    }

    /// Kani-only direct entry to the *core* negative-PnL principal settlement
    /// transition, bypassing the loop-based `validate_with_market` precondition
    /// and the (default-off) audit scans. This exists so an inductive proof can
    /// `assume(canonical_inv(s))` over a loop-free decomposed invariant and then
    /// apply the bare transition `f`, proving `INV(s) => INV(f(s))` without
    /// paying the cost of the full O(N) shape scan inside the assume.
    #[cfg(kani)]
    pub fn kani_settle_negative_pnl_from_principal_core_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        self.settle_negative_pnl_from_principal_core_not_atomic(account)
    }

    fn settle_negative_pnl_from_principal_core_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let pnl = account.header.pnl.get();
        if pnl >= 0 {
            return Ok(0);
        }
        let loss = pnl.unsigned_abs();
        let paid = account.header.capital.get().min(loss);
        if paid == 0 {
            self.header.bankruptcy_hlock_active = 1;
            return Ok(0);
        }

        let vault_before = self.header.vault.get();
        let capital = account
            .header
            .capital
            .get()
            .checked_sub(paid)
            .ok_or(V16Error::CounterUnderflow)?;
        let c_tot = self
            .header
            .c_tot
            .get()
            .checked_sub(paid)
            .ok_or(V16Error::CounterUnderflow)?;
        let paid_i128 = i128::try_from(paid).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = pnl
            .checked_add(paid_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        account.header.capital = V16PodU128::new(capital);
        self.header.c_tot = V16PodU128::new(c_tot);
        self.set_account_pnl_after_principal_settlement(account, new_pnl)?;
        if new_pnl < 0 {
            self.header.bankruptcy_hlock_active = 1;
        }
        TokenValueFlowProofV16::account_capital_to_realized_loss(
            paid,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        account.header.health_cert.valid = 0;
        Ok(paid)
    }

    fn charge_account_fee_current_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        requested_fee: u128,
    ) -> V16Result<u128> {
        if requested_fee == 0 || account.header.pnl.get() < 0 {
            return Ok(0);
        }
        let charged = requested_fee.min(account.header.capital.get());
        if charged == 0 {
            return Ok(0);
        }
        let vault_before = self.header.vault.get();
        let capital = account
            .header
            .capital
            .get()
            .checked_sub(charged)
            .ok_or(V16Error::CounterUnderflow)?;
        let c_tot = self
            .header
            .c_tot
            .get()
            .checked_sub(charged)
            .ok_or(V16Error::CounterUnderflow)?;
        let insurance = self
            .header
            .insurance
            .get()
            .checked_add(charged)
            .ok_or(V16Error::ArithmeticOverflow)?;
        account.header.capital = V16PodU128::new(capital);
        self.header.c_tot = V16PodU128::new(c_tot);
        self.header.insurance = V16PodU128::new(insurance);
        TokenValueFlowProofV16::account_capital_to_insurance(
            charged,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        account.header.health_cert.valid = 0;
        Ok(charged)
    }

    #[cfg(kani)]
    pub fn kani_charge_account_fee_current_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        requested_fee: u128,
    ) -> V16Result<u128> {
        self.charge_account_fee_current_not_atomic(account, requested_fee)
    }

    fn charge_account_fee_after_loss_settlement(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        requested_fee: u128,
    ) -> V16Result<u128> {
        self.settle_account_side_effects_not_atomic(
            account,
            self.header.config.public_b_chunk_atoms.get(),
        )?;
        if decode_bool(account.header.b_stale_state)? || Self::has_b_stale_leg(&account.as_view())?
        {
            return Err(V16Error::BStale);
        }
        self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        let charged = self.charge_account_fee_current_not_atomic(account, requested_fee)?;
        self.validate_shape_audit_scan()?;
        Ok(charged)
    }

    pub fn charge_account_fee_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        requested_fee: u128,
    ) -> V16Result<u128> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        self.charge_account_fee_after_loss_settlement(account, requested_fee)
    }

    fn resolved_positive_payout_ready(&self) -> V16Result<bool> {
        if self.header.b_stale_account_count.get() != 0
            || self.header.stale_certificate_count.get() != 0
            || self.header.negative_pnl_account_count.get() != 0
        {
            return Ok(false);
        }
        let configured_assets = self.header.config.max_market_slots.get() as usize;
        let mut i = 0usize;
        while i < configured_assets {
            let slot = self
                .markets
                .get(i)
                .ok_or(V16Error::InvalidLeg)?
                .engine_slot();
            let asset = slot.asset.try_to_runtime()?;
            if slot.pending_domain_loss_barrier_long.get() != 0
                || slot.pending_domain_loss_barrier_short.get() != 0
                || asset.stored_pos_count_long != 0
                || asset.stored_pos_count_short != 0
                || asset.stale_account_count_long != 0
                || asset.stale_account_count_short != 0
            {
                return Ok(false);
            }
            i += 1;
        }
        Ok(true)
    }

    fn recompute_resolved_payout_rate(&mut self) -> V16Result<()> {
        let mut ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        let total_bound_num = ledger
            .terminal_claim_exact_receipts_num
            .checked_add(ledger.terminal_claim_bound_unreceipted_num)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if total_bound_num == 0 {
            ledger.current_payout_rate_num = 1;
            ledger.current_payout_rate_den = 1;
        } else {
            ledger.current_payout_rate_num = ledger
                .snapshot_residual
                .checked_mul(BOUND_SCALE)
                .ok_or(V16Error::ArithmeticOverflow)?
                .min(total_bound_num);
            ledger.current_payout_rate_den = total_bound_num;
        }
        self.header.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&ledger);
        Ok(())
    }

    fn initialize_resolved_payout_ledger_if_needed(&mut self) -> V16Result<()> {
        if decode_bool(self.header.payout_snapshot_captured)? {
            return Ok(());
        }
        let snapshot_residual = self.residual();
        self.header.payout_snapshot = V16PodU128::new(snapshot_residual);
        self.header.payout_snapshot_pnl_pos_tot = V16PodU128::new(self.junior_claim_bound());
        self.header.payout_snapshot_captured = 1;
        self.header.resolved_payout_ledger =
            ResolvedPayoutLedgerV16Account::from_runtime(&ResolvedPayoutLedgerV16 {
                snapshot_residual,
                terminal_claim_exact_receipts_num: 0,
                terminal_claim_bound_unreceipted_num: self.header.pnl_pos_bound_tot_num.get(),
                current_payout_rate_num: 0,
                current_payout_rate_den: 0,
                snapshot_slot: self
                    .header
                    .resolved_slot
                    .get()
                    .max(self.header.current_slot.get()),
                payout_halted: false,
                finalized: false,
            });
        self.recompute_resolved_payout_rate()
    }

    fn create_resolved_payout_receipt_if_needed(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        if decode_bool(account.header.resolved_payout_receipt.present)? {
            return Ok(());
        }
        self.initialize_resolved_payout_ledger_if_needed()?;
        let terminal_positive_claim_face = account.header.pnl.get().max(0) as u128;
        let prior_bound_contribution_num =
            V16Core::bound_num_from_amount(terminal_positive_claim_face)?;
        let mut ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        if V16Core::bound_num_from_amount(terminal_positive_claim_face)?
            > prior_bound_contribution_num
            || prior_bound_contribution_num > ledger.terminal_claim_bound_unreceipted_num
        {
            ledger.payout_halted = true;
            self.header.resolved_payout_ledger =
                ResolvedPayoutLedgerV16Account::from_runtime(&ledger);
            return Err(V16Error::RecoveryRequired);
        }
        ledger.terminal_claim_bound_unreceipted_num = ledger
            .terminal_claim_bound_unreceipted_num
            .checked_sub(prior_bound_contribution_num)
            .ok_or(V16Error::CounterUnderflow)?;
        ledger.terminal_claim_exact_receipts_num = ledger
            .terminal_claim_exact_receipts_num
            .checked_add(V16Core::bound_num_from_amount(
                terminal_positive_claim_face,
            )?)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.header.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&ledger);
        account.header.resolved_payout_receipt =
            ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16 {
                present: true,
                prior_bound_contribution_num,
                live_released_face_at_receipt: 0,
                terminal_positive_claim_face,
                paid_effective: 0,
                finalized: false,
            });
        self.recompute_resolved_payout_rate()
    }

    fn resolved_receipt_claimable_now(&self, receipt: ResolvedPayoutReceiptV16) -> V16Result<u128> {
        let ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        Self::resolved_receipt_claimable_against_ledger(receipt, ledger)
    }

    fn resolved_receipt_claimable_against_ledger(
        receipt: ResolvedPayoutReceiptV16,
        ledger: ResolvedPayoutLedgerV16,
    ) -> V16Result<u128> {
        PortfolioV16View::validate_resolved_payout_receipt_static(receipt)?;
        if !receipt.present {
            return Ok(0);
        }
        if ledger.payout_halted {
            return Err(V16Error::RecoveryRequired);
        }
        let gross = wide_mul_div_floor_u128(
            receipt.terminal_positive_claim_face,
            ledger.current_payout_rate_num,
            ledger.current_payout_rate_den,
        );
        gross
            .checked_sub(receipt.paid_effective)
            .ok_or(V16Error::InvalidLeg)
    }

    #[cfg(kani)]
    pub fn kani_resolved_receipt_claimable_against_ledger(
        receipt: ResolvedPayoutReceiptV16,
        ledger: ResolvedPayoutLedgerV16,
    ) -> V16Result<u128> {
        Self::resolved_receipt_claimable_against_ledger(receipt, ledger)
    }

    fn preflight_convert_released_pnl_to_capital(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            || decode_bool(self.header.payout_snapshot_captured)?
        {
            return Err(V16Error::LockActive);
        }
        self.ensure_favorable_action_allowed(account)
    }

    fn convert_released_pnl_to_capital_core_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let pos = account.header.pnl.get().max(0) as u128;
        let released = pos.saturating_sub(account.header.reserved_pnl.get());
        if released == 0 {
            return Ok(0);
        }
        if Self::account_has_source_claims(&account.as_view())?
            && self.account_has_active_source_claim_exposure(&account.as_view())?
        {
            return Err(V16Error::LockActive);
        }
        let converted = if Self::account_has_source_claims(&account.as_view())? {
            self.account_source_realizable_support(&account.as_view(), released)?
        } else if decode_market_mode(self.header.mode)? == MarketModeV16::Live {
            0
        } else {
            self.haircut_effective_support(released, self.residual(), self.junior_claim_bound())?
        };
        if converted == 0 {
            return Err(V16Error::LockActive);
        }
        let vault_before = self.header.vault.get();
        let consumption = if Self::account_has_source_claims(&account.as_view())? {
            self.create_and_consume_account_source_credit_for_effective_not_atomic(
                account, converted,
            )?
        } else {
            let residual = self.residual();
            let junior_bound = self.junior_claim_bound();
            SourceCreditConsumptionV16 {
                face_burn: self.face_claim_to_burn_for_support(
                    converted,
                    residual,
                    junior_bound,
                )?,
                counterparty_credit_consumed: 0,
                insurance_credit_consumed: 0,
            }
        };
        let face_i128 =
            i128::try_from(consumption.face_burn).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = account
            .header
            .pnl
            .get()
            .checked_sub(face_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_account_pnl(account, new_pnl)?;
        account.header.capital = V16PodU128::new(
            account
                .header
                .capital
                .get()
                .checked_add(converted)
                .ok_or(V16Error::ArithmeticOverflow)?,
        );
        self.header.c_tot = V16PodU128::new(
            self.header
                .c_tot
                .get()
                .checked_add(converted)
                .ok_or(V16Error::ArithmeticOverflow)?,
        );
        self.header.pnl_matured_pos_tot = V16PodU128::new(
            self.header
                .pnl_matured_pos_tot
                .get()
                .saturating_sub(consumption.face_burn),
        );
        let protocol_surplus_consumed = converted
            .checked_sub(consumption.counterparty_credit_consumed)
            .and_then(|v| v.checked_sub(consumption.insurance_credit_consumed))
            .ok_or(V16Error::CounterUnderflow)?;
        TokenValueFlowProofV16::support_to_account_capital(
            converted,
            consumption.counterparty_credit_consumed,
            consumption.insurance_credit_consumed,
            protocol_surplus_consumed,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        account.header.health_cert.valid = 0;
        Ok(converted)
    }

    pub fn convert_released_pnl_to_capital_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        self.preflight_convert_released_pnl_to_capital(&account.as_view())?;
        let converted = self.convert_released_pnl_to_capital_core_not_atomic(account)?;
        if converted != 0 {
            self.validate_shape()?;
            account.validate_with_market(&self.as_view())?;
        }
        Ok(converted)
    }

    pub fn release_account_source_credit_liens_if_unneeded_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        self.settle_account_side_effects_not_atomic(
            account,
            self.header.config.public_b_chunk_atoms.get(),
        )?;
        self.certify_account_after_local_settlement_with_price_override(account, None)?;
        let no_positive = Self::account_no_positive_credit_equity(&account.as_view())?;
        let cert = account.header.health_cert.try_to_runtime()?;
        if no_positive < 0 || (no_positive as u128) < cert.certified_initial_req {
            return Err(V16Error::LockActive);
        }

        let mut released_effective = 0u128;
        let domain_count = self
            .configured_domain_count()?
            .min(account.source_domains.len());
        let mut d = 0usize;
        while d < domain_count {
            let effective = account.source_domains[d]
                .source_lien_effective_reserved
                .get();
            let counterparty_backing = account.source_domains[d]
                .source_lien_counterparty_backing_num
                .get();
            let insurance_backing = account.source_domains[d]
                .source_lien_insurance_backing_num
                .get();
            if counterparty_backing != 0 {
                self.release_source_credit_lien_from_counterparty_not_atomic(
                    d,
                    counterparty_backing,
                )?;
            }
            if insurance_backing != 0 {
                self.release_source_credit_lien_from_insurance_not_atomic(d, insurance_backing)?;
            }
            if effective != 0 {
                released_effective = released_effective
                    .checked_add(effective)
                    .ok_or(V16Error::ArithmeticOverflow)?;
                let source = &mut account.source_domains[d];
                source.source_claim_liened_num = V16PodU128::new(0);
                source.source_claim_counterparty_liened_num = V16PodU128::new(0);
                source.source_claim_insurance_liened_num = V16PodU128::new(0);
                source.source_lien_effective_reserved = V16PodU128::new(0);
                source.source_lien_counterparty_backing_num = V16PodU128::new(0);
                source.source_lien_insurance_backing_num = V16PodU128::new(0);
                source.source_lien_fee_last_slot = V16PodU64::new(0);
                Self::clear_account_source_claim_market_id_if_empty(account, d);
            }
            d += 1;
        }
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()?;
        Ok(released_effective)
    }

    pub fn impair_account_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
    ) -> V16Result<u128> {
        self.domain_asset_side(domain)?;
        account.validate_with_market(&self.as_view())?;
        let source = account
            .source_domains
            .get(domain)
            .ok_or(V16Error::InvalidLeg)?;
        let insurance_backing = source.source_lien_insurance_backing_num.get();
        if insurance_backing == 0 {
            return Ok(0);
        }
        let effective = insurance_backing / BOUND_SCALE;
        let face = source.source_claim_insurance_liened_num.get();
        if effective == 0 || face == 0 {
            return Err(V16Error::InvalidLeg);
        }

        self.impair_source_credit_lien_from_insurance_core_not_atomic(domain, insurance_backing)?;
        let effective = Self::impair_account_source_credit_insurance_lien_fields(
            account, domain, face, effective,
        )?;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()?;
        Ok(effective)
    }

    fn preflight_cure_and_cancel_close(
        &self,
        account: &PortfolioV16View<'_>,
        optional_deposit: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        let ledger = account.header.close_progress.try_to_runtime()?;
        if !ledger.active
            || ledger.finalized
            || ledger.canceled
            || ledger.has_irreversible_progress()
            || ledger.residual_remaining != ledger.gross_loss_at_close_start
        {
            return Err(V16Error::LockActive);
        }
        let domain =
            self.insurance_domain_index(ledger.asset_index as usize, ledger.domain_side)?;
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot();
        let barrier_count = match side {
            SideV16::Long => slot.pending_domain_loss_barrier_long.get(),
            SideV16::Short => slot.pending_domain_loss_barrier_short.get(),
        };
        if barrier_count == 0 {
            return Err(V16Error::LockActive);
        }
        let escrow_total = account
            .header
            .cancel_deposit_escrow
            .get()
            .checked_add(optional_deposit)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let new_vault = self
            .header
            .vault
            .get()
            .checked_add(optional_deposit)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if new_vault > MAX_VAULT_TVL {
            return Err(V16Error::InvalidConfig);
        }
        let _new_capital = account
            .header
            .capital
            .get()
            .checked_add(escrow_total)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let _new_c_tot = self
            .header
            .c_tot
            .get()
            .checked_add(escrow_total)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(())
    }

    fn cure_and_cancel_close_with_cert_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        optional_deposit: u128,
        cert: HealthCertV16,
    ) -> V16Result<()> {
        self.preflight_cure_and_cancel_close(&account.as_view(), optional_deposit)?;
        let ledger = account.header.close_progress.try_to_runtime()?;
        let vault_before = self.header.vault.get();
        let escrow_before = account.header.cancel_deposit_escrow.get();
        let escrow_total = escrow_before
            .checked_add(optional_deposit)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let new_vault = self
            .header
            .vault
            .get()
            .checked_add(optional_deposit)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let new_capital = account
            .header
            .capital
            .get()
            .checked_add(escrow_total)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let new_c_tot = self
            .header
            .c_tot
            .get()
            .checked_add(escrow_total)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let escrow_i128 = i128::try_from(escrow_total).map_err(|_| V16Error::ArithmeticOverflow)?;
        let cured_equity = cert
            .certified_equity
            .checked_add(escrow_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if cured_equity < 0 || (cured_equity as u128) < cert.certified_initial_req {
            return Err(V16Error::InvalidConfig);
        }

        self.header.vault = V16PodU128::new(new_vault);
        self.header.c_tot = V16PodU128::new(new_c_tot);
        account.header.capital = V16PodU128::new(new_capital);
        account.header.cancel_deposit_escrow = V16PodU128::new(0);
        let domain =
            self.insurance_domain_index(ledger.asset_index as usize, ledger.domain_side)?;
        let count = self
            .pending_domain_loss_barrier_count(ledger.asset_index as usize, ledger.domain_side)?;
        if count == 0 {
            return Err(V16Error::CounterUnderflow);
        }
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.pending_domain_loss_barrier_long = V16PodU64::new(count - 1),
            SideV16::Short => slot.pending_domain_loss_barrier_short = V16PodU64::new(count - 1),
        }
        account.header.close_progress =
            CloseProgressLedgerV16Account::from_runtime(&CloseProgressLedgerV16 {
                active: false,
                finalized: false,
                canceled: true,
                ..ledger
            });
        TokenValueFlowProofV16::close_cure_to_account_capital(
            optional_deposit,
            escrow_before,
            escrow_total,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()
    }

    pub fn cure_and_cancel_close_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        optional_deposit: u128,
    ) -> V16Result<()> {
        self.preflight_cure_and_cancel_close(&account.as_view(), optional_deposit)?;
        let cert = self.full_account_refresh_not_atomic(account)?;
        self.cure_and_cancel_close_with_cert_not_atomic(account, optional_deposit, cert)
    }

    pub fn sync_account_fee_to_slot_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        now_slot: u64,
        fee_rate_per_slot: u128,
    ) -> V16Result<u128> {
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? == MarketModeV16::Recovery {
            return Err(V16Error::LockActive);
        }
        if now_slot < account.header.last_fee_slot.get() {
            return Err(V16Error::Stale);
        }
        let nonflat = !active_bitmap_is_empty(account.header.active_bitmap.map(V16PodU64::get));
        let fee_anchor = if decode_market_mode(self.header.mode)? == MarketModeV16::Live
            && nonflat
            && now_slot > self.header.slot_last.get()
        {
            self.header.slot_last.get()
        } else if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved {
            self.header.resolved_slot.get()
        } else {
            now_slot
        };
        if fee_anchor <= account.header.last_fee_slot.get() {
            return Ok(0);
        }
        let dt = fee_anchor - account.header.last_fee_slot.get();
        let raw_fee = U256::from_u128(fee_rate_per_slot)
            .checked_mul(U256::from_u64(dt))
            .ok_or(V16Error::ArithmeticOverflow)?;
        let requested_fee = raw_fee.try_into_u128().unwrap_or(u128::MAX);
        if decode_market_mode(self.header.mode)? == MarketModeV16::Live && nonflat {
            if let PermissionlessProgressOutcomeV16::AccountBChunk(_) = self
                .settle_account_side_effects_not_atomic(
                    account,
                    self.header.config.public_b_chunk_atoms.get(),
                )?
            {
                return Err(V16Error::BStale);
            }
        }
        self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        let charged = self.charge_account_fee_current_not_atomic(account, requested_fee)?;
        account.header.last_fee_slot = V16PodU64::new(fee_anchor);
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()?;
        Ok(charged)
    }

    pub fn withdraw_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        amount: u128,
    ) -> V16Result<()> {
        if amount == 0 {
            return Ok(());
        }
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        if !active_bitmap_is_empty(account.header.active_bitmap.map(V16PodU64::get)) {
            return Err(V16Error::Stale);
        }
        if account.header.close_progress.try_to_runtime()? != CloseProgressLedgerV16::EMPTY {
            return Err(V16Error::LockActive);
        }
        self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        if account.header.pnl.get() < 0 || amount > account.header.capital.get() {
            return Err(V16Error::LockActive);
        }
        let post_capital = account
            .header
            .capital
            .get()
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        let equity_after = account_equity_from_parts(
            post_capital,
            account.header.pnl.get(),
            account.header.fee_credits.get(),
        )?;
        if equity_after < 0 {
            return Err(V16Error::InvalidConfig);
        }

        let vault_before = self.header.vault.get();
        let c_tot = self
            .header
            .c_tot
            .get()
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        let vault = self
            .header
            .vault
            .get()
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        account.header.capital = V16PodU128::new(post_capital);
        self.header.c_tot = V16PodU128::new(c_tot);
        self.header.vault = V16PodU128::new(vault);
        TokenValueFlowProofV16::account_capital_to_external_out(amount, vault_before, vault)?
            .validate()?;
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()
    }

    pub fn resolve_market_not_atomic(&mut self, resolved_slot: u64) -> V16Result<()> {
        if decode_market_mode(self.header.mode)? == MarketModeV16::Recovery {
            return Err(V16Error::LockActive);
        }
        if resolved_slot < self.header.current_slot.get() {
            return Err(V16Error::Stale);
        }
        self.header.mode = encode_market_mode(MarketModeV16::Resolved);
        self.header.resolved_slot = V16PodU64::new(resolved_slot);
        self.header.current_slot = V16PodU64::new(resolved_slot);
        self.header.loss_stale_active = 0;
        self.validate_shape()
    }

    pub fn claim_resolved_payout_topup_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved
            || !decode_bool(self.header.payout_snapshot_captured)?
        {
            return Err(V16Error::LockActive);
        }
        let mut receipt = account.header.resolved_payout_receipt.try_to_runtime()?;
        let claimable = self.resolved_receipt_claimable_now(receipt)?;
        if claimable == 0 {
            return Ok(0);
        }
        let payout = claimable.min(self.header.vault.get());
        receipt = apply_resolved_payout_receipt_payment(receipt, payout)?;
        let vault_before = self.header.vault.get();
        self.header.vault = V16PodU128::new(
            self.header
                .vault
                .get()
                .checked_sub(payout)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        account.header.resolved_payout_receipt =
            ResolvedPayoutReceiptV16Account::from_runtime(&receipt);
        TokenValueFlowProofV16::capital_and_resolved_payout_to_external_out(
            0,
            payout,
            payout,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())?;
        Ok(payout)
    }

    pub fn refine_resolved_unreceipted_bound_not_atomic(
        &mut self,
        decrease_num: u128,
    ) -> V16Result<()> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved
            || !decode_bool(self.header.payout_snapshot_captured)?
        {
            return Err(V16Error::LockActive);
        }
        let mut ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        let old_num = ledger.current_payout_rate_num;
        let old_den = ledger.current_payout_rate_den;
        ledger.terminal_claim_bound_unreceipted_num = ledger
            .terminal_claim_bound_unreceipted_num
            .checked_sub(decrease_num)
            .ok_or(V16Error::CounterUnderflow)?;
        self.header.resolved_payout_ledger = ResolvedPayoutLedgerV16Account::from_runtime(&ledger);
        self.recompute_resolved_payout_rate()?;
        let next = self.header.resolved_payout_ledger.try_to_runtime()?;
        if !fraction_ge(
            next.current_payout_rate_num,
            next.current_payout_rate_den,
            old_num,
            old_den,
        )? {
            return Err(V16Error::InvalidConfig);
        }
        self.validate_shape()
    }

    pub fn close_resolved_account_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        fee_rate_per_slot: u128,
    ) -> V16Result<ResolvedCloseOutcomeV16> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved {
            return Err(V16Error::LockActive);
        }
        if let PermissionlessProgressOutcomeV16::AccountBChunk(_) = self
            .settle_account_side_effects_not_atomic(
                account,
                self.header.config.public_b_chunk_atoms.get(),
            )?
        {
            self.validate_shape()?;
            return Ok(ResolvedCloseOutcomeV16::ProgressOnly);
        }
        if self
            .resolved_unattributed_insolvent_negative_pnl_requires_recovery(&account.as_view())?
        {
            return Err(V16Error::RecoveryRequired);
        }
        self.sync_account_fee_to_slot_not_atomic(
            account,
            self.header.resolved_slot.get(),
            fee_rate_per_slot,
        )?;
        if self
            .resolved_unattributed_insolvent_negative_pnl_requires_recovery(&account.as_view())?
        {
            return Err(V16Error::RecoveryRequired);
        }
        self.settle_negative_pnl_from_principal_not_atomic(account)?;
        if account.header.pnl.get() < 0 {
            self.settle_resolved_bankruptcy_negative_pnl(account)?;
        }
        self.detach_solvent_active_legs_for_resolved_close(account)?;
        if !active_bitmap_is_empty(account.header.active_bitmap.map(V16PodU64::get))
            || account.header.pnl.get() < 0
            || decode_bool(account.header.b_stale_state)?
            || decode_bool(account.header.stale_state)?
        {
            return Ok(ResolvedCloseOutcomeV16::ProgressOnly);
        }
        if account.header.pnl.get() > 0 && !self.resolved_positive_payout_ready()? {
            return Ok(ResolvedCloseOutcomeV16::ProgressOnly);
        }
        let mut payout_receipt = None;
        let pnl_payout = if account.header.pnl.get() > 0
            || decode_bool(account.header.resolved_payout_receipt.present)?
        {
            self.create_resolved_payout_receipt_if_needed(account)?;
            let receipt = account.header.resolved_payout_receipt.try_to_runtime()?;
            let claimable = self.resolved_receipt_claimable_now(receipt)?;
            payout_receipt = Some(receipt);
            claimable
        } else {
            0
        };
        let account_capital = account.header.capital.get();
        let payout = account_capital
            .checked_add(pnl_payout)
            .ok_or(V16Error::ArithmeticOverflow)?
            .min(self.header.vault.get());
        let capital_paid = account_capital.min(payout);
        let resolved_paid = payout
            .checked_sub(capital_paid)
            .ok_or(V16Error::CounterUnderflow)?;
        if let Some(mut receipt) = payout_receipt {
            receipt = apply_resolved_payout_receipt_payment(receipt, resolved_paid)?;
            account.header.resolved_payout_receipt =
                ResolvedPayoutReceiptV16Account::from_runtime(&receipt);
        }
        let vault_before = self.header.vault.get();
        self.header.vault = V16PodU128::new(
            self.header
                .vault
                .get()
                .checked_sub(payout)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        self.header.c_tot = V16PodU128::new(
            self.header
                .c_tot
                .get()
                .saturating_sub(account_capital.min(self.header.c_tot.get())),
        );
        self.set_account_pnl(account, 0)?;
        account.header.capital = V16PodU128::new(0);
        account.header.reserved_pnl = V16PodU128::new(0);
        account.header.fee_credits = V16PodI128::new(0);
        account.header.health_cert.valid = 0;
        TokenValueFlowProofV16::capital_and_resolved_payout_to_external_out(
            capital_paid,
            resolved_paid,
            payout,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())?;
        Ok(ResolvedCloseOutcomeV16::Closed { payout })
    }

    fn detach_solvent_active_legs_for_resolved_close(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        if account.header.pnl.get() < 0
            || decode_bool(account.header.b_stale_state)?
            || decode_bool(account.header.stale_state)?
            || account
                .header
                .close_progress
                .try_to_runtime()?
                .has_pending_residual()
        {
            return Ok(());
        }

        let configured_max = self.header.config.max_market_slots.get() as usize;
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active {
                if leg.b_stale || leg.stale {
                    return Ok(());
                }
                let asset_index = leg.asset_index as usize;
                if asset_index >= configured_max {
                    return Err(V16Error::InvalidLeg);
                }
                if self.has_pending_domain_loss_barrier(asset_index, leg.side)? {
                    return Ok(());
                }
                let (k_target, f_target) = self.kf_target_for_leg(asset_index, leg)?;
                if k_target != leg.k_snap || f_target != leg.f_snap {
                    return Ok(());
                }
                if self.b_target_for_leg(asset_index, leg)? != leg.b_snap {
                    return Ok(());
                }
            }
            slot += 1;
        }

        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            if leg.active {
                self.clear_leg(account, leg.asset_index as usize)?;
            }
            slot += 1;
        }
        Ok(())
    }

    pub fn deposit_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        amount: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if amount == 0 {
            return Ok(());
        }
        let vault_before = self.header.vault.get();
        let capital = account
            .header
            .capital
            .get()
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let c_tot = self
            .header
            .c_tot
            .get()
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let vault = self
            .header
            .vault
            .get()
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        TokenValueFlowProofV16::external_in_to_account_capital(amount, vault_before, vault)?
            .validate()?;
        account.header.capital = V16PodU128::new(capital);
        account.header.health_cert.valid = 0;
        self.header.c_tot = V16PodU128::new(c_tot);
        self.header.vault = V16PodU128::new(vault);
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()
    }

    pub fn mark_asset_drain_only_not_atomic(&mut self, asset_index: usize) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        let mut asset = self.asset_state(asset_index)?;
        match asset.lifecycle {
            AssetLifecycleV16::Active => {
                let (next_asset_set_epoch, next_risk_epoch) =
                    self.checked_asset_set_epoch_bump()?;
                asset.lifecycle = AssetLifecycleV16::DrainOnly;
                self.set_asset_state(asset_index, asset)?;
                self.commit_asset_set_epoch_bump(next_asset_set_epoch, next_risk_epoch);
                self.validate_shape()
            }
            AssetLifecycleV16::DrainOnly => Ok(()),
            _ => Err(V16Error::LockActive),
        }
    }

    pub fn retire_empty_asset_not_atomic(
        &mut self,
        asset_index: usize,
        now_slot: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if now_slot < self.header.current_slot.get() {
            return Err(V16Error::Stale);
        }
        let mut asset = self.asset_state(asset_index)?;
        match asset.lifecycle {
            AssetLifecycleV16::Active
            | AssetLifecycleV16::DrainOnly
            | AssetLifecycleV16::Recovery => {
                self.require_empty_asset_lifecycle_state(asset_index)?;
                let (next_asset_set_epoch, next_risk_epoch) =
                    self.checked_asset_set_epoch_bump()?;
                asset.lifecycle = AssetLifecycleV16::Retired;
                asset.retired_slot = now_slot;
                self.set_asset_state(asset_index, asset)?;
                self.header.current_slot = V16PodU64::new(now_slot);
                self.commit_asset_set_epoch_bump(next_asset_set_epoch, next_risk_epoch);
                self.validate_shape()
            }
            AssetLifecycleV16::Retired => {
                self.require_empty_asset_lifecycle_state(asset_index)?;
                self.validate_shape()
            }
            _ => Err(V16Error::LockActive),
        }
    }

    pub fn activate_empty_market_not_atomic(
        &mut self,
        asset_index: u32,
        authenticated_price: u64,
        now_slot: u64,
    ) -> V16Result<()> {
        let slot = self
            .markets
            .get_mut(asset_index as usize)
            .ok_or(V16Error::InvalidLeg)?;
        self.header.activate_empty_market_slot_not_atomic(
            asset_index,
            slot,
            authenticated_price,
            now_slot,
        )
    }

    fn leg_is_dead_for_forfeit(&self, asset_index: usize, side: SideV16) -> V16Result<bool> {
        let side_mode = self.side_mode_for(asset_index, side)?;
        let asset_lifecycle = self.asset_state(asset_index)?.lifecycle;
        Ok(
            decode_market_mode(self.header.mode)? == MarketModeV16::Recovery
                || asset_lifecycle == AssetLifecycleV16::Recovery
                || matches!(
                    side_mode,
                    SideModeV16::DrainOnly | SideModeV16::ResetPending
                ),
        )
    }

    fn settle_forfeited_leg_kf_effects(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
    ) -> V16Result<(u128, u128, u128, u128)> {
        let Some(leg_slot) = Self::active_leg_slot_for_asset(&account.as_view(), asset_index)?
        else {
            return Ok((0, 0, 0, 0));
        };
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
        let (k_now, f_now) = self.kf_target_for_leg(asset_index, leg)?;
        let den = leg
            .a_basis
            .checked_mul(POS_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let k_delta = scaled_adl_delta_fast(
            leg.basis_pos_q.unsigned_abs(),
            leg.a_basis,
            leg.k_snap,
            k_now,
        )
        .unwrap_or_else(|| {
            wide_signed_mul_div_floor_from_k_pair(
                leg.basis_pos_q.unsigned_abs(),
                leg.k_snap,
                k_now,
                den,
            )
        });
        let f_delta = scaled_adl_delta_fast(
            leg.basis_pos_q.unsigned_abs(),
            leg.a_basis,
            leg.f_snap,
            f_now,
        )
        .unwrap_or_else(|| {
            wide_signed_mul_div_floor_from_k_pair(
                leg.basis_pos_q.unsigned_abs(),
                leg.f_snap,
                f_now,
                den,
            )
        });
        let net = k_delta
            .checked_add(f_delta)
            .ok_or(V16Error::ArithmeticOverflow)?;
        validate_non_min_i128(net)?;

        let mut loss_settled = 0u128;
        let mut support_consumed = 0u128;
        let mut junior_face_burned = 0u128;
        let mut positive_pnl_forfeited = 0u128;
        if net < 0 {
            loss_settled = net.unsigned_abs();
            let support = self.apply_haircut_bounded_close_loss_to_pnl(account, loss_settled)?;
            support_consumed = support.support_consumed;
            junior_face_burned = support.junior_face_burned;
        } else {
            positive_pnl_forfeited = net as u128;
        }

        leg.k_snap = k_now;
        leg.f_snap = f_now;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&leg);
        account.header.health_cert.valid = 0;
        Ok((
            loss_settled,
            positive_pnl_forfeited,
            support_consumed,
            junior_face_burned,
        ))
    }

    pub fn forfeit_recovery_leg_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
        b_delta_budget: u128,
    ) -> V16Result<DeadLegForfeitOutcomeV16> {
        account.validate_with_market(&self.as_view())?;
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
            || b_delta_budget == 0
        {
            return Err(V16Error::InvalidLeg);
        }
        let leg = Self::active_leg_for_asset(&account.as_view(), asset_index)?;
        if !leg.active {
            return Err(V16Error::InvalidLeg);
        }
        if !self.leg_is_dead_for_forfeit(asset_index, leg.side)? {
            return Err(V16Error::LockActive);
        }

        let (k_target, f_target) = self.kf_target_for_leg(asset_index, leg)?;
        let b_target = self.b_target_for_leg(asset_index, leg)?;
        if account.header.pnl.get() == 0
            && k_target == leg.k_snap
            && f_target == leg.f_snap
            && b_target <= leg.b_snap
            && !account
                .header
                .close_progress
                .try_to_runtime()?
                .has_pending_residual()
        {
            self.clear_leg(account, asset_index)?;
            return Ok(DeadLegForfeitOutcomeV16 {
                detached: true,
                positive_pnl_forfeited: 0,
                loss_settled: 0,
                support_consumed: 0,
                junior_face_burned: 0,
                principal_used: 0,
                insurance_used: 0,
                residual_booked: 0,
                explicit_loss: 0,
            });
        }

        let (loss_settled, positive_pnl_forfeited, support_consumed, junior_face_burned) =
            self.settle_forfeited_leg_kf_effects(account, asset_index)?;

        let mut total_loss_settled = loss_settled;
        let refreshed = Self::active_leg_for_asset(&account.as_view(), asset_index)?;
        if self.b_target_for_leg(asset_index, refreshed)? > refreshed.b_snap {
            self.mark_leg_b_stale(account, asset_index)?;
            let chunk = self.settle_account_b_chunk(account, asset_index, b_delta_budget)?;
            total_loss_settled = total_loss_settled
                .checked_add(chunk.loss)
                .ok_or(V16Error::ArithmeticOverflow)?;
            if chunk.remaining_after != 0 {
                self.validate_shape()?;
                account.validate_with_market(&self.as_view())?;
                return Ok(DeadLegForfeitOutcomeV16 {
                    detached: false,
                    positive_pnl_forfeited,
                    loss_settled: total_loss_settled,
                    support_consumed,
                    junior_face_burned,
                    principal_used: 0,
                    insurance_used: 0,
                    residual_booked: 0,
                    explicit_loss: 0,
                });
            }
        }

        let principal_used = self.settle_negative_pnl_from_principal_not_atomic(account)?;
        let bankruptcy_residual_after_principal = if account.header.pnl.get() < 0 {
            account.header.pnl.get().unsigned_abs()
        } else {
            0
        };
        let gross_close_loss = bankruptcy_residual_after_principal
            .checked_add(support_consumed)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if gross_close_loss != 0 {
            self.begin_close_progress_ledger(
                account,
                asset_index,
                opposite_side(leg.side),
                gross_close_loss,
            )?;
            if support_consumed != 0 {
                self.advance_close_progress_ledger(
                    account,
                    support_consumed,
                    junior_face_burned,
                    0,
                    0,
                    0,
                )?;
            }
        }

        let insurance_used =
            self.consume_domain_insurance_for_negative_pnl(asset_index, leg.side, account)?;
        if insurance_used != 0 {
            self.advance_close_progress_ledger(account, 0, 0, insurance_used, 0, 0)?;
        }

        let residual = if account.header.pnl.get() < 0 {
            account.header.pnl.get().unsigned_abs()
        } else {
            0
        };
        let mut residual_booked = 0u128;
        let mut explicit_loss = 0u128;
        if residual != 0 {
            let outcome = self.book_bankruptcy_residual_chunk_for_account_core(
                account,
                asset_index,
                leg.side,
                residual,
            )?;
            residual_booked = outcome.booked_loss;
            explicit_loss = outcome.explicit_loss;
            let cleared = residual_booked
                .checked_add(explicit_loss)
                .ok_or(V16Error::ArithmeticOverflow)?
                .min(residual);
            let cleared_i128 = i128::try_from(cleared).map_err(|_| V16Error::ArithmeticOverflow)?;
            let new_pnl = account
                .header
                .pnl
                .get()
                .checked_add(cleared_i128)
                .ok_or(V16Error::ArithmeticOverflow)?;
            self.set_account_pnl(account, new_pnl)?;
        }

        let detached = account.header.pnl.get() >= 0
            && !account
                .header
                .close_progress
                .try_to_runtime()?
                .has_pending_residual();
        if detached {
            self.clear_leg(account, asset_index)?;
        }
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())?;

        Ok(DeadLegForfeitOutcomeV16 {
            detached,
            positive_pnl_forfeited,
            loss_settled: total_loss_settled,
            support_consumed,
            junior_face_burned,
            principal_used,
            insurance_used,
            residual_booked,
            explicit_loss,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct PortfolioLegV16Account {
    pub active: u8,
    pub asset_index: V16PodU32,
    pub market_id: V16PodU64,
    pub side: u8,
    pub basis_pos_q: V16PodI128,
    pub a_basis: V16PodU128,
    pub k_snap: V16PodI128,
    pub f_snap: V16PodI128,
    pub epoch_snap: V16PodU64,
    pub loss_weight: V16PodU128,
    pub b_snap: V16PodU128,
    pub b_rem: V16PodU128,
    pub b_epoch_snap: V16PodU64,
    pub b_stale: u8,
    pub stale: u8,
}

impl PortfolioLegV16Account {
    pub fn from_runtime(value: &PortfolioLegV16) -> Self {
        Self {
            active: encode_bool(value.active),
            asset_index: V16PodU32::new(value.asset_index),
            market_id: V16PodU64::new(value.market_id),
            side: encode_side(value.side),
            basis_pos_q: V16PodI128::new(value.basis_pos_q),
            a_basis: V16PodU128::new(value.a_basis),
            k_snap: V16PodI128::new(value.k_snap),
            f_snap: V16PodI128::new(value.f_snap),
            epoch_snap: V16PodU64::new(value.epoch_snap),
            loss_weight: V16PodU128::new(value.loss_weight),
            b_snap: V16PodU128::new(value.b_snap),
            b_rem: V16PodU128::new(value.b_rem),
            b_epoch_snap: V16PodU64::new(value.b_epoch_snap),
            b_stale: encode_bool(value.b_stale),
            stale: encode_bool(value.stale),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<PortfolioLegV16> {
        let out = PortfolioLegV16 {
            active: decode_bool(self.active)?,
            asset_index: self.asset_index.get(),
            market_id: self.market_id.get(),
            side: decode_side(self.side)?,
            basis_pos_q: self.basis_pos_q.get(),
            a_basis: self.a_basis.get(),
            k_snap: self.k_snap.get(),
            f_snap: self.f_snap.get(),
            epoch_snap: self.epoch_snap.get(),
            loss_weight: self.loss_weight.get(),
            b_snap: self.b_snap.get(),
            b_rem: self.b_rem.get(),
            b_epoch_snap: self.b_epoch_snap.get(),
            b_stale: decode_bool(self.b_stale)?,
            stale: decode_bool(self.stale)?,
        };
        if out.active {
            validate_active_leg(out)?;
        } else if !out.is_empty() {
            return Err(V16Error::HiddenLeg);
        }
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct HealthCertV16Account {
    pub certified_equity: V16PodI128,
    pub certified_initial_req: V16PodU128,
    pub certified_maintenance_req: V16PodU128,
    pub certified_liq_deficit: V16PodU128,
    pub certified_worst_case_loss: V16PodU128,
    pub cert_oracle_epoch: V16PodU64,
    pub cert_funding_epoch: V16PodU64,
    pub cert_risk_epoch: V16PodU64,
    pub cert_asset_set_epoch: V16PodU64,
    pub active_bitmap_at_cert: [V16PodU64; V16_ACTIVE_BITMAP_WORDS],
    pub valid: u8,
}

impl HealthCertV16Account {
    pub fn from_runtime(value: &HealthCertV16) -> Self {
        Self {
            certified_equity: V16PodI128::new(value.certified_equity),
            certified_initial_req: V16PodU128::new(value.certified_initial_req),
            certified_maintenance_req: V16PodU128::new(value.certified_maintenance_req),
            certified_liq_deficit: V16PodU128::new(value.certified_liq_deficit),
            certified_worst_case_loss: V16PodU128::new(value.certified_worst_case_loss),
            cert_oracle_epoch: V16PodU64::new(value.cert_oracle_epoch),
            cert_funding_epoch: V16PodU64::new(value.cert_funding_epoch),
            cert_risk_epoch: V16PodU64::new(value.cert_risk_epoch),
            cert_asset_set_epoch: V16PodU64::new(value.cert_asset_set_epoch),
            active_bitmap_at_cert: value.active_bitmap_at_cert.map(V16PodU64::new),
            valid: encode_bool(value.valid),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<HealthCertV16> {
        let out = HealthCertV16 {
            certified_equity: self.certified_equity.get(),
            certified_initial_req: self.certified_initial_req.get(),
            certified_maintenance_req: self.certified_maintenance_req.get(),
            certified_liq_deficit: self.certified_liq_deficit.get(),
            certified_worst_case_loss: self.certified_worst_case_loss.get(),
            cert_oracle_epoch: self.cert_oracle_epoch.get(),
            cert_funding_epoch: self.cert_funding_epoch.get(),
            cert_risk_epoch: self.cert_risk_epoch.get(),
            cert_asset_set_epoch: self.cert_asset_set_epoch.get(),
            active_bitmap_at_cert: self.active_bitmap_at_cert.map(|v| v.get()),
            valid: decode_bool(self.valid)?,
        };
        validate_non_min_i128(out.certified_equity)?;
        Ok(out)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct CloseProgressLedgerV16Account {
    pub active: u8,
    pub finalized: u8,
    pub canceled: u8,
    pub close_id: V16PodU64,
    pub asset_index: V16PodU32,
    pub market_id: V16PodU64,
    pub domain_side: u8,
    pub gross_loss_at_close_start: V16PodU128,
    pub drift_reference_slot: V16PodU64,
    pub max_close_slot: V16PodU64,
    pub support_consumed: V16PodU128,
    pub junior_face_burned: V16PodU128,
    pub insurance_spent: V16PodU128,
    pub b_loss_booked: V16PodU128,
    pub explicit_loss_assigned: V16PodU128,
    pub quantity_adl_applied_q: V16PodU128,
    pub drift_consumed: V16PodU128,
    pub residual_remaining: V16PodU128,
}

impl CloseProgressLedgerV16Account {
    pub fn from_runtime(value: &CloseProgressLedgerV16) -> Self {
        Self {
            active: encode_bool(value.active),
            finalized: encode_bool(value.finalized),
            canceled: encode_bool(value.canceled),
            close_id: V16PodU64::new(value.close_id),
            asset_index: V16PodU32::new(value.asset_index),
            market_id: V16PodU64::new(value.market_id),
            domain_side: encode_side(value.domain_side),
            gross_loss_at_close_start: V16PodU128::new(value.gross_loss_at_close_start),
            drift_reference_slot: V16PodU64::new(value.drift_reference_slot),
            max_close_slot: V16PodU64::new(value.max_close_slot),
            support_consumed: V16PodU128::new(value.support_consumed),
            junior_face_burned: V16PodU128::new(value.junior_face_burned),
            insurance_spent: V16PodU128::new(value.insurance_spent),
            b_loss_booked: V16PodU128::new(value.b_loss_booked),
            explicit_loss_assigned: V16PodU128::new(value.explicit_loss_assigned),
            quantity_adl_applied_q: V16PodU128::new(value.quantity_adl_applied_q),
            drift_consumed: V16PodU128::new(value.drift_consumed),
            residual_remaining: V16PodU128::new(value.residual_remaining),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<CloseProgressLedgerV16> {
        Ok(CloseProgressLedgerV16 {
            active: decode_bool(self.active)?,
            finalized: decode_bool(self.finalized)?,
            canceled: decode_bool(self.canceled)?,
            close_id: self.close_id.get(),
            asset_index: self.asset_index.get(),
            market_id: self.market_id.get(),
            domain_side: decode_side(self.domain_side)?,
            gross_loss_at_close_start: self.gross_loss_at_close_start.get(),
            drift_reference_slot: self.drift_reference_slot.get(),
            max_close_slot: self.max_close_slot.get(),
            support_consumed: self.support_consumed.get(),
            junior_face_burned: self.junior_face_burned.get(),
            insurance_spent: self.insurance_spent.get(),
            b_loss_booked: self.b_loss_booked.get(),
            explicit_loss_assigned: self.explicit_loss_assigned.get(),
            quantity_adl_applied_q: self.quantity_adl_applied_q.get(),
            drift_consumed: self.drift_consumed.get(),
            residual_remaining: self.residual_remaining.get(),
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ResolvedPayoutLedgerV16Account {
    pub snapshot_residual: V16PodU128,
    pub terminal_claim_exact_receipts_num: V16PodU128,
    pub terminal_claim_bound_unreceipted_num: V16PodU128,
    pub current_payout_rate_num: V16PodU128,
    pub current_payout_rate_den: V16PodU128,
    pub snapshot_slot: V16PodU64,
    pub payout_halted: u8,
    pub finalized: u8,
}

impl ResolvedPayoutLedgerV16Account {
    pub fn from_runtime(value: &ResolvedPayoutLedgerV16) -> Self {
        Self {
            snapshot_residual: V16PodU128::new(value.snapshot_residual),
            terminal_claim_exact_receipts_num: V16PodU128::new(
                value.terminal_claim_exact_receipts_num,
            ),
            terminal_claim_bound_unreceipted_num: V16PodU128::new(
                value.terminal_claim_bound_unreceipted_num,
            ),
            current_payout_rate_num: V16PodU128::new(value.current_payout_rate_num),
            current_payout_rate_den: V16PodU128::new(value.current_payout_rate_den),
            snapshot_slot: V16PodU64::new(value.snapshot_slot),
            payout_halted: encode_bool(value.payout_halted),
            finalized: encode_bool(value.finalized),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<ResolvedPayoutLedgerV16> {
        Ok(ResolvedPayoutLedgerV16 {
            snapshot_residual: self.snapshot_residual.get(),
            terminal_claim_exact_receipts_num: self.terminal_claim_exact_receipts_num.get(),
            terminal_claim_bound_unreceipted_num: self.terminal_claim_bound_unreceipted_num.get(),
            current_payout_rate_num: self.current_payout_rate_num.get(),
            current_payout_rate_den: self.current_payout_rate_den.get(),
            snapshot_slot: self.snapshot_slot.get(),
            payout_halted: decode_bool(self.payout_halted)?,
            finalized: decode_bool(self.finalized)?,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct ResolvedPayoutReceiptV16Account {
    pub prior_bound_contribution_num: V16PodU128,
    pub live_released_face_at_receipt: V16PodU128,
    pub terminal_positive_claim_face: V16PodU128,
    pub paid_effective: V16PodU128,
    pub present: u8,
    pub finalized: u8,
}

impl ResolvedPayoutReceiptV16Account {
    pub fn from_runtime(value: &ResolvedPayoutReceiptV16) -> Self {
        Self {
            prior_bound_contribution_num: V16PodU128::new(value.prior_bound_contribution_num),
            live_released_face_at_receipt: V16PodU128::new(value.live_released_face_at_receipt),
            terminal_positive_claim_face: V16PodU128::new(value.terminal_positive_claim_face),
            paid_effective: V16PodU128::new(value.paid_effective),
            present: encode_bool(value.present),
            finalized: encode_bool(value.finalized),
        }
    }

    pub fn try_to_runtime(&self) -> V16Result<ResolvedPayoutReceiptV16> {
        Ok(ResolvedPayoutReceiptV16 {
            present: decode_bool(self.present)?,
            prior_bound_contribution_num: self.prior_bound_contribution_num.get(),
            live_released_face_at_receipt: self.live_released_face_at_receipt.get(),
            terminal_positive_claim_face: self.terminal_positive_claim_face.get(),
            paid_effective: self.paid_effective.get(),
            finalized: decode_bool(self.finalized)?,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct PortfolioSourceDomainV16Account {
    pub source_claim_market_id: V16PodU64,
    pub source_claim_bound_num: V16PodU128,
    pub source_claim_liened_num: V16PodU128,
    pub source_claim_counterparty_liened_num: V16PodU128,
    pub source_claim_insurance_liened_num: V16PodU128,
    pub source_lien_effective_reserved: V16PodU128,
    pub source_lien_counterparty_backing_num: V16PodU128,
    pub source_lien_insurance_backing_num: V16PodU128,
    pub source_lien_fee_last_slot: V16PodU64,
    pub source_claim_impaired_num: V16PodU128,
    pub source_lien_impaired_effective_reserved: V16PodU128,
}

impl PortfolioSourceDomainV16Account {}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct PortfolioAccountV16Account {
    pub provenance_header: ProvenanceHeaderV16Account,
    pub owner: [u8; 32],
    pub capital: V16PodU128,
    pub pnl: V16PodI128,
    pub reserved_pnl: V16PodU128,
    pub fee_credits: V16PodI128,
    pub cancel_deposit_escrow: V16PodU128,
    pub last_fee_slot: V16PodU64,
    pub active_bitmap: [V16PodU64; V16_ACTIVE_BITMAP_WORDS],
    pub legs: [PortfolioLegV16Account; V16_MAX_PORTFOLIO_ASSETS_N],
    pub health_cert: HealthCertV16Account,
    pub stale_state: u8,
    pub b_stale_state: u8,
    pub rebalance_lock: u8,
    pub liquidation_lock: u8,
    pub close_progress: CloseProgressLedgerV16Account,
    pub resolved_payout_receipt: ResolvedPayoutReceiptV16Account,
}

impl Default for PortfolioAccountV16Account {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

impl PortfolioAccountV16Account {
    pub fn try_empty(header: ProvenanceHeaderV16Account) -> V16Result<Self> {
        let owner = header.try_to_runtime()?.owner;
        let mut legs = [PortfolioLegV16Account::default(); V16_MAX_PORTFOLIO_ASSETS_N];
        let empty_leg = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        let mut i = 0usize;
        while i < V16_MAX_PORTFOLIO_ASSETS_N {
            legs[i] = empty_leg;
            i += 1;
        }
        Ok(Self {
            provenance_header: header,
            owner,
            capital: V16PodU128::new(0),
            pnl: V16PodI128::new(0),
            reserved_pnl: V16PodU128::new(0),
            fee_credits: V16PodI128::new(0),
            cancel_deposit_escrow: V16PodU128::new(0),
            last_fee_slot: V16PodU64::new(0),
            active_bitmap: [V16PodU64::new(0); V16_ACTIVE_BITMAP_WORDS],
            legs,
            health_cert: HealthCertV16Account::default(),
            stale_state: encode_bool(false),
            b_stale_state: encode_bool(false),
            rebalance_lock: encode_bool(false),
            liquidation_lock: encode_bool(false),
            close_progress: CloseProgressLedgerV16Account::default(),
            resolved_payout_receipt: ResolvedPayoutReceiptV16Account::default(),
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountBSettlementChunkV16 {
    pub delta_b: u128,
    pub loss: u128,
    pub new_remainder: u128,
    pub remaining_after: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RiskScoreV16 {
    pub certified_liq_deficit: u128,
    pub unsettled_b_loss_bound: u128,
    pub stale_loss_bound: u128,
    pub gross_risk_notional: u128,
    pub active_leg_count: u32,
}

impl RiskScoreV16 {
    pub fn strictly_reduces_from(self, before: Self) -> bool {
        self < before
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionlessProgressOutcomeV16 {
    AccountCurrent,
    AccountBChunk(AccountBSettlementChunkV16),
    ResidualBooked(BResidualBookingOutcomeV16),
    RecoveryDeclared(PermissionlessRecoveryReasonV16),
}

pub fn risk_notional_ceil(abs_pos_q: u128, price: u64) -> V16Result<u128> {
    if abs_pos_q == 0 {
        return Ok(0);
    }
    if let Some(product) = abs_pos_q.checked_mul(price as u128) {
        let q = product / POS_SCALE;
        let r = product % POS_SCALE;
        return q
            .checked_add(u128::from(r != 0))
            .ok_or(V16Error::ArithmeticOverflow);
    }
    checked_mul_div_ceil_u256(
        U256::from_u128(abs_pos_q),
        U256::from_u128(price as u128),
        U256::from_u128(POS_SCALE),
    )
    .and_then(|v| v.try_into_u128())
    .ok_or(V16Error::ArithmeticOverflow)
}

pub fn account_equity_from_parts(capital: u128, pnl: i128, fee_credits: i128) -> V16Result<i128> {
    validate_non_min_i128(pnl)?;
    validate_fee_credits(fee_credits)?;
    let capital = i128::try_from(capital).map_err(|_| V16Error::ArithmeticOverflow)?;
    let fee_debt =
        i128::try_from(fee_credits.unsigned_abs()).map_err(|_| V16Error::ArithmeticOverflow)?;
    capital
        .checked_add(pnl)
        .and_then(|v| v.checked_sub(fee_debt))
        .ok_or(V16Error::ArithmeticOverflow)
}

fn position_delta_increases_risk(current: i128, delta_q: i128) -> V16Result<bool> {
    let next = current
        .checked_add(delta_q)
        .ok_or(V16Error::ArithmeticOverflow)?;
    validate_basis_or_zero(next)?;
    Ok(next.unsigned_abs() > current.unsigned_abs())
}

fn margin_requirement(notional: u128, bps: u64, floor: u128) -> V16Result<u128> {
    if notional == 0 {
        return Ok(0);
    }
    if let Some(product) = notional.checked_mul(bps as u128) {
        return Ok((product / MAX_MARGIN_BPS as u128).max(floor));
    }
    let raw = wide_mul_div_floor_u128(notional, bps as u128, MAX_MARGIN_BPS as u128);
    Ok(raw.max(floor))
}

fn trade_notional_floor(size_q: u128, exec_price: u64) -> V16Result<u128> {
    if size_q == 0 {
        return Ok(0);
    }
    if let Some(product) = size_q.checked_mul(exec_price as u128) {
        return Ok(product / POS_SCALE);
    }
    let (q, _) = mul_div_floor_u256_with_rem(
        U256::from_u128(size_q),
        U256::from_u128(exec_price as u128),
        U256::from_u128(POS_SCALE),
    );
    q.try_into_u128().ok_or(V16Error::ArithmeticOverflow)
}

#[cfg(kani)]
pub fn kani_trade_notional_floor(size_q: u128, exec_price: u64) -> V16Result<u128> {
    trade_notional_floor(size_q, exec_price)
}

fn checked_fee_bps(notional: u128, fee_bps: u64) -> V16Result<u128> {
    if notional == 0 || fee_bps == 0 {
        return Ok(0);
    }
    if let Some(product) = notional.checked_mul(fee_bps as u128) {
        let den = MAX_MARGIN_BPS as u128;
        let q = product / den;
        let r = product % den;
        return q
            .checked_add(u128::from(r != 0))
            .ok_or(V16Error::ArithmeticOverflow);
    }
    checked_mul_div_ceil_u256(
        U256::from_u128(notional),
        U256::from_u128(fee_bps as u128),
        U256::from_u128(MAX_MARGIN_BPS as u128),
    )
    .and_then(|v| v.try_into_u128())
    .ok_or(V16Error::ArithmeticOverflow)
}

#[cfg(kani)]
pub fn kani_checked_fee_bps(notional: u128, fee_bps: u64) -> V16Result<u128> {
    checked_fee_bps(notional, fee_bps)
}

fn checked_i128_mul(a: i128, b: i128) -> V16Result<i128> {
    let out = a.checked_mul(b).ok_or(V16Error::ArithmeticOverflow)?;
    validate_non_min_i128(out)?;
    Ok(out)
}

fn add_non_min_i128(a: i128, b: i128) -> V16Result<i128> {
    let out = a.checked_add(b).ok_or(V16Error::ArithmeticOverflow)?;
    validate_non_min_i128(out)?;
    Ok(out)
}

fn adjust_u128(current: u128, old: u128, new: u128) -> V16Result<u128> {
    if new >= old {
        current
            .checked_add(new - old)
            .ok_or(V16Error::ArithmeticOverflow)
    } else {
        current
            .checked_sub(old - new)
            .ok_or(V16Error::CounterUnderflow)
    }
}

#[cfg(kani)]
pub fn kani_adjust_u128(current: u128, old: u128, new: u128) -> V16Result<u128> {
    adjust_u128(current, old, new)
}

fn encode_bool(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn decode_bool(value: u8) -> V16Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_side(value: SideV16) -> u8 {
    match value {
        SideV16::Long => 0,
        SideV16::Short => 1,
    }
}

fn decode_side(value: u8) -> V16Result<SideV16> {
    match value {
        0 => Ok(SideV16::Long),
        1 => Ok(SideV16::Short),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_side_mode(value: SideModeV16) -> u8 {
    match value {
        SideModeV16::Normal => 0,
        SideModeV16::DrainOnly => 1,
        SideModeV16::ResetPending => 2,
    }
}

fn decode_side_mode(value: u8) -> V16Result<SideModeV16> {
    match value {
        0 => Ok(SideModeV16::Normal),
        1 => Ok(SideModeV16::DrainOnly),
        2 => Ok(SideModeV16::ResetPending),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_asset_lifecycle(value: AssetLifecycleV16) -> u8 {
    match value {
        AssetLifecycleV16::Disabled => 0,
        AssetLifecycleV16::PendingActivation => 1,
        AssetLifecycleV16::Active => 2,
        AssetLifecycleV16::DrainOnly => 3,
        AssetLifecycleV16::Retired => 4,
        AssetLifecycleV16::Recovery => 5,
    }
}

fn decode_asset_lifecycle(value: u8) -> V16Result<AssetLifecycleV16> {
    match value {
        0 => Ok(AssetLifecycleV16::Disabled),
        1 => Ok(AssetLifecycleV16::PendingActivation),
        2 => Ok(AssetLifecycleV16::Active),
        3 => Ok(AssetLifecycleV16::DrainOnly),
        4 => Ok(AssetLifecycleV16::Retired),
        5 => Ok(AssetLifecycleV16::Recovery),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_market_mode(value: MarketModeV16) -> u8 {
    match value {
        MarketModeV16::Live => 0,
        MarketModeV16::Resolved => 1,
        MarketModeV16::Recovery => 2,
    }
}

fn decode_market_mode(value: u8) -> V16Result<MarketModeV16> {
    match value {
        0 => Ok(MarketModeV16::Live),
        1 => Ok(MarketModeV16::Resolved),
        2 => Ok(MarketModeV16::Recovery),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_backing_bucket_status(value: BackingBucketStatusV16) -> u8 {
    match value {
        BackingBucketStatusV16::Empty => 0,
        BackingBucketStatusV16::Fresh => 1,
        BackingBucketStatusV16::Expired => 2,
        BackingBucketStatusV16::Impaired => 3,
    }
}

fn decode_backing_bucket_status(value: u8) -> V16Result<BackingBucketStatusV16> {
    match value {
        0 => Ok(BackingBucketStatusV16::Empty),
        1 => Ok(BackingBucketStatusV16::Fresh),
        2 => Ok(BackingBucketStatusV16::Expired),
        3 => Ok(BackingBucketStatusV16::Impaired),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn encode_recovery_reason(value: PermissionlessRecoveryReasonV16) -> u8 {
    match value {
        PermissionlessRecoveryReasonV16::BelowProgressFloor => 0,
        PermissionlessRecoveryReasonV16::BlockedSegmentHeadroomOrRepresentability => 1,
        PermissionlessRecoveryReasonV16::AccountBSettlementCannotProgress => 2,
        PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted => 3,
        PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress => 4,
        PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow => 5,
        PermissionlessRecoveryReasonV16::OracleOrTargetUnavailableByAuthenticatedPolicy => 6,
        PermissionlessRecoveryReasonV16::CounterOrEpochOverflowDeclaredRecovery => 7,
    }
}

fn decode_recovery_reason(value: u8) -> V16Result<PermissionlessRecoveryReasonV16> {
    match value {
        0 => Ok(PermissionlessRecoveryReasonV16::BelowProgressFloor),
        1 => Ok(PermissionlessRecoveryReasonV16::BlockedSegmentHeadroomOrRepresentability),
        2 => Ok(PermissionlessRecoveryReasonV16::AccountBSettlementCannotProgress),
        3 => Ok(PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted),
        4 => Ok(PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress),
        5 => Ok(PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow),
        6 => Ok(PermissionlessRecoveryReasonV16::OracleOrTargetUnavailableByAuthenticatedPolicy),
        7 => Ok(PermissionlessRecoveryReasonV16::CounterOrEpochOverflowDeclaredRecovery),
        _ => Err(V16Error::InvalidConfig),
    }
}

fn validate_basis_or_zero(basis_pos_q: i128) -> V16Result<()> {
    if basis_pos_q == 0 {
        Ok(())
    } else {
        validate_basis(basis_pos_q)
    }
}

fn signed_position(leg: PortfolioLegV16) -> i128 {
    if !leg.active {
        0
    } else {
        match leg.side {
            SideV16::Long => leg.basis_pos_q.unsigned_abs() as i128,
            SideV16::Short => -(leg.basis_pos_q.unsigned_abs() as i128),
        }
    }
}

fn opposite_side(side: SideV16) -> SideV16 {
    match side {
        SideV16::Long => SideV16::Short,
        SideV16::Short => SideV16::Long,
    }
}

fn fraction_ge(lhs_num: u128, lhs_den: u128, rhs_num: u128, rhs_den: u128) -> V16Result<bool> {
    if lhs_den == 0 || rhs_den == 0 {
        return Err(V16Error::InvalidConfig);
    }
    let lhs = U256::from_u128(lhs_num)
        .checked_mul(U256::from_u128(rhs_den))
        .ok_or(V16Error::ArithmeticOverflow)?;
    let rhs = U256::from_u128(rhs_num)
        .checked_mul(U256::from_u128(lhs_den))
        .ok_or(V16Error::ArithmeticOverflow)?;
    Ok(lhs >= rhs)
}

fn quarantine_remainder(remainder: &mut u128, dust: &mut u128) -> V16Result<()> {
    if *remainder == 0 {
        return Ok(());
    }
    let new_dust = dust
        .checked_add(*remainder)
        .ok_or(V16Error::ArithmeticOverflow)?;
    if new_dust >= SOCIAL_LOSS_DEN {
        return Err(V16Error::RecoveryRequired);
    }
    *dust = new_dust;
    *remainder = 0;
    Ok(())
}

fn validate_non_min_i128(v: i128) -> V16Result<()> {
    if v == i128::MIN {
        return Err(V16Error::ArithmeticOverflow);
    }
    Ok(())
}

fn validate_fee_credits(v: i128) -> V16Result<()> {
    validate_non_min_i128(v)?;
    if v > 0 {
        return Err(V16Error::InvalidLeg);
    }
    Ok(())
}

fn validate_basis(basis_pos_q: i128) -> V16Result<()> {
    if basis_pos_q == 0
        || basis_pos_q == i128::MIN
        || basis_pos_q.unsigned_abs() > MAX_POSITION_ABS_Q
    {
        return Err(V16Error::InvalidLeg);
    }
    Ok(())
}

fn validate_active_leg(leg: PortfolioLegV16) -> V16Result<()> {
    validate_non_min_i128(leg.k_snap)?;
    validate_non_min_i128(leg.f_snap)?;
    let current_loss_weight = if leg.basis_pos_q == 0 {
        0
    } else {
        validate_basis(leg.basis_pos_q)?;
        loss_weight_for_basis(leg.basis_pos_q.unsigned_abs(), leg.a_basis)?
    };
    if !(MIN_A_SIDE..=ADL_ONE).contains(&leg.a_basis)
        || leg.loss_weight == 0
        || leg.loss_weight < current_loss_weight
        || leg.loss_weight > SOCIAL_LOSS_DEN
        || leg.b_rem >= SOCIAL_LOSS_DEN
        || leg.b_epoch_snap != leg.epoch_snap
    {
        return Err(V16Error::InvalidLeg);
    }
    Ok(())
}

fn snapshot_epoch_bound_to_side(epoch_snap: u64, side_epoch: u64, mode: SideModeV16) -> bool {
    epoch_snap == side_epoch
        || (mode == SideModeV16::ResetPending && epoch_snap.checked_add(1) == Some(side_epoch))
}

fn leg_snapshots_bound_to_asset_side(asset: AssetStateV16, leg: PortfolioLegV16) -> bool {
    let (side_epoch, mode) = match leg.side {
        SideV16::Long => (asset.epoch_long, asset.mode_long),
        SideV16::Short => (asset.epoch_short, asset.mode_short),
    };
    snapshot_epoch_bound_to_side(leg.epoch_snap, side_epoch, mode)
        && snapshot_epoch_bound_to_side(leg.b_epoch_snap, side_epoch, mode)
}

fn same_side_risk_reduction_or_flat_obligation(current: i128, next: i128) -> bool {
    current != 0
        && (next == 0 || current.signum() == next.signum())
        && next.unsigned_abs() < current.unsigned_abs()
}

fn loss_weight_for_basis(abs_basis_q: u128, a_basis: u128) -> V16Result<u128> {
    if a_basis == 0 {
        return Err(V16Error::InvalidLeg);
    }
    checked_mul_div_ceil_u256(
        U256::from_u128(abs_basis_q),
        U256::from_u128(SOCIAL_WEIGHT_SCALE),
        U256::from_u128(a_basis),
    )
    .and_then(|v| v.try_into_u128())
    .ok_or(V16Error::ArithmeticOverflow)
}

fn scaled_adl_delta_fast(abs_basis_q: u128, a_basis: u128, then: i128, now: i128) -> Option<i128> {
    if abs_basis_q == 0 {
        return Some(0);
    }
    if a_basis != ADL_ONE {
        return None;
    }
    let adl_one_i = i128::try_from(ADL_ONE).ok()?;
    let delta = now.checked_sub(then)?;
    if delta % adl_one_i != 0 {
        return None;
    }
    let scaled_delta = delta / adl_one_i;
    let basis_i = i128::try_from(abs_basis_q).ok()?;
    let numerator = scaled_delta.checked_mul(basis_i)?;
    Some(floor_div_signed_conservative_i128(numerator, POS_SCALE))
}

#[cfg(kani)]
pub fn kani_scaled_adl_delta_fast(
    abs_basis_q: u128,
    a_basis: u128,
    then: i128,
    now: i128,
) -> Option<i128> {
    scaled_adl_delta_fast(abs_basis_q, a_basis, then, now)
}
