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
#[cfg(kani)]
pub const PORTFOLIO_SOURCE_DOMAIN_CAP: usize = 4;
#[cfg(not(kani))]
pub const PORTFOLIO_SOURCE_DOMAIN_CAP: usize = 2 * V16_MAX_PORTFOLIO_ASSETS_N;
pub const V16_ACTIVE_BITMAP_WORDS: usize = (V16_MAX_PORTFOLIO_ASSETS_N + 63) / 64;
pub type V16ActiveBitmap = [u64; V16_ACTIVE_BITMAP_WORDS];
pub const V16_EMPTY_ACTIVE_BITMAP: V16ActiveBitmap = [0; V16_ACTIVE_BITMAP_WORDS];
pub const V16_BACKING_BUCKETS_PER_DOMAIN: usize = 1;
// Bump whenever the on-chain account/header Pod layout changes (see the
// PortfolioAccountV16Account size assertion). 17: added funding flow counters.
pub const V16_LAYOUT_DISCRIMINATOR: u16 = 17;
pub const V16_ACCOUNT_VERSION: u16 = 1;
pub const BACKING_FEE_RATE_DEN_E9: u128 = 1_000_000_000;
pub const MAX_BACKING_FEE_RATE_E9_PER_SLOT: u64 = 1_000_000_000;
pub const MAX_BACKING_FEE_UTIL_BPS: u64 = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackingDomainFeeSplitV16 {
    pub lien_delta_atoms: u128,
    pub total_fee: u128,
    pub provider_fee: u128,
    pub insurance_fee: u128,
}

pub fn backing_domain_fee_split_for_lien_delta_num(
    lien_delta_num: u128,
    fee_bps: u16,
    insurance_share_bps: u16,
) -> V16Result<BackingDomainFeeSplitV16> {
    if fee_bps as u64 > MAX_MARGIN_BPS || insurance_share_bps as u64 > MAX_MARGIN_BPS {
        return Err(V16Error::InvalidConfig);
    }
    if lien_delta_num % BOUND_SCALE != 0 {
        return Err(V16Error::InvalidConfig);
    }
    let lien_delta_atoms = lien_delta_num / BOUND_SCALE;
    let total_fee = checked_fee_bps(lien_delta_atoms, fee_bps as u64)?;
    let insurance_fee = wide_mul_div_floor_u128(
        total_fee,
        insurance_share_bps as u128,
        MAX_MARGIN_BPS as u128,
    );
    let provider_fee = total_fee
        .checked_sub(insurance_fee)
        .ok_or(V16Error::CounterUnderflow)?;
    Ok(BackingDomainFeeSplitV16 {
        lien_delta_atoms,
        total_fee,
        provider_fee,
        insurance_fee,
    })
}

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

// Contract layer: an earnings withdrawal debits vault and bucket earnings in
// exact lockstep (never more than earned, never from elsewhere).
#[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128)>| match result {
    Ok((v, e)) => *v == vault.wrapping_sub(amount) && *e == bucket_earnings.wrapping_sub(amount),
    Err(_) => true,
}))]
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

fn health_cert_after_capital_debit(cert: HealthCertV16, amount: u128) -> V16Result<HealthCertV16> {
    let amount_i128 = i128::try_from(amount).map_err(|_| V16Error::ArithmeticOverflow)?;
    let next_equity = cert
        .certified_equity
        .checked_sub(amount_i128)
        .ok_or(V16Error::ArithmeticOverflow)?;
    if next_equity < 0 || (next_equity as u128) < cert.certified_initial_req {
        return Err(V16Error::LockActive);
    }
    Ok(HealthCertV16 {
        certified_equity: next_equity,
        certified_liq_deficit: if next_equity < 0 {
            next_equity.unsigned_abs()
        } else {
            cert.certified_maintenance_req
                .saturating_sub(next_equity as u128)
        },
        ..cert
    })
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

/// REALIZABILITY — the no-DoS auto-crank dispatch-seam invariant.
///
/// `RefreshAccount { asset_index: None }` is the UNIQUE [`AutoCrankPlanV16`] whose
/// dispatch hard-requires a caller-supplied oracle observation: there is no active
/// account leg from which to choose a committed asset, so it must accrue a NEW
/// price. `RefreshAccount { asset_index: Some(_) }` and EVERY other plan are
/// dispatchable from committed on-chain state alone — `SettleBChunk` ignores
/// price, `Liquidate` reads the current health cert, `DeclareRecovery` /
/// `CloseResolved` / `NoAction` take no price — so a keeper holding no fresh
/// observation can still drive the account forward (no liveness stall). A wrapper
/// may call this to decide whether it must source an oracle observation before
/// cranking.
///
/// The exhaustive match forces a conscious classification if a plan variant is
/// added. `proof_v16_auto_crank_refresh_is_unique_observation_requiring_plan` pins
/// this truth table, and the spec matrix
/// `v16_auto_crank_progress_realizable_without_observation_for_every_class` ties
/// it to the REAL dispatch for every reachable ActionableState class — so the
/// predicate cannot silently drift from behaviour. This is the seam that hid the
/// b-stale / committed-state-liquidation stall before the observation fallback.
pub fn auto_crank_plan_requires_caller_observation(plan: &AutoCrankPlanV16) -> bool {
    match plan {
        AutoCrankPlanV16::RefreshAccount { asset_index } => asset_index.is_none(),
        AutoCrankPlanV16::SettleBChunk { .. }
        | AutoCrankPlanV16::Liquidate { .. }
        | AutoCrankPlanV16::DeclareRecovery { .. }
        | AutoCrankPlanV16::CloseResolved
        | AutoCrankPlanV16::NoAction => false,
    }
}

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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
fn active_bitmap_set(bitmap: &mut V16ActiveBitmap, leg_slot_index: usize) -> V16Result<()> {
    if leg_slot_index >= V16_MAX_PORTFOLIO_ASSETS_N {
        return Err(V16Error::InvalidConfig);
    }
    let word = leg_slot_index / 64;
    let bit = leg_slot_index % 64;
    bitmap[word] |= 1u64 << bit;
    Ok(())
}

#[inline]
fn active_bitmap_clear(bitmap: &mut V16ActiveBitmap, leg_slot_index: usize) -> V16Result<()> {
    if leg_slot_index >= V16_MAX_PORTFOLIO_ASSETS_N {
        return Err(V16Error::InvalidConfig);
    }
    let word = leg_slot_index / 64;
    let bit = leg_slot_index % 64;
    bitmap[word] &= !(1u64 << bit);
    Ok(())
}

#[inline]
fn active_bitmap_with_cleared(
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

fn liquidation_risk_notional_ceil(abs_pos_q: u128, price: u64) -> V16Result<u128> {
    if abs_pos_q == 0 {
        return Ok(0);
    }
    if abs_pos_q > MAX_POSITION_ABS_Q || price > MAX_ORACLE_PRICE {
        return Err(V16Error::InvalidConfig);
    }
    let product = abs_pos_q * price as u128;
    (product / POS_SCALE)
        .checked_add(u128::from(product % POS_SCALE != 0))
        .ok_or(V16Error::ArithmeticOverflow)
}

fn liquidation_leg_maintenance_requirement(
    config: V16Config,
    abs_q: u128,
    side: SideV16,
    effective_price: u64,
    raw_target_price: u64,
) -> V16Result<u128> {
    if abs_q == 0 {
        return Ok(0);
    }
    if config.maintenance_margin_bps > MAX_MARGIN_BPS {
        return Err(V16Error::InvalidConfig);
    }
    let risk_notional = liquidation_risk_notional_ceil(abs_q, effective_price)?;
    let adverse_delta =
        V16Core::target_effective_lag_adverse_delta(side, effective_price, raw_target_price);
    let target_lag_penalty = liquidation_risk_notional_ceil(abs_q, adverse_delta)?;
    let base = ((risk_notional * config.maintenance_margin_bps as u128) / MAX_MARGIN_BPS as u128)
        .max(config.min_nonzero_mm_req);
    base.checked_add(target_lag_penalty)
        .ok_or(V16Error::ArithmeticOverflow)
}

fn liquidation_projected_healthy_after_close(
    config: V16Config,
    cert: HealthCertV16,
    capital: u128,
    pnl: i128,
    leg: PortfolioLegV16,
    effective_price: u64,
    raw_target_price: u64,
    fee_bps: u64,
    close_q: u128,
) -> V16Result<bool> {
    let old_abs_q = leg.basis_pos_q.unsigned_abs();
    if close_q == 0 || close_q > old_abs_q {
        return Ok(false);
    }
    let old_maintenance = liquidation_leg_maintenance_requirement(
        config,
        old_abs_q,
        leg.side,
        effective_price,
        raw_target_price,
    )?;
    let new_abs_q = old_abs_q - close_q;
    let new_maintenance = liquidation_leg_maintenance_requirement(
        config,
        new_abs_q,
        leg.side,
        effective_price,
        raw_target_price,
    )?;
    let fee_notional = liquidation_risk_notional_ceil(close_q, effective_price)?;
    let fee = liquidation_fee_for_close(
        fee_notional,
        fee_bps,
        config.min_liquidation_abs,
        config.liquidation_fee_cap,
        close_q == old_abs_q,
    )?;
    let charged_fee = if pnl >= 0 { fee.min(capital) } else { 0 };
    Ok(liquidation_projected_health_deficit_from_parts(
        cert.certified_equity,
        cert.certified_maintenance_req,
        old_maintenance,
        new_maintenance,
        charged_fee,
    )? == 0)
}

fn liquidation_partial_close_is_healthy(
    config: V16Config,
    cert: HealthCertV16,
    capital: u128,
    pnl: i128,
    leg: PortfolioLegV16,
    effective_price: u64,
    raw_target_price: u64,
    fee_bps: u64,
    close_q: u128,
) -> V16Result<bool> {
    match liquidation_projected_healthy_after_close(
        config,
        cert,
        capital,
        pnl,
        leg,
        effective_price,
        raw_target_price,
        fee_bps,
        close_q,
    ) {
        // A partial close below the configured absolute fee floor is not an
        // admissible liquidation chunk. Every other error remains fail-closed.
        Err(V16Error::NonProgress) => Ok(false),
        result => result,
    }
}

fn min_abs_q_for_risk_notional_at_least(
    risk_notional: u128,
    effective_price: u64,
) -> V16Result<u128> {
    if risk_notional == 0 {
        return Ok(0);
    }
    if effective_price == 0 {
        return Err(V16Error::InvalidConfig);
    }
    let numerator = risk_notional - 1;
    if let Some(product) = numerator.checked_mul(POS_SCALE) {
        return (product / effective_price as u128)
            .checked_add(1)
            .ok_or(V16Error::ArithmeticOverflow);
    }
    U256::from_u128(numerator)
        .checked_mul(U256::from_u128(POS_SCALE))
        .and_then(|v| v.checked_div(U256::from_u128(effective_price as u128)))
        .and_then(|v| v.try_into_u128())
        .and_then(|v| v.checked_add(1))
        .ok_or(V16Error::ArithmeticOverflow)
}

fn liquidation_partial_search_hi(
    config: V16Config,
    old_abs_q: u128,
    effective_price: u64,
) -> V16Result<u128> {
    if config.maintenance_margin_bps == 0 {
        return Ok(0);
    }
    let floor_exit_notional = V16Config::checked_mul_div_ceil_to_u128(
        config.min_nonzero_mm_req,
        MAX_MARGIN_BPS as u128,
        config.maintenance_margin_bps as u128,
    )?;
    let floor_exit_q = min_abs_q_for_risk_notional_at_least(floor_exit_notional, effective_price)?;
    Ok(old_abs_q.saturating_sub(floor_exit_q))
}

fn liquidation_projected_health_deficit_from_parts(
    certified_equity: i128,
    certified_maintenance_req: u128,
    old_leg_maintenance: u128,
    new_leg_maintenance: u128,
    charged_fee: u128,
) -> V16Result<u128> {
    let post_maintenance = certified_maintenance_req
        .checked_sub(old_leg_maintenance)
        .and_then(|v| v.checked_add(new_leg_maintenance))
        .ok_or(V16Error::ArithmeticOverflow)?;
    let charged_fee_i128 = i128::try_from(charged_fee).map_err(|_| V16Error::ArithmeticOverflow)?;
    let post_equity = certified_equity
        .checked_sub(charged_fee_i128)
        .ok_or(V16Error::ArithmeticOverflow)?;
    if post_equity < 0 {
        return post_maintenance
            .checked_add(post_equity.unsigned_abs())
            .ok_or(V16Error::ArithmeticOverflow);
    }
    Ok(post_maintenance.saturating_sub(post_equity as u128))
}

fn liquidation_engine_close_request_q(
    config: V16Config,
    cert: HealthCertV16,
    capital: u128,
    pnl: i128,
    leg: PortfolioLegV16,
    effective_price: u64,
    raw_target_price: u64,
    fee_bps: u64,
) -> V16Result<u128> {
    let old_abs_q = leg.basis_pos_q.unsigned_abs();
    if old_abs_q == 0 {
        return Err(V16Error::InvalidLeg);
    }
    if cert.certified_equity < 0 || pnl < 0 {
        return Ok(old_abs_q);
    }
    if !liquidation_projected_healthy_after_close(
        config,
        cert,
        capital,
        pnl,
        leg,
        effective_price,
        raw_target_price,
        fee_bps,
        old_abs_q,
    )? {
        return Ok(old_abs_q);
    }

    // The absolute maintenance floor makes projected health non-monotone: a
    // partial close can become healthy, unhealthy again as its fee grows while
    // maintenance is flat, then healthy on the full-close discontinuity. Search
    // only the proportional-margin prefix. Crossing into the floor region is a
    // deterministic dust/full-close policy.
    let partial_hi = liquidation_partial_search_hi(config, old_abs_q, effective_price)?;
    if partial_hi == 0
        || !liquidation_partial_close_is_healthy(
            config,
            cert,
            capital,
            pnl,
            leg,
            effective_price,
            raw_target_price,
            fee_bps,
            partial_hi,
        )?
    {
        return Ok(old_abs_q);
    }

    let mut lo = 1u128;
    let mut hi = partial_hi;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let healthy = liquidation_partial_close_is_healthy(
            config,
            cert,
            capital,
            pnl,
            leg,
            effective_price,
            raw_target_price,
            fee_bps,
            mid,
        )?;
        if healthy {
            hi = mid;
        } else {
            lo = mid.checked_add(1).ok_or(V16Error::ArithmeticOverflow)?;
        }
    }
    if liquidation_partial_close_is_healthy(
        config,
        cert,
        capital,
        pnl,
        leg,
        effective_price,
        raw_target_price,
        fee_bps,
        lo,
    )? {
        Ok(lo)
    } else {
        Ok(old_abs_q)
    }
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
    fn loss_stale_trade_scope_allowed(
        market_loss_stale_active: bool,
        trade_asset_loss_stale: bool,
        long_account_loss_stale_exposed: bool,
        short_account_loss_stale_exposed: bool,
    ) -> bool {
        market_loss_stale_active
            && !trade_asset_loss_stale
            && !long_account_loss_stale_exposed
            && !short_account_loss_stale_exposed
    }

    fn prepare_asset_recovery_transition(
        mut asset: AssetStateV16,
        asset_set_epoch: u64,
        risk_epoch: u64,
    ) -> V16Result<(AssetStateV16, u64, u64)> {
        match asset.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => {
                if asset.effective_price == 0 || asset.effective_price > MAX_ORACLE_PRICE {
                    return Err(V16Error::InvalidConfig);
                }
                let next_asset_set_epoch = asset_set_epoch
                    .checked_add(1)
                    .ok_or(V16Error::CounterOverflow)?;
                let next_risk_epoch = risk_epoch.checked_add(1).ok_or(V16Error::CounterOverflow)?;
                asset.lifecycle = AssetLifecycleV16::Recovery;
                asset.raw_oracle_target_price = asset.effective_price;
                Ok((asset, next_asset_set_epoch, next_risk_epoch))
            }
            AssetLifecycleV16::Recovery => Ok((asset, asset_set_epoch, risk_epoch)),
            _ => Err(V16Error::LockActive),
        }
    }

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

    // Contract layer: the support availability for a domain is EXACTLY the
    // unliened counterparty backing plus the unencumbered insurance
    // reservation — the quantity that caps every realize/cure consumption.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<u128>| match result {
        Ok(a) => state.fresh_reserved_backing_num >= state.valid_liened_backing_num
            && state.insurance_credit_reserved_num
                >= state.valid_liened_insurance_num.wrapping_add(state.impaired_liened_insurance_num)
            && *a == (state.fresh_reserved_backing_num.wrapping_sub(state.valid_liened_backing_num))
                + (state.insurance_credit_reserved_num
                    - state.valid_liened_insurance_num
                    - state.impaired_liened_insurance_num),
        Err(_) => true,
    }))]
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

    #[cfg(any(kani, feature = "fuzz"))]
    // Contract layer: granting a claim raises the bound and exact trackers by
    // exactly the granted faces (bound >= exact enforced at entry) and touches
    // nothing else in the domain ledger.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(exact_claim_num <= claim_bound_num))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<SourceCreditStateV16>| match result {
        Ok(s) => s.positive_claim_bound_num == source.positive_claim_bound_num.wrapping_add(claim_bound_num)
            && s.exact_positive_claim_num == source.exact_positive_claim_num.wrapping_add(exact_claim_num)
            && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num
            && s.valid_liened_backing_num == source.valid_liened_backing_num
            && s.spent_backing_num == source.spent_backing_num
            && s.credit_rate_num == source.credit_rate_num,
        Err(_) => true,
    }))]
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

    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(new_signed != 0))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(PortfolioLegV16, AssetStateV16)>| match result {
        Ok((l, a)) => {
            let old_abs = leg.basis_pos_q.unsigned_abs();
            let new_abs = new_signed.unsigned_abs();
            l.basis_pos_q == new_signed
                && l.loss_weight == (if preserve_pending_obligation_weight { leg.loss_weight } else { new_weight })
                && l.active == leg.active
                && l.asset_index == leg.asset_index
                && l.market_id == leg.market_id
                && l.side == leg.side
                && l.a_basis == leg.a_basis
                && l.k_snap == leg.k_snap
                && l.f_snap == leg.f_snap
                && l.epoch_snap == leg.epoch_snap
                && l.b_snap == leg.b_snap
                && l.b_rem == leg.b_rem
                && l.b_epoch_snap == leg.b_epoch_snap
                && l.b_stale == leg.b_stale
                && l.stale == leg.stale
                && (match leg.side {
                    SideV16::Long => a.oi_eff_long_q
                            == asset.oi_eff_long_q.wrapping_sub(old_abs).wrapping_add(new_abs)
                        && a.oi_eff_short_q == asset.oi_eff_short_q
                        && a.loss_weight_sum_short == asset.loss_weight_sum_short
                        && a.loss_weight_sum_long == (if preserve_pending_obligation_weight {
                            asset.loss_weight_sum_long
                        } else {
                            asset.loss_weight_sum_long.wrapping_sub(leg.loss_weight).wrapping_add(new_weight)
                        }),
                    SideV16::Short => a.oi_eff_short_q
                            == asset.oi_eff_short_q.wrapping_sub(old_abs).wrapping_add(new_abs)
                        && a.oi_eff_long_q == asset.oi_eff_long_q
                        && a.loss_weight_sum_long == asset.loss_weight_sum_long
                        && a.loss_weight_sum_short == (if preserve_pending_obligation_weight {
                            asset.loss_weight_sum_short
                        } else {
                            asset.loss_weight_sum_short.wrapping_sub(leg.loss_weight).wrapping_add(new_weight)
                        }),
                })
                && a.market_id == asset.market_id
                && a.retired_slot == asset.retired_slot
                && a.lifecycle == asset.lifecycle
                && a.raw_oracle_target_price == asset.raw_oracle_target_price
                && a.effective_price == asset.effective_price
                && a.fund_px_last == asset.fund_px_last
                && a.slot_last == asset.slot_last
                && a.a_long == asset.a_long
                && a.a_short == asset.a_short
                && a.k_long == asset.k_long
                && a.k_short == asset.k_short
                && a.f_long_num == asset.f_long_num
                && a.f_short_num == asset.f_short_num
                && a.k_epoch_start_long == asset.k_epoch_start_long
                && a.k_epoch_start_short == asset.k_epoch_start_short
                && a.f_epoch_start_long_num == asset.f_epoch_start_long_num
                && a.f_epoch_start_short_num == asset.f_epoch_start_short_num
                && a.b_long_num == asset.b_long_num
                && a.b_short_num == asset.b_short_num
                && a.b_epoch_start_long_num == asset.b_epoch_start_long_num
                && a.b_epoch_start_short_num == asset.b_epoch_start_short_num
                && a.stored_pos_count_long == asset.stored_pos_count_long
                && a.stored_pos_count_short == asset.stored_pos_count_short
                && a.stale_account_count_long == asset.stale_account_count_long
                && a.stale_account_count_short == asset.stale_account_count_short
                && a.pending_obligation_count_long == asset.pending_obligation_count_long
                && a.pending_obligation_count_short == asset.pending_obligation_count_short
                && a.social_loss_remainder_long_num == asset.social_loss_remainder_long_num
                && a.social_loss_remainder_short_num == asset.social_loss_remainder_short_num
                && a.social_loss_dust_long_num == asset.social_loss_dust_long_num
                && a.social_loss_dust_short_num == asset.social_loss_dust_short_num
                && a.explicit_unallocated_loss_long == asset.explicit_unallocated_loss_long
                && a.explicit_unallocated_loss_short == asset.explicit_unallocated_loss_short
                && a.epoch_long == asset.epoch_long
                && a.epoch_short == asset.epoch_short
                && a.mode_long == asset.mode_long
                && a.mode_short == asset.mode_short
        },
        Err(_) => true,
    }))]
    pub(crate) fn kernel_resize_leg_same_side(
        mut leg: PortfolioLegV16,
        mut asset: AssetStateV16,
        new_signed: i128,
        new_weight: u128,
        preserve_pending_obligation_weight: bool,
    ) -> V16Result<(PortfolioLegV16, AssetStateV16)> {
        let old_abs = leg.basis_pos_q.unsigned_abs();
        let new_abs = new_signed.unsigned_abs();
        match leg.side {
            SideV16::Long => {
                asset.oi_eff_long_q = adjust_u128(asset.oi_eff_long_q, old_abs, new_abs)?;
                if !preserve_pending_obligation_weight {
                    asset.loss_weight_sum_long =
                        adjust_u128(asset.loss_weight_sum_long, leg.loss_weight, new_weight)?;
                }
            }
            SideV16::Short => {
                asset.oi_eff_short_q = adjust_u128(asset.oi_eff_short_q, old_abs, new_abs)?;
                if !preserve_pending_obligation_weight {
                    asset.loss_weight_sum_short =
                        adjust_u128(asset.loss_weight_sum_short, leg.loss_weight, new_weight)?;
                }
            }
        }
        leg.basis_pos_q = new_signed;
        if !preserve_pending_obligation_weight {
            leg.loss_weight = new_weight;
        }
        Ok((leg, asset))
    }

    /// PRODUCTION KERNEL (spec #37 gate): the trade-finalization initial-
    /// margin decision — an EXACT total decision contract: the gate admits
    /// precisely the states with a VALID certificate whose certified equity
    /// is nonnegative and covers the certified initial requirement, and
    /// rejects everything else. This is the verified-certificate gate the
    /// maker-exemption rule rides on.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<()>| {
        let admit = cert.valid
            && cert.certified_equity >= 0
            && (cert.certified_equity as u128) >= cert.certified_initial_req;
        match result {
            Ok(()) => admit,
            Err(_) => !admit,
        }
    }))]
    pub(crate) fn kernel_initial_margin_gate(cert: HealthCertV16) -> V16Result<()> {
        if !cert.valid {
            return Err(V16Error::Stale);
        }
        let equity = cert.certified_equity;
        if equity < 0 || (equity as u128) < cert.certified_initial_req {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    /// PRODUCTION KERNEL (spec #37 locked lane): the no-positive-credit
    /// margin decision — exact total decision over the locked-lane equity
    /// (capital + min(pnl,0) - |fee debt|) against the certified initial
    /// requirement: positive PnL credit can never satisfy IM under h-lock.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(
        pnl > i128::MIN && fee_credits > i128::MIN && capital < 1u128 << 100
    ))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<()>| match result {
        Ok(()) => {
            let equity = (capital as i128)
                .wrapping_add(if pnl < 0 { pnl } else { 0 })
                .wrapping_sub(fee_credits.unsigned_abs() as i128);
            equity >= 0 && (equity as u128) >= certified_initial_req
        },
        Err(_) => true,
    }))]
    pub(crate) fn kernel_locked_margin_gate(
        capital: u128,
        pnl: i128,
        fee_credits: i128,
        certified_initial_req: u128,
    ) -> V16Result<()> {
        let capital_i = i128::try_from(capital).map_err(|_| V16Error::ArithmeticOverflow)?;
        let fee_debt =
            i128::try_from(fee_credits.unsigned_abs()).map_err(|_| V16Error::ArithmeticOverflow)?;
        let equity = capital_i
            .checked_add(pnl.min(0))
            .ok_or(V16Error::ArithmeticOverflow)?
            .checked_sub(fee_debt)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if equity < 0 || (equity as u128) < certified_initial_req {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    /// PRODUCTION KERNEL (#37 batch projection): accumulate one fill's
    /// outcome into the running batch outcome. EXACT sum: fill_count += 1,
    /// fees and notional add by exactly the fill's amounts, the three flags
    /// are monotone-OR. Proving this is the batch-projection invariant — a
    /// batch outcome is exactly the fold of its per-fill outcomes, so a batch
    /// is equivalent to its sequence of single fills (no hidden extra work).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BatchTradeOutcomeV16, bool, bool, bool)>| match result {
        Ok((o, ri, lhsc, shsc)) =>
            o.fill_count == outcome.fill_count.wrapping_add(1)
            && o.fee_a == outcome.fee_a.wrapping_add(applied.fee_a)
            && o.fee_b == outcome.fee_b.wrapping_add(applied.fee_b)
            && o.notional == outcome.notional.wrapping_add(applied.notional)
            && *ri == (risk_increasing || applied.risk_increasing)
            && *lhsc == (long_has_source_claims || applied.long_has_source_claims)
            && *shsc == (short_has_source_claims || applied.short_has_source_claims),
        Err(_) => true,
    }))]
    pub(crate) fn kernel_accumulate_batch_trade(
        mut outcome: BatchTradeOutcomeV16,
        risk_increasing: bool,
        long_has_source_claims: bool,
        short_has_source_claims: bool,
        applied: TradeApplyOutcomeV16,
    ) -> V16Result<(BatchTradeOutcomeV16, bool, bool, bool)> {
        outcome.fill_count = outcome
            .fill_count
            .checked_add(1)
            .ok_or(V16Error::CounterOverflow)?;
        outcome.fee_a = outcome
            .fee_a
            .checked_add(applied.fee_a)
            .ok_or(V16Error::ArithmeticOverflow)?;
        outcome.fee_b = outcome
            .fee_b
            .checked_add(applied.fee_b)
            .ok_or(V16Error::ArithmeticOverflow)?;
        outcome.notional = outcome
            .notional
            .checked_add(applied.notional)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((
            outcome,
            risk_increasing | applied.risk_increasing,
            long_has_source_claims | applied.long_has_source_claims,
            short_has_source_claims | applied.short_has_source_claims,
        ))
    }

    /// PRODUCTION KERNEL (liveness rank): the close-progress ledger advance.
    /// Each partition category grows by exactly its delta; residual_remaining
    /// is recomputed to the exact partition identity and STRICTLY DECREASES by
    /// the booked total — the well-founded rank for close progress (a close
    /// terminates in at most gross_loss booked value); finalization is exactly
    /// residual == 0; over-booking fails closed; every immutable identity
    /// field is frozen.
    pub(crate) fn kernel_advance_close_ledger(
        mut ledger: CloseProgressLedgerV16,
        support_consumed: u128,
        junior_face_burned: u128,
        insurance_spent: u128,
        b_loss_booked: u128,
        explicit_loss_assigned: u128,
    ) -> V16Result<CloseProgressLedgerV16> {
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
            .ok_or(V16Error::ArithmeticOverflow)?;
        let progress = progress
            .checked_add(ledger.b_loss_booked)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let progress = progress
            .checked_add(ledger.explicit_loss_assigned)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if progress > total_loss {
            return Err(V16Error::ArithmeticOverflow);
        }
        ledger.residual_remaining = total_loss - progress;
        if ledger.residual_remaining == 0 {
            ledger.finalized = true;
        }
        Ok(ledger)
    }

    /// PRODUCTION KERNEL (value safety, roadmap S-A1/S-L2 foundation): the
    /// principal-settlement core of negative-PnL liquidation/close. Given an
    /// account's capital, the group junior total c_tot, and a (negative) pnl,
    /// pay the loss from principal FIRST: `paid = min(capital, |pnl|)`, and drop
    /// BOTH capital and c_tot by exactly `paid` (conservation — the principal
    /// debit leaves the junior pool by exactly what the account loses, nothing
    /// created or stranded). Pure scalar; the settle/close glue calls exactly
    /// this for the arithmetic.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(pnl <= 0 && pnl > i128::MIN))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128, u128, i128)>| match result {
        Ok((paid, new_capital, new_c_tot, new_pnl)) => {
            let loss = pnl.unsigned_abs();
            *paid == if capital < loss { capital } else { loss }
                // exact debit of capital and the junior total by the SAME paid
                && *new_capital == capital - *paid
                && *new_c_tot == c_tot - *paid
                // conservation: each layer drops by exactly paid (no leak)
                && new_capital.wrapping_add(*paid) == capital
                && new_c_tot.wrapping_add(*paid) == c_tot
                // loss is reduced by exactly the principal paid
                && *new_pnl == pnl + (*paid as i128)
                && *new_pnl <= 0
        }
        Err(_) => true,
    }))]
    pub(crate) fn kernel_settle_principal(
        capital: u128,
        c_tot: u128,
        pnl: i128,
    ) -> V16Result<(u128, u128, u128, i128)> {
        let loss = pnl.unsigned_abs();
        let paid = capital.min(loss);
        let new_capital = capital
            .checked_sub(paid)
            .ok_or(V16Error::CounterUnderflow)?;
        let new_c_tot = c_tot.checked_sub(paid).ok_or(V16Error::CounterUnderflow)?;
        let paid_i128 = i128::try_from(paid).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = pnl
            .checked_add(paid_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((paid, new_capital, new_c_tot, new_pnl))
    }

    /// PRODUCTION KERNEL (value safety, roadmap S-L2 insurance layer): the
    /// insurance-draw core of negative-PnL liquidation. Draw is capped by BOTH
    /// the remaining deficit and the DOMAIN's own available insurance —
    /// `used = min(residual, domain_available)` — so it never exceeds the
    /// deficit and never spends beyond this domain's budget (domain isolation).
    /// Conservation: the insurance pool drops by exactly `used` and the domain
    /// spent ledger rises by exactly `used`. Pure scalar; the consume glue calls
    /// exactly this for the arithmetic.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(pnl < 0 && pnl > i128::MIN))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128, u128, i128)>| match result {
        Ok((used, new_insurance, new_spent, new_pnl)) => {
            let residual = pnl.unsigned_abs();
            *used == if residual < domain_available { residual } else { domain_available }
                && *used <= domain_available   // never spends beyond the domain budget (S-L2 cap)
                && *used <= residual            // never exceeds the deficit
                && *new_insurance == insurance - *used   // pool drops by exactly used
                && *new_spent == spent + *used           // spent rises by exactly used
                && new_insurance.wrapping_add(*used) == insurance
                && *new_pnl == pnl + (*used as i128)     // loss reduced by exactly used
                && *new_pnl <= 0                          // cannot overshoot positive (used <= residual)
        }
        Err(_) => true,
    }))]
    pub(crate) fn kernel_consume_insurance_layer(
        domain_available: u128,
        insurance: u128,
        spent: u128,
        pnl: i128,
    ) -> V16Result<(u128, u128, u128, i128)> {
        let residual = pnl.unsigned_abs();
        let used = residual.min(domain_available);
        let new_insurance = insurance
            .checked_sub(used)
            .ok_or(V16Error::CounterUnderflow)?;
        let new_spent = spent
            .checked_add(used)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let used_i128 = i128::try_from(used).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = pnl
            .checked_add(used_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((used, new_insurance, new_spent, new_pnl))
    }

    /// PRODUCTION KERNEL (value safety, roadmap S-C2 / 3A.3): the resolved-payout
    /// draining step. Given the (already-computed) claimable and the vault,
    /// `payout = min(claimable, vault)` and the vault drops by EXACTLY payout —
    /// the pool is never overdrawn (payout <= vault) and never leaks (new_vault +
    /// payout == vault). Pure scalar; the topup glue calls exactly this for the
    /// draining arithmetic (claimable's wide rate-division stays upstream and is
    /// covered by reference-model conformance fuzz).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &(u128, u128)| {
        let (payout, new_vault) = *result;
        payout == if claimable < vault { claimable } else { vault }
            && payout <= claimable
            && payout <= vault
            && new_vault == vault - payout
            && new_vault.wrapping_add(payout) == vault   // conservation: no leak
    }))]
    pub(crate) fn kernel_resolved_payout_step(claimable: u128, vault: u128) -> (u128, u128) {
        let payout = claimable.min(vault);
        let new_vault = vault - payout; // payout <= vault by construction
        (payout, new_vault)
    }

    /// PRODUCTION KERNEL (roadmap 3A.1 trade spine): classify a position change
    /// from (current signed, new signed) into its leg route — the EXACT total
    /// decision the position-delta body dispatches on. `delta != 0` is enforced
    /// upstream, so `current == 0 => new != 0`. Pure scalar.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &PositionRouteV16| {
        match r {
            PositionRouteV16::Attach => current == 0,
            PositionRouteV16::Clear => current != 0 && new == 0,
            PositionRouteV16::Flip => current != 0 && new != 0 && current.signum() != new.signum(),
            PositionRouteV16::Resize => current != 0 && new != 0 && current.signum() == new.signum(),
        }
    }))]
    pub(crate) fn kernel_classify_position_delta(current: i128, new: i128) -> PositionRouteV16 {
        if current == 0 {
            PositionRouteV16::Attach
        } else if new == 0 {
            PositionRouteV16::Clear
        } else if current.signum() != new.signum() {
            PositionRouteV16::Flip
        } else {
            PositionRouteV16::Resize
        }
    }

    /// PRODUCTION KERNEL (roadmap 3A.2 risk-reduction / S-L3, A5.dec rank): the
    /// position-reduction core of liquidation/rebalance. Clamps the requested
    /// close to the leg and produces the toward-zero signed delta:
    /// `reduce_q = min(requested, |pre|)`, `delta = -reduce_q` (long) / `+reduce_q`
    /// (short). Proves the rank component STRICTLY decreases: |pre + delta| ==
    /// |pre| - reduce_q (never over-closes, never flips), and a full close clears
    /// the position to zero. Pure scalar; the reduce glue calls exactly this.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(pre_basis_signed > i128::MIN
        && match side { SideV16::Long => pre_basis_signed >= 0, SideV16::Short => pre_basis_signed <= 0 }))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, i128)>| match result {
        Ok((reduce_q, delta)) => {
            let pre_abs = pre_basis_signed.unsigned_abs();
            let new_signed = pre_basis_signed.wrapping_add(*delta);
            *reduce_q == if requested < pre_abs { requested } else { pre_abs }
                && *reduce_q <= pre_abs                              // never over-closes
                && new_signed.unsigned_abs() == pre_abs - *reduce_q  // reduced by EXACTLY reduce_q
                && new_signed.unsigned_abs() <= pre_abs              // rank non-increase
                && (*reduce_q != pre_abs || new_signed == 0)         // full close clears
                // STRICT progress (3C): a nonzero request on a nonzero position
                // always closes some and strictly shrinks the risk component.
                && (!(pre_abs > 0 && requested > 0)
                    || (*reduce_q > 0 && new_signed.unsigned_abs() < pre_abs))
        }
        Err(_) => true,
    }))]
    pub(crate) fn kernel_reduce_position_delta(
        pre_basis_signed: i128,
        side: SideV16,
        requested: u128,
    ) -> V16Result<(u128, i128)> {
        let pre_abs = pre_basis_signed.unsigned_abs();
        let reduce_q = requested.min(pre_abs);
        let reduce_i128 = i128::try_from(reduce_q).map_err(|_| V16Error::ArithmeticOverflow)?;
        let delta = match side {
            SideV16::Long => reduce_i128
                .checked_neg()
                .ok_or(V16Error::ArithmeticOverflow)?,
            SideV16::Short => reduce_i128,
        };
        Ok((reduce_q, delta))
    }

    /// PRODUCTION KERNEL (engine.md asset self-selection): the bounded first-match
    /// leg/asset scan. Given a per-leg actionability flag array (the caller sets
    /// `flags[i]` to the production predicate for slot i — e.g. an active b-stale
    /// leg, or an active leg eligible for liquidation), return the FIRST actionable
    /// slot, so the engine — not the caller — picks the asset for the selected
    /// continuation. PROVES (engine.md proof targets): the returned slot is IN
    /// RANGE and ACTIONABLE (`flags[slot]`), it is the FIRST such slot, and the
    /// scan is COMPLETE (returns Some IFF any flag is set). Bounded by
    /// V16_MAX_PORTFOLIO_ASSETS_N (no unbounded loop). Pure.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &Option<usize>| {
        match r {
            Some(i) => {
                *i < V16_MAX_PORTFOLIO_ASSETS_N
                    && flags[*i]
                    && {
                        // first match: every earlier slot is not actionable
                        let mut j = 0;
                        let mut earlier_clear = true;
                        while j < *i {
                            if flags[j] {
                                earlier_clear = false;
                            }
                            j += 1;
                        }
                        earlier_clear
                    }
            }
            None => {
                // completeness: no slot is actionable
                let mut j = 0;
                let mut any = false;
                while j < V16_MAX_PORTFOLIO_ASSETS_N {
                    if flags[j] {
                        any = true;
                    }
                    j += 1;
                }
                !any
            }
        }
    }))]
    pub(crate) fn first_actionable_slot(
        flags: [bool; V16_MAX_PORTFOLIO_ASSETS_N],
    ) -> Option<usize> {
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            if flags[slot] {
                return Some(slot);
            }
            slot += 1;
        }
        None
    }

    /// PRODUCTION KERNEL (engine.md selection semantics): from the actionable
    /// summary and the ENGINE-selected assets, choose the single highest-priority
    /// bounded plan, carrying the engine-chosen asset (the caller chooses neither
    /// the action nor the asset). PROVES: TOTALITY (an actionable account yields a
    /// non-NoAction plan), PRIORITY DETERMINISM (the documented order: recovery >
    /// resolved-close > b-stale settle > liquidate > refresh), and SELECTED-ASSET
    /// fidelity (SettleBChunk/Liquidate carry exactly the provided engine-selected
    /// slot). `pending_close` is classifier-unreachable (build_actionable_summary
    /// never sets it — a leg-bearing pending close is liquidatable; see the
    /// AdvanceClose note), so it is required absent here. Pure.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(!summary.pending_close))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &AutoCrankPlanV16| {
        let recovery = summary.expired_close || summary.recovery_eligible;
        match r {
            AutoCrankPlanV16::NoAction => !summary.is_actionable(),
            AutoCrankPlanV16::DeclareRecovery { reason } => recovery && *reason == recovery_reason,
            AutoCrankPlanV16::CloseResolved => !recovery && summary.resolved_winner,
            AutoCrankPlanV16::SettleBChunk { asset_index } =>
                !recovery && !summary.resolved_winner && summary.b_stale && *asset_index == b_stale_slot,
            AutoCrankPlanV16::Liquidate { asset_index } =>
                !recovery && !summary.resolved_winner && !summary.b_stale && summary.liquidatable
                    && *asset_index == liq_slot,
            AutoCrankPlanV16::RefreshAccount { asset_index } =>
                !recovery && !summary.resolved_winner && !summary.b_stale && !summary.liquidatable
                    && summary.stale && *asset_index == refresh_asset,
        }
    }))]
    pub(crate) fn select_auto_crank_plan(
        summary: ActionableSummaryV16,
        b_stale_slot: usize,
        liq_slot: usize,
        refresh_asset: Option<usize>,
        recovery_reason: PermissionlessRecoveryReasonV16,
    ) -> AutoCrankPlanV16 {
        if summary.expired_close || summary.recovery_eligible {
            AutoCrankPlanV16::DeclareRecovery {
                reason: recovery_reason,
            }
        } else if summary.resolved_winner {
            AutoCrankPlanV16::CloseResolved
        } else if summary.b_stale {
            AutoCrankPlanV16::SettleBChunk {
                asset_index: b_stale_slot,
            }
        } else if summary.liquidatable {
            AutoCrankPlanV16::Liquidate {
                asset_index: liq_slot,
            }
        } else if summary.stale {
            AutoCrankPlanV16::RefreshAccount {
                asset_index: refresh_asset,
            }
        } else {
            AutoCrankPlanV16::NoAction
        }
    }

    /// PRODUCTION FIDELITY BUILDER (roadmap 3C): map a trade request + config to
    /// the scalar request-guard summary. Each flag is EXACTLY the corresponding
    /// validate_trade_request leaf, so the public validator's accept/reject
    /// decision equals `all_pass()` — the summary faithfully represents the real
    /// guards (no hidden scalar guard outside it). Pure scalar; validate_trade_
    /// request calls this.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|s: &TradeRequestGuardSummaryV16| {
        s.asset_configured == ((request.asset_index as u64) < max_market_slots as u64)
            && s.size_nonzero == (request.size_q != 0)
            && s.size_in_cap == (request.size_q.unsigned_abs() <= MAX_TRADE_SIZE_Q)
            && s.price_nonzero == (request.exec_price != 0)
            && s.price_in_range == (request.exec_price <= MAX_ORACLE_PRICE)
            && s.fee_bps_in_cap == (request.fee_bps <= max_trading_fee_bps)
    }))]
    pub(crate) fn build_trade_request_guard_summary(
        request: TradeRequestV16,
        max_market_slots: u32,
        max_trading_fee_bps: u64,
    ) -> TradeRequestGuardSummaryV16 {
        TradeRequestGuardSummaryV16 {
            asset_configured: (request.asset_index as u64) < max_market_slots as u64,
            size_nonzero: request.size_q != 0,
            size_in_cap: request.size_q.unsigned_abs() <= MAX_TRADE_SIZE_Q,
            price_nonzero: request.exec_price != 0,
            price_in_range: request.exec_price <= MAX_ORACLE_PRICE,
            fee_bps_in_cap: request.fee_bps <= max_trading_fee_bps,
        }
    }

    /// PRODUCTION FIDELITY (roadmap 3C step 2, NB1 accounts_current leaf): a
    /// health certificate is current/certifiable for a favorable action IFF it
    /// is valid, all four market epochs (oracle/funding/risk/asset-set) match the
    /// cert's, and the cert was issued against the account's current active
    /// bitmap. ensure_favorable_action_current_certificate calls this, so the
    /// `accounts_current` summary flag faithfully represents the production
    /// currentness gate (no hidden staleness reject outside it). Pure scalar.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &bool| {
        *result
            == (cert.valid
                && cert.cert_oracle_epoch == oracle_epoch
                && cert.cert_funding_epoch == funding_epoch
                && cert.cert_risk_epoch == risk_epoch
                && cert.cert_asset_set_epoch == asset_set_epoch
                && cert.active_bitmap_at_cert == account_bitmap)
    }))]
    pub(crate) fn kernel_cert_is_current(
        cert: HealthCertV16,
        oracle_epoch: u64,
        funding_epoch: u64,
        risk_epoch: u64,
        asset_set_epoch: u64,
        account_bitmap: V16ActiveBitmap,
    ) -> bool {
        cert.valid
            && cert.cert_oracle_epoch == oracle_epoch
            && cert.cert_funding_epoch == funding_epoch
            && cert.cert_risk_epoch == risk_epoch
            && cert.cert_asset_set_epoch == asset_set_epoch
            && cert.active_bitmap_at_cert == account_bitmap
    }

    /// PRODUCTION FIDELITY BUILDER (roadmap 3C step 4, actionable-state
    /// classifier): assemble the ActionableState summary from the seven evaluated
    /// per-class eligibility signals. build_actionable_summary computes each
    /// signal from real account/market state (cert currentness, b-stale leg,
    /// close-ledger active/expired, certified liquidation deficit, resolved-
    /// insolvency recovery predicate, resolved positive-payout readiness) and
    /// calls this; the contract pins each summary flag to its signal so the
    /// classifier faithfully feeds the proven select_progress_witness. Pure.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &ActionableSummaryV16| {
        r.stale == stale
            && r.b_stale == b_stale
            && r.pending_close == pending_close
            && r.expired_close == expired_close
            && r.liquidatable == liquidatable
            && r.recovery_eligible == recovery_eligible
            && r.resolved_winner == resolved_winner
    }))]
    pub(crate) fn actionable_summary_from_signals(
        stale: bool,
        b_stale: bool,
        pending_close: bool,
        expired_close: bool,
        liquidatable: bool,
        recovery_eligible: bool,
        resolved_winner: bool,
    ) -> ActionableSummaryV16 {
        ActionableSummaryV16 {
            stale,
            b_stale,
            pending_close,
            expired_close,
            liquidatable,
            recovery_eligible,
            resolved_winner,
        }
    }

    /// PRODUCTION KERNEL (roadmap 3B.6, Pillar S/L S-A1 cap): the social-loss
    /// chunk cap — the bookable chunk is `min(residual_remaining, public chunk
    /// cap)`, so a single step books NO MORE than the outstanding residual and
    /// NO MORE than the per-call public cap. Pure scalar (the delta_b/remainder
    /// SPLIT divides by weight_sum and is discharged by reference-model fuzz —
    /// social_loss_book_split conformance — since the symbolic u128 division
    /// resists Kani). The capacity glue calls exactly this for the cap.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &u128| {
        *result == if residual_remaining < public_chunk_cap { residual_remaining } else { public_chunk_cap }
            && *result <= residual_remaining   // never books beyond the residual
            && *result <= public_chunk_cap      // never books beyond the per-call cap
    }))]
    pub(crate) fn kernel_social_loss_chunk_cap(
        residual_remaining: u128,
        public_chunk_cap: u128,
    ) -> u128 {
        residual_remaining.min(public_chunk_cap)
    }

    /// PRODUCTION KERNEL (roadmap workstream B.3, social-loss shell): book one
    /// live social-loss residual chunk to the loss side's B-index. No-LoF shell
    /// properties (the division (delta_b,new_rem) is via social_loss_book_split,
    /// discharged by reference-model fuzz; these properties hold for ANY split):
    /// when a chunk is booked (Some), it books EXACTLY engine_chunk
    /// (booked_loss==engine_chunk), conserves the residual
    /// (remaining_after+booked_loss==residual_remaining), assigns NO explicit loss
    /// on the live path (explicit_loss==0), and makes real B-rank progress
    /// (delta_b>0). Booking only ADDS to the loss side's b_num (monotone, never
    /// burns value) and is SIDE-ISOLATED: the opposite (non-loss) side's B-index
    /// and social-loss remainder are never touched (no cross-side contamination);
    /// it operates on a single asset, so there is structurally no cross-asset
    /// mutation either.
    #[cfg_attr(all(kani, feature = "contracts"), kani::modifies(asset))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &V16Result<Option<BResidualBookingOutcomeV16>>| {
        // SIDE ISOLATION + MONOTONICITY (holds on every path): the loss side's
        // B-index only grows; the OTHER side's B-index + remainder are unchanged.
        let isolated = match opp {
            SideV16::Long => {
                asset.b_long_num >= old(asset.b_long_num)
                    && asset.b_short_num == old(asset.b_short_num)
                    && asset.social_loss_remainder_short_num
                        == old(asset.social_loss_remainder_short_num)
            }
            SideV16::Short => {
                asset.b_short_num >= old(asset.b_short_num)
                    && asset.b_long_num == old(asset.b_long_num)
                    && asset.social_loss_remainder_long_num
                        == old(asset.social_loss_remainder_long_num)
            }
        };
        isolated
            && match r {
                Ok(Some(o)) => {
                    o.explicit_loss == 0
                        && o.booked_loss == engine_chunk
                        && o.delta_b > 0
                        && o.remaining_after.checked_add(o.booked_loss)
                            == Some(residual_remaining)
                }
                _ => true,
            }
    }))]
    pub(crate) fn apply_bankruptcy_residual_chunk_to_loss_side(
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
        // PRODUCTION HELPER: the booking division (delta_b, new_rem).
        let (delta_b, new_rem) = social_loss_book_split(engine_chunk, rem, weight_sum)?;
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

    /// PRODUCTION KERNEL (roadmap workstream B.2, bankruptcy-residual conservation
    /// COMPOSITION): given residual_remaining>0 and `booked` = the (already
    /// conservation-proven) result of apply_bankruptcy_residual_chunk_to_loss_side
    /// (Some when a chunk was booked, None when it could not be — no loss-side
    /// weight, no B-headroom, or no positive delta), decide the step's terminal
    /// outcome. A booked chunk's outcome is carried through; an unbookable residual
    /// is recorded ENTIRELY as explicit loss in a resolved market, else it signals
    /// DeclareRecovery. PROVES residual conservation for EVERY Outcome path
    /// (booked_loss+explicit_loss+remaining_after == residual_remaining), composing
    /// the leaf booking contract into the step decision — no value is created or
    /// lost regardless of which branch is taken. Pure.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(
        match booked {
            Some(o) => o.booked_loss.checked_add(o.explicit_loss)
                .and_then(|v| v.checked_add(o.remaining_after)) == Some(residual_remaining),
            None => true,
        }
    ))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &BResidualStepV16| {
        match r {
            BResidualStepV16::Outcome(o) => {
                o.booked_loss.checked_add(o.explicit_loss)
                    .and_then(|v| v.checked_add(o.remaining_after))
                    == Some(residual_remaining)
            }
            BResidualStepV16::DeclareRecovery => true,
        }
    }))]
    pub(crate) fn kernel_bresidual_step(
        residual_remaining: u128,
        booked: Option<BResidualBookingOutcomeV16>,
        resolved: bool,
    ) -> BResidualStepV16 {
        match booked {
            Some(o) => BResidualStepV16::Outcome(o),
            None => {
                if resolved {
                    BResidualStepV16::Outcome(BResidualBookingOutcomeV16 {
                        booked_loss: 0,
                        explicit_loss: residual_remaining,
                        delta_b: 0,
                        remaining_after: 0,
                    })
                } else {
                    BResidualStepV16::DeclareRecovery
                }
            }
        }
    }

    /// PRODUCTION KERNEL (roadmap workstream B.2, resolved-bankruptcy PnL
    /// settlement): reduce an account's NEGATIVE PnL by the loss the residual
    /// booking just absorbed. `cleared = min(booked_loss + explicit_loss, |pnl|)`
    /// — capped at the outstanding loss so it can never exceed it — and
    /// new_pnl = pnl + cleared. PROVES the loss only SHRINKS toward zero
    /// (pnl <= new_pnl <= 0; never over-cleared into a spurious positive credit)
    /// and its magnitude drops by EXACTLY cleared (|new_pnl| == |pnl| - cleared).
    /// Pure scalar; the resolved-bankruptcy settle glue calls exactly this for the
    /// PnL update after book_bankruptcy_residual_chunk_for_account_core.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(pnl < 0 && pnl > i128::MIN))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|r: &V16Result<i128>| match r {
        Ok(new_pnl) => {
            let loss = pnl.unsigned_abs();
            let cleared = booked_loss.saturating_add(explicit_loss).min(loss);
            *new_pnl == pnl + (cleared as i128)
                && *new_pnl <= 0
                && *new_pnl >= pnl
                && new_pnl.unsigned_abs() == loss - cleared
        }
        Err(_) => true,
    }))]
    pub(crate) fn kernel_settle_resolved_pnl_after_booking(
        pnl: i128,
        booked_loss: u128,
        explicit_loss: u128,
    ) -> V16Result<i128> {
        let loss = pnl.unsigned_abs();
        let cleared = booked_loss
            .checked_add(explicit_loss)
            .ok_or(V16Error::ArithmeticOverflow)?
            .min(loss);
        let cleared_i128 = i128::try_from(cleared).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = pnl
            .checked_add(cleared_i128)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok(new_pnl)
    }

    /// PRODUCTION KERNEL (liveness rank): the B-settlement leg advance.
    /// b_snap moves FORWARD by exactly delta_b — the well-founded rank
    /// component for the B-settlement progress theorem: each successful
    /// chunk strictly decreases the leg's distance to its B target.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<PortfolioLegV16>| match result {
        Ok(l) => l.b_snap == leg.b_snap.wrapping_add(delta_b)
            && l.b_snap >= leg.b_snap
            && l.b_rem == new_remainder
            && l.b_stale == (remaining_after != 0)
            && l.active == leg.active
                && l.asset_index == leg.asset_index
                && l.market_id == leg.market_id
                && l.side == leg.side
                && l.basis_pos_q == leg.basis_pos_q
                && l.a_basis == leg.a_basis
                && l.k_snap == leg.k_snap
                && l.f_snap == leg.f_snap
                && l.epoch_snap == leg.epoch_snap
                && l.loss_weight == leg.loss_weight
                && l.b_epoch_snap == leg.b_epoch_snap
                && l.stale == leg.stale,
        Err(_) => true,
    }))]
    pub(crate) fn kernel_advance_leg_b_snap(
        mut leg: PortfolioLegV16,
        delta_b: u128,
        new_remainder: u128,
        remaining_after: u128,
    ) -> V16Result<PortfolioLegV16> {
        leg.b_snap = leg
            .b_snap
            .checked_add(delta_b)
            .ok_or(V16Error::ArithmeticOverflow)?;
        leg.b_rem = new_remainder;
        leg.b_stale = remaining_after != 0;
        Ok(leg)
    }

    /// PRODUCTION KERNEL: the clear-leg asset transform — decrement the
    /// side's stored-position count (and pending-obligation count for a
    /// zero-basis obligation leg), and unless the leg predates a side reset,
    /// fold its social-loss dust and remove its OI and loss weight. Pure on
    /// (PortfolioLegV16, AssetStateV16); the clear-leg glue calls exactly this.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<AssetStateV16>| match result {
        Ok(a) => {
            let prior_reset = match leg.side {
                SideV16::Long => asset.mode_long == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_long),
                SideV16::Short => asset.mode_short == SideModeV16::ResetPending
                    && leg.epoch_snap.checked_add(1) == Some(asset.epoch_short),
            };
            let obligation = leg.basis_pos_q == 0 && leg.loss_weight != 0;
            (match leg.side {
                SideV16::Long => a.stored_pos_count_long == asset.stored_pos_count_long.wrapping_sub(1)
                    && a.stored_pos_count_short == asset.stored_pos_count_short
                    && a.pending_obligation_count_long
                        == (if obligation { asset.pending_obligation_count_long.wrapping_sub(1) } else { asset.pending_obligation_count_long })
                    && a.pending_obligation_count_short == asset.pending_obligation_count_short
                    && (if prior_reset {
                        a.oi_eff_long_q == asset.oi_eff_long_q
                            && a.loss_weight_sum_long == asset.loss_weight_sum_long
                            && a.social_loss_dust_long_num == asset.social_loss_dust_long_num
                    } else {
                        a.oi_eff_long_q == asset.oi_eff_long_q.wrapping_sub(leg.basis_pos_q.unsigned_abs())
                            && a.loss_weight_sum_long == asset.loss_weight_sum_long.wrapping_sub(leg.loss_weight)
                            && a.social_loss_dust_long_num == asset.social_loss_dust_long_num.wrapping_add(leg.b_rem)
                            && (leg.b_rem == 0 || a.social_loss_dust_long_num < SOCIAL_LOSS_DEN)
                    })
                    && a.oi_eff_short_q == asset.oi_eff_short_q
                    && a.loss_weight_sum_short == asset.loss_weight_sum_short
                    && a.social_loss_dust_short_num == asset.social_loss_dust_short_num,
                SideV16::Short => a.stored_pos_count_short == asset.stored_pos_count_short.wrapping_sub(1)
                    && a.stored_pos_count_long == asset.stored_pos_count_long
                    && a.pending_obligation_count_short
                        == (if obligation { asset.pending_obligation_count_short.wrapping_sub(1) } else { asset.pending_obligation_count_short })
                    && a.pending_obligation_count_long == asset.pending_obligation_count_long
                    && (if prior_reset {
                        a.oi_eff_short_q == asset.oi_eff_short_q
                            && a.loss_weight_sum_short == asset.loss_weight_sum_short
                            && a.social_loss_dust_short_num == asset.social_loss_dust_short_num
                    } else {
                        a.oi_eff_short_q == asset.oi_eff_short_q.wrapping_sub(leg.basis_pos_q.unsigned_abs())
                            && a.loss_weight_sum_short == asset.loss_weight_sum_short.wrapping_sub(leg.loss_weight)
                            && a.social_loss_dust_short_num == asset.social_loss_dust_short_num.wrapping_add(leg.b_rem)
                            && (leg.b_rem == 0 || a.social_loss_dust_short_num < SOCIAL_LOSS_DEN)
                    })
                    && a.oi_eff_long_q == asset.oi_eff_long_q
                    && a.loss_weight_sum_long == asset.loss_weight_sum_long
                    && a.social_loss_dust_long_num == asset.social_loss_dust_long_num,
            })
                && a.market_id == asset.market_id
                && a.retired_slot == asset.retired_slot
                && a.lifecycle == asset.lifecycle
                && a.raw_oracle_target_price == asset.raw_oracle_target_price
                && a.effective_price == asset.effective_price
                && a.fund_px_last == asset.fund_px_last
                && a.slot_last == asset.slot_last
                && a.a_long == asset.a_long
                && a.a_short == asset.a_short
                && a.k_long == asset.k_long
                && a.k_short == asset.k_short
                && a.f_long_num == asset.f_long_num
                && a.f_short_num == asset.f_short_num
                && a.k_epoch_start_long == asset.k_epoch_start_long
                && a.k_epoch_start_short == asset.k_epoch_start_short
                && a.f_epoch_start_long_num == asset.f_epoch_start_long_num
                && a.f_epoch_start_short_num == asset.f_epoch_start_short_num
                && a.b_long_num == asset.b_long_num
                && a.b_short_num == asset.b_short_num
                && a.b_epoch_start_long_num == asset.b_epoch_start_long_num
                && a.b_epoch_start_short_num == asset.b_epoch_start_short_num
                && a.stale_account_count_long == asset.stale_account_count_long
                && a.stale_account_count_short == asset.stale_account_count_short
                && a.social_loss_remainder_long_num == asset.social_loss_remainder_long_num
                && a.social_loss_remainder_short_num == asset.social_loss_remainder_short_num
                && a.explicit_unallocated_loss_long == asset.explicit_unallocated_loss_long
                && a.explicit_unallocated_loss_short == asset.explicit_unallocated_loss_short
                && a.epoch_long == asset.epoch_long
                && a.epoch_short == asset.epoch_short
                && a.mode_long == asset.mode_long
                && a.mode_short == asset.mode_short
        },
        Err(_) => true,
    }))]
    pub(crate) fn kernel_clear_leg(
        leg: PortfolioLegV16,
        mut asset: AssetStateV16,
    ) -> V16Result<AssetStateV16> {
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
        Ok(asset)
    }

    /// PRODUCTION KERNEL: the attach-leg core — snapshot the side's basis
    /// anchors, gate the a-basis range, add open interest, and construct the
    /// new leg. Pure on (AssetStateV16, scalars); the attach glue calls
    /// exactly this. loss_weight is caller-supplied (division-bearing).
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(basis_pos_q != 0 && basis_pos_q > i128::MIN))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(AssetStateV16, PortfolioLegV16)>| match result {
        Ok((a, l)) => {
            let abs_q = basis_pos_q.unsigned_abs();
            // the new leg is constructed EXACTLY from the declared sources
            l.active && l.asset_index == asset_index_u32 && l.market_id == asset.market_id
                && l.side == side && l.basis_pos_q == basis_pos_q && l.loss_weight == loss_weight
                && l.b_rem == 0 && !l.b_stale && !l.stale
                && (match side {
                    SideV16::Long => l.a_basis == asset.a_long && l.k_snap == asset.k_long
                        && l.f_snap == asset.f_long_num && l.b_snap == asset.b_long_num
                        && l.epoch_snap == asset.epoch_long && l.b_epoch_snap == asset.epoch_long
                        && a.oi_eff_long_q == asset.oi_eff_long_q.wrapping_add(abs_q)
                        && a.loss_weight_sum_long == asset.loss_weight_sum_long.wrapping_add(loss_weight)
                        && a.stored_pos_count_long == asset.stored_pos_count_long.wrapping_add(1)
                                && a.oi_eff_short_q == asset.oi_eff_short_q
                        && a.loss_weight_sum_short == asset.loss_weight_sum_short,
                    SideV16::Short => l.a_basis == asset.a_short && l.k_snap == asset.k_short
                        && l.f_snap == asset.f_short_num && l.b_snap == asset.b_short_num
                        && l.epoch_snap == asset.epoch_short && l.b_epoch_snap == asset.epoch_short
                        && a.oi_eff_short_q == asset.oi_eff_short_q.wrapping_add(abs_q)
                        && a.loss_weight_sum_short == asset.loss_weight_sum_short.wrapping_add(loss_weight)
                        && a.stored_pos_count_short == asset.stored_pos_count_short.wrapping_add(1)
                                && a.oi_eff_long_q == asset.oi_eff_long_q
                        && a.loss_weight_sum_long == asset.loss_weight_sum_long,
                })
                && a.market_id == asset.market_id
                && a.retired_slot == asset.retired_slot
                && a.lifecycle == asset.lifecycle
                && a.raw_oracle_target_price == asset.raw_oracle_target_price
                && a.effective_price == asset.effective_price
                && a.fund_px_last == asset.fund_px_last
                && a.slot_last == asset.slot_last
                && a.a_long == asset.a_long
                && a.a_short == asset.a_short
                && a.k_long == asset.k_long
                && a.k_short == asset.k_short
                && a.f_long_num == asset.f_long_num
                && a.f_short_num == asset.f_short_num
                && a.k_epoch_start_long == asset.k_epoch_start_long
                && a.k_epoch_start_short == asset.k_epoch_start_short
                && a.f_epoch_start_long_num == asset.f_epoch_start_long_num
                && a.f_epoch_start_short_num == asset.f_epoch_start_short_num
                && a.b_long_num == asset.b_long_num
                && a.b_short_num == asset.b_short_num
                && a.b_epoch_start_long_num == asset.b_epoch_start_long_num
                && a.b_epoch_start_short_num == asset.b_epoch_start_short_num
                && a.stale_account_count_long == asset.stale_account_count_long
                && a.stale_account_count_short == asset.stale_account_count_short
                && a.pending_obligation_count_long == asset.pending_obligation_count_long
                && a.pending_obligation_count_short == asset.pending_obligation_count_short
                && a.social_loss_remainder_long_num == asset.social_loss_remainder_long_num
                && a.social_loss_remainder_short_num == asset.social_loss_remainder_short_num
                && a.social_loss_dust_long_num == asset.social_loss_dust_long_num
                && a.social_loss_dust_short_num == asset.social_loss_dust_short_num
                && a.explicit_unallocated_loss_long == asset.explicit_unallocated_loss_long
                && a.explicit_unallocated_loss_short == asset.explicit_unallocated_loss_short
                && a.epoch_long == asset.epoch_long
                && a.epoch_short == asset.epoch_short
                && a.mode_long == asset.mode_long
                && a.mode_short == asset.mode_short
        },
        Err(_) => true,
    }))]
    pub(crate) fn kernel_attach_leg(
        mut asset: AssetStateV16,
        side: SideV16,
        basis_pos_q: i128,
        loss_weight: u128,
        asset_index_u32: u32,
    ) -> V16Result<(AssetStateV16, PortfolioLegV16)> {
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
        if loss_weight == 0 {
            return Err(V16Error::InvalidLeg);
        }
        add_open_interest_for_new_position(
            &mut asset,
            side,
            basis_pos_q.unsigned_abs(),
            loss_weight,
        )?;
        let leg = PortfolioLegV16 {
            active: true,
            asset_index: asset_index_u32,
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
        };
        Ok((asset, leg))
    }

    // Contract layer: lien creation is a pure encumbrance relabel — fresh
    // unliened moves to valid liened atom-for-atom, fresh_reserved (and every
    // stock-relevant quantity) is untouched, and zero-amount is the identity.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => (amount == 0 && *b == bucket && *s == source)
            || (b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num.wrapping_sub(amount)
                && b.valid_liened_backing_num == bucket.valid_liened_backing_num.wrapping_add(amount)
                && b.consumed_liened_backing_num == bucket.consumed_liened_backing_num
                && s.valid_liened_backing_num == source.valid_liened_backing_num.wrapping_add(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num
                && s.spent_backing_num == source.spent_backing_num),
        Err(_) => true,
    }))]
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

    // Contract layer: adding fresh backing raises fresh unliened and
    // fresh_reserved by the FULL amount (the refill only clears receivable
    // bookkeeping against consumed history; spent is cumulative audit state
    // and never decreases).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num.wrapping_add(amount)
            && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num.wrapping_add(amount)
            && b.valid_liened_backing_num == bucket.valid_liened_backing_num
            && s.valid_liened_backing_num == source.valid_liened_backing_num
            && s.spent_backing_num == source.spent_backing_num
            && s.provider_receivable_num <= source.provider_receivable_num
            && b.consumed_liened_backing_num <= bucket.consumed_liened_backing_num
            && source.provider_receivable_num.wrapping_sub(s.provider_receivable_num)
                == bucket.consumed_liened_backing_num.wrapping_sub(b.consumed_liened_backing_num),
        Err(_) => true,
    }))]
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

    // Contract layer: principal withdrawal debits fresh_unliened and
    // fresh_reserved in exact lockstep, never touches liened/consumed
    // encumbrance, and zero-amount is the identity.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => (amount == 0 && *b == bucket && *s == source)
            || (b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num.wrapping_sub(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num.wrapping_sub(amount)
                && b.valid_liened_backing_num == bucket.valid_liened_backing_num
                && b.consumed_liened_backing_num == bucket.consumed_liened_backing_num
                && s.valid_liened_backing_num == source.valid_liened_backing_num
                && s.spent_backing_num == source.spent_backing_num
                && s.provider_receivable_num == source.provider_receivable_num),
        Err(_) => true,
    }))]
    fn prepare_counterparty_backing_withdraw_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.status != BackingBucketStatusV16::Fresh
            || bucket.fresh_unliened_backing_num < amount
            || source.fresh_reserved_backing_num < amount
        {
            return Err(V16Error::LockActive);
        }
        bucket.fresh_unliened_backing_num -= amount;
        source.fresh_reserved_backing_num -= amount;
        if bucket.fresh_unliened_backing_num == 0 && bucket.valid_liened_backing_num == 0 {
            if bucket.impaired_liened_backing_num != 0 {
                bucket.status = BackingBucketStatusV16::Impaired;
            } else if bucket.consumed_liened_backing_num != 0 {
                bucket.status = BackingBucketStatusV16::Expired;
            } else {
                bucket.status = BackingBucketStatusV16::Empty;
                bucket.expiry_slot = 0;
            }
        }
        Ok((bucket, source))
    }

    #[cfg(any(kani, feature = "fuzz"))]
    // Contract layer: lien release is the un-pledge relabel — valid liened
    // returns to fresh unliened atom-for-atom, fresh_reserved and all stock
    // quantities untouched; zero-amount is the identity.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => (amount == 0 && *b == bucket && *s == source)
            || (b.valid_liened_backing_num == bucket.valid_liened_backing_num.wrapping_sub(amount)
                && b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num.wrapping_add(amount)
                && b.consumed_liened_backing_num == bucket.consumed_liened_backing_num
                && s.valid_liened_backing_num == source.valid_liened_backing_num.wrapping_sub(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num
                && s.spent_backing_num == source.spent_backing_num
                && s.provider_receivable_num == source.provider_receivable_num),
        Err(_) => true,
    }))]
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

    // Expiry-agnostic counterparty lien release for terminal (Resolved) wind-down.
    // Identical to prepare_counterparty_lien_release_delta but WITHOUT the lending-time
    // freshness guard (Fresh status / expiry_slot > current_slot): unwinding returns
    // liened backing to the provider's unliened pool, it is not re-lending, so a
    // time-expired bucket must not block it. Without this, a market that resolves past a
    // backing bucket's expiry re-introduces the Finding-A close deadlock.
    // Contract layer: the TERMINAL release performs the identical relabel as
    // the Live release but is expiry/status-agnostic (Finding-C semantics).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => (amount == 0 && *b == bucket && *s == source)
            || (b.valid_liened_backing_num == bucket.valid_liened_backing_num.wrapping_sub(amount)
                && b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num.wrapping_add(amount)
                && b.consumed_liened_backing_num == bucket.consumed_liened_backing_num
                && s.valid_liened_backing_num == source.valid_liened_backing_num.wrapping_sub(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num
                && s.spent_backing_num == source.spent_backing_num
                && s.provider_receivable_num == source.provider_receivable_num),
        Err(_) => true,
    }))]
    fn prepare_counterparty_lien_terminal_release_delta(
        mut bucket: BackingBucketV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(BackingBucketV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((bucket, source));
        }
        if bucket.valid_liened_backing_num < amount || source.valid_liened_backing_num < amount {
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

    // Contract layer: consuming X of a valid lien moves it to consumed/spent/
    // receivable in exact lockstep, leaves fresh_unliened untouched, and debits
    // fresh_reserved by exactly X. Mirrors the standalone delta proofs.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => b.valid_liened_backing_num == bucket.valid_liened_backing_num.wrapping_sub(amount)
            && b.consumed_liened_backing_num == bucket.consumed_liened_backing_num.wrapping_add(amount)
            && b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num
            && s.valid_liened_backing_num == source.valid_liened_backing_num.wrapping_sub(amount)
            && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num.wrapping_sub(amount)
            && s.spent_backing_num == source.spent_backing_num.wrapping_add(amount)
            && s.provider_receivable_num == source.provider_receivable_num.wrapping_add(amount),
        Err(_) => true,
    }))]
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

    #[cfg(any(kani, feature = "fuzz"))]
    // Contract layer: impairment forfeits the liened principal out of the
    // backing class entirely — valid liened AND fresh_reserved fall by X while
    // the impaired audit counters rise by X (the stock-side destination is the
    // junior residual pool, enforced at the aggregate layer).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(BackingBucketV16, SourceCreditStateV16)>| match result {
        Ok((b, s)) => (amount == 0 && *b == bucket && *s == source)
            || (b.valid_liened_backing_num == bucket.valid_liened_backing_num.wrapping_sub(amount)
                && b.impaired_liened_backing_num == bucket.impaired_liened_backing_num.wrapping_add(amount)
                && b.fresh_unliened_backing_num == bucket.fresh_unliened_backing_num
                && s.valid_liened_backing_num == source.valid_liened_backing_num.wrapping_sub(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num.wrapping_sub(amount)
                && s.impaired_liened_backing_num == source.impaired_liened_backing_num.wrapping_add(amount)
                && s.spent_backing_num == source.spent_backing_num),
        Err(_) => true,
    }))]
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

    // Contract layer: insurance lien creation encumbers reserved credit only
    // (valid liened rises in reservation and source in lockstep; the
    // reservation total and all backing-side state untouched).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)>| match result {
        Ok((r, s)) => (amount == 0 && *r == reservation && *s == source)
            || (r.valid_liened_insurance_num == reservation.valid_liened_insurance_num.wrapping_add(amount)
                && r.insurance_credit_reserved_num == reservation.insurance_credit_reserved_num
                && r.impaired_liened_insurance_num == reservation.impaired_liened_insurance_num
                && s.valid_liened_insurance_num == source.valid_liened_insurance_num.wrapping_add(amount)
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num
                && s.spent_backing_num == source.spent_backing_num),
        Err(_) => true,
    }))]
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

    #[cfg(any(kani, feature = "fuzz"))]
    // Contract layer: insurance lien release un-encumbers reserved credit —
    // exact mirror of creation; reservation total and backing-side untouched.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)>| match result {
        Ok((r, s)) => (amount == 0 && *r == reservation && *s == source)
            || (r.valid_liened_insurance_num == reservation.valid_liened_insurance_num.wrapping_sub(amount)
                && r.insurance_credit_reserved_num == reservation.insurance_credit_reserved_num
                && r.impaired_liened_insurance_num == reservation.impaired_liened_insurance_num
                && s.valid_liened_insurance_num == source.valid_liened_insurance_num.wrapping_sub(amount)
                && s.insurance_credit_reserved_num == source.insurance_credit_reserved_num
                && s.fresh_reserved_backing_num == source.fresh_reserved_backing_num),
        Err(_) => true,
    }))]
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

    // Contract layer: the TERMINAL insurance release un-RESERVES the full
    // amount (unlike the Live release, which keeps the reservation), splitting
    // the lien release valid-first-then-impaired on both ledgers in lockstep.
    // (First contract draft claimed Live semantics; the contract check's
    // counterexample exposed the wind-down difference.)
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)>| match result {
        Ok((r, s)) => (amount == 0 && *r == reservation && *s == source)
            || {
                let vr = if reservation.valid_liened_insurance_num < amount {
                    reservation.valid_liened_insurance_num
                } else {
                    amount
                };
                let ir = amount.wrapping_sub(vr);
                r.valid_liened_insurance_num == reservation.valid_liened_insurance_num.wrapping_sub(vr)
                    && r.impaired_liened_insurance_num == reservation.impaired_liened_insurance_num.wrapping_sub(ir)
                    && r.insurance_credit_reserved_num == reservation.insurance_credit_reserved_num.wrapping_sub(amount)
                    && s.valid_liened_insurance_num == source.valid_liened_insurance_num.wrapping_sub(vr)
                    && s.impaired_liened_insurance_num == source.impaired_liened_insurance_num.wrapping_sub(ir)
                    && s.insurance_credit_reserved_num == source.insurance_credit_reserved_num.wrapping_sub(amount)
            },
        Err(_) => true,
    }))]
    fn prepare_insurance_lien_terminal_release_delta(
        mut reservation: InsuranceCreditReservationV16,
        mut source: SourceCreditStateV16,
        amount: u128,
    ) -> V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)> {
        if amount == 0 {
            return Ok((reservation, source));
        }
        Self::validate_bound_num_atom_aligned(amount)?;
        if reservation.insurance_credit_reserved_num < amount
            || source.insurance_credit_reserved_num < amount
        {
            return Err(V16Error::CounterUnderflow);
        }
        let valid_release = amount.min(reservation.valid_liened_insurance_num);
        if source.valid_liened_insurance_num < valid_release {
            return Err(V16Error::CounterUnderflow);
        }
        let impaired_release = amount
            .checked_sub(valid_release)
            .ok_or(V16Error::CounterUnderflow)?;
        if reservation.impaired_liened_insurance_num < impaired_release
            || source.impaired_liened_insurance_num < impaired_release
        {
            return Err(V16Error::CounterUnderflow);
        }
        reservation.valid_liened_insurance_num -= valid_release;
        source.valid_liened_insurance_num -= valid_release;
        reservation.impaired_liened_insurance_num -= impaired_release;
        source.impaired_liened_insurance_num -= impaired_release;
        reservation.insurance_credit_reserved_num -= amount;
        source.insurance_credit_reserved_num -= amount;
        Ok((reservation, source))
    }

    #[cfg(any(kani, feature = "fuzz"))]
    // Contract layer: insurance impairment relabels valid liened to impaired
    // inside the reservation (audit trail), reserved totals untouched.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(InsuranceCreditReservationV16, SourceCreditStateV16)>| match result {
        Ok((r, s)) => (amount == 0 && *r == reservation && *s == source)
            || (r.valid_liened_insurance_num == reservation.valid_liened_insurance_num.wrapping_sub(amount)
                && r.impaired_liened_insurance_num == reservation.impaired_liened_insurance_num.wrapping_add(amount)
                && r.insurance_credit_reserved_num == reservation.insurance_credit_reserved_num
                && s.valid_liened_insurance_num == source.valid_liened_insurance_num.wrapping_sub(amount)
                && s.impaired_liened_insurance_num == source.impaired_liened_insurance_num.wrapping_add(amount)),
        Err(_) => true,
    }))]
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
        let base_initial = margin_requirement(
            risk_notional,
            config.initial_margin_bps,
            config.min_nonzero_im_req,
        )?;
        let base_maintenance = margin_requirement(
            risk_notional,
            config.maintenance_margin_bps,
            config.min_nonzero_mm_req,
        )?;
        Self::health_requirements_from_base_and_target_lag(
            base_initial,
            base_maintenance,
            risk_notional,
            target_lag_penalty,
        )
    }

    // Contract layer: the oracle-lag penalty raises initial margin,
    // maintenance margin, and risk notional by EXACTLY the penalty — lag can
    // only tighten requirements, never loosen them, and tightens all three
    // uniformly.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128, u128)>| match result {
        Ok((im, mm, rn)) => *im == base_initial.wrapping_add(target_lag_penalty)
            && *mm == base_maintenance.wrapping_add(target_lag_penalty)
            && *rn == risk_notional.wrapping_add(target_lag_penalty),
        Err(_) => true,
    }))]
    fn health_requirements_from_base_and_target_lag(
        base_initial: u128,
        base_maintenance: u128,
        risk_notional: u128,
        target_lag_penalty: u128,
    ) -> V16Result<(u128, u128, u128)> {
        Ok((
            base_initial
                .checked_add(target_lag_penalty)
                .ok_or(V16Error::ArithmeticOverflow)?,
            base_maintenance
                .checked_add(target_lag_penalty)
                .ok_or(V16Error::ArithmeticOverflow)?,
            risk_notional
                .checked_add(target_lag_penalty)
                .ok_or(V16Error::ArithmeticOverflow)?,
        ))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HLockLaneV16 {
    HMin,
    HMax,
}

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideV16 {
    Long,
    Short,
}

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideModeV16 {
    Normal,
    DrainOnly,
    ResetPending,
}

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackingBucketStatusV16 {
    Empty,
    Fresh,
    Expired,
    Impaired,
}

#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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

    fn validate_public_user_fund_shape(&self) -> V16Result<()> {
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
}

#[repr(C)]
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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

    const fn is_empty_amount_shape(self) -> bool {
        self.positive_claim_bound_num == 0
            && self.exact_positive_claim_num == 0
            && self.fresh_reserved_backing_num == 0
            && self.spent_backing_num == 0
            && self.provider_receivable_num == 0
            && self.valid_liened_backing_num == 0
            && self.impaired_liened_backing_num == 0
            && self.insurance_credit_reserved_num == 0
            && self.valid_liened_insurance_num == 0
            && self.impaired_liened_insurance_num == 0
            && (self.credit_rate_num == 0 || self.credit_rate_num == CREDIT_RATE_SCALE)
    }
}

impl Default for SourceCreditStateV16 {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[repr(C)]
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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
}

pub struct PortfolioV16ViewMut<'a> {
    pub header: &'a mut PortfolioAccountV16Account,
}

impl<'a> PortfolioV16View<'a> {
    pub fn new(header: &'a PortfolioAccountV16Account) -> Self {
        Self { header }
    }

    #[inline]
    fn source_domains(&self) -> &[PortfolioSourceDomainV16Account; PORTFOLIO_SOURCE_DOMAIN_CAP] {
        &self.header.source_domains
    }

    fn source_domain_slot(&self, domain: usize) -> V16Result<Option<usize>> {
        let domain_u32 = u32::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?;
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = self.header.source_domains[slot];
            if source.domain.get() == domain_u32 && source.is_occupied() {
                return Ok(Some(slot));
            }
            slot += 1;
        }
        Ok(None)
    }

    fn source_domain(&self, domain: usize) -> V16Result<PortfolioSourceDomainV16Account> {
        Ok(match self.source_domain_slot(domain)? {
            Some(slot) => self.header.source_domains[slot],
            None => PortfolioSourceDomainV16Account::default(),
        })
    }
}

impl<'a> PortfolioV16ViewMut<'a> {
    pub fn new(header: &'a mut PortfolioAccountV16Account) -> Self {
        let mut view = Self { header };
        view.compact_source_domains();
        view
    }

    pub fn as_view(&self) -> PortfolioV16View<'_> {
        PortfolioV16View {
            header: self.header,
        }
    }

    fn source_domain_slot(&self, domain: usize) -> V16Result<Option<usize>> {
        let domain_u32 = u32::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?;
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = self.header.source_domains[slot];
            let source_domain = source.domain.get();
            if source_domain == domain_u32 {
                if source.is_occupied() {
                    return Ok(Some(slot));
                }
                if source.has_default_sparse_tag() {
                    return Ok(None);
                }
            } else if source.has_default_sparse_tag() && !source.is_occupied() {
                return Ok(None);
            }
            slot += 1;
        }
        Ok(None)
    }

    fn source_domain_slot_or_insert(&mut self, domain: usize) -> V16Result<usize> {
        let domain_u32 = u32::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?;
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = self.header.source_domains[slot];
            let source_domain = source.domain.get();
            if source_domain == domain_u32 {
                if source.is_occupied() || !source.has_default_sparse_tag() {
                    return Ok(slot);
                }
                if source.has_default_sparse_tag() {
                    self.header.source_domains[slot].domain = V16PodU32::new(domain_u32);
                    return Ok(slot);
                }
            } else if source.has_default_sparse_tag() && !source.is_occupied() {
                self.header.source_domains[slot].domain = V16PodU32::new(domain_u32);
                return Ok(slot);
            }
            slot += 1;
        }
        Err(V16Error::LockActive)
    }

    fn source_domain_mut_or_insert(
        &mut self,
        domain: usize,
    ) -> V16Result<&mut PortfolioSourceDomainV16Account> {
        let slot = self.source_domain_slot_or_insert(domain)?;
        Ok(&mut self.header.source_domains[slot])
    }

    fn reset_source_domain_slot_if_empty(&mut self, slot: usize) -> bool {
        if slot < PORTFOLIO_SOURCE_DOMAIN_CAP
            && !self.header.source_domains[slot].is_occupied()
            && !self.header.source_domains[slot].has_default_sparse_tag()
        {
            self.header.source_domains[slot] = PortfolioSourceDomainV16Account::default();
            return true;
        }
        false
    }

    fn compact_source_domains(&mut self) {
        let mut write = 0usize;
        let mut read = 0usize;
        while read < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = self.header.source_domains[read];
            if source.is_occupied() {
                if write != read {
                    self.header.source_domains[write] = source;
                    self.header.source_domains[read] = PortfolioSourceDomainV16Account::default();
                }
                write += 1;
            }
            read += 1;
        }
        while write < PORTFOLIO_SOURCE_DOMAIN_CAP {
            self.header.source_domains[write] = PortfolioSourceDomainV16Account::default();
            write += 1;
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
        if self.header.residual_spent_principal_atoms_total.get()
            > self.header.residual_crystallized_loss_atoms_total.get()
        {
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
        let mut seen = [u32::MAX; PORTFOLIO_SOURCE_DOMAIN_CAP];
        let mut seen_count = 0usize;
        let mut slot_index = 0usize;
        while slot_index < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = self.source_domains()[slot_index];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                slot_index += 1;
                continue;
            }
            if !source.is_occupied() {
                return Err(V16Error::HiddenLeg);
            }
            let d_u32 = source.domain.get();
            let d = d_u32 as usize;
            if d >= configured_domains {
                return Err(V16Error::HiddenLeg);
            }
            let mut seen_i = 0usize;
            while seen_i < seen_count {
                if seen[seen_i] == d_u32 {
                    return Err(V16Error::HiddenLeg);
                }
                seen_i += 1;
            }
            seen[seen_count] = d_u32;
            seen_count += 1;
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
            slot_index += 1;
        }
        Ok(())
    }

    fn source_claim_bound_sum_num(&self) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < PORTFOLIO_SOURCE_DOMAIN_CAP {
            sum = sum
                .checked_add(self.source_domains()[d].source_claim_bound_num.get())
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

    fn is_empty_for_dematerialization(&self) -> V16Result<bool> {
        if !active_bitmap_is_empty(self.header.active_bitmap.map(V16PodU64::get))
            || self.header.capital.get() != 0
            || self.header.pnl.get() != 0
            || self.header.reserved_pnl.get() != 0
            || self.header.fee_credits.get() != 0
            || self.header.cancel_deposit_escrow.get() != 0
            || decode_bool(self.header.stale_state)?
            || decode_bool(self.header.b_stale_state)?
            || decode_bool(self.header.rebalance_lock)?
            || decode_bool(self.header.liquidation_lock)?
        {
            return Ok(false);
        }

        let close_progress = self.header.close_progress.try_to_runtime()?;
        let inert_canceled_close = close_progress.canceled
            && !close_progress.active
            && !close_progress.finalized
            && close_progress.close_id != 0
            && !close_progress.has_irreversible_progress()
            && close_progress.residual_remaining == close_progress.gross_loss_at_close_start;
        if !close_progress.is_empty()
            && !inert_canceled_close
            && !close_progress.is_finalized_inert()
        {
            return Ok(false);
        }

        let receipt = self.header.resolved_payout_receipt.try_to_runtime()?;
        if receipt.present && !receipt.finalized {
            return Ok(false);
        }

        let mut d = 0usize;
        while d < PORTFOLIO_SOURCE_DOMAIN_CAP {
            if self.header.source_domains[d].is_occupied() {
                return Ok(false);
            }
            d += 1;
        }
        Ok(true)
    }
}

impl<'a> PortfolioV16ViewMut<'a> {
    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {
        self.as_view().validate_with_market(market)
    }
}

#[repr(C)]
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
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

    fn has_pending_residual(self) -> bool {
        self.active && !self.finalized && !self.canceled && self.residual_remaining != 0
    }

    fn has_irreversible_progress(self) -> bool {
        self.support_consumed != 0
            || self.junior_face_burned != 0
            || self.insurance_spent != 0
            || self.b_loss_booked != 0
            || self.explicit_loss_assigned != 0
            || self.quantity_adl_applied_q != 0
            || self.drift_consumed != 0
    }

    fn is_finalized_inert(self) -> bool {
        self.active && self.finalized && !self.canceled && self.residual_remaining == 0
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
    /// Signed base quantity. Positive makes the first account long; negative
    /// makes the first account short.
    pub size_q: i128,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchTradeOutcomeV16 {
    pub fill_count: u32,
    pub fee_a: u128,
    pub fee_b: u128,
    pub notional: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TradeApplyOutcomeV16 {
    fee_a: u128,
    fee_b: u128,
    notional: u128,
    risk_increasing: bool,
    long_has_source_claims: bool,
    short_has_source_claims: bool,
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

/// The leg route a position change dispatches to (roadmap 3A.1).
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionRouteV16 {
    Attach,
    Clear,
    Flip,
    Resize,
}

/// Compact ActionableState summary (roadmap Phase 4 / 3A.4): which of the seven
/// ActionableState classes are live for an account/market. The liveness
/// selector reads only this — never per-class witnesses that another active
/// class could invalidate.
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionableSummaryV16 {
    pub stale: bool,             // A1
    pub b_stale: bool,           // A2
    pub pending_close: bool,     // A3
    pub expired_close: bool,     // A4
    pub liquidatable: bool,      // A5
    pub recovery_eligible: bool, // A6
    pub resolved_winner: bool,   // A7
}

impl ActionableSummaryV16 {
    pub fn is_actionable(self) -> bool {
        self.stale
            || self.b_stale
            || self.pending_close
            || self.expired_close
            || self.liquidatable
            || self.recovery_eligible
            || self.resolved_winner
    }
}

/// One oracle observation a keeper submits to the auto-crank (engine.md). The
/// caller supplies a bounded set of these for the assets it has fresh data for;
/// the ENGINE selects which (if any) the highest-priority step needs. The caller
/// never chooses the action TYPE or the asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoCrankObservationV16 {
    pub asset_index: usize,
    pub effective_price: u64,
    pub funding_rate_e9: i128,
}

/// Work a keeper submits to the order-insensitive auto-crank (engine.md):
/// observations that may land in any order and the resolved-close fee rate. NO
/// caller-chosen action, asset, liquidation size, or liquidation fee — the engine
/// selects the step/asset, closes the selected leg, and derives the fee from
/// config.
#[derive(Clone, Copy, Debug)]
pub struct AutoCrankWorkV16<'a> {
    pub now_slot: u64,
    pub observations: &'a [AutoCrankObservationV16],
    pub resolved_close_fee_rate_per_slot: u128,
}

/// The bounded progress step the engine SELECTED from current state (engine.md).
/// The asset/leg is chosen by the engine, not the caller. One auto-crank call
/// dispatches exactly one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoCrankPlanV16 {
    NoAction,
    RefreshAccount {
        asset_index: Option<usize>,
    },
    SettleBChunk {
        asset_index: usize,
    },
    Liquidate {
        asset_index: usize,
    },
    DeclareRecovery {
        reason: PermissionlessRecoveryReasonV16,
    },
    CloseResolved,
}

/// The outcome of one self-classifying crank step: nothing actionable, a
/// permissionless-progress outcome (refresh / settle-B / liquidate / recovery),
/// or a resolved-close outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoCrankOutcomeV16 {
    NoAction,
    Progressed(PermissionlessProgressOutcomeV16),
    ResolvedClose(ResolvedCloseOutcomeV16),
}

/// Result of `permissionless_auto_crank_not_atomic`: the engine-selected plan
/// (including the engine-chosen asset) and the dispatched outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoCrankResultV16 {
    pub selected: AutoCrankPlanV16,
    pub outcome: AutoCrankOutcomeV16,
}

/// Scalar request/config guard summary (roadmap 3C fidelity layer): the exact
/// per-leaf decisions validate_trade_request makes. build_trade_request_guard_
/// summary computes these from the request + config, and validate_trade_request
/// admits IFF all pass — so this summary is production-faithful, not a model.
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeRequestGuardSummaryV16 {
    pub asset_configured: bool, // asset_index < max_market_slots
    pub size_nonzero: bool,     // size_q != 0
    pub size_in_cap: bool,      // |size_q| <= MAX_TRADE_SIZE_Q
    pub price_nonzero: bool,    // exec_price != 0
    pub price_in_range: bool,   // exec_price <= MAX_ORACLE_PRICE
    pub fee_bps_in_cap: bool,   // fee_bps <= max_trading_fee_bps
}

impl TradeRequestGuardSummaryV16 {
    pub fn all_pass(self) -> bool {
        self.asset_configured
            && self.size_nonzero
            && self.size_in_cap
            && self.price_nonzero
            && self.price_in_range
            && self.fee_bps_in_cap
    }
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

#[cfg(kani)]
pub const V16_TOKEN_VALUE_CLASS_COUNT: usize = 17;
#[cfg(not(kani))]
const V16_TOKEN_VALUE_CLASS_COUNT: usize = 17;

#[repr(u8)]
#[cfg_attr(all(kani, feature = "contracts"), derive(kani::Arbitrary))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(kani)]
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

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(not(kani))]
#[allow(dead_code)]
enum TokenValueClassV16 {
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
#[cfg_attr(
    all(kani, any(feature = "contracts", feature = "closure")),
    derive(kani::Arbitrary)
)]
#[cfg(kani)]
pub struct TokenValueFlowProofV16 {
    pub debits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    pub credits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    pub external_quote_in: u128,
    pub external_quote_out: u128,
    pub vault_before: u128,
    pub vault_after: u128,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(not(kani))]
struct TokenValueFlowProofV16 {
    debits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    credits: [u128; V16_TOKEN_VALUE_CLASS_COUNT],
    external_quote_in: u128,
    external_quote_out: u128,
    vault_before: u128,
    vault_after: u128,
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

    fn external_in_to_insurance_capital(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_in = amount;
        proof.credit(TokenValueClassV16::ExternalQuote, amount)?;
        proof.debit(TokenValueClassV16::InsuranceCapital, amount)?;
        Ok(proof)
    }

    fn insurance_capital_to_external_out(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.external_quote_out = amount;
        proof.debit(TokenValueClassV16::InsuranceCapital, amount)?;
        proof.credit(TokenValueClassV16::ExternalQuote, amount)?;
        Ok(proof)
    }

    fn insurance_capital_to_account_capital(
        amount: u128,
        vault_before: u128,
        vault_after: u128,
    ) -> V16Result<Self> {
        let mut proof = Self::empty(vault_before, vault_after);
        proof.debit(TokenValueClassV16::InsuranceCapital, amount)?;
        proof.credit(TokenValueClassV16::AccountCapital, amount)?;
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

    // P5 probe (timeboxed): the minimal &mut-self contract experiment — flat
    // Copy struct, single-array-field mutation. PASS opens the &mut-self
    // stratum for contracts; FAIL/timeout closes it and finalizes the
    // exclusion map.
    #[cfg_attr(all(kani, feature = "contracts"), kani::modifies(self))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<()>| match result {
        Ok(()) => self.debits[class as usize]
            == old(self.debits[class as usize]).wrapping_add(amount),
        Err(_) => true,
    }))]
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
struct ReservationEncumbranceProofV16 {
    domain: u16,
    exact_positive_claim_num: u128,
    positive_claim_bound_num: u128,
    source_fresh_reserved_backing_num: u128,
    source_spent_backing_num: u128,
    source_provider_receivable_num: u128,
    bucket_fresh_unliened_backing_num: u128,
    bucket_valid_liened_backing_num: u128,
    bucket_consumed_liened_backing_num: u128,
    source_valid_liened_backing_num: u128,
    source_impaired_liened_backing_num: u128,
    bucket_impaired_liened_backing_num: u128,
    source_insurance_credit_reserved_num: u128,
    reservation_insurance_credit_reserved_num: u128,
    source_valid_liened_insurance_num: u128,
    reservation_valid_liened_insurance_num: u128,
    source_impaired_liened_insurance_num: u128,
    reservation_impaired_liened_insurance_num: u128,
    source_credit_rate_num: u128,
}

impl ReservationEncumbranceProofV16 {
    fn validate(&self) -> V16Result<()> {
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
#[cfg(kani)]
pub struct StockReconciliationProofV16 {
    pub token_vault: u128,
    pub senior_capital_total: u128,
    pub insurance_capital: u128,
    pub backing_provider_earnings: u128,
    pub counterparty_backing_principal: u128,
    pub settlement_rounding_residue_total: u128,
    pub unallocated_protocol_surplus: u128,
}

#[cfg(kani)]
impl StockReconciliationProofV16 {
    pub fn validate(&self) -> V16Result<()> {
        let accounted = self
            .senior_capital_total
            .checked_add(self.insurance_capital)
            .and_then(|v| v.checked_add(self.backing_provider_earnings))
            .and_then(|v| v.checked_add(self.counterparty_backing_principal))
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
struct SourceCreditLienAggregateProofV16 {
    domain: u16,
    source_claim_bound_num: u128,
    face_claim_locked_num: u128,
    counterparty_face_claim_locked_num: u128,
    insurance_face_claim_locked_num: u128,
    effective_credit_reserved: u128,
    counterparty_backing_reserved_num: u128,
    insurance_backing_reserved_num: u128,
    impaired_face_claim_num: u128,
    impaired_effective_credit_reserved: u128,
}

impl SourceCreditLienAggregateProofV16 {
    fn validate(&self) -> V16Result<()> {
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

/// The terminal decision of one bankruptcy-residual booking step (roadmap
/// workstream B.2): either a conserving outcome to return `Ok`, or a signal that
/// the caller must declare permissionless recovery and return `Err`. Produced by
/// the proven `kernel_bresidual_step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BResidualStepV16 {
    Outcome(BResidualBookingOutcomeV16),
    DeclareRecovery,
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

fn slot_resolved_payout_blockers_v16(slot: &EngineAssetSlotV16Account) -> V16Result<u64> {
    let asset = slot.asset.try_to_runtime()?;
    asset
        .stored_pos_count_long
        .checked_add(asset.stored_pos_count_short)
        .and_then(|v| v.checked_add(asset.stale_account_count_long))
        .and_then(|v| v.checked_add(asset.stale_account_count_short))
        .and_then(|v| v.checked_add(slot.pending_domain_loss_barrier_long.get()))
        .and_then(|v| v.checked_add(slot.pending_domain_loss_barrier_short.get()))
        .ok_or(V16Error::CounterOverflow)
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
    #[cfg(any(test, kani, feature = "audit-scan"))]
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

    fn validate_market_id_binding(&self) -> V16Result<()> {
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
            && state.impaired_liened_insurance_num.get() == 0;
        all_non_rate_fields_empty
            && (state.credit_rate_num.get() == 0
                || state.credit_rate_num.get() == CREDIT_RATE_SCALE)
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
    fn backing_bucket_account_is_empty_for_activation(state: BackingBucketV16Account) -> bool {
        state.market_id.get() == 0
            && state.fresh_unliened_backing_num.get() == 0
            && state.valid_liened_backing_num.get() == 0
            && state.consumed_liened_backing_num.get() == 0
            && state.impaired_liened_backing_num.get() == 0
            && state.expiry_slot.get() == 0
            && state.status == 0
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
    fn insurance_reservation_account_is_empty_for_activation(
        state: InsuranceCreditReservationV16Account,
    ) -> bool {
        state.insurance_credit_reserved_num.get() == 0
            && state.valid_liened_insurance_num.get() == 0
            && state.impaired_liened_insurance_num.get() == 0
            && state.consumed_insurance_num.get() == 0
            && state.source_credit_epoch.get() == 0
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
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
    pub backing_provider_earnings_total: V16PodU128,
    pub source_claim_bound_total_num: V16PodU128,
    pub source_fresh_backing_total_num: V16PodU128,
    pub source_insurance_credit_reserved_total_atoms: V16PodU128,
    pub insurance_domain_budget_remaining_total: V16PodU128,
    pub resolved_payout_blocker_count: V16PodU64,
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
    fn dynamic_asset_slot_stride<T: MarketWrapperPod>() -> usize {
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
            backing_provider_earnings_total: V16PodU128::default(),
            source_claim_bound_total_num: V16PodU128::default(),
            source_fresh_backing_total_num: V16PodU128::default(),
            source_insurance_credit_reserved_total_atoms: V16PodU128::default(),
            insurance_domain_budget_remaining_total: V16PodU128::default(),
            resolved_payout_blocker_count: V16PodU64::default(),
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

    #[cfg(any(test, kani, feature = "audit-scan"))]
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

    #[cfg(any(test, kani, feature = "audit-scan"))]
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

#[cfg(any(test, kani, feature = "audit-scan"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MarketAggregateTotalsV16 {
    backing_provider_earnings: u128,
    source_claim_bound_num: u128,
    source_fresh_backing_num: u128,
    source_insurance_credit_reserved_atoms: u128,
    insurance_domain_budget_remaining_atoms: u128,
    resolved_payout_blocker_count: u64,
}

impl<'a, T> MarketGroupV16View<'a, T> {
    pub fn validate_shape(&self) -> V16Result<()> {
        MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static(
            self.markets.len(),
            self.header.asset_slot_capacity.get() as usize,
            self.header.config.max_market_slots.get() as usize,
        )?;
        self.header.config.try_to_runtime_shape()?;
        decode_bool(self.header.bankruptcy_hlock_active)?;
        decode_bool(self.header.threshold_stress_active)?;
        decode_bool(self.header.loss_stale_active)?;
        decode_bool(self.header.payout_snapshot_captured)?;
        self.header.recovery_reason.try_to_runtime()?;
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

        self.validate_header_aggregate_totals()?;
        #[cfg(any(test, kani, feature = "audit-scan"))]
        {
            self.validate_shape_full_audit_scan()?;
        }
        Ok(())
    }

    fn validate_header_aggregate_totals(&self) -> V16Result<()> {
        let senior = self
            .header
            .c_tot
            .get()
            .checked_add(self.header.insurance.get())
            .and_then(|v| v.checked_add(self.header.backing_provider_earnings_total.get()))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if senior > self.header.vault.get() {
            return Err(V16Error::InvalidConfig);
        }
        // Recoverable counterparty backing principal is also a vault claim:
        // every deposit moves vault and backing in lockstep, every consume that
        // re-credits c_tot debits backing first, so the strengthened stack must
        // always be covered. A state violating it is double-promising atoms.
        let senior_with_backing = senior
            .checked_add(self.header.source_fresh_backing_total_num.get() / BOUND_SCALE)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if senior_with_backing > self.header.vault.get() {
            return Err(V16Error::InvalidConfig);
        }
        if self
            .header
            .source_insurance_credit_reserved_total_atoms
            .get()
            > self.header.insurance.get()
            || self.header.insurance_domain_budget_remaining_total.get()
                > self.header.insurance.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        // The group junior-claim bound is the denominator for the non-source
        // haircut and the resolved-payout snapshot, so it must never understate
        // the aggregate per-domain source claims it haircuts against; an
        // understated bound shrinks that denominator and over-credits support.
        // The credit/burn paths maintain this in lockstep; enforce it here so a
        // deserialized or future-corrupted state cannot slip through.
        if self.header.pnl_pos_bound_tot_num.get() < self.header.source_claim_bound_total_num.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
    fn validate_shape_full_audit_scan(&self) -> V16Result<()> {
        let totals = self.compute_aggregate_totals_and_validate_slots()?;
        if totals.backing_provider_earnings != self.header.backing_provider_earnings_total.get()
            || totals.source_claim_bound_num != self.header.source_claim_bound_total_num.get()
            || totals.source_fresh_backing_num != self.header.source_fresh_backing_total_num.get()
            || totals.source_insurance_credit_reserved_atoms
                != self
                    .header
                    .source_insurance_credit_reserved_total_atoms
                    .get()
            || totals.insurance_domain_budget_remaining_atoms
                != self.header.insurance_domain_budget_remaining_total.get()
            || totals.resolved_payout_blocker_count
                != self.header.resolved_payout_blocker_count.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
    fn compute_aggregate_totals_and_validate_slots(&self) -> V16Result<MarketAggregateTotalsV16> {
        let mode = decode_market_mode(self.header.mode)?;
        let configured_assets = self.header.config.max_market_slots.get() as usize;
        let mut totals = MarketAggregateTotalsV16::default();
        let mut i = 0usize;
        while i < self.markets.len() {
            let slot = self.markets[i].engine_slot();
            totals.resolved_payout_blocker_count = totals
                .resolved_payout_blocker_count
                .checked_add(slot_resolved_payout_blockers_v16(slot)?)
                .ok_or(V16Error::CounterOverflow)?;
            totals.backing_provider_earnings = totals
                .backing_provider_earnings
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
            let source_credit_long = slot.source_credit_long.try_to_runtime()?;
            totals.source_claim_bound_num = totals
                .source_claim_bound_num
                .checked_add(source_credit_long.positive_claim_bound_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            totals.source_fresh_backing_num = totals
                .source_fresh_backing_num
                .checked_add(source_credit_long.fresh_reserved_backing_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            Self::validate_domain_shape_for_view(
                asset.market_id,
                source_credit_long,
                slot.backing_long.try_to_runtime()?,
                slot.insurance_reservation_long.try_to_runtime()?,
                slot.insurance_domain_budget_long.get(),
                slot.insurance_domain_spent_long.get(),
                slot.pending_domain_loss_barrier_long.get(),
                self.header.current_slot.get(),
                &mut totals.source_insurance_credit_reserved_atoms,
            )?;
            totals.insurance_domain_budget_remaining_atoms = totals
                .insurance_domain_budget_remaining_atoms
                .checked_add(
                    slot.insurance_domain_budget_long
                        .get()
                        .checked_sub(slot.insurance_domain_spent_long.get())
                        .ok_or(V16Error::InvalidConfig)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            let source_credit_short = slot.source_credit_short.try_to_runtime()?;
            totals.source_claim_bound_num = totals
                .source_claim_bound_num
                .checked_add(source_credit_short.positive_claim_bound_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            totals.source_fresh_backing_num = totals
                .source_fresh_backing_num
                .checked_add(source_credit_short.fresh_reserved_backing_num)
                .ok_or(V16Error::ArithmeticOverflow)?;
            Self::validate_domain_shape_for_view(
                asset.market_id,
                source_credit_short,
                slot.backing_short.try_to_runtime()?,
                slot.insurance_reservation_short.try_to_runtime()?,
                slot.insurance_domain_budget_short.get(),
                slot.insurance_domain_spent_short.get(),
                slot.pending_domain_loss_barrier_short.get(),
                self.header.current_slot.get(),
                &mut totals.source_insurance_credit_reserved_atoms,
            )?;
            totals.insurance_domain_budget_remaining_atoms = totals
                .insurance_domain_budget_remaining_atoms
                .checked_add(
                    slot.insurance_domain_budget_short
                        .get()
                        .checked_sub(slot.insurance_domain_spent_short.get())
                        .ok_or(V16Error::InvalidConfig)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            i += 1;
        }
        Ok(totals)
    }

    #[cfg(any(test, kani, feature = "audit-scan"))]
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

    #[cfg(any(test, kani, feature = "audit-scan"))]
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

    pub fn register_empty_materialized_portfolio_not_atomic(
        &mut self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if !account.is_empty_for_dematerialization()? {
            return Err(V16Error::LockActive);
        }
        self.header.materialized_portfolio_count = V16PodU64::new(
            self.header
                .materialized_portfolio_count
                .get()
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
        );
        self.validate_shape_audit_scan()
    }

    pub fn deregister_empty_materialized_portfolio_not_atomic(
        &mut self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if !account.is_empty_for_dematerialization()? {
            return Err(V16Error::LockActive);
        }
        self.header.materialized_portfolio_count = V16PodU64::new(
            self.header
                .materialized_portfolio_count
                .get()
                .checked_sub(1)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        self.validate_shape_audit_scan()
    }

    #[cfg(kani)]
    pub fn refresh_header_aggregate_totals_for_test(&mut self) -> V16Result<()> {
        let totals = self
            .as_view()
            .compute_aggregate_totals_and_validate_slots()?;
        self.header.backing_provider_earnings_total =
            V16PodU128::new(totals.backing_provider_earnings);
        self.header.source_claim_bound_total_num = V16PodU128::new(totals.source_claim_bound_num);
        self.header.source_fresh_backing_total_num =
            V16PodU128::new(totals.source_fresh_backing_num);
        self.header.source_insurance_credit_reserved_total_atoms =
            V16PodU128::new(totals.source_insurance_credit_reserved_atoms);
        self.header.insurance_domain_budget_remaining_total =
            V16PodU128::new(totals.insurance_domain_budget_remaining_atoms);
        self.header.resolved_payout_blocker_count =
            V16PodU64::new(totals.resolved_payout_blocker_count);
        Ok(())
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
        if account.header.residual_spent_principal_atoms_total.get()
            > account.header.residual_crystallized_loss_atoms_total.get()
        {
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

    // Contract layer: aggregate maintenance is exact — the returned total
    // shifts by precisely (new - old), in either direction, or errors.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<u128>| match result {
        Ok(t) => (*t as i128).wrapping_sub(total as i128)
            == (new as i128).wrapping_sub(old as i128),
        Err(_) => true,
    }))]
    fn apply_total_delta(total: u128, old: u128, new: u128) -> V16Result<u128> {
        if new >= old {
            total
                .checked_add(new - old)
                .ok_or(V16Error::CounterOverflow)
        } else {
            total
                .checked_sub(old - new)
                .ok_or(V16Error::CounterUnderflow)
        }
    }

    fn apply_total_delta_u64(total: u64, old: u64, new: u64) -> V16Result<u64> {
        if new >= old {
            total
                .checked_add(new - old)
                .ok_or(V16Error::CounterOverflow)
        } else {
            total
                .checked_sub(old - new)
                .ok_or(V16Error::CounterUnderflow)
        }
    }

    fn domain_budget_remaining_parts(budget: u128, spent: u128) -> V16Result<u128> {
        budget.checked_sub(spent).ok_or(V16Error::InvalidConfig)
    }

    fn update_source_credit_aggregate_totals(
        &mut self,
        old: SourceCreditStateV16,
        new: SourceCreditStateV16,
    ) -> V16Result<()> {
        let old_insurance_atoms =
            V16Core::amount_from_bound_num(old.insurance_credit_reserved_num)?;
        let new_insurance_atoms =
            V16Core::amount_from_bound_num(new.insurance_credit_reserved_num)?;
        self.header.source_claim_bound_total_num = V16PodU128::new(Self::apply_total_delta(
            self.header.source_claim_bound_total_num.get(),
            old.positive_claim_bound_num,
            new.positive_claim_bound_num,
        )?);
        self.header.source_insurance_credit_reserved_total_atoms =
            V16PodU128::new(Self::apply_total_delta(
                self.header
                    .source_insurance_credit_reserved_total_atoms
                    .get(),
                old_insurance_atoms,
                new_insurance_atoms,
            )?);
        self.header.source_fresh_backing_total_num = V16PodU128::new(Self::apply_total_delta(
            self.header.source_fresh_backing_total_num.get(),
            old.fresh_reserved_backing_num,
            new.fresh_reserved_backing_num,
        )?);
        Ok(())
    }

    fn update_backing_aggregate_totals(
        &mut self,
        old: BackingBucketV16,
        new: BackingBucketV16,
    ) -> V16Result<()> {
        self.header.backing_provider_earnings_total = V16PodU128::new(Self::apply_total_delta(
            self.header.backing_provider_earnings_total.get(),
            old.utilization_fee_earnings,
            new.utilization_fee_earnings,
        )?);
        Ok(())
    }

    fn update_resolved_payout_blocker_total(&mut self, old: u64, new: u64) -> V16Result<()> {
        self.header.resolved_payout_blocker_count = V16PodU64::new(Self::apply_total_delta_u64(
            self.header.resolved_payout_blocker_count.get(),
            old,
            new,
        )?);
        Ok(())
    }

    // Senior backing-provider earnings (LP utilization fees) summed across every
    // domain — the same quantity validate_shape's senior stack includes.
    fn backing_provider_earnings_total(&self) -> u128 {
        self.header.backing_provider_earnings_total.get()
    }

    // Recoverable counterparty backing principal (Fresh-bucket unliened+liened
    // atoms across every domain): provider-withdrawable whenever the domain is
    // fully backed, with no mode or payout-snapshot gate, so it is a senior-side
    // claim on the vault — never junior surplus.
    fn source_fresh_backing_total_atoms(&self) -> u128 {
        self.header.source_fresh_backing_total_num.get() / BOUND_SCALE
    }

    // Junior (positive-PnL) payout pool = vault minus ALL senior claims: capital
    // (c_tot), insurance, backing-provider earnings, AND recoverable counterparty
    // backing principal. Omitting a senior claim here over-states the pool and
    // promises the same vault atoms to two parties: a haircut resolved-close
    // over-pays winners out of value its owner can still withdraw (or, when the
    // final validate_shape catches it, deadlocks the close permanently).
    fn residual(&self) -> u128 {
        self.header.vault.get().saturating_sub(
            self.header
                .c_tot
                .get()
                .saturating_add(self.header.insurance.get())
                .saturating_add(self.backing_provider_earnings_total())
                .saturating_add(self.source_fresh_backing_total_atoms()),
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
        let old_source = self.source_credit_for_domain_shape(domain)?;
        self.update_source_credit_aggregate_totals(old_source, source)?;
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
        let old_bucket = self.backing_bucket_for_domain(domain)?;
        self.update_backing_aggregate_totals(old_bucket, bucket)?;
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    fn reservation_encumbrance_proof_for_domain(
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

    #[cfg(any(kani, feature = "fuzz"))]
    pub fn add_fresh_counterparty_backing_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
        expiry_slot: u64,
    ) -> V16Result<()> {
        self.add_fresh_counterparty_backing_unchecked(domain, amount, expiry_slot)?;
        self.validate_shape()
    }

    /// Deposits external quote into a source domain's fresh counterparty backing.
    ///
    /// `amount` is quote atoms. The backing ledger stores BOUND_SCALE-scaled
    /// amounts, while vault stores quote atoms, so both sides move in one engine
    /// transition.
    pub fn deposit_fresh_counterparty_backing_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
        expiry_slot: u64,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let backing_num = V16Core::bound_num_from_amount(amount)?;
        self.add_fresh_counterparty_backing_unchecked(domain, backing_num, expiry_slot)?;
        self.header.vault = V16PodU128::new(
            self.header
                .vault
                .get()
                .checked_add(amount)
                .ok_or(V16Error::ArithmeticOverflow)?,
        );
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    /// Withdraws unliened fresh counterparty-backing principal.
    ///
    /// The withdrawal is allowed only if the source domain remains fully backed
    /// (`credit_rate_num == CREDIT_RATE_SCALE`) after the principal leaves. This
    /// prevents a backing provider from withdrawing principal that currently
    /// supports outstanding source-credit claims.
    pub fn withdraw_fresh_counterparty_backing_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let backing_num = V16Core::bound_num_from_amount(amount)?;
        let (bucket, source) = V16Core::prepare_counterparty_backing_withdraw_delta(
            self.backing_bucket_for_domain(domain)?,
            self.source_credit_for_domain(domain)?,
            backing_num,
        )?;
        let (source, next_risk_epoch) = V16Core::prepare_source_credit_domain_recompute_for_epoch(
            source,
            self.header.risk_epoch.get(),
        )?;
        if source.credit_rate_num != CREDIT_RATE_SCALE {
            return Err(V16Error::LockActive);
        }
        let next_vault = self
            .header
            .vault
            .get()
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
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
        self.header.vault = V16PodU128::new(next_vault);
        self.validate_source_domain_ledger(domain)?;
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    // Expiry-agnostic counterparty lien release for terminal (Resolved) wind-down; see
    // prepare_counterparty_lien_terminal_release_delta. Mirrors
    // release_source_credit_lien_from_counterparty_not_atomic but does not gate on
    // bucket freshness/expiry, so a market that resolves past a bucket's expiry can
    // still release the lien and wind the winner down.
    fn release_source_credit_lien_from_counterparty_terminal_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (bucket, source) = V16Core::prepare_counterparty_lien_terminal_release_delta(
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
        self.validate_shape()
    }

    #[cfg(any(kani, feature = "fuzz"))]
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

    #[cfg(any(kani, feature = "fuzz"))]
    pub fn consume_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.consume_source_credit_lien_from_counterparty_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    #[cfg(any(kani, feature = "fuzz"))]
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

    #[cfg(any(kani, feature = "fuzz"))]
    pub fn impair_source_credit_lien_from_counterparty_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.impair_source_credit_lien_from_counterparty_core_not_atomic(domain, amount)?;
        self.validate_shape()
    }

    #[cfg(any(kani, feature = "fuzz"))]
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
        let current_reserved_atoms =
            V16Core::amount_from_bound_num(current_reservation.insurance_credit_reserved_num)?;
        let new_reserved_atoms = V16Core::amount_from_bound_num(new_reserved)?;
        let live_source_credit_insurance_atoms = Self::apply_total_delta(
            self.header
                .source_insurance_credit_reserved_total_atoms
                .get(),
            current_reserved_atoms,
            new_reserved_atoms,
        )?;
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    fn release_source_credit_lien_from_insurance_terminal_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let (reservation, source) = V16Core::prepare_insurance_lien_terminal_release_delta(
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

    #[cfg(any(kani, feature = "fuzz"))]
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
        self.set_domain_insurance_spent_core(domain, next_domain_spent)?;
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    #[cfg(any(kani, feature = "fuzz"))]
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

    #[cfg(any(kani, feature = "fuzz"))]
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

    /// Credits already-collected backing utilization fees to the provider bucket.
    ///
    /// This does not debit an account or increase the vault; callers must have already
    /// moved value into vault slack. The method rejects if the resulting senior
    /// backing-provider claim would exceed vault coverage.
    // Contract layer: crediting provider earnings raises the global and
    // bucket earnings ledgers in exact lockstep and only succeeds when the
    // resulting senior stack (c_tot + insurance + earnings) stays vault-
    // covered — earnings can never be minted beyond vault backing.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128)>| match result {
        Ok((et, be)) => *et == earnings_total.wrapping_add(amount)
            && *be == bucket_earnings.wrapping_add(amount)
            && c_tot.wrapping_add(insurance) + *et <= vault,
        Err(_) => true,
    }))]
    fn credit_backing_provider_earnings_delta(
        vault: u128,
        c_tot: u128,
        insurance: u128,
        earnings_total: u128,
        bucket_earnings: u128,
        amount: u128,
    ) -> V16Result<(u128, u128)> {
        let next_earnings_total = earnings_total
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        let senior = c_tot
            .checked_add(insurance)
            .and_then(|v| v.checked_add(next_earnings_total))
            .ok_or(V16Error::ArithmeticOverflow)?;
        if senior > vault {
            return Err(V16Error::LockActive);
        }
        let next_bucket_earnings = bucket_earnings
            .checked_add(amount)
            .ok_or(V16Error::CounterOverflow)?;
        Ok((next_earnings_total, next_bucket_earnings))
    }

    pub fn credit_backing_provider_earnings_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        if amount == 0 {
            return Ok(());
        }
        let mut bucket = self.backing_bucket_for_domain(domain)?;
        if bucket.status != BackingBucketStatusV16::Fresh
            || bucket.expiry_slot <= self.header.current_slot.get()
        {
            return Err(V16Error::LockActive);
        }
        let (_, next_bucket_earnings) = Self::credit_backing_provider_earnings_delta(
            self.header.vault.get(),
            self.header.c_tot.get(),
            self.header.insurance.get(),
            self.header.backing_provider_earnings_total.get(),
            bucket.utilization_fee_earnings,
            amount,
        )?;
        bucket.utilization_fee_earnings = next_bucket_earnings;
        self.set_backing_bucket_for_domain(domain, bucket)?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    /// Charges an account-level backing fee and routes it to backing-provider
    /// earnings and/or domain insurance.
    ///
    /// The wrapper owns fee-policy calculation. The engine owns the value
    /// transition: account capital and `c_tot` decrease by
    /// `provider_fee + insurance_fee`; provider earnings and domain insurance
    /// budget increase by their exact routed amounts; vault is unchanged.
    pub fn charge_account_backing_fee_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        provider_domain: usize,
        provider_fee: u128,
        insurance_domain: usize,
        insurance_fee: u128,
    ) -> V16Result<u128> {
        self.domain_asset_side(provider_domain)?;
        self.domain_asset_side(insurance_domain)?;
        let total_fee = provider_fee
            .checked_add(insurance_fee)
            .ok_or(V16Error::ArithmeticOverflow)?;
        if total_fee == 0 {
            return Ok(0);
        }
        account.validate_with_market(&self.as_view())?;
        self.ensure_favorable_action_current_certificate(&account.as_view())?;
        if account.header.pnl.get() < 0 || account.header.capital.get() < total_fee {
            return Err(V16Error::LockActive);
        }
        let cert = health_cert_after_capital_debit(
            account.header.health_cert.try_to_runtime()?,
            total_fee,
        )?;
        account.header.capital = V16PodU128::new(
            account
                .header
                .capital
                .get()
                .checked_sub(total_fee)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        self.header.c_tot = V16PodU128::new(
            self.header
                .c_tot
                .get()
                .checked_sub(total_fee)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        account.header.health_cert = HealthCertV16Account::from_runtime(&cert);

        if provider_fee != 0 {
            let mut bucket = self.backing_bucket_for_domain(provider_domain)?;
            if bucket.status != BackingBucketStatusV16::Fresh
                || bucket.expiry_slot <= self.header.current_slot.get()
            {
                return Err(V16Error::LockActive);
            }
            bucket.utilization_fee_earnings = bucket
                .utilization_fee_earnings
                .checked_add(provider_fee)
                .ok_or(V16Error::CounterOverflow)?;
            self.set_backing_bucket_for_domain(provider_domain, bucket)?;
            self.validate_source_domain_ledger(provider_domain)?;
        }

        if insurance_fee != 0 {
            let next_insurance = self
                .header
                .insurance
                .get()
                .checked_add(insurance_fee)
                .ok_or(V16Error::CounterOverflow)?;
            let (budget, _) = self.domain_insurance_budget_spent(insurance_domain)?;
            let next_budget = budget
                .checked_add(insurance_fee)
                .ok_or(V16Error::CounterOverflow)?;
            self.header.insurance = V16PodU128::new(next_insurance);
            self.set_domain_insurance_budget_core(insurance_domain, next_budget, next_insurance)?;
            self.validate_source_domain_ledger(insurance_domain)?;
        }

        TokenValueFlowProofV16::account_capital_to_insurance(
            insurance_fee,
            self.header.vault.get(),
            self.header.vault.get(),
        )?
        .validate()?;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()?;
        Ok(total_fee)
    }

    fn account_source_claim_bound_sum_num(account: &PortfolioV16View<'_>) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = account.source_domains()[d];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            sum = sum
                .checked_add(source.source_claim_bound_num.get())
                .ok_or(V16Error::ArithmeticOverflow)?;
            d += 1;
        }
        Ok(sum)
    }

    fn account_has_source_claims(account: &PortfolioV16View<'_>) -> V16Result<bool> {
        Ok(Self::account_source_claim_bound_sum_num(account)? != 0)
    }

    fn source_claim_unliened_num(account: &PortfolioV16View<'_>, domain: usize) -> V16Result<u128> {
        let source = account.source_domain(domain)?;
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

    fn burn_impaired_account_source_claim_fields(
        account: &mut PortfolioV16ViewMut<'_>,
        slot: usize,
        burn_num: u128,
    ) -> V16Result<(u128, u128)> {
        if burn_num == 0 {
            return Ok((0, 0));
        }
        if slot >= PORTFOLIO_SOURCE_DOMAIN_CAP {
            return Err(V16Error::InvalidLeg);
        }
        let source = account.header.source_domains[slot];
        let impaired_burn = source.source_claim_impaired_num.get().min(burn_num);
        if impaired_burn == 0 {
            return Ok((0, 0));
        }
        let next_impaired = source
            .source_claim_impaired_num
            .get()
            .checked_sub(impaired_burn)
            .ok_or(V16Error::CounterUnderflow)?;
        let source = &mut account.header.source_domains[slot];
        source.source_claim_bound_num = V16PodU128::new(
            source
                .source_claim_bound_num
                .get()
                .checked_sub(impaired_burn)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        source.source_claim_impaired_num = V16PodU128::new(next_impaired);
        let impaired_effective_burn = if next_impaired == 0 {
            source.source_lien_impaired_effective_reserved.get()
        } else {
            V16Core::amount_from_bound_num(impaired_burn)?
                .min(source.source_lien_impaired_effective_reserved.get())
        };
        source.source_lien_impaired_effective_reserved = V16PodU128::new(
            source
                .source_lien_impaired_effective_reserved
                .get()
                .checked_sub(impaired_effective_burn)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        if next_impaired == 0 {
            source.source_lien_impaired_capital_at_risk_fee_revenue = V16PodU128::new(0);
        }
        Ok((impaired_burn, impaired_effective_burn))
    }

    #[cfg(any(kani, feature = "fuzz"))]
    fn impair_account_source_credit_insurance_lien_fields(
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        face: u128,
        effective: u128,
    ) -> V16Result<u128> {
        let source = account.source_domain_mut_or_insert(domain)?;
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
        // Genesis counter: move the pro-rata live capital-at-risk fee revenue to the impaired counter,
        // matched to the backing capital that actually crystallized. Compute BEFORE shrinking the live
        // effective reserve (the denominator). Floor rounding keeps dust with the still-live counter
        // (conservative for residual farming; never over-credits).
        let live_effective_before = source.source_lien_effective_reserved.get();
        let live_fee = source.source_lien_capital_at_risk_fee_revenue.get();
        let fee_to_crystallize = if live_effective_before == 0 || live_fee == 0 {
            0
        } else {
            U256::from_u128(live_fee)
                .checked_mul(U256::from_u128(effective))
                .ok_or(V16Error::ArithmeticOverflow)?
                .checked_div(U256::from_u128(live_effective_before))
                .ok_or(V16Error::ArithmeticOverflow)?
                .try_into_u128()
                .ok_or(V16Error::ArithmeticOverflow)?
        };
        source.source_lien_capital_at_risk_fee_revenue = V16PodU128::new(
            live_fee
                .checked_sub(fee_to_crystallize)
                .ok_or(V16Error::CounterUnderflow)?,
        );
        source.source_lien_impaired_capital_at_risk_fee_revenue = V16PodU128::new(
            source
                .source_lien_impaired_capital_at_risk_fee_revenue
                .get()
                .checked_add(fee_to_crystallize)
                .ok_or(V16Error::CounterOverflow)?,
        );
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

    // Release one domain's counterparty/insurance source-credit lien (returning the
    // reserved backing) and clear the account's per-domain lien fields. Mirrors the
    // per-domain body of release_account_source_credit_liens_if_unneeded_not_atomic
    // but with no mode gate, for use during terminal (Resolved) wind-down.
    fn release_account_source_credit_lien_for_domain_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        d: usize,
    ) -> V16Result<()> {
        let slot = account.source_domain_slot(d)?.ok_or(V16Error::InvalidLeg)?;
        let counterparty_backing = account.header.source_domains[slot]
            .source_lien_counterparty_backing_num
            .get();
        let insurance_backing = account.header.source_domains[slot]
            .source_lien_insurance_backing_num
            .get();
        if counterparty_backing != 0 {
            // Expiry-agnostic: terminal wind-down returns backing, so a time-expired
            // bucket must not block the release (Finding C).
            self.release_source_credit_lien_from_counterparty_terminal_not_atomic(
                d,
                counterparty_backing,
            )?;
        }
        if insurance_backing != 0 {
            self.release_source_credit_lien_from_insurance_terminal_not_atomic(
                d,
                insurance_backing,
            )?;
        }
        let source = &mut account.header.source_domains[slot];
        source.source_claim_liened_num = V16PodU128::new(0);
        source.source_claim_counterparty_liened_num = V16PodU128::new(0);
        source.source_claim_insurance_liened_num = V16PodU128::new(0);
        source.source_lien_effective_reserved = V16PodU128::new(0);
        source.source_lien_counterparty_backing_num = V16PodU128::new(0);
        source.source_lien_insurance_backing_num = V16PodU128::new(0);
        source.source_lien_fee_last_slot = V16PodU64::new(0);
        source.source_lien_capital_at_risk_fee_revenue = V16PodU128::new(0);
        account.reset_source_domain_slot_if_empty(slot);
        Ok(())
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP && burn_num != 0 {
            let source_snapshot = account.header.source_domains[slot];
            if source_snapshot.has_default_sparse_tag() && !source_snapshot.is_occupied() {
                break;
            }
            if !source_snapshot.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source_snapshot.domain.get() as usize;
            self.domain_asset_side(d)?;
            // Terminal wind-down: a counterparty/insurance source-credit lien is created
            // in Live to collateralize unrealized PnL and can only be released in Live.
            // Forcing the winner's claim to zero in Resolved (close_resolved ->
            // set_account_pnl(0)) would otherwise dead-lock on the liened portion
            // (burn can only consume the unliened part -> LockActive forever). In
            // Resolved mode release the domain's lien (returning backing) so the claim
            // is burnable and the account/market can actually wind down.
            if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved
                && account.header.source_domains[slot]
                    .source_claim_liened_num
                    .get()
                    != 0
            {
                self.release_account_source_credit_lien_for_domain_not_atomic(account, d)?;
            }
            let burnable = Self::source_claim_unliened_num(&account.as_view(), d)?;
            let burn = burnable.min(burn_num);
            if burn != 0 {
                let source = &mut account.header.source_domains[slot];
                source.source_claim_bound_num = V16PodU128::new(
                    source
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
                account.reset_source_domain_slot_if_empty(slot);
                self.recompute_source_credit_domain_after_mutation(d)?;
            }
            if burn_num != 0 {
                let (impaired_burn, impaired_effective_burn) =
                    Self::burn_impaired_account_source_claim_fields(account, slot, burn_num)?;
                if impaired_burn != 0 {
                    if decode_market_mode(self.header.mode)? == MarketModeV16::Resolved
                        && impaired_effective_burn != 0
                    {
                        let impaired_insurance_backing =
                            V16Core::bound_num_from_amount(impaired_effective_burn)?;
                        self.release_source_credit_lien_from_insurance_terminal_not_atomic(
                            d,
                            impaired_insurance_backing,
                        )?;
                    }
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
                    account.reset_source_domain_slot_if_empty(slot);
                    self.recompute_source_credit_domain_after_mutation(d)?;
                }
            }
            slot += 1;
        }
        if burn_num != 0 {
            return Err(V16Error::LockActive);
        }
        account.compact_source_domains();
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
        let mut remaining_num = V16Core::bound_num_from_amount(face_claim)?;
        let mut support_num = U256::ZERO;
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP && remaining_num != 0 {
            let source = account.source_domains()[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if !source.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source.domain.get() as usize;
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
            slot += 1;
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
        let mut remaining_num = V16Core::bound_num_from_amount(face_claim)?;
        let mut support_num = U256::ZERO;
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP && remaining_num != 0 {
            let source = account.source_domains()[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if !source.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source.domain.get() as usize;
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
            slot += 1;
        }
        support_num
            .checked_div(U256::from_u128(BOUND_SCALE))
            .and_then(|v| v.try_into_u128())
            .ok_or(V16Error::ArithmeticOverflow)
    }

    fn source_credit_available_backing_num(&self, domain: usize) -> V16Result<u128> {
        V16Core::available_backing_num_for_source_credit_state(
            self.source_credit_for_domain(domain)?,
        )
    }

    fn valid_source_lien_effective_reserved_sum(account: &PortfolioV16View<'_>) -> V16Result<u128> {
        let mut sum = 0u128;
        let mut d = 0usize;
        while d < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = account.source_domains()[d];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            sum = sum
                .checked_add(source.source_lien_effective_reserved.get())
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

    // Contract layer: re-pointing a domain's spent counter shifts the global
    // remaining-budget total by exactly the remaining delta and keeps the
    // total covered by insurance.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(old_spent <= budget && new_spent <= budget))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<u128>| match result {
        Ok(t) => {
            let old_remaining = budget.wrapping_sub(old_spent);
            let new_remaining = budget.wrapping_sub(new_spent);
            (if new_remaining >= old_remaining {
                *t == total_remaining + (new_remaining.wrapping_sub(old_remaining))
            } else {
                *t == total_remaining - (old_remaining.wrapping_sub(new_remaining))
            }) && *t <= insurance
        },
        Err(_) => true,
    }))]
    fn set_domain_insurance_spent_delta(
        total_remaining: u128,
        insurance: u128,
        budget: u128,
        old_spent: u128,
        new_spent: u128,
    ) -> V16Result<u128> {
        let old_remaining = Self::domain_budget_remaining_parts(budget, old_spent)?;
        let new_remaining = Self::domain_budget_remaining_parts(budget, new_spent)?;
        let next_total = Self::apply_total_delta(total_remaining, old_remaining, new_remaining)?;
        if next_total > insurance {
            return Err(V16Error::LockActive);
        }
        Ok(next_total)
    }

    fn set_domain_insurance_spent_core(&mut self, domain: usize, spent: u128) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let (old_budget, old_spent) = self.domain_insurance_budget_spent(domain)?;
        let next_total = Self::set_domain_insurance_spent_delta(
            self.header.insurance_domain_budget_remaining_total.get(),
            self.header.insurance.get(),
            old_budget,
            old_spent,
            spent,
        )?;
        self.header.insurance_domain_budget_remaining_total = V16PodU128::new(next_total);
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.insurance_domain_spent_long = V16PodU128::new(spent),
            SideV16::Short => slot.insurance_domain_spent_short = V16PodU128::new(spent),
        }
        Ok(())
    }

    /// Sets the spent amount for a domain insurance budget while preserving the
    /// aggregate remaining-budget invariant.
    pub fn set_domain_insurance_spent(&mut self, domain: usize, spent: u128) -> V16Result<()> {
        self.set_domain_insurance_spent_core(domain, spent)?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    fn clear_terminal_spent_domain_budget_pair(
        budget: V16PodU128,
        spent: V16PodU128,
    ) -> V16Result<(V16PodU128, V16PodU128)> {
        let budget = budget.get();
        let spent = spent.get();
        if spent == 0 {
            return Ok((V16PodU128::new(budget), V16PodU128::default()));
        }
        if budget != spent {
            return Err(V16Error::LockActive);
        }
        Ok((V16PodU128::default(), V16PodU128::default()))
    }

    fn set_domain_insurance_budget_core(
        &mut self,
        domain: usize,
        budget: u128,
        insurance_limit: u128,
    ) -> V16Result<()> {
        let (asset_index, side) = self.domain_asset_side(domain)?;
        let (old_budget, spent) = self.domain_insurance_budget_spent(domain)?;
        let next_total = Self::set_domain_insurance_budget_delta(
            self.header.insurance_domain_budget_remaining_total.get(),
            insurance_limit,
            old_budget,
            spent,
            budget,
        )?;
        self.header.insurance_domain_budget_remaining_total = V16PodU128::new(next_total);
        let slot = self.markets[asset_index].engine_slot_mut();
        match side {
            SideV16::Long => slot.insurance_domain_budget_long = V16PodU128::new(budget),
            SideV16::Short => slot.insurance_domain_budget_short = V16PodU128::new(budget),
        }
        Ok(())
    }

    // Contract layer: re-pointing a domain's budget shifts the global
    // remaining total by exactly the remaining delta, capped by the
    // insurance limit.
    #[cfg_attr(all(kani, feature = "contracts"), kani::requires(spent <= old_budget && spent <= new_budget))]
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<u128>| match result {
        Ok(t) => {
            let old_remaining = old_budget.wrapping_sub(spent);
            let new_remaining = new_budget.wrapping_sub(spent);
            (if new_remaining >= old_remaining {
                *t == total_remaining + (new_remaining.wrapping_sub(old_remaining))
            } else {
                *t == total_remaining - (old_remaining.wrapping_sub(new_remaining))
            }) && *t <= insurance_limit
        },
        Err(_) => true,
    }))]
    fn set_domain_insurance_budget_delta(
        total_remaining: u128,
        insurance_limit: u128,
        old_budget: u128,
        spent: u128,
        new_budget: u128,
    ) -> V16Result<u128> {
        let old_remaining = Self::domain_budget_remaining_parts(old_budget, spent)?;
        let new_remaining = Self::domain_budget_remaining_parts(new_budget, spent)?;
        let next_total = Self::apply_total_delta(total_remaining, old_remaining, new_remaining)?;
        if next_total > insurance_limit {
            return Err(V16Error::LockActive);
        }
        Ok(next_total)
    }

    fn set_domain_insurance_budget_not_atomic(
        &mut self,
        domain: usize,
        budget: u128,
    ) -> V16Result<()> {
        self.set_domain_insurance_budget_core(domain, budget, self.header.insurance.get())?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    /// Credits already-collected insurance to a domain budget without changing vault
    /// or total insurance.
    pub fn credit_domain_insurance_budget_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        let (budget, _) = self.domain_insurance_budget_spent(domain)?;
        let next_budget = budget
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_domain_insurance_budget_not_atomic(domain, next_budget)
    }

    /// Deposits external quote into insurance and credits the same domain budget.
    pub fn deposit_domain_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        let vault_before = self.header.vault.get();
        let next_vault = vault_before
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let next_insurance = self
            .header
            .insurance
            .get()
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let (budget, _) = self.domain_insurance_budget_spent(domain)?;
        let next_budget = budget
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;

        self.set_domain_insurance_budget_core(domain, next_budget, next_insurance)?;
        self.header.vault = V16PodU128::new(next_vault);
        self.header.insurance = V16PodU128::new(next_insurance);
        TokenValueFlowProofV16::external_in_to_insurance_capital(amount, vault_before, next_vault)?
            .validate()?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    // Contract layer: a domain-insurance withdrawal debits vault, insurance,
    // and the domain budget in exact lockstep, and is gated by BOTH the
    // globally-unreserved insurance and the domain's unreserved budget
    // remainder — reserved credit can never be withdrawn.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128, u128)>| match result {
        Ok((v, i, b)) => *v == vault.wrapping_sub(amount)
            && *i == insurance.wrapping_sub(amount)
            && *b == budget.wrapping_sub(amount)
            && amount <= insurance.saturating_sub(source_reserved_atoms)
            && amount <= budget.saturating_sub(spent).saturating_sub(domain_reserved_atoms),
        Err(_) => true,
    }))]
    fn withdraw_domain_insurance_delta(
        vault: u128,
        insurance: u128,
        source_reserved_atoms: u128,
        budget: u128,
        spent: u128,
        domain_reserved_atoms: u128,
        amount: u128,
    ) -> V16Result<(u128, u128, u128)> {
        let global_available = insurance.saturating_sub(source_reserved_atoms);
        let budget_remaining = budget
            .saturating_sub(spent)
            .saturating_sub(domain_reserved_atoms);
        if amount > global_available.min(budget_remaining) || amount > vault {
            return Err(V16Error::LockActive);
        }
        let next_vault = vault
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        let next_insurance = insurance
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        let next_budget = budget
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        Ok((next_vault, next_insurance, next_budget))
    }

    /// Withdraws available insurance from a single domain budget.
    pub fn withdraw_domain_insurance_not_atomic(
        &mut self,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        self.domain_asset_side(domain)?;
        let (budget, spent) = self.domain_insurance_budget_spent(domain)?;
        let domain_reserved_atoms = V16Core::amount_from_bound_num(
            self.insurance_reservation_for_domain(domain)?
                .insurance_credit_reserved_num,
        )?;
        let vault_before = self.header.vault.get();
        let (next_vault, next_insurance, next_budget) = Self::withdraw_domain_insurance_delta(
            vault_before,
            self.header.insurance.get(),
            self.header
                .source_insurance_credit_reserved_total_atoms
                .get(),
            budget,
            spent,
            domain_reserved_atoms,
            amount,
        )?;
        let next_insurance = V16PodU128::new(next_insurance);
        let next_vault = V16PodU128::new(next_vault);
        self.set_domain_insurance_budget_core(domain, next_budget, next_insurance.get())?;
        self.header.vault = next_vault;
        self.header.insurance = next_insurance;
        TokenValueFlowProofV16::insurance_capital_to_external_out(
            amount,
            vault_before,
            self.header.vault.get(),
        )?
        .validate()?;
        self.validate_source_domain_ledger(domain)?;
        self.validate_shape()
    }

    /// Pays an account from unbudgeted insurance surplus, e.g. a crank reward.
    ///
    /// Budgeted domain insurance remains isolated and cannot be consumed by this path.
    // Contract layer: the insurance->account credit moves exactly X from
    // insurance into BOTH the account capital and the senior capital stock
    // (vault flat at the caller), and never leaves insurance below the
    // remaining domain budget claims.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, u128, u128)>| match result {
        Ok((i, c, cap)) => *i == insurance.wrapping_sub(amount)
            && *c == c_tot.wrapping_add(amount)
            && *cap == capital.wrapping_add(amount)
            && *i >= budget_remaining,
        Err(_) => true,
    }))]
    fn credit_account_from_insurance_delta(
        insurance: u128,
        budget_remaining: u128,
        c_tot: u128,
        capital: u128,
        amount: u128,
    ) -> V16Result<(u128, u128, u128)> {
        let next_insurance = insurance
            .checked_sub(amount)
            .ok_or(V16Error::CounterUnderflow)?;
        if budget_remaining > next_insurance {
            return Err(V16Error::LockActive);
        }
        let next_c_tot = c_tot
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        let next_capital = capital
            .checked_add(amount)
            .ok_or(V16Error::ArithmeticOverflow)?;
        Ok((next_insurance, next_c_tot, next_capital))
    }

    pub fn credit_account_from_insurance_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        amount: u128,
    ) -> V16Result<()> {
        if amount == 0 {
            return Ok(());
        }
        account.validate_with_market(&self.as_view())?;
        let (next_insurance, next_c_tot, next_capital) = Self::credit_account_from_insurance_delta(
            self.header.insurance.get(),
            self.header.insurance_domain_budget_remaining_total.get(),
            self.header.c_tot.get(),
            account.header.capital.get(),
            amount,
        )?;
        let vault = self.header.vault.get();

        self.header.insurance = V16PodU128::new(next_insurance);
        self.header.c_tot = V16PodU128::new(next_c_tot);
        account.header.capital = V16PodU128::new(next_capital);
        account.header.health_cert.valid = 0;
        TokenValueFlowProofV16::insurance_capital_to_account_capital(amount, vault, vault)?
            .validate()?;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()
    }

    fn available_domain_insurance(&self, domain: usize) -> V16Result<u128> {
        let (budget, spent) = self.domain_insurance_budget_spent(domain)?;
        let domain_reserved_atoms = V16Core::amount_from_bound_num(
            self.insurance_reservation_for_domain(domain)?
                .insurance_credit_reserved_num,
        )?;
        let global_available = self.header.insurance.get().saturating_sub(
            self.header
                .source_insurance_credit_reserved_total_atoms
                .get(),
        );
        let budget_remaining = budget
            .saturating_sub(spent)
            .saturating_sub(domain_reserved_atoms);
        Ok(global_available.min(budget_remaining))
    }

    pub fn domain_insurance_budget_remaining(&self, domain: usize) -> V16Result<u128> {
        let (budget, spent) = self.domain_insurance_budget_spent(domain)?;
        Self::domain_budget_remaining_parts(budget, spent)
    }

    pub fn domain_insurance_withdraw_capacity(&self, domain: usize) -> V16Result<u128> {
        self.domain_asset_side(domain)?;
        Ok(self
            .available_domain_insurance(domain)?
            .min(self.header.vault.get()))
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
        let domain_available = self.available_domain_insurance(domain)?;
        let (_, spent_before) = self.domain_insurance_budget_spent(domain)?;
        // PRODUCTION KERNEL: insurance-draw core (capped by deficit AND domain
        // budget; conserves pool -> spent).
        let (used, next_insurance, next_spent, new_pnl) = V16Core::kernel_consume_insurance_layer(
            domain_available,
            self.header.insurance.get(),
            spent_before,
            account.header.pnl.get(),
        )?;
        if used == 0 {
            return Ok(0);
        }
        let vault_before = self.header.vault.get();
        self.header.insurance = V16PodU128::new(next_insurance);
        self.set_domain_insurance_spent_core(domain, next_spent)?;
        self.set_account_pnl(account, new_pnl)?;
        TokenValueFlowProofV16::validate_insurance_to_close_insurance_spent(
            used,
            vault_before,
            self.header.vault.get(),
        )?;
        account.header.health_cert.valid = 0;
        Ok(used)
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
        self.set_domain_insurance_spent_core(domain, next_domain_spent)?;
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP && remaining != 0 {
            let source = account.header.source_domains[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if !source.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source.domain.get() as usize;
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
            slot += 1;
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

    fn create_account_source_credit_lien_for_effective_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        effective_credit: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        self.domain_asset_side(domain)?;
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
        let source = account.source_domain_mut_or_insert(domain)?;
        Self::apply_account_source_credit_lien_delta(
            source,
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP && remaining != 0 {
            let source = account.header.source_domains[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if !source.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source.domain.get() as usize;
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
            slot += 1;
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
        let (asset_index, _) = self.domain_asset_side(domain)?;
        let market_id = self.markets[asset_index].engine.asset.market_id.get();
        if market_id == 0 {
            return Err(V16Error::InvalidLeg);
        }
        let source = account.source_domain_mut_or_insert(domain)?;
        source.domain =
            V16PodU32::new(u32::try_from(domain).map_err(|_| V16Error::ArithmeticOverflow)?);
        if source.source_claim_market_id.get() == 0 {
            source.source_claim_market_id = V16PodU64::new(market_id);
            return Ok(());
        }
        if source.source_claim_market_id.get() != market_id {
            return Err(V16Error::HiddenLeg);
        }
        Ok(())
    }

    fn record_account_residual_crystallized_loss(
        account: &mut PortfolioV16ViewMut<'_>,
        atoms: u128,
    ) -> V16Result<()> {
        if atoms == 0 {
            return Ok(());
        }
        account.header.residual_crystallized_loss_atoms_total = V16PodU128::new(
            account
                .header
                .residual_crystallized_loss_atoms_total
                .get()
                .checked_add(atoms)
                .ok_or(V16Error::CounterOverflow)?,
        );
        Ok(())
    }

    fn add_account_u128_counter(counter: &mut V16PodU128, atoms: u128) -> V16Result<()> {
        if atoms == 0 {
            return Ok(());
        }
        *counter = V16PodU128::new(
            counter
                .get()
                .checked_add(atoms)
                .ok_or(V16Error::CounterOverflow)?,
        );
        Ok(())
    }

    fn record_account_funding_flow(
        account: &mut PortfolioV16ViewMut<'_>,
        side: SideV16,
        f_delta: i128,
    ) -> V16Result<()> {
        if f_delta < 0 {
            let atoms = f_delta.unsigned_abs();
            match side {
                SideV16::Long => {
                    Self::add_account_u128_counter(
                        &mut account.header.funding_long_paid_atoms_total,
                        atoms,
                    )?;
                }
                SideV16::Short => {
                    Self::add_account_u128_counter(
                        &mut account.header.funding_short_paid_atoms_total,
                        atoms,
                    )?;
                }
            }
        } else if f_delta > 0 {
            let atoms = f_delta as u128;
            match side {
                SideV16::Long => {
                    Self::add_account_u128_counter(
                        &mut account.header.funding_long_received_atoms_total,
                        atoms,
                    )?;
                }
                SideV16::Short => {
                    Self::add_account_u128_counter(
                        &mut account.header.funding_short_received_atoms_total,
                        atoms,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn transfer_account_residual_reward_credit(
        trader: &mut PortfolioV16ViewMut<'_>,
        lp: &mut PortfolioV16ViewMut<'_>,
        principal_atoms: u128,
    ) -> V16Result<u128> {
        if principal_atoms == 0 {
            return Ok(0);
        }
        let crystallized = trader.header.residual_crystallized_loss_atoms_total.get();
        let spent = trader.header.residual_spent_principal_atoms_total.get();
        let available = crystallized
            .checked_sub(spent)
            .ok_or(V16Error::CounterUnderflow)?;
        let credit = available.min(principal_atoms);
        if credit == 0 {
            return Ok(0);
        }
        trader.header.residual_spent_principal_atoms_total =
            V16PodU128::new(spent.checked_add(credit).ok_or(V16Error::CounterOverflow)?);
        lp.header.residual_received_atoms_total = V16PodU128::new(
            lp.header
                .residual_received_atoms_total
                .get()
                .checked_add(credit)
                .ok_or(V16Error::CounterOverflow)?,
        );
        Ok(credit)
    }

    fn initial_margin_requirement_for_abs_q(
        config: V16Config,
        abs_q: u128,
        price: u64,
    ) -> V16Result<u128> {
        let notional = risk_notional_ceil(abs_q, price)?;
        margin_requirement(
            notional,
            config.initial_margin_bps,
            config.min_nonzero_im_req,
        )
    }

    fn increased_initial_margin_principal(
        config: V16Config,
        old_abs_q: u128,
        new_abs_q: u128,
        price: u64,
    ) -> V16Result<u128> {
        if new_abs_q <= old_abs_q {
            return Ok(0);
        }
        let old_req = Self::initial_margin_requirement_for_abs_q(config, old_abs_q, price)?;
        let new_req = Self::initial_margin_requirement_for_abs_q(config, new_abs_q, price)?;
        Ok(new_req.saturating_sub(old_req))
    }

    fn transfer_trade_residual_reward_credit(
        &self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        trade_preflight: &TradePositionPreflightV16,
        asset_index: usize,
    ) -> V16Result<()> {
        let config = self.header.config.try_to_runtime_shape()?;
        let price = self.asset_state(asset_index)?.effective_price;
        let long_principal = Self::increased_initial_margin_principal(
            config,
            trade_preflight.long_old_abs_q,
            trade_preflight.long_new_abs_q,
            price,
        )?;
        let short_principal = Self::increased_initial_margin_principal(
            config,
            trade_preflight.short_old_abs_q,
            trade_preflight.short_new_abs_q,
            price,
        )?;
        if trade_preflight.long_new_abs_q > trade_preflight.long_old_abs_q {
            Self::transfer_account_residual_reward_credit(
                long_account,
                short_account,
                short_principal,
            )?;
        }
        if trade_preflight.short_new_abs_q > trade_preflight.short_old_abs_q {
            Self::transfer_account_residual_reward_credit(
                short_account,
                long_account,
                long_principal,
            )?;
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

    /// Grants source-attributed positive PnL to an account — the first-class
    /// API for crediting a source-backed claim (e.g. an external settlement
    /// feed). Live-mode only. The grant is pure notional attribution: no quote
    /// value moves; the account claim bound, the domain claim bound/exact, and
    /// the group junior aggregates move in lockstep through the same canonical
    /// setter every internal attribution uses, and the final shape validation
    /// re-checks the global junior-bound >= sum(domain claims) invariant.
    pub fn add_account_source_positive_pnl_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
        amount: u128,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        if amount == 0 {
            return Ok(());
        }
        let delta = i128::try_from(amount).map_err(|_| V16Error::ArithmeticOverflow)?;
        let new_pnl = account
            .header
            .pnl
            .get()
            .checked_add(delta)
            .ok_or(V16Error::ArithmeticOverflow)?;
        self.set_account_pnl_with_source(account, new_pnl, domain)?;
        account.header.health_cert.valid = 0;
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())
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
                let source = account.source_domain_mut_or_insert(domain)?;
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
        Self::record_account_residual_crystallized_loss(account, backing)?;
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
        Self::kf_target_for_leg_from_asset(asset, leg)
    }

    fn kf_target_for_leg_from_asset(
        asset: AssetStateV16,
        leg: PortfolioLegV16,
    ) -> V16Result<(i128, i128)> {
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
        Self::b_target_for_leg_from_asset(asset, leg)
    }

    fn b_target_for_leg_from_asset(asset: AssetStateV16, leg: PortfolioLegV16) -> V16Result<u128> {
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
    fn leg_kf_delta_components_for_settlement_from_asset(
        asset: AssetStateV16,
        leg: PortfolioLegV16,
    ) -> V16Result<(i128, i128, i128, i128, i128)> {
        let (k_now, f_now) = Self::kf_target_for_leg_from_asset(asset, leg)?;
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
        Ok((k_now, f_now, k_delta, f_delta, net))
    }

    #[cfg(kani)]
    #[inline(always)]
    fn leg_kf_delta_for_settlement_from_asset(
        asset: AssetStateV16,
        leg: PortfolioLegV16,
    ) -> V16Result<(i128, i128, i128)> {
        let (k_now, f_now, _k_delta, _f_delta, net) =
            Self::leg_kf_delta_components_for_settlement_from_asset(asset, leg)?;
        Ok((k_now, f_now, net))
    }

    #[cfg(kani)]
    #[inline(always)]
    fn leg_kf_delta_for_settlement(&self, leg: PortfolioLegV16) -> V16Result<(i128, i128, i128)> {
        let asset = self.asset_state(leg.asset_index as usize)?;
        Self::leg_kf_delta_for_settlement_from_asset(asset, leg)
    }

    fn settle_leg_kf_effects_at_slot(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        leg_slot: usize,
    ) -> V16Result<()> {
        if leg_slot >= V16_MAX_PORTFOLIO_ASSETS_N {
            return Err(V16Error::InvalidLeg);
        }
        let leg = account.header.legs[leg_slot].try_to_runtime()?;
        if !leg.active {
            return Ok(());
        }
        let asset_index = leg.asset_index as usize;
        let asset = self.asset_state(asset_index)?;
        self.settle_leg_kf_effects_at_slot_with_asset(account, leg_slot, asset)
    }

    fn settle_leg_kf_effects_at_slot_with_asset(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        leg_slot: usize,
        asset: AssetStateV16,
    ) -> V16Result<()> {
        if leg_slot >= V16_MAX_PORTFOLIO_ASSETS_N {
            return Err(V16Error::InvalidLeg);
        }
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
        if !leg.active {
            return Ok(());
        }
        let asset_index = leg.asset_index as usize;
        if asset_index >= self.header.config.max_market_slots.get() as usize
            || asset_index >= self.markets.len()
            || asset.market_id == 0
        {
            return Err(V16Error::InvalidLeg);
        }
        let (k_now, f_now, _k_delta, f_delta, net) =
            Self::leg_kf_delta_components_for_settlement_from_asset(asset, leg)?;
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
        Self::record_account_funding_flow(account, leg.side, f_delta)?;
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
        let source_claim_sum_num = if account.header.source_domains[0].is_sparse_tail_default() {
            0
        } else {
            account
                .as_view()
                .validate_source_credit_shape_with_market(&self.as_view())?;
            Self::account_source_claim_bound_sum_num(&account.as_view())?
        };
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
            self.settle_leg_kf_effects_at_slot_with_asset(account, slot, asset)?;
            let mut refreshed = account.header.legs[slot].try_to_runtime()?;
            let target = Self::b_target_for_leg_from_asset(asset, refreshed)?;
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
        if decode_bool(account.header.b_stale_state)? {
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = account.header.source_domains[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if !source.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source.domain.get() as usize;
            total_charged = total_charged
                .checked_add(
                    self.collect_account_backing_utilization_fee_for_domain_not_atomic(account, d)?,
                )
                .ok_or(V16Error::ArithmeticOverflow)?;
            slot += 1;
        }
        account.compact_source_domains();
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
        let slot = match account.source_domain_slot(domain)? {
            Some(slot) => slot,
            None => return Ok(0),
        };
        let lien_backing_num = account.header.source_domains[slot]
            .source_lien_counterparty_backing_num
            .get();
        if lien_backing_num == 0 {
            account.header.source_domains[slot].source_lien_fee_last_slot = V16PodU64::new(0);
            account.reset_source_domain_slot_if_empty(slot);
            return Ok(0);
        }
        let last_slot = account.header.source_domains[slot]
            .source_lien_fee_last_slot
            .get();
        if last_slot == 0 {
            account.header.source_domains[slot].source_lien_fee_last_slot =
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
        // Carry the floored-away fractional accrual forward: when the rent quote
        // floors to zero this interval, do NOT advance the fee cursor, so a later
        // collection over a longer interval crosses >= 1 atom (no free lien). The
        // lien_backing_num == 0 and last_slot == 0 cases were handled above, so a
        // stuck cursor here only ever means real, still-accruing rent.
        if fee == 0 {
            return Ok(0);
        }
        account.header.source_domains[slot].source_lien_fee_last_slot =
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
        // Genesis counter: this fee was charged while the domain's backing lien was live and at risk
        // (lien_backing_num > 0 above), so it is capital-at-risk fee revenue for this source domain.
        account.header.source_domains[slot].source_lien_capital_at_risk_fee_revenue =
            V16PodU128::new(
                account.header.source_domains[slot]
                    .source_lien_capital_at_risk_fee_revenue
                    .get()
                    .checked_add(charged)
                    .ok_or(V16Error::CounterOverflow)?,
            );
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

    fn settle_account_b_chunk(
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
        let leg = account.header.legs[leg_slot].try_to_runtime()?;
        let leg = V16Core::kernel_advance_leg_b_snap(
            leg,
            chunk.delta_b,
            chunk.new_remainder,
            chunk.remaining_after,
        )?;
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

    fn require_asset_mark_pushable(&self, asset_index: usize) -> V16Result<()> {
        match self.asset_state(asset_index)?.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => Ok(()),
            _ => Err(V16Error::LockActive),
        }
    }

    fn asset_local_has_position_or_loss_state(&self, asset_index: usize) -> V16Result<bool> {
        self.validate_configured_asset_index(asset_index)?;
        let asset = self.asset_state(asset_index)?;
        let slot = self.markets[asset_index].engine_slot();
        Ok(asset.oi_eff_long_q != 0
            || asset.oi_eff_short_q != 0
            || asset.stored_pos_count_long != 0
            || asset.stored_pos_count_short != 0
            || asset.stale_account_count_long != 0
            || asset.stale_account_count_short != 0
            || asset.b_long_num != 0
            || asset.b_short_num != 0
            || asset.b_epoch_start_long_num != 0
            || asset.b_epoch_start_short_num != 0
            || asset.loss_weight_sum_long != 0
            || asset.loss_weight_sum_short != 0
            || asset.social_loss_remainder_long_num != 0
            || asset.social_loss_remainder_short_num != 0
            || asset.social_loss_dust_long_num != 0
            || asset.social_loss_dust_short_num != 0
            || asset.explicit_unallocated_loss_long != 0
            || asset.explicit_unallocated_loss_short != 0
            || asset.mode_long != SideModeV16::Normal
            || asset.mode_short != SideModeV16::Normal
            || slot.pending_domain_loss_barrier_long.get() != 0
            || slot.pending_domain_loss_barrier_short.get() != 0)
    }

    fn group_has_position_or_loss_state_for_oracle_reset(&self) -> V16Result<bool> {
        if self.header.pnl_pos_tot.get() != 0
            || self.header.stale_certificate_count.get() != 0
            || self.header.b_stale_account_count.get() != 0
            || self.header.negative_pnl_account_count.get() != 0
            || decode_bool(self.header.bankruptcy_hlock_active)?
            || decode_bool(self.header.threshold_stress_active)?
            || decode_bool(self.header.loss_stale_active)?
            || self.header.recovery_reason.try_to_runtime()?.is_some()
        {
            return Ok(true);
        }
        let configured_assets = self.header.config.max_market_slots.get() as usize;
        let mut i = 0usize;
        while i < configured_assets {
            if self.asset_local_has_position_or_loss_state(i)? {
                return Ok(true);
            }
            i += 1;
        }
        Ok(false)
    }

    pub fn set_asset_raw_oracle_target_not_atomic(
        &mut self,
        asset_index: usize,
        raw_oracle_target_price: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            || raw_oracle_target_price == 0
            || raw_oracle_target_price > MAX_ORACLE_PRICE
        {
            return Err(V16Error::InvalidConfig);
        }
        self.require_asset_mark_pushable(asset_index)?;
        let mut asset = self.asset_state(asset_index)?;
        asset.raw_oracle_target_price = raw_oracle_target_price;
        self.set_asset_state(asset_index, asset)?;
        self.validate_shape()
    }

    pub fn reset_empty_asset_oracle_anchor_not_atomic(
        &mut self,
        asset_index: usize,
        authenticated_price: u64,
        now_slot: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            || authenticated_price == 0
            || authenticated_price > MAX_ORACLE_PRICE
            || now_slot < self.header.current_slot.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        let mut asset = self.asset_state(asset_index)?;
        if asset.lifecycle != AssetLifecycleV16::Active
            || self.group_has_position_or_loss_state_for_oracle_reset()?
        {
            return Err(V16Error::LockActive);
        }
        asset.raw_oracle_target_price = authenticated_price;
        asset.effective_price = authenticated_price;
        asset.fund_px_last = authenticated_price;
        asset.slot_last = now_slot;
        self.set_asset_state(asset_index, asset)?;
        self.header.current_slot = V16PodU64::new(now_slot);
        self.header.slot_last = V16PodU64::new(now_slot);
        self.validate_shape()
    }

    pub fn force_asset_recovery_not_atomic(
        &mut self,
        asset_index: usize,
        now_slot: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            || now_slot < self.header.current_slot.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        let asset = self.asset_state(asset_index)?;
        match asset.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => {
                let (asset, next_asset_set_epoch, next_risk_epoch) =
                    V16Core::prepare_asset_recovery_transition(
                        asset,
                        self.header.asset_set_epoch.get(),
                        self.header.risk_epoch.get(),
                    )?;
                self.set_asset_state(asset_index, asset)?;
                self.commit_asset_set_epoch_bump(next_asset_set_epoch, next_risk_epoch);
                self.validate_shape()
            }
            AssetLifecycleV16::Recovery => self.validate_shape(),
            _ => Err(V16Error::LockActive),
        }
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
        // Hot paths are asset-local: scanning all markets here makes every
        // crank/trade depend on total dynamic asset count. `slot_last` and
        // `loss_stale_active` summarize only the touched asset; safety gates
        // use account/asset-local stale checks.
        self.header.slot_last = V16PodU64::new(asset.slot_last);
        self.header.loss_stale_active = encode_bool(asset.slot_last < now_slot);
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

    fn declare_permissionless_recovery(
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

    /// INTERNAL dispatch primitive (NOT a public/wrapper entrypoint): executes one
    /// CALLER-CHOSEN action (Refresh / SettleB / Liquidate / Recover). It is unsafe
    /// for opportunistic, out-of-order keepers because the caller's chosen action
    /// can be stale by execution time. The single public permissionless route is
    /// `permissionless_auto_crank_not_atomic`, which selects the action + asset from
    /// current on-chain state; it dispatches to this primitive. pub(crate) so the
    /// wrapper sees one crank API; the proof/fuzz suites reach it via
    /// `kani_permissionless_crank`.
    pub(crate) fn permissionless_crank_not_atomic(
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

    /// PRODUCTION CLASSIFIER (roadmap 3C step 4): map the real account/market
    /// state to the ActionableState summary the self-classifying crank dispatches
    /// from. Each flag is exactly its production eligibility predicate, MODE-
    /// GATED so every flag that can be set has a currently-valid dispatch target:
    ///   stale            — Live, health cert not current (kernel_cert_is_current==false)
    ///   b_stale          — Live, some active leg flagged b-stale (has_b_stale_leg)
    ///   pending_close    — Live, a close-progress ledger is active
    ///   expired_close    — Live, that ledger is past its max-close slot
    ///   liquidatable     — Live, current cert with nonzero certified liq deficit
    ///   recovery_eligible— Resolved, unattributed-insolvent negative-PnL recovery
    ///   resolved_winner  — Resolved, positive PnL, resolved payout ready
    /// Assembled via the proven actionable_summary_from_signals kernel. Live-only
    /// flags need cert currentness only where their entrypoint does (liquidate),
    /// so refresh is selected first for a stale account and the deficit is read
    /// from a fresh cert on the next step.
    pub fn build_actionable_summary(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<ActionableSummaryV16> {
        let mode = decode_market_mode(self.header.mode)?;
        let live = mode == MarketModeV16::Live;
        let resolved = mode == MarketModeV16::Resolved;

        let cert = account.header.health_cert.try_to_runtime()?;
        let cert_current = V16Core::kernel_cert_is_current(
            cert,
            self.header.oracle_epoch.get(),
            self.header.funding_epoch.get(),
            self.header.risk_epoch.get(),
            self.header.asset_set_epoch.get(),
            account.header.active_bitmap.map(V16PodU64::get),
        );
        let ledger = account.header.close_progress.try_to_runtime()?;

        let has_open_risk =
            !active_bitmap_is_empty(account.header.active_bitmap.map(V16PodU64::get));
        // A close ledger with residual_remaining==0 is already fully booked/covered
        // (e.g. insurance absorbed the loss); only OUTSTANDING residual is real,
        // actionable close work. The `active` flag can linger past that.
        let close_outstanding = ledger.active && ledger.residual_remaining > 0;
        let stale = live && !cert_current;
        let b_stale = live && Self::has_b_stale_leg(account)?;
        // pending_close is NOT proactively classified: the close-ledger residual is
        // booked ONLY inside the liquidation/resolved path that owns it
        // (book_bankruptcy_residual_chunk_for_account_core) — settle_account_b_chunk
        // does not touch it, so an AdvanceClose->SettleB dispatch would not advance
        // the ledger. An outstanding Live close with a leg is liquidatable (the
        // residual is an open deficit), so the Liquidate continuation books the
        // residual chunk; the leg-less / expired cases are handled by expired_close
        // -> recovery or are the documented backstopped A3 route. AdvanceClose is
        // therefore classifier-unreachable (the proven selector still admits it).
        let pending_close = false;
        // Expired outstanding close -> terminal recovery (Recover needs no leg).
        let expired_close =
            live && close_outstanding && self.header.current_slot.get() > ledger.max_close_slot;
        // liquidatable requires a current certified deficit AND actual open risk:
        // a stale cert can still report a deficit after the position was already
        // closed, but with no active leg there is nothing to liquidate (the real
        // liquidate entrypoint requires an active leg), so the flag must be false.
        let liquidatable = live && cert_current && cert.certified_liq_deficit != 0 && has_open_risk;
        // Permissionless recovery (declare_permissionless_recovery) is a LIVE-mode
        // action — it rejects Resolved mode with LockActive. The proactive Live
        // recovery condition the auto-crank declares is an EXPIRED outstanding
        // close (expired_close -> DeclareRecovery, reason
        // ActiveBankruptCloseCannotProgress); every other recovery reason is
        // declared REACTIVELY inside the dispatched crank op when it detects
        // non-progress (BIndexHeadroomExhausted, etc.). Resolved bad-debt
        // wind-down is handled by CloseResolved itself, not by the recovery
        // selector flag, so recovery_eligible stays in the summary type for the
        // proven selector but is driven by expired_close.
        let recovery_eligible = false;
        // resolved_winner routes to close_resolved, which LAZILY captures the
        // payout snapshot itself (initialize_resolved_payout_ledger_if_needed is
        // reached only via close_resolved -> create_resolved_payout_receipt) — so
        // we must NOT gate on payout_snapshot_captured: doing so would deadlock the
        // FIRST winner (snapshot never captured -> never classified -> never
        // captured). resolved_positive_payout_ready (all blocking counts zero) is
        // exactly close_resolved's own precondition for proceeding with payout.
        let resolved_winner =
            resolved && account.header.pnl.get() > 0 && self.resolved_positive_payout_ready()?;

        Ok(V16Core::actionable_summary_from_signals(
            stale,
            b_stale,
            pending_close,
            expired_close,
            liquidatable,
            recovery_eligible,
            resolved_winner,
        ))
    }

    /// PRODUCTION SELF-CLASSIFYING CRANK (roadmap 3C step 4): the keeper no longer
    /// chooses the action. build_actionable_summary classifies the account and
    /// the proven select_progress_witness picks the unique, overlap-safe,
    /// highest-priority continuation, which this dispatches to the matching proven
    /// entrypoint. The caller supplies only the unavoidable oracle observation +
    /// action parameters via the hint. Returns the selected continuation (None
    /// when not actionable) and the dispatched outcome. Each continuation's
    /// dispatch target is valid in the mode that the classifier gated its flag to.
    /// ENGINE asset self-selection (engine.md): scan the account's bounded legs and
    /// return, for each asset-scoped continuation, the engine-chosen asset_index —
    /// the FIRST active b-stale leg's asset (SettleBChunk), and the FIRST active
    /// leg's asset (used for both Liquidate and the refresh accrual target). The
    /// selection is proven in-range / actionable / first-match / complete by the
    /// first_actionable_slot contract; the slot->asset_index read is by inspection.
    fn auto_crank_selected_assets(
        account: &PortfolioV16View<'_>,
    ) -> V16Result<(Option<usize>, Option<usize>)> {
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut active_flags = [false; V16_MAX_PORTFOLIO_ASSETS_N];
        let mut b_stale_flags = [false; V16_MAX_PORTFOLIO_ASSETS_N];
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            let leg = account.header.legs[slot].try_to_runtime()?;
            let active = active_bitmap_get(bitmap, slot) && leg.active;
            active_flags[slot] = active;
            b_stale_flags[slot] = active && leg.b_stale;
            slot += 1;
        }
        let asset_of = |s: Option<usize>| -> V16Result<Option<usize>> {
            match s {
                Some(i) => Ok(Some(
                    account.header.legs[i].try_to_runtime()?.asset_index as usize,
                )),
                None => Ok(None),
            }
        };
        let b_stale_asset = asset_of(V16Core::first_actionable_slot(b_stale_flags))?;
        let active_asset = asset_of(V16Core::first_actionable_slot(active_flags))?;
        Ok((b_stale_asset, active_asset))
    }

    /// THE SINGLE PUBLIC PERMISSIONLESS CRANK (engine.md): the only crank the
    /// wrapper should call — order-insensitive and engine-selected, built for a
    /// swarm of opportunistic keepers whose transactions land out of order. The
    /// per-action primitives (refresh / settle-B / liquidate / recover /
    /// close-resolved) are internal dispatch targets, not wrapper entrypoints.
    ///
    /// Calling convention (wrapper side):
    /// 1. Decode a public auto-crank instruction carrying a bounded set of oracle
    ///    OBSERVATIONS (asset, authenticated price, funding) — one per asset the
    ///    keeper has fresh data for — plus the `resolved_close_fee_rate_per_slot`.
    /// 2. Authenticate clock/slot + each observation against the oracle.
    /// 3. Build `AutoCrankWorkV16 { now_slot, observations,
    ///    resolved_close_fee_rate_per_slot }` and call this ONCE (one ix = one
    ///    step; never loop to a fixed point — CU).
    /// 4. The engine classifies the account, selects the highest-priority step AND
    ///    its asset (self-selected — the caller never chooses action or asset),
    ///    matches the observation that step needs, derives the liquidation fee from
    ///    CONFIG (never the caller), and dispatches one bounded primitive.
    /// 5. On `Ok(result)`, mirror any wrapper-owned token/custody movement keyed off
    ///    `result.selected` (the `AutoCrankPlanV16`): refresh / settle-B move no
    ///    custody; liquidate / close-resolved may. `NoAction` => nothing was needed.
    ///    `Err(NonProgress)` => the selected step needed an observation the caller
    ///    did not supply (currently the no-active-asset refresh fallback, or a
    ///    stale/late tx whose task changed) — no mutation, so SVM rollback is a
    ///    clean no-op and arbitrary landing order is safe.
    pub fn permissionless_auto_crank_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        work: AutoCrankWorkV16<'_>,
    ) -> V16Result<AutoCrankResultV16> {
        let summary = self.build_actionable_summary(&account.as_view())?;
        let (b_stale_asset, active_asset) = Self::auto_crank_selected_assets(&account.as_view())?;
        let recovery_reason = if summary.expired_close {
            PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress
        } else {
            PermissionlessRecoveryReasonV16::ExplicitLossOrDustAuditOverflow
        };
        // PRODUCTION KERNEL: the proven plan selector (priority + totality +
        // engine-selected asset). refresh accrues the first active leg's asset.
        let plan = V16Core::select_auto_crank_plan(
            summary,
            b_stale_asset.unwrap_or(0),
            active_asset.unwrap_or(0),
            active_asset,
            recovery_reason,
        );

        let obs_or_current_asset = |me: &Self, i: usize| -> V16Result<AutoCrankObservationV16> {
            match work
                .observations
                .iter()
                .copied()
                .find(|o| o.asset_index == i)
            {
                Some(obs) => Ok(obs),
                None => {
                    let asset = me.asset_state(i)?;
                    Ok(AutoCrankObservationV16 {
                        asset_index: i,
                        effective_price: asset.effective_price,
                        funding_rate_e9: 0,
                    })
                }
            }
        };
        let crank_with = |me: &mut Self,
                          account: &mut PortfolioV16ViewMut<'_>,
                          asset_index: usize,
                          obs: AutoCrankObservationV16,
                          action: PermissionlessCrankActionV16|
         -> V16Result<PermissionlessProgressOutcomeV16> {
            me.permissionless_crank_not_atomic(
                account,
                PermissionlessCrankRequestV16 {
                    now_slot: work.now_slot,
                    asset_index,
                    effective_price: obs.effective_price,
                    funding_rate_e9: obs.funding_rate_e9,
                    action,
                },
            )
        };

        let outcome = match plan {
            AutoCrankPlanV16::NoAction => AutoCrankOutcomeV16::NoAction,
            AutoCrankPlanV16::RefreshAccount { asset_index } => {
                // Accrue the engine-selected active asset from either a fresh
                // observation or committed state; if no active asset exists, use
                // the first supplied observation to give the refresh a price.
                let (ai, obs) = match asset_index {
                    Some(i) => (i, obs_or_current_asset(self, i)?),
                    None => {
                        let o = work
                            .observations
                            .first()
                            .copied()
                            .ok_or(V16Error::NonProgress)?;
                        (o.asset_index, o)
                    }
                };
                AutoCrankOutcomeV16::Progressed(crank_with(
                    self,
                    account,
                    ai,
                    obs,
                    PermissionlessCrankActionV16::Refresh,
                )?)
            }
            AutoCrankPlanV16::SettleBChunk { asset_index } => {
                let obs = obs_or_current_asset(self, asset_index)?;
                AutoCrankOutcomeV16::Progressed(crank_with(
                    self,
                    account,
                    asset_index,
                    obs,
                    PermissionlessCrankActionV16::SettleB { asset_index },
                )?)
            }
            AutoCrankPlanV16::Liquidate { asset_index } => {
                let obs = obs_or_current_asset(self, asset_index)?;
                AutoCrankOutcomeV16::Progressed(crank_with(
                    self,
                    account,
                    asset_index,
                    obs,
                    PermissionlessCrankActionV16::Liquidate(LiquidationRequestV16 { asset_index }),
                )?)
            }
            AutoCrankPlanV16::DeclareRecovery { reason } => {
                // recovery declaration needs no observation.
                AutoCrankOutcomeV16::Progressed(self.permissionless_crank_not_atomic(
                    account,
                    PermissionlessCrankRequestV16 {
                        now_slot: work.now_slot,
                        asset_index: 0,
                        effective_price: 0,
                        funding_rate_e9: 0,
                        action: PermissionlessCrankActionV16::Recover(reason),
                    },
                )?)
            }
            AutoCrankPlanV16::CloseResolved => {
                AutoCrankOutcomeV16::ResolvedClose(self.close_resolved_account_not_atomic(
                    account,
                    work.resolved_close_fee_rate_per_slot,
                )?)
            }
        };
        Ok(AutoCrankResultV16 {
            selected: plan,
            outcome,
        })
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
        let old_blockers =
            slot_resolved_payout_blockers_v16(self.markets[asset_index].engine_slot())?;
        let new_blockers = {
            let slot = self.markets[asset_index].engine_slot_mut();
            let old_asset = slot.asset;
            slot.asset = AssetStateV16Account::from_runtime(&asset);
            match slot_resolved_payout_blockers_v16(slot) {
                Ok(value) => value,
                Err(err) => {
                    slot.asset = old_asset;
                    return Err(err);
                }
            }
        };
        self.update_resolved_payout_blocker_total(old_blockers, new_blockers)?;
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
        MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static(
            self.markets.len(),
            self.header.asset_slot_capacity.get() as usize,
            self.header.config.max_market_slots.get() as usize,
        )?;
        #[cfg(any(test, kani, feature = "audit-scan"))]
        {
            self.header
                .validate_dynamic_market_slots_shape(self.markets)?;
        }
        Ok(())
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

    // Contract: asset restart bumps each of the four global counters by
    // exactly one (fresh market id, activation count, asset-set epoch, risk
    // epoch) -- a new market identity that fails-closed old market-id-bound
    // legs (#29 lifecycle).
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u64, u64, u64, u64)>| match result {
        Ok((m, a, ase, re)) => *m == next_market_id_before.wrapping_add(1)
            && *a == activation_count_before.wrapping_add(1)
            && *ase == asset_set_epoch_before.wrapping_add(1)
            && *re == risk_epoch_before.wrapping_add(1),
        Err(_) => true,
    }))]
    fn asset_restart_next_counters(
        next_market_id_before: u64,
        activation_count_before: u64,
        asset_set_epoch_before: u64,
        risk_epoch_before: u64,
    ) -> V16Result<(u64, u64, u64, u64)> {
        Ok((
            next_market_id_before
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
            activation_count_before
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
            asset_set_epoch_before
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
            risk_epoch_before
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
        ))
    }

    // Contract (#29 lifecycle safety): an asset restart PRESERVES the domain
    // insurance budgets exactly and resets EVERYTHING else to a fresh Active
    // asset at the new market id and authenticated price -- no old position,
    // backing, source-credit, lien, B, or barrier state survives (those live
    // in the EMPTY slot the restart builds), so a restarted slot cannot carry
    // latent funds or risk while the insurance authority's budget is retained.
    fn restarted_asset_slot_preserving_insurance_budget(
        old_slot: &EngineAssetSlotV16Account,
        market_id: u64,
        authenticated_price: u64,
        now_slot: u64,
    ) -> EngineAssetSlotV16Account {
        let mut asset = AssetStateV16::default();
        asset.market_id = market_id;
        asset.lifecycle = AssetLifecycleV16::Active;
        asset.raw_oracle_target_price = authenticated_price;
        asset.effective_price = authenticated_price;
        asset.fund_px_last = authenticated_price;
        asset.slot_last = now_slot;
        let mut restarted = EngineAssetSlotV16Account::empty_for_market(market_id);
        restarted.asset = AssetStateV16Account::from_runtime(&asset);
        restarted.insurance_domain_budget_long = old_slot.insurance_domain_budget_long;
        restarted.insurance_domain_budget_short = old_slot.insurance_domain_budget_short;
        restarted
    }

    fn canonical_retired_asset_slot(old_asset: AssetStateV16) -> EngineAssetSlotV16Account {
        let mut canonical_asset = AssetStateV16::default();
        canonical_asset.market_id = old_asset.market_id;
        canonical_asset.retired_slot = old_asset.retired_slot;
        canonical_asset.lifecycle = AssetLifecycleV16::Retired;
        canonical_asset.raw_oracle_target_price = old_asset.raw_oracle_target_price;
        canonical_asset.effective_price = old_asset.effective_price;
        canonical_asset.fund_px_last = old_asset.fund_px_last;
        canonical_asset.slot_last = old_asset.slot_last;
        let mut canonical_slot = EngineAssetSlotV16Account::empty_for_market(old_asset.market_id);
        canonical_slot.asset = AssetStateV16Account::from_runtime(&canonical_asset);
        canonical_slot
    }

    fn require_asset_live_reducible(&self, asset_index: usize) -> V16Result<()> {
        let asset = self.asset_state(asset_index)?;
        match asset.lifecycle {
            AssetLifecycleV16::Active | AssetLifecycleV16::DrainOnly => Ok(()),
            _ => Err(V16Error::LockActive),
        }
    }

    fn require_empty_asset_lifecycle_state(&self, asset_index: usize) -> V16Result<()> {
        self.require_empty_asset_lifecycle_state_with_spent_policy(asset_index, false)
    }

    fn require_empty_asset_lifecycle_state_with_spent_policy(
        &self,
        asset_index: usize,
        allow_terminal_spent_budget: bool,
    ) -> V16Result<()> {
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
        let long_budget = slot.insurance_domain_budget_long.get();
        let long_spent = slot.insurance_domain_spent_long.get();
        let short_budget = slot.insurance_domain_budget_short.get();
        let short_spent = slot.insurance_domain_spent_short.get();
        let spent_blocks_empty = if allow_terminal_spent_budget {
            (long_spent != 0 && long_budget != long_spent)
                || (short_spent != 0 && short_budget != short_spent)
        } else {
            long_spent != 0 || short_spent != 0
        };

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
            || spent_blocks_empty
            || !long_source.is_empty_amount_shape()
            || !short_source.is_empty_amount_shape()
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

    pub fn finalize_side_reset_not_atomic(
        &mut self,
        asset_index: usize,
        side: SideV16,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let pending_barrier = self.pending_domain_loss_barrier_count(asset_index, side)?;
        let mut asset = self.asset_state(asset_index)?;
        match side {
            SideV16::Long => match asset.mode_long {
                SideModeV16::Normal => return Ok(()),
                SideModeV16::DrainOnly => return Err(V16Error::LockActive),
                SideModeV16::ResetPending => {
                    if asset.stored_pos_count_long != 0
                        || asset.stale_account_count_long != 0
                        || asset.pending_obligation_count_long != 0
                        || pending_barrier != 0
                    {
                        return Err(V16Error::Stale);
                    }
                    asset.mode_long = SideModeV16::Normal;
                }
            },
            SideV16::Short => match asset.mode_short {
                SideModeV16::Normal => return Ok(()),
                SideModeV16::DrainOnly => return Err(V16Error::LockActive),
                SideModeV16::ResetPending => {
                    if asset.stored_pos_count_short != 0
                        || asset.stale_account_count_short != 0
                        || asset.pending_obligation_count_short != 0
                        || pending_barrier != 0
                    {
                        return Err(V16Error::Stale);
                    }
                    asset.mode_short = SideModeV16::Normal;
                }
            },
        }
        self.set_asset_state(asset_index, asset)?;
        self.header.risk_epoch = V16PodU64::new(
            self.header
                .risk_epoch
                .get()
                .checked_add(1)
                .ok_or(V16Error::CounterOverflow)?,
        );
        self.as_view().validate_header_aggregate_totals()?;
        self.validate_shape_audit_scan()
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
        // PRODUCTION KERNEL: cert currentness (valid + all epochs + bitmap match).
        if !V16Core::kernel_cert_is_current(
            cert,
            self.header.oracle_epoch.get(),
            self.header.funding_epoch.get(),
            self.header.risk_epoch.get(),
            self.header.asset_set_epoch.get(),
            account.header.active_bitmap.map(V16PodU64::get),
        ) {
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = account.source_domains()[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if source.is_occupied() && source.source_claim_bound_num.get() != 0 {
                let d = source.domain.get() as usize;
                if self.domain_asset_side(d).is_ok()
                    && self.account_has_active_exposure_for_source_domain(account, d)?
                {
                    return Ok(true);
                }
            }
            slot += 1;
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
        Ok(pending_domain_loss_barrier_blocks_position_change(
            self.position_change_touches_pending_domain_loss_barrier(asset_index, current, next)?,
            current,
            next,
        ))
    }

    fn h_lock_lane(
        &self,
        account: Option<&PortfolioV16View<'_>>,
        instruction_bankruptcy_candidate: bool,
    ) -> V16Result<HLockLaneV16> {
        if let Some(account) = account {
            if decode_bool(account.header.stale_state)?
                || decode_bool(account.header.b_stale_state)?
                || self.account_has_loss_stale_live_leg(account)?
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
        {
            return Ok(HLockLaneV16::HMax);
        }
        Ok(HLockLaneV16::HMin)
    }

    fn asset_has_target_effective_lag(&self, asset_index: usize) -> V16Result<bool> {
        let asset = self.asset_state(asset_index)?;
        Ok(asset.raw_oracle_target_price != asset.effective_price)
    }

    fn asset_is_loss_stale(&self, asset_index: usize) -> V16Result<bool> {
        let asset = self.asset_state(asset_index)?;
        Ok(asset_contributes_to_loss_stale_summary(asset)
            && asset.slot_last < self.header.current_slot.get())
    }

    fn account_has_loss_stale_live_leg(&self, account: &PortfolioV16View<'_>) -> V16Result<bool> {
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            if active_bitmap_get(bitmap, slot) {
                let leg = account.header.legs[slot].try_to_runtime()?;
                if !leg.active {
                    return Err(V16Error::HiddenLeg);
                }
                if self.asset_is_loss_stale(leg.asset_index as usize)? {
                    return Ok(true);
                }
            }
            slot += 1;
        }
        Ok(false)
    }

    fn can_ignore_unrelated_loss_stale_for_trade(
        &self,
        long_account: &PortfolioV16View<'_>,
        short_account: &PortfolioV16View<'_>,
        asset_index: usize,
    ) -> V16Result<bool> {
        if !decode_bool(self.header.loss_stale_active)? {
            return Ok(false);
        }
        Ok(V16Core::loss_stale_trade_scope_allowed(
            true,
            self.asset_is_loss_stale(asset_index)?,
            self.account_has_loss_stale_live_leg(long_account)?,
            self.account_has_loss_stale_live_leg(short_account)?,
        ))
    }

    fn account_fee_anchor_for_loss_currentness(
        &self,
        account: &PortfolioV16View<'_>,
        now_slot: u64,
    ) -> V16Result<u64> {
        let mut anchor = now_slot;
        let bitmap = account.header.active_bitmap.map(V16PodU64::get);
        let mut slot = 0usize;
        while slot < V16_MAX_PORTFOLIO_ASSETS_N {
            if active_bitmap_get(bitmap, slot) {
                let leg = account.header.legs[slot].try_to_runtime()?;
                if !leg.active {
                    return Err(V16Error::HiddenLeg);
                }
                let asset = self.asset_state(leg.asset_index as usize)?;
                if asset_contributes_to_loss_stale_summary(asset) {
                    anchor = anchor.min(asset.slot_last);
                }
            }
            slot += 1;
        }
        Ok(anchor)
    }

    fn validate_trade_request(&self, request: TradeRequestV16) -> V16Result<()> {
        let config = self.header.config.try_to_runtime_shape()?;
        // PRODUCTION FIDELITY (3C): the scalar request/config guards as a
        // contract-proven summary; admit IFF every leaf passes.
        if !V16Core::build_trade_request_guard_summary(
            request,
            config.max_market_slots,
            config.max_trading_fee_bps,
        )
        .all_pass()
        {
            return Err(V16Error::InvalidConfig);
        }
        Ok(())
    }

    // Contract layer: the fill sign convention — long receives the signed
    // size, short receives its exact negation, abs matches, zero rejected.
    // Every trade leg delta in the engine flows through this convention.
    #[cfg_attr(all(kani, feature = "contracts"), kani::ensures(|result: &V16Result<(u128, i128, i128)>| match result {
        Ok((abs, long_d, short_d)) => *abs == size_q.unsigned_abs()
            && *abs <= MAX_TRADE_SIZE_Q
            && *long_d == size_q
            && *short_d == -size_q
            && *long_d + *short_d == 0
            && size_q != 0,
        Err(_) => true,
    }))]
    fn trade_signed_size_deltas(size_q: i128) -> V16Result<(u128, i128, i128)> {
        if size_q == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let abs_size_q = size_q.unsigned_abs();
        if abs_size_q > MAX_TRADE_SIZE_Q {
            return Err(V16Error::InvalidConfig);
        }
        let opposite_delta = size_q.checked_neg().ok_or(V16Error::ArithmeticOverflow)?;
        Ok((abs_size_q, size_q, opposite_delta))
    }

    fn validate_trade_position_preflight(
        &self,
        long_account: &PortfolioV16View<'_>,
        short_account: &PortfolioV16View<'_>,
        request: TradeRequestV16,
    ) -> V16Result<TradePositionPreflightV16> {
        let (_, long_delta, short_delta) = Self::trade_signed_size_deltas(request.size_q)?;
        let long_lookup =
            Self::position_delta_lookup_for_asset(long_account, request.asset_index, long_delta)?;
        let short_lookup =
            Self::position_delta_lookup_for_asset(short_account, request.asset_index, short_delta)?;
        let risk_increasing = position_delta_increases_risk(long_lookup.current_q, long_delta)?
            || position_delta_increases_risk(short_lookup.current_q, short_delta)?;
        let target_effective_lag = self.asset_has_target_effective_lag(request.asset_index)?;
        let blocked_by_pending_domain_barrier = pending_domain_loss_barrier_blocks_position_change(
            self.position_change_touches_pending_domain_loss_barrier(
                request.asset_index,
                long_lookup.current_q,
                long_lookup.next_q,
            )?,
            long_lookup.current_q,
            long_lookup.next_q,
        )
            || pending_domain_loss_barrier_blocks_position_change(
                self.position_change_touches_pending_domain_loss_barrier(
                    request.asset_index,
                    short_lookup.current_q,
                    short_lookup.next_q,
                )?,
                short_lookup.current_q,
                short_lookup.next_q,
            );
        trade_preflight_risk_gate(
            risk_increasing,
            self.asset_is_loss_stale(request.asset_index)?,
            target_effective_lag,
            blocked_by_pending_domain_barrier,
        )?;
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
        let asset = self.asset_state(asset_index)?;
        self.require_asset_active_for_risk_increase(asset_index)?;
        let a_basis = match side {
            SideV16::Long => asset.a_long,
            SideV16::Short => asset.a_short,
        };
        let loss_weight = loss_weight_for_basis(basis_pos_q.unsigned_abs(), a_basis)?;
        let (asset, new_leg) =
            V16Core::kernel_attach_leg(asset, side, basis_pos_q, loss_weight, asset_index as u32)?;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&new_leg);
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
        let asset = self.asset_state(asset_index)?;
        let asset = V16Core::kernel_clear_leg(leg, asset)?;
        account.header.legs[leg_slot] =
            PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        let mut bitmap = account.header.active_bitmap.map(V16PodU64::get);
        active_bitmap_clear(&mut bitmap, leg_slot)?;
        account.header.active_bitmap = bitmap.map(V16PodU64::new);
        account.header.health_cert.valid = 0;
        self.set_asset_state(asset_index, asset)?;
        Ok(())
    }

    fn clear_resolved_close_leg(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        asset_index: usize,
    ) -> V16Result<()> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved {
            return Err(V16Error::LockActive);
        }
        let leg_slot = Self::require_active_leg_slot_for_asset(&account.as_view(), asset_index)?;
        let mut leg = account.header.legs[leg_slot].try_to_runtime()?;
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
        leg.b_rem = 0;
        let asset = self.asset_state(asset_index)?;
        let asset = V16Core::kernel_clear_leg(leg, asset)?;
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
        if pending_domain_loss_barrier_blocks_position_change(
            self.position_change_touches_pending_domain_loss_barrier(asset_index, current, new)?,
            current,
            new,
        ) {
            return Err(V16Error::LockActive);
        }
        // PRODUCTION KERNEL: classify the route (Attach/Clear/Flip/Resize) — the
        // exact decision this body dispatches on, factored out and contracted.
        let route = V16Core::kernel_classify_position_delta(current, new);
        if route == PositionRouteV16::Attach {
            let side = if new > 0 {
                SideV16::Long
            } else {
                SideV16::Short
            };
            let leg_slot = lookup.empty_slot.ok_or(V16Error::InvalidLeg)?;
            return self.attach_leg_at_slot(account, asset_index, side, new, leg_slot);
        }
        let leg_slot = existing_slot.ok_or(V16Error::InvalidLeg)?;
        if route == PositionRouteV16::Clear {
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
        if route == PositionRouteV16::Flip {
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
        let old_leg = account.header.legs[leg_slot].try_to_runtime()?;
        let new_weight = loss_weight_for_basis(new.unsigned_abs(), old_leg.a_basis)?;
        let preserve_pending_obligation_weight =
            same_side_risk_reduction_or_flat_obligation(current, new)
                && self.has_pending_domain_loss_barrier(asset_index, old_leg.side)?;
        let asset = self.asset_state(asset_index)?;
        let (new_leg, new_asset) = V16Core::kernel_resize_leg_same_side(
            old_leg,
            asset,
            new,
            new_weight,
            preserve_pending_obligation_weight,
        )?;
        account.header.legs[leg_slot] = PortfolioLegV16Account::from_runtime(&new_leg);
        account.header.health_cert.valid = 0;
        self.set_asset_state(asset_index, new_asset)?;
        Ok(())
    }

    fn set_pending_domain_loss_barrier_count(
        &mut self,
        asset_index: usize,
        side: SideV16,
        value: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let slot = self.markets[asset_index].engine_slot();
        let old_blockers = slot_resolved_payout_blockers_v16(slot)?;
        let old_value = match side {
            SideV16::Long => slot.pending_domain_loss_barrier_long.get(),
            SideV16::Short => slot.pending_domain_loss_barrier_short.get(),
        };
        let new_blockers = old_blockers
            .checked_sub(old_value)
            .and_then(|v| v.checked_add(value))
            .ok_or(V16Error::CounterOverflow)?;
        self.update_resolved_payout_blocker_total(old_blockers, new_blockers)?;

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
        let current = account.header.close_progress.try_to_runtime()?;
        if current.active && !current.is_finalized_inert() {
            return Err(V16Error::LockActive);
        }
        let domain = self.insurance_domain_index(asset_index, domain_side)?;
        if self.pending_domain_loss_barrier_count(asset_index, domain_side)? != 0 {
            return Err(V16Error::LockActive);
        }
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
        let ledger = account.header.close_progress.try_to_runtime()?;
        self.ensure_close_progress_not_expired(ledger)?;
        let was_pending = ledger.has_pending_residual();
        let domain_side = ledger.domain_side;
        let asset_index = ledger.asset_index as usize;
        if !ledger.active || ledger.finalized {
            return Err(V16Error::LockActive);
        }
        let ledger_advanced = V16Core::kernel_advance_close_ledger(
            ledger,
            support_consumed,
            junior_face_burned,
            insurance_spent,
            b_loss_booked,
            explicit_loss_assigned,
        )?;
        let mut ledger = ledger_advanced;
        let _ = &mut ledger;
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
        // PRODUCTION KERNEL: cap the chunk by residual AND the per-call public cap.
        let candidate = V16Core::kernel_social_loss_chunk_cap(
            residual_remaining,
            self.header.config.public_b_chunk_atoms.get(),
        );
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
        let resolved = decode_market_mode(self.header.mode)? == MarketModeV16::Resolved;
        // Determine whether a chunk can be booked (and the recovery reason if not).
        // weight_sum==0 -> no loss-side to absorb (ActiveBankruptClose); engine
        // chunk 0 / apply None -> no B-headroom (BIndexHeadroomExhausted). The
        // booked asset is written back only when a chunk is actually booked.
        let (booked, new_asset, recovery_reason) = if weight_sum == 0 {
            (
                None,
                None,
                PermissionlessRecoveryReasonV16::ActiveBankruptCloseCannotProgress,
            )
        } else {
            let engine_chunk = self.bankruptcy_residual_single_step_capacity(
                asset_index,
                bankrupt_side,
                residual_remaining,
            )?;
            if engine_chunk == 0 {
                (
                    None,
                    None,
                    PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
                )
            } else {
                let mut asset_mut = asset;
                match V16Core::apply_bankruptcy_residual_chunk_to_loss_side(
                    &mut asset_mut,
                    opp,
                    engine_chunk,
                    residual_remaining,
                )? {
                    Some(outcome) => (
                        Some(outcome),
                        Some(asset_mut),
                        PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
                    ),
                    None => (
                        None,
                        None,
                        PermissionlessRecoveryReasonV16::BIndexHeadroomExhausted,
                    ),
                }
            }
        };
        // PRODUCTION KERNEL: the conservation-proven step decision.
        match V16Core::kernel_bresidual_step(residual_remaining, booked, resolved) {
            BResidualStepV16::Outcome(outcome) => {
                if let Some(asset_mut) = new_asset {
                    self.set_asset_state(asset_index, asset_mut)?;
                }
                self.header.bankruptcy_hlock_active = 1;
                Ok(outcome)
            }
            BResidualStepV16::DeclareRecovery => {
                self.declare_permissionless_recovery(recovery_reason)?;
                Err(V16Error::RecoveryRequired)
            }
        }
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
        if request.asset_index >= config.max_market_slots as usize {
            return Err(V16Error::InvalidConfig);
        }
        let fee_bps = config.liquidation_fee_bps;
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
        let asset = self.asset_state(request.asset_index)?;
        let close_request_q = liquidation_engine_close_request_q(
            config,
            cert,
            account.header.capital.get(),
            account.header.pnl.get(),
            leg,
            asset.effective_price,
            asset.raw_oracle_target_price,
            fee_bps,
        )?;
        // PRODUCTION KERNEL: clamp + toward-zero reduction delta — the SAME
        // risk-reduction kernel rebalance uses, so A5.dec's strict-progress
        // contract now governs the real liquidation route (3C). Liquidation size
        // is engine-selected: close just enough to restore maintenance health
        // when possible, otherwise close the selected leg fully.
        let (close_q, close_delta) =
            V16Core::kernel_reduce_position_delta(leg.basis_pos_q, leg.side, close_request_q)?;
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
        let fee_notional = liquidation_risk_notional_ceil(close_q, asset.effective_price)?;
        let fee = liquidation_fee_for_close(
            fee_notional,
            fee_bps,
            config.min_liquidation_abs,
            config.liquidation_fee_cap,
            close_q == leg.basis_pos_q.unsigned_abs(),
        )?;
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
        // PRODUCTION KERNEL: clamp + toward-zero reduction delta (rank decrease,
        // never over-closes, full close clears).
        let (reduce_q, reduce_delta) =
            V16Core::kernel_reduce_position_delta(leg.basis_pos_q, leg.side, request.reduce_q)?;
        if reduce_q == 0 {
            return Err(V16Error::NonProgress);
        }
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
            && self
                .ensure_favorable_action_current_certificate(&account.as_view())
                .is_ok()
            && !Self::has_b_stale_leg(&account.as_view())?
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
        V16Core::kernel_initial_margin_gate(cert)
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
        validate_non_min_i128(account.header.pnl.get())?;
        validate_fee_credits(account.header.fee_credits.get())?;
        let cert = account.header.health_cert.try_to_runtime()?;
        V16Core::kernel_locked_margin_gate(
            account.header.capital.get(),
            account.header.pnl.get(),
            account.header.fee_credits.get(),
            cert.certified_initial_req,
        )
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

    fn apply_trade_after_refresh_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        request: TradeRequestV16,
        recertify_after_fill: bool,
    ) -> V16Result<TradeApplyOutcomeV16> {
        let (abs_size_q, long_delta, short_delta) = Self::trade_signed_size_deltas(request.size_q)?;
        let trade_preflight = self.validate_trade_position_preflight(
            &long_account.as_view(),
            &short_account.as_view(),
            request,
        )?;
        let risk_increasing = trade_preflight.risk_increasing;
        if risk_increasing {
            self.require_asset_active_for_risk_increase(request.asset_index)?;
        }
        let notional = trade_notional_floor(abs_size_q, request.exec_price)?;
        let fee_notional = trade_fee_notional_ceil(abs_size_q, request.exec_price)?;
        let fee = checked_fee_bps(fee_notional, request.fee_bps)?;
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
        self.transfer_trade_residual_reward_credit(
            long_account,
            short_account,
            &trade_preflight,
            request.asset_index,
        )?;
        if recertify_after_fill {
            let price = self.asset_state(request.asset_index)?.effective_price;
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
        }
        Ok(TradeApplyOutcomeV16 {
            fee_a,
            fee_b,
            notional,
            risk_increasing,
            long_has_source_claims: trade_preflight.long_has_source_claims,
            short_has_source_claims: trade_preflight.short_has_source_claims,
        })
    }

    fn accumulate_batch_trade_apply(
        outcome: &mut BatchTradeOutcomeV16,
        risk_increasing: &mut bool,
        long_has_source_claims: &mut bool,
        short_has_source_claims: &mut bool,
        applied: TradeApplyOutcomeV16,
    ) -> V16Result<()> {
        let (o, ri, lhsc, shsc) = V16Core::kernel_accumulate_batch_trade(
            *outcome,
            *risk_increasing,
            *long_has_source_claims,
            *short_has_source_claims,
            applied,
        )?;
        *outcome = o;
        *risk_increasing = ri;
        *long_has_source_claims = lhsc;
        *short_has_source_claims = shsc;
        Ok(())
    }

    fn finish_trade_checks_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        locked: bool,
        risk_increasing: bool,
        long_has_source_claims: bool,
        short_has_source_claims: bool,
    ) -> V16Result<()> {
        if risk_increasing && !locked {
            if long_has_source_claims {
                self.create_initial_margin_source_lien_if_needed(long_account)?;
            }
            if short_has_source_claims {
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
        Ok(())
    }

    pub fn execute_trade_with_fee_loss_stale_scoped_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        request: TradeRequestV16,
    ) -> V16Result<TradeOutcomeV16> {
        let outcome = self.execute_batch_with_fee_loss_stale_scoped_not_atomic(
            long_account,
            short_account,
            core::slice::from_ref(&request),
        )?;
        Ok(TradeOutcomeV16 {
            fee_a: outcome.fee_a,
            fee_b: outcome.fee_b,
            notional: outcome.notional,
        })
    }

    pub fn execute_batch_with_fee_loss_stale_scoped_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        requests: &[TradeRequestV16],
    ) -> V16Result<BatchTradeOutcomeV16> {
        self.validate_unconfigured_market_tail()?;
        let mut ignore_unrelated_loss_stale =
            decode_bool(self.header.loss_stale_active)? && !requests.is_empty();
        if ignore_unrelated_loss_stale {
            let mut i = 0usize;
            while i < requests.len() {
                if !self.can_ignore_unrelated_loss_stale_for_trade(
                    &long_account.as_view(),
                    &short_account.as_view(),
                    requests[i].asset_index,
                )? {
                    ignore_unrelated_loss_stale = false;
                    break;
                }
                i += 1;
            }
        }
        let restore_loss_stale_active = self.header.loss_stale_active;
        if ignore_unrelated_loss_stale {
            self.header.loss_stale_active = 0;
        }
        let result = self.execute_batch_with_fee_after_tail_validation_not_atomic(
            long_account,
            short_account,
            requests,
        );
        if ignore_unrelated_loss_stale {
            self.header.loss_stale_active = restore_loss_stale_active;
        }
        let outcome = result?;
        self.validate_shape()?;
        long_account.validate_with_market(&self.as_view())?;
        short_account.validate_with_market(&self.as_view())?;
        Ok(outcome)
    }

    fn execute_batch_with_fee_after_tail_validation_not_atomic(
        &mut self,
        long_account: &mut PortfolioV16ViewMut<'_>,
        short_account: &mut PortfolioV16ViewMut<'_>,
        requests: &[TradeRequestV16],
    ) -> V16Result<BatchTradeOutcomeV16> {
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live {
            return Err(V16Error::LockActive);
        }
        let config = self.header.config.try_to_runtime_shape()?;
        if requests.is_empty() {
            return Err(V16Error::NonProgress);
        }
        if requests.len() > config.max_portfolio_assets as usize {
            return Err(V16Error::InvalidConfig);
        }
        let mut i = 0usize;
        while i < requests.len() {
            self.validate_trade_request(requests[i])?;
            i += 1;
        }
        self.settle_account_for_position_action_and_refresh_not_atomic(long_account)?;
        self.settle_account_for_position_action_and_refresh_not_atomic(short_account)?;

        let locked = self.h_lock_lane(Some(&long_account.as_view()), false)? == HLockLaneV16::HMax
            || self.h_lock_lane(Some(&short_account.as_view()), false)? == HLockLaneV16::HMax;
        let mut outcome = BatchTradeOutcomeV16 {
            fill_count: 0,
            fee_a: 0,
            fee_b: 0,
            notional: 0,
        };
        let mut risk_increasing = false;
        let mut long_has_source_claims = false;
        let mut short_has_source_claims = false;
        let recertify_after_fill = requests.len() == 1;
        let mut i = 0usize;
        while i < requests.len() {
            let applied = self.apply_trade_after_refresh_not_atomic(
                long_account,
                short_account,
                requests[i],
                recertify_after_fill,
            )?;
            Self::accumulate_batch_trade_apply(
                &mut outcome,
                &mut risk_increasing,
                &mut long_has_source_claims,
                &mut short_has_source_claims,
                applied,
            )?;
            i += 1;
        }
        if !recertify_after_fill {
            self.certify_account_after_local_settlement_with_price_override(long_account, None)?;
            self.certify_account_after_local_settlement_with_price_override(short_account, None)?;
        }
        self.finish_trade_checks_not_atomic(
            long_account,
            short_account,
            locked,
            risk_increasing,
            long_has_source_claims,
            short_has_source_claims,
        )?;
        Ok(outcome)
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
        self.header.bankruptcy_hlock_active = 1;
        self.set_account_pnl(account, 0)?;
        account.header.health_cert.valid = 0;
        Ok(())
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
        // PRODUCTION KERNEL: reduce the negative PnL by the absorbed loss
        // (cleared = min(booked+explicit, |pnl|)); proven to only shrink the loss
        // toward zero and never over-clear. residual == |pnl| here, so the
        // kernel's |pnl|-cap is exactly the production min(.., residual).
        let new_pnl = V16Core::kernel_settle_resolved_pnl_after_booking(
            account.header.pnl.get(),
            outcome.booked_loss,
            outcome.explicit_loss,
        )?;
        self.set_account_pnl(account, new_pnl)?;
        account.header.health_cert.valid = 0;
        Ok(())
    }

    fn settle_negative_pnl_from_principal_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        account.validate_with_market(&self.as_view())?;
        let paid = self.settle_negative_pnl_from_principal_core_not_atomic(account)?;
        self.validate_account_audit_scan(&account.as_view())?;
        self.validate_shape_audit_scan()?;
        Ok(paid)
    }

    fn settle_negative_pnl_from_principal_core_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let pnl = account.header.pnl.get();
        if pnl >= 0 {
            return Ok(0);
        }
        // PRODUCTION KERNEL: principal-settlement core (exact paid + conservation).
        let (paid, capital, c_tot, new_pnl) = V16Core::kernel_settle_principal(
            account.header.capital.get(),
            self.header.c_tot.get(),
            pnl,
        )?;
        if paid == 0 {
            self.header.bankruptcy_hlock_active = 1;
            return Ok(0);
        }

        let vault_before = self.header.vault.get();
        account.header.capital = V16PodU128::new(capital);
        self.header.c_tot = V16PodU128::new(c_tot);
        self.set_account_pnl_after_principal_settlement(account, new_pnl)?;
        Self::record_account_residual_crystallized_loss(account, paid)?;
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

    // Kani-only entry to the bare negative-PnL principal-settlement transition,
    // bypassing the O(N) loop-based shape validation so an inductive proof can
    // assume a decomposed loop-free invariant and apply just this transition.
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

    fn charge_account_fee_not_atomic(
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
            || self.header.resolved_payout_blocker_count.get() != 0
        {
            return Ok(false);
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

    #[cfg(any(kani, feature = "fuzz"))]
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
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source_snapshot = account.header.source_domains[slot];
            if source_snapshot.has_default_sparse_tag() && !source_snapshot.is_occupied() {
                break;
            }
            if !source_snapshot.is_occupied() {
                slot += 1;
                continue;
            }
            let d = source_snapshot.domain.get() as usize;
            self.domain_asset_side(d)?;
            let effective = source_snapshot.source_lien_effective_reserved.get();
            let counterparty_backing = source_snapshot.source_lien_counterparty_backing_num.get();
            let insurance_backing = source_snapshot.source_lien_insurance_backing_num.get();
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
                let source = &mut account.header.source_domains[slot];
                source.source_claim_liened_num = V16PodU128::new(0);
                source.source_claim_counterparty_liened_num = V16PodU128::new(0);
                source.source_claim_insurance_liened_num = V16PodU128::new(0);
                source.source_lien_effective_reserved = V16PodU128::new(0);
                source.source_lien_counterparty_backing_num = V16PodU128::new(0);
                source.source_lien_insurance_backing_num = V16PodU128::new(0);
                source.source_lien_fee_last_slot = V16PodU64::new(0);
                account.reset_source_domain_slot_if_empty(slot);
            }
            slot += 1;
        }
        account.compact_source_domains();
        account.header.health_cert.valid = 0;
        account.validate_with_market(&self.as_view())?;
        self.validate_shape()?;
        Ok(released_effective)
    }

    #[cfg(any(kani, feature = "fuzz"))]
    pub fn impair_account_source_credit_lien_from_insurance_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
        domain: usize,
    ) -> V16Result<u128> {
        self.domain_asset_side(domain)?;
        account.validate_with_market(&self.as_view())?;
        let source = account.as_view().source_domain(domain)?;
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
        self.set_pending_domain_loss_barrier_count(asset_index, side, count - 1)?;
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
        let fee_anchor = if decode_market_mode(self.header.mode)? == MarketModeV16::Live && nonflat
        {
            self.account_fee_anchor_for_loss_currentness(&account.as_view(), now_slot)?
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
        // A `canceled` close ledger (left behind by cure_and_cancel_close) is inert:
        // validate_close_progress_ledger_with_market guarantees it carries no
        // irreversible progress and residual_remaining == gross_loss_at_close_start,
        // so it represents no obligation. Blocking withdraw on it permanently freezes a
        // flat, solvent user who cured a forced close. Only an active/in-progress close
        // ledger must block withdrawal.
        let close_progress = account.header.close_progress.try_to_runtime()?;
        if close_progress != CloseProgressLedgerV16::EMPTY
            && !close_progress.canceled
            && !close_progress.is_finalized_inert()
        {
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

    // A resolved-payout receipt that has been paid its full entitlement at the TERMINAL
    // payout rate (no unreceipted bound remains, so the rate can no longer rise) holds
    // only unrecoverable insolvency bad debt: the haircut shortfall (face - paid) is not
    // an open obligation. The only finalize site requires paid_effective == FULL face,
    // which is unreachable under a haircut, so the receipt would otherwise stay
    // `present && !finalized` forever and permanently block the portfolio from
    // dematerializing (stranding insurance + backing earnings + residual vault + rent).
    // Clear such a fully-diluted receipt so the account can close.
    fn clear_fully_diluted_resolved_receipt_if_terminal(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<()> {
        let receipt = account.header.resolved_payout_receipt.try_to_runtime()?;
        if !receipt.present || receipt.finalized {
            return Ok(());
        }
        let ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
        // Rate is terminal only once all junior bound has been receipted/refined.
        if ledger.terminal_claim_bound_unreceipted_num != 0 {
            return Ok(());
        }
        if self.resolved_receipt_claimable_now(receipt)? != 0 {
            return Ok(());
        }
        account.header.resolved_payout_receipt =
            ResolvedPayoutReceiptV16Account::from_runtime(&ResolvedPayoutReceiptV16::EMPTY);
        Ok(())
    }

    fn preflight_claim_resolved_payout_topup(
        &self,
        account: &PortfolioV16View<'_>,
    ) -> V16Result<()> {
        account.validate_with_market(&self.as_view())?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Resolved
            || !decode_bool(self.header.payout_snapshot_captured)?
        {
            return Err(V16Error::LockActive);
        }
        Ok(())
    }

    /// Terminal realization for a source-backed winner: realize the account's
    /// outstanding source-credit claims against their domain backing at the
    /// current credit rate (lien consumption -> capital credit) BEFORE the claim
    /// face enters the junior receipt pool. A settlement-quality claim is
    /// realizable at rate in Live (convert_released_pnl_to_capital); resolution
    /// must not strip that entitlement -- without this step the wind-down
    /// RELEASES the backing to the provider while the winner is haircut from a
    /// residual pool that (correctly) excludes the very backing underwriting the
    /// claim. Residual-neutral: capital/c_tot grow by exactly the consumed
    /// backing atoms, so the payout snapshot does not depend on realization
    /// order.
    fn realize_source_backed_claims_for_resolved_close_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        if decode_bool(account.header.resolved_payout_receipt.present)? {
            // The face is already frozen into the receipt pool.
            return Ok(0);
        }
        if !Self::account_has_source_claims(&account.as_view())? {
            return Ok(0);
        }
        let pos = account.header.pnl.get().max(0) as u128;
        if pos == 0 {
            return Ok(0);
        }
        // Release the account's persisted liens first (the terminal burn would do
        // this anyway): the conversion consumes only UNLIENED claims, so a still-
        // liened claim would make the realizable estimate exceed what consumption
        // can deliver and dead-lock the close with LockActive. In the same sweep,
        // expire any domain whose backing bucket has lapsed (status Fresh but
        // expiry_slot <= current_slot): realization is best-effort, and querying
        // realizable support against a past-expiry bucket would otherwise return
        // Stale and strand the winner's close. Expiry forfeits the lapsed
        // principal to the junior pool (the documented expiry semantics), drops
        // the domain's credit rate to zero, and lets this step fall through to
        // the junior receipt path instead of reverting.
        let current_slot = self.header.current_slot.get();
        let mut slot = 0usize;
        while slot < PORTFOLIO_SOURCE_DOMAIN_CAP {
            let source = account.header.source_domains[slot];
            if source.has_default_sparse_tag() && !source.is_occupied() {
                break;
            }
            if source.is_occupied() {
                let d = source.domain.get() as usize;
                if source.source_claim_liened_num.get() != 0 {
                    self.release_account_source_credit_lien_for_domain_not_atomic(account, d)?;
                }
                let bucket = self.backing_bucket_for_domain(d)?;
                if bucket.status == BackingBucketStatusV16::Fresh
                    && bucket.expiry_slot <= current_slot
                {
                    self.expire_source_backing_bucket_not_atomic(d, current_slot)?;
                }
            }
            slot += 1;
        }
        if self.account_source_realizable_support(&account.as_view(), pos)? == 0 {
            return Ok(0);
        }
        // Terminal: PnL reservations no longer gate realization.
        account.header.reserved_pnl = V16PodU128::new(0);
        let converted = self.convert_released_pnl_to_capital_core_not_atomic(account)?;
        // If the payout snapshot was captured before this account realized (another
        // winner closed first), the realized face is still counted in the ledger's
        // unreceipted bound. Refine it out, or the stale bound dilutes the payout
        // rate forever and blocks every remaining receipt from reaching the
        // terminal rate (never finalized, never clearable).
        if decode_bool(self.header.payout_snapshot_captured)? {
            let pos_after = account.header.pnl.get().max(0) as u128;
            let face_burned = pos
                .checked_sub(pos_after)
                .ok_or(V16Error::CounterUnderflow)?;
            let ledger = self.header.resolved_payout_ledger.try_to_runtime()?;
            let decrease_num = V16Core::bound_num_from_amount(face_burned)?
                .min(ledger.terminal_claim_bound_unreceipted_num);
            if decrease_num != 0 {
                self.refine_resolved_unreceipted_bound_not_atomic(decrease_num)?;
            }
        }
        Ok(converted)
    }

    fn claim_resolved_payout_topup_core_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        let mut receipt = account.header.resolved_payout_receipt.try_to_runtime()?;
        let claimable = self.resolved_receipt_claimable_now(receipt)?;
        if claimable == 0 {
            // Fully paid at the current rate; if that rate is terminal the receipt is
            // fully diluted -- clear it so the portfolio can dematerialize.
            self.clear_fully_diluted_resolved_receipt_if_terminal(account)?;
            return Ok(0);
        }
        let vault_before = self.header.vault.get();
        // PRODUCTION KERNEL: draining step (payout = min(claimable, vault); vault
        // drops by exactly payout, never overdrawn, never leaked).
        let (payout, new_vault) = V16Core::kernel_resolved_payout_step(claimable, vault_before);
        receipt = apply_resolved_payout_receipt_payment(receipt, payout)?;
        self.header.vault = V16PodU128::new(new_vault);
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
        Ok(payout)
    }

    pub fn claim_resolved_payout_topup_not_atomic(
        &mut self,
        account: &mut PortfolioV16ViewMut<'_>,
    ) -> V16Result<u128> {
        self.preflight_claim_resolved_payout_topup(&account.as_view())?;
        let payout = self.claim_resolved_payout_topup_core_not_atomic(account)?;
        self.validate_shape()?;
        account.validate_with_market(&self.as_view())?;
        Ok(payout)
    }

    fn refine_resolved_unreceipted_bound_not_atomic(
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
        self.as_view().validate_header_aggregate_totals()?;
        self.validate_shape_audit_scan()
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
        self.sync_account_fee_to_slot_not_atomic(
            account,
            self.header.resolved_slot.get(),
            fee_rate_per_slot,
        )?;
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
        self.realize_source_backed_claims_for_resolved_close_not_atomic(account)?;
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
        // If the receipt is now fully paid at the terminal payout rate, dematerialize it
        // (insolvency bad-debt shortfall is not an open obligation) so this close fully
        // settles instead of leaving the portfolio stuck present-but-unfinalized.
        self.clear_fully_diluted_resolved_receipt_if_terminal(account)?;
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
                self.clear_resolved_close_leg(account, leg.asset_index as usize)?;
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

    /// Restarts an empty Recovery/Retired asset with a fresh market_id.
    ///
    /// Domain insurance budgets are preserved exactly. All position, loss,
    /// source-credit, backing, reservation, spent, and barrier state must already
    /// be empty, so stale legs from the old market_id fail closed after restart.
    pub fn restart_empty_asset_preserving_insurance_budget_not_atomic(
        &mut self,
        asset_index: usize,
        authenticated_price: u64,
        now_slot: u64,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        if decode_market_mode(self.header.mode)? != MarketModeV16::Live
            || authenticated_price == 0
            || authenticated_price > MAX_ORACLE_PRICE
            || now_slot < self.header.current_slot.get()
        {
            return Err(V16Error::InvalidConfig);
        }
        let old_asset = self.asset_state(asset_index)?;
        if !matches!(
            old_asset.lifecycle,
            AssetLifecycleV16::Recovery | AssetLifecycleV16::Retired
        ) || old_asset.market_id == 0
        {
            return Err(V16Error::LockActive);
        }
        self.require_empty_asset_lifecycle_state(asset_index)?;

        let market_id = self.header.next_market_id.get();
        if market_id == 0 {
            return Err(V16Error::InvalidConfig);
        }
        let (next_market_id, next_activation_count, next_asset_set_epoch, next_risk_epoch) =
            Self::asset_restart_next_counters(
                market_id,
                self.header.asset_activation_count.get(),
                self.header.asset_set_epoch.get(),
                self.header.risk_epoch.get(),
            )?;

        let slot = self.markets[asset_index].engine_slot_mut();
        *slot = Self::restarted_asset_slot_preserving_insurance_budget(
            slot,
            market_id,
            authenticated_price,
            now_slot,
        );

        self.header.next_market_id = V16PodU64::new(next_market_id);
        self.header.current_slot = V16PodU64::new(now_slot);
        self.header.asset_activation_count = V16PodU64::new(next_activation_count);
        self.header.last_asset_activation_slot = V16PodU64::new(now_slot);
        self.header.asset_set_epoch = V16PodU64::new(next_asset_set_epoch);
        self.header.risk_epoch = V16PodU64::new(next_risk_epoch);
        self.validate_shape()
    }

    /// Clears spent-only insurance-domain ledgers on an otherwise empty terminal asset.
    ///
    /// This is value-neutral: it may only erase a domain whose remaining budget
    /// is already zero (`budget == spent`). If `budget > spent`, callers must
    /// withdraw the remaining domain budget before terminal cleanup.
    pub fn clear_terminal_spent_domain_budgets_for_empty_asset_not_atomic(
        &mut self,
        asset_index: usize,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let asset = self.asset_state(asset_index)?;
        if !matches!(
            asset.lifecycle,
            AssetLifecycleV16::Recovery | AssetLifecycleV16::Retired
        ) {
            return Err(V16Error::LockActive);
        }
        self.require_empty_asset_lifecycle_state_with_spent_policy(asset_index, true)?;
        let slot = self.markets[asset_index].engine_slot_mut();
        let (budget_long, spent_long) = Self::clear_terminal_spent_domain_budget_pair(
            slot.insurance_domain_budget_long,
            slot.insurance_domain_spent_long,
        )?;
        let (budget_short, spent_short) = Self::clear_terminal_spent_domain_budget_pair(
            slot.insurance_domain_budget_short,
            slot.insurance_domain_spent_short,
        )?;
        slot.insurance_domain_budget_long = budget_long;
        slot.insurance_domain_spent_long = spent_long;
        slot.insurance_domain_budget_short = budget_short;
        slot.insurance_domain_spent_short = spent_short;
        self.validate_shape()
    }

    /// Rewrites an empty retired slot into the canonical retired representation.
    ///
    /// This is value-neutral and only succeeds after all domain budgets, spent
    /// amounts, source-credit, backing, reservations, and barriers are zero.
    pub fn canonicalize_retired_empty_asset_slot_not_atomic(
        &mut self,
        asset_index: usize,
    ) -> V16Result<()> {
        self.validate_configured_asset_index(asset_index)?;
        let old_asset = self.asset_state(asset_index)?;
        if old_asset.lifecycle != AssetLifecycleV16::Retired
            || old_asset.market_id == 0
            || old_asset.retired_slot == 0
        {
            return Err(V16Error::LockActive);
        }
        let slot = self.markets[asset_index].engine_slot();
        if slot.insurance_domain_budget_long.get() != 0
            || slot.insurance_domain_budget_short.get() != 0
            || slot.insurance_domain_spent_long.get() != 0
            || slot.insurance_domain_spent_short.get() != 0
        {
            return Err(V16Error::LockActive);
        }
        self.require_empty_asset_lifecycle_state(asset_index)?;

        *self.markets[asset_index].engine_slot_mut() =
            Self::canonical_retired_asset_slot(old_asset);
        self.validate_shape()
    }

    #[cfg(any(test, kani, feature = "fuzz"))]
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

        Self::record_account_funding_flow(account, leg.side, f_delta)?;
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
    pub domain: V16PodU32,
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
    // Genesis residual-farming counter (anti-wash). Collateral-atom non-rebatable fee revenue
    // generated WHILE this domain's backing lien was live and at risk. On residual crystallization
    // the pro-rata share moves from the live counter to the impaired counter; the genesis farm reads
    // the impaired counter to cap residual reward weight. Event-local, never a cumulative total.
    pub source_lien_capital_at_risk_fee_revenue: V16PodU128,
    pub source_lien_impaired_capital_at_risk_fee_revenue: V16PodU128,
}

impl PortfolioSourceDomainV16Account {
    #[inline]
    pub fn is_occupied(self) -> bool {
        self.source_claim_bound_num.get() != 0
            || self.source_claim_liened_num.get() != 0
            || self.source_claim_counterparty_liened_num.get() != 0
            || self.source_claim_insurance_liened_num.get() != 0
            || self.source_lien_effective_reserved.get() != 0
            || self.source_lien_counterparty_backing_num.get() != 0
            || self.source_lien_insurance_backing_num.get() != 0
            || self.source_lien_fee_last_slot.get() != 0
            || self.source_claim_impaired_num.get() != 0
            || self.source_lien_impaired_effective_reserved.get() != 0
            || self.source_lien_capital_at_risk_fee_revenue.get() != 0
            || self.source_lien_impaired_capital_at_risk_fee_revenue.get() != 0
    }

    #[inline]
    fn has_default_sparse_tag(self) -> bool {
        self.domain.get() == 0 && self.source_claim_market_id.get() == 0
    }

    #[inline]
    fn is_sparse_tail_default(self) -> bool {
        self.has_default_sparse_tag() && !self.is_occupied()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable, bytemuck::Pod)]
pub struct PortfolioAccountV16Account {
    pub provenance_header: ProvenanceHeaderV16Account,
    pub owner: [u8; 32],
    pub capital: V16PodU128,
    pub pnl: V16PodI128,
    pub reserved_pnl: V16PodU128,
    // Monotonic reward-accounting counters. These do not affect margin or solvency:
    // crystallized_loss increments only when real account capital is consumed by
    // loss settlement; spent_principal caps how much of that budget has rewarded
    // counterparties; received is the LP-side total earned for matching that budget.
    pub residual_crystallized_loss_atoms_total: V16PodU128,
    pub residual_spent_principal_atoms_total: V16PodU128,
    pub residual_received_atoms_total: V16PodU128,
    pub funding_long_paid_atoms_total: V16PodU128,
    pub funding_long_received_atoms_total: V16PodU128,
    pub funding_short_paid_atoms_total: V16PodU128,
    pub funding_short_received_atoms_total: V16PodU128,
    pub fee_credits: V16PodI128,
    pub cancel_deposit_escrow: V16PodU128,
    pub last_fee_slot: V16PodU64,
    pub active_bitmap: [V16PodU64; V16_ACTIVE_BITMAP_WORDS],
    pub legs: [PortfolioLegV16Account; V16_MAX_PORTFOLIO_ASSETS_N],
    pub source_domains: [PortfolioSourceDomainV16Account; PORTFOLIO_SOURCE_DOMAIN_CAP],
    pub health_cert: HealthCertV16Account,
    pub stale_state: u8,
    pub b_stale_state: u8,
    pub rebalance_lock: u8,
    pub liquidation_lock: u8,
    pub close_progress: CloseProgressLedgerV16Account,
    pub resolved_payout_receipt: ResolvedPayoutReceiptV16Account,
}

// Compile-time layout guard: any change to the PortfolioAccountV16Account Pod
// layout fails the build here. When it does, bump V16_LAYOUT_DISCRIMINATOR and
// update this expected size deliberately (no deployed markets => no migration).
// Gated to non-kani: under `cfg(kani)` PORTFOLIO_SOURCE_DOMAIN_CAP is reduced for
// proof tractability, so the production on-chain layout is the non-kani one.
#[cfg(not(kani))]
const _: () = assert!(core::mem::size_of::<PortfolioAccountV16Account>() == 9291);

impl Default for PortfolioAccountV16Account {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

impl PortfolioAccountV16Account {
    pub fn init_empty_in_place(&mut self, header: ProvenanceHeaderV16Account) -> V16Result<()> {
        let owner = header.try_to_runtime()?.owner;
        self.provenance_header = header;
        self.owner = owner;
        self.capital = V16PodU128::new(0);
        self.pnl = V16PodI128::new(0);
        self.reserved_pnl = V16PodU128::new(0);
        self.residual_crystallized_loss_atoms_total = V16PodU128::new(0);
        self.residual_spent_principal_atoms_total = V16PodU128::new(0);
        self.residual_received_atoms_total = V16PodU128::new(0);
        self.funding_long_paid_atoms_total = V16PodU128::new(0);
        self.funding_long_received_atoms_total = V16PodU128::new(0);
        self.funding_short_paid_atoms_total = V16PodU128::new(0);
        self.funding_short_received_atoms_total = V16PodU128::new(0);
        self.fee_credits = V16PodI128::new(0);
        self.cancel_deposit_escrow = V16PodU128::new(0);
        self.last_fee_slot = V16PodU64::new(0);
        self.active_bitmap = [V16PodU64::new(0); V16_ACTIVE_BITMAP_WORDS];

        let empty_leg = PortfolioLegV16Account::from_runtime(&PortfolioLegV16::EMPTY);
        let mut i = 0usize;
        while i < V16_MAX_PORTFOLIO_ASSETS_N {
            self.legs[i] = empty_leg;
            i += 1;
        }
        let mut i = 0usize;
        while i < PORTFOLIO_SOURCE_DOMAIN_CAP {
            self.source_domains[i] = PortfolioSourceDomainV16Account::default();
            i += 1;
        }

        self.health_cert = HealthCertV16Account::default();
        self.stale_state = encode_bool(false);
        self.b_stale_state = encode_bool(false);
        self.rebalance_lock = encode_bool(false);
        self.liquidation_lock = encode_bool(false);
        self.close_progress = CloseProgressLedgerV16Account::default();
        self.resolved_payout_receipt = ResolvedPayoutReceiptV16Account::default();
        Ok(())
    }

    #[cfg(kani)]
    pub fn try_empty(header: ProvenanceHeaderV16Account) -> V16Result<Self> {
        let mut out = Self::default();
        out.init_empty_in_place(header)?;
        Ok(out)
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
struct RiskScoreV16 {
    certified_liq_deficit: u128,
    unsettled_b_loss_bound: u128,
    stale_loss_bound: u128,
    gross_risk_notional: u128,
    active_leg_count: u32,
}

impl RiskScoreV16 {
    fn strictly_reduces_from(self, before: Self) -> bool {
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

fn risk_notional_ceil(abs_pos_q: u128, price: u64) -> V16Result<u128> {
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

fn account_equity_from_parts(capital: u128, pnl: i128, fee_credits: i128) -> V16Result<i128> {
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

fn trade_preflight_risk_gate(
    risk_increasing: bool,
    asset_loss_stale: bool,
    target_effective_lag: bool,
    touches_pending_domain_barrier: bool,
) -> V16Result<()> {
    if touches_pending_domain_barrier
        || (risk_increasing && (asset_loss_stale || target_effective_lag))
    {
        return Err(V16Error::LockActive);
    }
    Ok(())
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

fn trade_fee_notional_ceil(size_q: u128, exec_price: u64) -> V16Result<u128> {
    if size_q == 0 || exec_price == 0 {
        return Ok(0);
    }
    risk_notional_ceil(size_q, exec_price)
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

fn liquidation_fee_for_close(
    fee_notional: u128,
    fee_bps: u64,
    min_liquidation_abs: u128,
    liquidation_fee_cap: u128,
    closes_full_position: bool,
) -> V16Result<u128> {
    if fee_notional > MAX_ACCOUNT_NOTIONAL || fee_bps > MAX_MARGIN_BPS {
        return Err(V16Error::InvalidConfig);
    }
    let product = fee_notional * fee_bps as u128;
    let raw_fee = (product / MAX_MARGIN_BPS as u128)
        .checked_add(u128::from(product % MAX_MARGIN_BPS as u128 != 0))
        .ok_or(V16Error::ArithmeticOverflow)?;
    liquidation_fee_from_raw_fee(
        raw_fee,
        min_liquidation_abs,
        liquidation_fee_cap,
        closes_full_position,
    )
}

fn liquidation_fee_from_raw_fee(
    raw_fee: u128,
    min_liquidation_abs: u128,
    liquidation_fee_cap: u128,
    closes_full_position: bool,
) -> V16Result<u128> {
    if !closes_full_position && min_liquidation_abs != 0 && raw_fee < min_liquidation_abs {
        return Err(V16Error::NonProgress);
    }
    Ok(raw_fee.max(min_liquidation_abs).min(liquidation_fee_cap))
}

/// Per-side trade fee atoms, mirroring the production derivation in
/// `execute_trade_*` exactly (ceil fee notional -> ceil bps). Proof shim for the
/// no-free-OI lower bound: a nonzero fill at nonzero price with nonzero fee must
/// charge >= 1 atom. See `proof_v16_nonzero_trade_charges_positive_fee_per_side`.
#[cfg(any(kani, feature = "fuzz"))]
pub fn kani_trade_fee_atoms_per_side(
    size_q: u128,
    exec_price: u64,
    fee_bps: u64,
) -> V16Result<u128> {
    let fee_notional = trade_fee_notional_ceil(size_q, exec_price)?;
    checked_fee_bps(fee_notional, fee_bps)
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

fn pending_domain_loss_barrier_blocks_position_change(
    touches_barrier: bool,
    current: i128,
    next: i128,
) -> bool {
    touches_barrier && !same_side_risk_reduction_or_flat_obligation(current, next)
}

/// PRODUCTION HELPER (roadmap 3B.6 split): the social-loss booking division —
/// `numerator = engine_chunk*SOCIAL_LOSS_DEN + carried_rem`, then
/// `delta_b = numerator / weight_sum`, `new_rem = numerator % weight_sum`.
/// The exact integer identity (`delta_b*weight_sum + new_rem == numerator` and
/// `new_rem < weight_sum`) is discharged by reference-model conformance fuzz
/// (social_loss_book_split_*) since the symbolic u128 division resists Kani.
fn social_loss_book_split(
    engine_chunk: u128,
    carried_rem: u128,
    weight_sum: u128,
) -> V16Result<(u128, u128)> {
    if weight_sum == 0 {
        return Err(V16Error::InvalidConfig);
    }
    let numerator = engine_chunk
        .checked_mul(SOCIAL_LOSS_DEN)
        .and_then(|v| v.checked_add(carried_rem))
        .ok_or(V16Error::ArithmeticOverflow)?;
    Ok((numerator / weight_sum, numerator % weight_sum))
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

// Non-production Kani proof harnesses (contract + closure layers) live in
// src/v16_proofs.rs to keep this file's audit surface minimal. The contract
// ATTRIBUTES (kani::requires/ensures/modifies) remain on their target fns
// above -- those are inherent to a function contract -- but every harness,
// helper, and witness body is in that separate cfg(kani) child module.
#[cfg(all(kani, any(feature = "contracts", feature = "closure")))]
#[path = "v16_proofs.rs"]
mod v16_proofs;

// Non-production cfg(kani) test wrapper API (kani_* shims) lives in
// src/v16_kani_api.rs to keep this file's audit surface minimal. Re-exported
// so existing percolator::v16::kani_* paths resolve unchanged.
#[cfg(any(kani, feature = "fuzz"))]
#[path = "v16_kani_api.rs"]
mod v16_kani_api;
#[cfg(any(kani, feature = "fuzz"))]
pub use v16_kani_api::*;
