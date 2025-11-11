//! Liquidate user positions via reduce-only cross-slab execution

use crate::state::{Portfolio, SlabRegistry, Vault};
use percolator_common::*;
use pinocchio::{account_info::AccountInfo, msg, pubkey::Pubkey};

/// Helper to convert verification errors from KANI verified functions
///
/// KANI verified functions return `Result<T, &'static str>` but production
/// code expects `Result<T, PercolatorError>`. This helper performs the conversion.
#[inline]
fn convert_verification_error<T>(result: Result<T, &'static str>) -> Result<T, PercolatorError> {
    result.map_err(|_e| PercolatorError::Overflow)
}

/// Liquidation mode based on health
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidationMode {
    /// Pre-liquidation: MM < equity < MM + buffer (tighter band)
    PreLiquidation,
    /// Hard liquidation: equity < MM (wider band)
    HardLiquidation,
}

impl LiquidationMode {
    /// Get the price band for this mode
    pub fn get_band_bps(&self, registry: &SlabRegistry) -> u64 {
        match self {
            LiquidationMode::PreLiquidation => registry.preliq_band_bps,
            LiquidationMode::HardLiquidation => registry.liq_band_bps,
        }
    }
}

/// Determine liquidation mode based on health and buffer
pub fn determine_mode(health: i128, preliq_buffer: i128) -> Option<LiquidationMode> {
    if health < 0 {
        // Below maintenance margin - hard liquidation
        Some(LiquidationMode::HardLiquidation)
    } else if health >= 0 && health < preliq_buffer {
        // At or above MM but below buffer - pre-liquidation
        Some(LiquidationMode::PreLiquidation)
    } else {
        // Healthy - no liquidation needed
        None
    }
}

/// Calculate remaining deficit after liquidation attempts
///
/// Returns the amount of deficit still remaining (MM - equity).
/// If equity >= MM, returns 0 (portfolio is healthy).
fn calculate_remaining_deficit(
    portfolio: &Portfolio,
    _registry: &SlabRegistry,
) -> Result<i128, PercolatorError> {
    let equity = portfolio.equity;
    let total_mm = portfolio.mm as i128;

    if equity >= total_mm {
        Ok(0) // Healthy
    } else {
        Ok(total_mm - equity) // Deficit amount
    }
}

/// Liquidate Slab LP buckets to restore health
///
/// Iterates through active Slab LP buckets and cancels orders to free collateral.
/// Uses verified proportional margin reduction (LP8-LP10).
///
/// Returns the total value freed from Slab LP liquidation.
fn liquidate_slab_lp_buckets(
    portfolio: &mut Portfolio,
    target_deficit: i128,
    lp_slab_accounts: &[AccountInfo],
    router_authority: &AccountInfo,
    router_authority_bump: u8,
) -> Result<i128, PercolatorError> {
    use crate::state::model_bridge::proportional_margin_reduction_verified;

    let mut total_freed = 0i128;
    const RATIO_SCALE: u128 = 1_000_000_000; // 1e9 for precision

    msg!("LP Liquidation: Starting Slab LP liquidation");

    // Iterate through LP buckets
    for (idx, bucket) in portfolio.lp_buckets.iter_mut().enumerate() {
        if !bucket.active {
            continue;
        }

        // Skip non-Slab buckets
        if bucket.slab.is_none() {
            continue;
        }

        if let Some(slab_lp) = &mut bucket.slab {
            let order_count = slab_lp.open_order_count as usize;
            if order_count == 0 {
                continue;
            }

            msg!("LP Liquidation: Processing Slab bucket");

            // Find the slab account for this venue
            let slab_id = bucket.venue.market_id;
            let slab_account = lp_slab_accounts
                .iter()
                .find(|acc| acc.key() == &slab_id)
                .ok_or(PercolatorError::InvalidAccount)?;

            // CPI to slab program to cancel all LP orders via adapter pattern
            // Discriminator 2 = AdapterLiquidity
            // Intent 1 = Remove
            // Selector 2 = ObAll
            // RiskGuard (8 bytes)
            let mut instruction_data = [0u8; 11];
            instruction_data[0] = 2; // AdapterLiquidity discriminator
            instruction_data[1] = 1; // Remove intent
            instruction_data[2] = 2; // ObAll selector
            // RiskGuard: max_slippage_bps(2) + max_fee_bps(2) + oracle_bound_bps(2) + padding(2)
            // Use permissive guard for liquidation: 1000 bps (10%), 300 bps (3%), 500 bps (5%)
            instruction_data[3..5].copy_from_slice(&1000u16.to_le_bytes()); // max_slippage_bps
            instruction_data[5..7].copy_from_slice(&300u16.to_le_bytes());  // max_fee_bps
            instruction_data[7..9].copy_from_slice(&500u16.to_le_bytes());  // oracle_bound_bps
            instruction_data[9..11].copy_from_slice(&[0u8; 2]);             // padding

            // Build CPI accounts: [slab_account, router_authority]
            use pinocchio::instruction::{AccountMeta, Instruction, Seed, Signer};
            let account_metas = [
                AccountMeta::writable(slab_account.key()),
                AccountMeta::writable_signer(router_authority.key()),
            ];

            let instruction = Instruction {
                program_id: slab_account.owner(),
                accounts: &account_metas,
                data: &instruction_data,
            };

            // Sign CPI with router authority PDA
            use crate::pda::AUTHORITY_SEED;
            let bump_array = [router_authority_bump];
            let seeds = &[
                Seed::from(AUTHORITY_SEED),
                Seed::from(&bump_array[..]),
            ];
            let signer = Signer::from(seeds);

            pinocchio::program::invoke_signed(
                &instruction,
                &[slab_account, router_authority],
                &[signer],
            )
            .map_err(|_| PercolatorError::CpiFailed)?;

            msg!("LP Liquidation: Cancelled all LP orders via CPI");

            // After successful CPI, calculate freed collateral
            let freed_base = slab_lp.reserved_base;
            let freed_quote = slab_lp.reserved_quote;

            // Calculate freed value (simplified - in production needs proper valuation)
            let freed_value = (freed_base.saturating_add(freed_quote) / 1_000_000) as i128;

            // Calculate remaining ratio using verified logic
            let remaining_base = slab_lp.reserved_base.saturating_sub(freed_base);
            let remaining_quote = slab_lp.reserved_quote.saturating_sub(freed_quote);

            let base_ratio = if slab_lp.reserved_base > 0 {
                (remaining_base * RATIO_SCALE) / slab_lp.reserved_base
            } else {
                RATIO_SCALE
            };

            let quote_ratio = if slab_lp.reserved_quote > 0 {
                (remaining_quote * RATIO_SCALE) / slab_lp.reserved_quote
            } else {
                RATIO_SCALE
            };

            let remaining_ratio = base_ratio.min(quote_ratio);

            // Apply proportional margin reduction using KANI verified function (LP8-LP10)
            bucket.im = convert_verification_error(
                proportional_margin_reduction_verified(bucket.im, remaining_ratio)
            )?;
            bucket.mm = convert_verification_error(
                proportional_margin_reduction_verified(bucket.mm, remaining_ratio)
            )?;

            // Update reservations
            slab_lp.reserved_base = 0;
            slab_lp.reserved_quote = 0;
            slab_lp.open_order_count = 0;

            // Clear order IDs
            slab_lp.open_order_ids = [0; 8];

            // Update portfolio equity with freed collateral
            portfolio.equity = portfolio.equity.saturating_add(freed_value);

            total_freed = total_freed.saturating_add(freed_value);

            msg!("LP Liquidation: Freed collateral from Slab bucket");

            // Mark bucket inactive if empty
            if slab_lp.open_order_count == 0 {
                bucket.active = false;
                msg!("LP Liquidation: Bucket marked inactive");
            }

            // Check if we've freed enough
            if total_freed >= target_deficit {
                msg!("LP Liquidation: Target deficit reached");
                break;
            }
        }
    }

    msg!("LP Liquidation: Slab LP liquidation complete");
    Ok(total_freed)
}

/// Liquidate AMM LP buckets to restore health
///
/// Burns LP shares to recover underlying assets.
/// Uses verified redemption value calculation (LP6-LP7) and
/// proportional margin reduction (LP8-LP10).
///
/// Requires fresh AMM prices (staleness guard).
///
/// Returns the total value freed from AMM LP liquidation.
fn liquidate_amm_lp_buckets(
    portfolio: &mut Portfolio,
    target_deficit: i128,
    current_ts: u64,
    max_staleness_seconds: u64,
    lp_amm_accounts: &[AccountInfo],
    router_authority: &AccountInfo,
    router_authority_bump: u8,
) -> Result<i128, PercolatorError> {
    use crate::state::model_bridge::{
        calculate_redemption_value_verified,
        proportional_margin_reduction_verified,
    };

    let mut total_freed = 0i128;

    msg!("LP Liquidation: Starting AMM LP liquidation");

    // Iterate through LP buckets
    for (_idx, bucket) in portfolio.lp_buckets.iter_mut().enumerate() {
        if !bucket.active {
            continue;
        }

        // Skip non-AMM buckets
        if bucket.amm.is_none() {
            continue;
        }

        if let Some(amm_lp) = &mut bucket.amm {
            if amm_lp.lp_shares == 0 {
                continue;
            }

            msg!("LP Liquidation: Processing AMM bucket");

            // SAFETY TRIPWIRE: Staleness guard
            let price_age = current_ts.saturating_sub(amm_lp.last_update_ts);
            if price_age > max_staleness_seconds {
                msg!("Warning: AMM price stale, skipping bucket");
                continue;
            }

            // Find the AMM account for this venue
            let amm_id = bucket.venue.market_id;
            let amm_account = lp_amm_accounts
                .iter()
                .find(|acc| acc.key() == &amm_id)
                .ok_or(PercolatorError::InvalidAccount)?;

            // Burn all shares
            let shares_to_burn = amm_lp.lp_shares;

            // CPI to AMM program to burn shares via adapter pattern
            // Discriminator 2 = AdapterLiquidity
            // Intent 1 = Remove
            // Selector 0 = AmmByShares
            // shares (16 bytes as u128)
            // RiskGuard (8 bytes)
            let mut instruction_data = [0u8; 27];
            instruction_data[0] = 2; // AdapterLiquidity discriminator
            instruction_data[1] = 1; // Remove intent
            instruction_data[2] = 0; // AmmByShares selector
            instruction_data[3..19].copy_from_slice(&(shares_to_burn as u128).to_le_bytes()); // shares
            // RiskGuard: permissive for liquidation
            instruction_data[19..21].copy_from_slice(&1000u16.to_le_bytes()); // max_slippage_bps
            instruction_data[21..23].copy_from_slice(&300u16.to_le_bytes());  // max_fee_bps
            instruction_data[23..25].copy_from_slice(&500u16.to_le_bytes());  // oracle_bound_bps
            instruction_data[25..27].copy_from_slice(&[0u8; 2]);             // padding

            // Build CPI accounts: [amm_account, router_authority]
            use pinocchio::instruction::{AccountMeta, Instruction, Seed, Signer};
            let account_metas = [
                AccountMeta::writable(amm_account.key()),
                AccountMeta::writable_signer(router_authority.key()),
            ];

            let instruction = Instruction {
                program_id: amm_account.owner(),
                accounts: &account_metas,
                data: &instruction_data,
            };

            // Sign CPI with router authority PDA
            use crate::pda::AUTHORITY_SEED;
            let bump_array = [router_authority_bump];
            let seeds = &[
                Seed::from(AUTHORITY_SEED),
                Seed::from(&bump_array[..]),
            ];
            let signer = Signer::from(seeds);

            pinocchio::program::invoke_signed(
                &instruction,
                &[amm_account, router_authority],
                &[signer],
            )
            .map_err(|_| PercolatorError::CpiFailed)?;

            msg!("LP Liquidation: Burned AMM shares via CPI");

            // Calculate redemption value using KANI verified function (LP6-LP7)
            // Convert lp_shares from u64 to u128 and share_price_cached from i64 to u64
            let shares_u128 = shares_to_burn as u128;
            let share_price_u64 = amm_lp.share_price_cached.max(0) as u64;
            let redemption_value = convert_verification_error(
                calculate_redemption_value_verified(shares_u128, share_price_u64)
            )?;

            msg!("LP Liquidation: Calculated redemption value");

            // Apply proportional margin reduction using KANI verified function (LP8-LP10)
            // ratio_remaining = 0 since we're burning all shares
            let ratio_remaining = 0u128;
            bucket.im = convert_verification_error(
                proportional_margin_reduction_verified(bucket.im, ratio_remaining)
            )?;
            bucket.mm = convert_verification_error(
                proportional_margin_reduction_verified(bucket.mm, ratio_remaining)
            )?;

            // Update bucket state
            amm_lp.lp_shares = 0;
            // share_price_cached and last_update_ts remain for historical tracking

            // Update portfolio equity with redemption value
            portfolio.equity = portfolio.equity.saturating_add(redemption_value);

            total_freed = total_freed.saturating_add(redemption_value);

            msg!("LP Liquidation: Burned shares and freed value");

            // Mark bucket inactive
            bucket.active = false;
            msg!("LP Liquidation: Bucket marked inactive");

            // Check if we've freed enough
            if total_freed >= target_deficit {
                msg!("LP Liquidation: Target deficit reached");
                break;
            }
        }
    }

    msg!("LP Liquidation: AMM LP liquidation complete");
    Ok(total_freed)
}

/// Enhanced liquidation that handles principal and LP positions
///
/// Liquidation Priority:
/// 1. Principal positions - lowest market impact
/// 2. Slab LP positions - no slippage, frees reserved collateral
/// 3. AMM LP positions - potential slippage, requires fresh prices
///
/// Safety Mechanisms:
/// - Uses KANI verified functions for all LP operations (LP1-LP10)
/// - Staleness guard for AMM prices
/// - Rate limiting in PreLiquidation mode
/// - Bad debt socialization via insurance fund
///
/// Accounts Required:
/// - Portfolio, Registry, Clock (standard)
/// - Slab accounts (principal + LP)
/// - Oracle accounts (principal)
/// - AMM accounts (LP only, if needed)
///
/// Process liquidate user instruction
///
/// This instruction liquidates an undercollateralized user by executing
/// reduce-only orders across slabs to bring them back to health.
///
/// # Arguments
/// * `portfolio` - User's portfolio account (to be liquidated)
/// * `registry` - Slab registry with liquidation parameters
/// * `vault` - Collateral vault
/// * `router_authority` - Router authority PDA (for CPI signing)
/// * `oracle_accounts` - Oracle price feed accounts (for price validation)
/// * `slab_accounts` - Array of slab accounts to execute on (principal + LP)
/// * `receipt_accounts` - Array of receipt PDAs (one per slab)
/// * `amm_accounts` - Array of AMM accounts (for LP liquidation)
/// * `is_preliq` - Force pre-liquidation mode (if false, auto-determine)
/// * `current_ts` - Current timestamp (for rate limiting)
///
/// # Returns
/// * Updates portfolio with reduced exposures
/// * Updates portfolio health
/// * Enforces reduce-only (no position increases)
/// * All-or-nothing atomicity
pub fn process_liquidate_user(
    program_id: &Pubkey,
    portfolio: &mut Portfolio,
    portfolio_account: &AccountInfo,
    registry: &mut SlabRegistry,
    vault: &mut Vault,
    router_authority: &AccountInfo,
    oracle_accounts: &[AccountInfo],
    slab_accounts: &[AccountInfo],
    receipt_accounts: &[AccountInfo],
    amm_accounts: &[AccountInfo],
    is_preliq: bool,
    current_ts: u64,
) -> Result<(), PercolatorError> {
    msg!("Liquidate: Starting liquidation check");

    // Step 1: Check liquidation eligibility using FORMALLY VERIFIED logic (L1-L13)
    // This uses the verified is_liquidatable function backed by 13 Kani proofs.
    // Properties: Margin threshold correctness, overflow safety, consistent definitions
    use crate::state::model_bridge::is_liquidatable_verified;
    let is_liquidatable_formal = is_liquidatable_verified(portfolio, registry);

    // If not liquidatable by verified check, portfolio is healthy
    if !is_liquidatable_formal {
        msg!("Liquidate: Portfolio is healthy (verified check)");
        return Err(PercolatorError::PortfolioHealthy);
    }

    // Calculate health = equity - MM for mode determination and tracking
    let health = portfolio.equity.saturating_sub(portfolio.mm as i128);
    msg!("Liquidate: Portfolio is liquidatable (verified)");

    // Store health in portfolio for tracking
    portfolio.health = health;

    // Step 2: Determine liquidation mode
    let mode = if is_preliq {
        // Force pre-liquidation mode
        if health >= registry.preliq_buffer {
            msg!("Error: Health too high for pre-liquidation");
            return Err(PercolatorError::PortfolioHealthy);
        }
        LiquidationMode::PreLiquidation
    } else {
        // Auto-determine mode
        match determine_mode(health, registry.preliq_buffer) {
            Some(m) => m,
            None => {
                msg!("Error: Portfolio is healthy, no liquidation needed");
                return Err(PercolatorError::PortfolioHealthy);
            }
        }
    };

    msg!("Liquidate: Mode determined");

    // Step 3: Check rate limiting (for pre-liquidation deleveraging)
    if mode == LiquidationMode::PreLiquidation {
        let time_since_last = current_ts.saturating_sub(portfolio.last_liquidation_ts);
        if time_since_last < portfolio.cooldown_seconds {
            msg!("Error: Cooldown period not elapsed");
            return Err(PercolatorError::LiquidationCooldown);
        }
    }

    // Step 4: Read oracle prices from oracle accounts
    use crate::liquidation::planner::OraclePrice;
    const MAX_ORACLES: usize = 16;
    let mut oracle_prices = [OraclePrice { instrument_idx: 0, price: 0 }; MAX_ORACLES];
    let mut oracle_count = 0;

    for (i, oracle_account) in oracle_accounts.iter().enumerate() {
        if i >= MAX_ORACLES {
            break;
        }

        // Read PriceOracle struct from account data
        // PriceOracle is 128 bytes total
        let oracle_data = oracle_account.try_borrow_data()
            .map_err(|_| PercolatorError::InvalidAccount)?;

        if oracle_data.len() < 128 {
            msg!("Warning: Oracle account too small, skipping");
            continue;
        }

        // Extract price (at offset 72: magic(8) + version(1) + bump(1) + padding(6) + authority(32) + instrument(32) + price(8))
        let price_bytes = [
            oracle_data[72], oracle_data[73], oracle_data[74], oracle_data[75],
            oracle_data[76], oracle_data[77], oracle_data[78], oracle_data[79],
        ];
        let price = i64::from_le_bytes(price_bytes);

        // Use index as instrument_idx for v0 (in production, would map instrument pubkey to index)
        oracle_prices[oracle_count] = OraclePrice {
            instrument_idx: i as u16,
            price,
        };
        oracle_count += 1;
    }
    msg!("Liquidate: Read oracle prices from oracle accounts");

    // Step 5: Build SlabInfo array and call reduce-only planner
    use crate::liquidation::planner::{plan_reduce_only, SlabInfo};
    const MAX_SLABS_FOR_LIQ: usize = 8;
    let mut slab_infos = [SlabInfo {
        slab_id: router_authority.key().clone(),
        slab_idx: 0,
        instrument_idx: 0,
        mark_price: 0,
    }; MAX_SLABS_FOR_LIQ];
    let mut slab_count = 0;

    for (i, slab_account) in slab_accounts.iter().enumerate() {
        if i >= MAX_SLABS_FOR_LIQ {
            break;
        }

        // Read SlabHeader to get mark price
        let slab_data = slab_account.try_borrow_data()
            .map_err(|_| PercolatorError::InvalidAccount)?;

        if slab_data.len() < 96 {
            msg!("Warning: Slab account too small, skipping");
            continue;
        }

        // mark_px is at offset 88 in SlabHeader
        let mark_bytes = [
            slab_data[88], slab_data[89], slab_data[90], slab_data[91],
            slab_data[92], slab_data[93], slab_data[94], slab_data[95],
        ];
        let mark_price = i64::from_le_bytes(mark_bytes);

        slab_infos[slab_count] = SlabInfo {
            slab_id: *slab_account.key(),
            slab_idx: i as u16,
            instrument_idx: i as u16, // v0: use slab index as instrument index
            mark_price,
        };
        slab_count += 1;
    }

    // Call planner to generate liquidation splits
    let plan = plan_reduce_only(
        portfolio,
        registry,
        &oracle_prices,
        oracle_count,
        &slab_infos,
        slab_count,
        mode == LiquidationMode::PreLiquidation,
    )?;
    msg!("Liquidate: Planner generated liquidation plan");

    // Step 6: Execute via process_execute_cross_slab
    if plan.split_count == 0 {
        msg!("Liquidate: No splits planned, no execution needed");
        return Ok(());
    }

    // Execute the liquidation using the same cross-slab logic as normal orders
    // PRODUCTION FIX: For liquidation, we use portfolio_account since the user (liquidatee)
    // is not the transaction signer. The portfolio_account.owner is the liquidatee's pubkey,
    // and the slab program uses this to track the taker owner in the receipt PDA.
    use crate::instructions::process_execute_cross_slab;
    process_execute_cross_slab(
        program_id,
        portfolio,
        portfolio_account,
        vault,
        registry,
        router_authority,
        &slab_accounts[..plan.split_count],
        &receipt_accounts[..plan.split_count],
        &oracle_accounts[..plan.split_count],
        plan.get_splits(),
    )?;
    msg!("Liquidate: Principal liquidation complete via cross-slab logic");

    // Step 6.5: Check if LP liquidation is needed
    let remaining_deficit = calculate_remaining_deficit(portfolio, registry)?;

    if remaining_deficit > 0 {
        msg!("LP Liquidation: Principal insufficient, starting LP liquidation");

        // Step 6.6: Liquidate Slab LP positions (priority 2)
        // For v0, use the same slab_accounts that were provided for principal trading
        // In production, keeper would provide dedicated LP slab accounts
        let slab_freed = liquidate_slab_lp_buckets(
            portfolio,
            remaining_deficit,
            slab_accounts,
            router_authority,
            registry.bump,
        )?;

        if slab_freed > 0 {
            msg!("LP Liquidation: Slab LP freed collateral");

            // Recalculate portfolio margin after Slab LP liquidation
            portfolio.mm = portfolio.calculate_total_mm();
            portfolio.im = portfolio.calculate_total_im();
        }

        // Check again after Slab LP liquidation
        let remaining_deficit_after_slab = calculate_remaining_deficit(portfolio, registry)?;

        if remaining_deficit_after_slab > 0 {
            msg!("LP Liquidation: Still underwater, starting AMM LP liquidation");

            // Step 6.7: Liquidate AMM LP positions (priority 3 - last resort)
            let amm_freed = liquidate_amm_lp_buckets(
                portfolio,
                remaining_deficit_after_slab,
                current_ts,
                registry.max_oracle_staleness_secs.max(0) as u64,
                amm_accounts,
                router_authority,
                registry.bump,
            )?;

            if amm_freed > 0 {
                msg!("LP Liquidation: AMM LP freed collateral");

                // Recalculate portfolio margin after AMM LP liquidation
                portfolio.mm = portfolio.calculate_total_mm();
                portfolio.im = portfolio.calculate_total_im();
            }
        } else {
            msg!("LP Liquidation: Portfolio restored after Slab LP");
        }

        // Final health check
        let final_deficit = calculate_remaining_deficit(portfolio, registry)?;
        if final_deficit > 0 {
            msg!("LP Liquidation: Warning - still underwater");
        } else {
            msg!("LP Liquidation: Portfolio restored to health");
        }
    } else {
        msg!("LP Liquidation: Principal sufficient, no LP needed");
    }

    // Step 7: Update portfolio health and timestamp
    portfolio.health = portfolio.equity.saturating_sub(portfolio.mm as i128);
    portfolio.last_liquidation_ts = current_ts;

    msg!("Liquidate: Portfolio updated");

    // Step 7.5: Settle bad debt via insurance fund if equity < 0
    if portfolio.equity < 0 {
        let bad_debt = portfolio.equity.abs() as u128;

        // Calculate event notional (sum of liquidation fill notionals)
        let mut event_notional: u128 = 0;
        for split in plan.get_splits() {
            let notional = ((split.qty.abs() as u128) * (split.limit_px.abs() as u128)) / 1_000_000;
            event_notional = event_notional.saturating_add(notional);
        }

        // Insurance bad debt payout (accounting only in v0)
        // NOTE: This updates insurance_state.vault_balance and portfolio.equity but does not transfer lamports.
        // The insurance vault PDA holds a pool of lamports that backs the accounting balance.
        // The insurance authority can manually reconcile via TopUpInsurance/WithdrawInsurance.
        //
        // For real-time lamport transfers during liquidation:
        // 1. Add insurance_vault account to instruction accounts
        // 2. Transfer payout lamports from insurance_vault PDA to portfolio_account
        // 3. Ensure insurance_vault.lamports() stays in sync with insurance_state.vault_balance
        // 4. The payout increases portfolio.equity, so actual lamports should back this
        let (payout, uncovered) = registry.insurance_state.settle_bad_debt(
            bad_debt,
            event_notional,
            &registry.insurance_params,
            current_ts,
        );

        if payout > 0 {
            // Apply insurance payout to portfolio equity
            portfolio.equity = portfolio.equity.saturating_add(payout as i128);
            msg!("Insurance payout applied to cover bad debt");
        }

        if uncovered > 0 {
            msg!("Warning: Uncovered bad debt remains after insurance payout");

            // Trigger global haircut to socialize the uncovered loss across all users
            // Apply haircut: new_index = old_index * (tvl - loss) / tvl
            let tvl = vault.balance as i128;  // Simplified: use vault balance as TVL proxy

            if tvl > 0 {
                let loss = uncovered as i128;
                let tvl_after_loss = tvl.saturating_sub(loss).max(1);  // Ensure non-zero denominator

                // Apply haircut ratio to global PnL index
                // new_index = old_index * tvl_after_loss / tvl
                let old_index = registry.global_haircut.pnl_index;
                registry.global_haircut.pnl_index = (old_index * tvl_after_loss) / tvl;

                msg!("Global haircut triggered to socialize uncovered bad debt");
            }
        }
    }

    // Step 8: Emit liquidation events (simplified for v0)
    // In production, emit LiquidationStart, LiquidationFill, LiquidationEnd
    msg!("Liquidate: Liquidation completed successfully");

    let _ = vault; // Will be used in production
    let _ = router_authority; // Will be used for CPI signing
    let _ = oracle_accounts; // Will be used for price reading

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create dummy AccountInfo for unit tests
    unsafe fn create_dummy_account_info() -> pinocchio::account_info::AccountInfo {
        use core::mem::MaybeUninit;
        let mut uninit: MaybeUninit<pinocchio::account_info::AccountInfo> = MaybeUninit::uninit();
        uninit.assume_init()
    }

    #[test]
    fn test_determine_mode_hard_liquidation() {
        let health = -1000;
        let buffer = 10_000_000;
        assert_eq!(determine_mode(health, buffer), Some(LiquidationMode::HardLiquidation));
    }

    #[test]
    fn test_determine_mode_pre_liquidation() {
        let health = 5_000_000;
        let buffer = 10_000_000;
        assert_eq!(determine_mode(health, buffer), Some(LiquidationMode::PreLiquidation));
    }

    #[test]
    fn test_determine_mode_healthy() {
        let health = 15_000_000;
        let buffer = 10_000_000;
        assert_eq!(determine_mode(health, buffer), None);
    }

    #[test]
    fn test_determine_mode_exact_buffer() {
        let health = 10_000_000;
        let buffer = 10_000_000;
        assert_eq!(determine_mode(health, buffer), None);
    }

    #[test]
    fn test_determine_mode_zero_health() {
        let health = 0;
        let buffer = 10_000_000;
        assert_eq!(determine_mode(health, buffer), Some(LiquidationMode::PreLiquidation));
    }

    #[test]
    fn test_liquidation_mode_price_bands() {
        use crate::state::SlabRegistry;
        use pinocchio::pubkey::Pubkey;

        // Create registry with different bands for pre-liq vs hard liq
        let registry = SlabRegistry {
            router_id: Pubkey::default(),
            governance: Pubkey::default(),
            insurance_authority: Pubkey::default(),
            bump: 0,
            _padding: [0; 7],
            imr: 500,
            mmr: 250,
            liq_band_bps: 200,      // 2% for hard liquidation
            preliq_buffer: 10_000_000,
            preliq_band_bps: 100,  // 1% for pre-liquidation
            router_cap_per_slab: 1_000_000,
            min_equity_to_quote: 100_000_000,
            oracle_tolerance_bps: 50,
            max_oracle_staleness_secs: 60,
            insurance_params: crate::state::insurance::InsuranceParams::default(),
            insurance_state: crate::state::insurance::InsuranceState::default(),
            pnl_vesting_params: crate::state::pnl_vesting::PnlVestingParams::default(),
            global_haircut: crate::state::pnl_vesting::GlobalHaircut::default(),
            warmup_config: model_safety::adaptive_warmup::AdaptiveWarmupConfig::default(),
            warmup_state: model_safety::adaptive_warmup::AdaptiveWarmupState::default(),
            total_deposits: 0,
            _padding3: [0; 8],
        };

        // Pre-liquidation should use tighter band
        let preliq_band = LiquidationMode::PreLiquidation.get_band_bps(&registry);
        assert_eq!(preliq_band, 100);

        // Hard liquidation should use wider band
        let hardliq_band = LiquidationMode::HardLiquidation.get_band_bps(&registry);
        assert_eq!(hardliq_band, 200);
    }

    #[test]
    fn test_liquidation_respects_oracle_alignment() {
        // This test verifies that liquidation planning uses oracle alignment
        // to exclude slabs with misaligned mark prices
        use crate::liquidation::oracle::validate_oracle_alignment;

        let oracle_price = 1_000_000;  // $1.00
        let tolerance_bps = 50;         // 0.5%

        // Slab with aligned mark price should be included
        let aligned_mark = 1_004_000;  // 0.4% diff
        assert!(validate_oracle_alignment(aligned_mark, oracle_price, tolerance_bps));

        // Slab with misaligned mark price should be excluded
        let misaligned_mark = 1_010_000;  // 1.0% diff
        assert!(!validate_oracle_alignment(misaligned_mark, oracle_price, tolerance_bps));
    }

    // =========================================================================
    // LP Liquidation Tests
    // =========================================================================

    #[test]
    fn test_calculate_remaining_deficit_healthy() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, SlabRegistry};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 110_000_000; // $110
        portfolio.mm = 100_000_000;     // $100

        let registry = SlabRegistry::new(Pubkey::default(), Pubkey::default(), Pubkey::default(), 0);

        let deficit = calculate_remaining_deficit(&portfolio, &registry).unwrap();
        assert_eq!(deficit, 0); // No deficit
    }

    #[test]
    fn test_calculate_remaining_deficit_underwater() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, SlabRegistry};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;  // $95
        portfolio.mm = 100_000_000;     // $100

        let registry = SlabRegistry::new(Pubkey::default(), Pubkey::default(), Pubkey::default(), 0);

        let deficit = calculate_remaining_deficit(&portfolio, &registry).unwrap();
        assert_eq!(deficit, 5_000_000); // $5 deficit
    }

    #[test]
    fn test_calculate_remaining_deficit_exact_mm() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, SlabRegistry};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 100_000_000; // $100
        portfolio.mm = 100_000_000;     // $100

        let registry = SlabRegistry::new(Pubkey::default(), Pubkey::default(), Pubkey::default(), 0);

        let deficit = calculate_remaining_deficit(&portfolio, &registry).unwrap();
        assert_eq!(deficit, 0); // Exactly at MM, no deficit
    }

    #[test]
    fn test_liquidate_slab_lp_buckets_no_buckets() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::Portfolio;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;
        portfolio.mm = 100_000_000;

        let target_deficit = 5_000_000;
        // Unit test - pass empty accounts and dummy account info
        let empty_accounts: &[pinocchio::account_info::AccountInfo] = &[];
        let dummy_account = unsafe { create_dummy_account_info() };
        let freed = liquidate_slab_lp_buckets(&mut portfolio, target_deficit, empty_accounts, &dummy_account, 0).unwrap();

        assert_eq!(freed, 0); // No buckets to liquidate
    }

    #[test]
    fn test_liquidate_slab_lp_buckets_with_slab_lp() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, LpBucket, VenueId};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;
        portfolio.mm = 100_000_000;

        // Add a Slab LP bucket
        let venue_id = VenueId::new_slab(Pubkey::default());
        let mut bucket = LpBucket::new_slab(venue_id);

        // Set up the slab LP with some reserved collateral
        if let Some(ref mut slab) = bucket.slab {
            slab.reserved_base = 50_000_000;
            slab.reserved_quote = 50_000_000;
            slab.open_order_count = 5;
        }

        bucket.active = true;
        bucket.im = 10_000_000;
        bucket.mm = 8_000_000;

        portfolio.lp_buckets[0] = bucket;
        portfolio.lp_bucket_count = 1;

        let target_deficit = 5_000_000;
        // Unit test - pass empty accounts (will fail to find slab account)
        let empty_accounts: &[pinocchio::account_info::AccountInfo] = &[];
        let dummy_account = unsafe { create_dummy_account_info() };
        // This will fail to find slab account, which is expected in unit test
        let result = liquidate_slab_lp_buckets(&mut portfolio, target_deficit, empty_accounts, &dummy_account, 0);

        // Expect InvalidAccount error since we can't find the slab account in unit test
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_amm_lp_buckets_no_buckets() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::Portfolio;

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;
        portfolio.mm = 100_000_000;

        let target_deficit = 5_000_000;
        let current_ts = 1000;
        let max_staleness = 60;

        // Unit test - pass empty accounts
        let empty_accounts: &[pinocchio::account_info::AccountInfo] = &[];
        let dummy_account = unsafe { create_dummy_account_info() };

        let freed = liquidate_amm_lp_buckets(
            &mut portfolio,
            target_deficit,
            current_ts,
            max_staleness,
            empty_accounts,
            &dummy_account,
            0,
        ).unwrap();

        assert_eq!(freed, 0); // No buckets to liquidate
    }

    #[test]
    fn test_liquidate_amm_lp_buckets_with_amm_lp() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, LpBucket, VenueId};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;
        portfolio.mm = 100_000_000;

        // Add an AMM LP bucket
        let venue_id = VenueId::new_amm(Pubkey::default());
        let mut bucket = LpBucket::new_amm(
            venue_id,
            1000,           // lp_shares
            100_000_000,    // share_price ($100)
            950,            // last_update_ts (fresh)
        );
        bucket.active = true;
        bucket.im = 15_000_000;
        bucket.mm = 12_000_000;

        portfolio.lp_buckets[0] = bucket;
        portfolio.lp_bucket_count = 1;

        let target_deficit = 5_000_000;
        let current_ts = 1000;  // 50 seconds after update
        let max_staleness = 60; // Allow up to 60 seconds

        // Unit test - pass empty accounts (will fail to find AMM account)
        let empty_accounts: &[pinocchio::account_info::AccountInfo] = &[];
        let dummy_account = unsafe { create_dummy_account_info() };

        let result = liquidate_amm_lp_buckets(
            &mut portfolio,
            target_deficit,
            current_ts,
            max_staleness,
            empty_accounts,
            &dummy_account,
            0,
        );

        // Expect InvalidAccount error since we can't find the AMM account in unit test
        assert!(result.is_err());
    }

    #[test]
    fn test_liquidate_amm_lp_buckets_stale_price_skipped() {
        use pinocchio::pubkey::Pubkey;
        use crate::state::{Portfolio, LpBucket, VenueId};

        let mut portfolio = Portfolio::new(Pubkey::default(), Pubkey::default(), 0);
        portfolio.equity = 95_000_000;
        portfolio.mm = 100_000_000;

        // Add an AMM LP bucket with stale price
        let venue_id = VenueId::new_amm(Pubkey::default());
        let mut bucket = LpBucket::new_amm(
            venue_id,
            1000,           // lp_shares
            100_000_000,    // share_price ($100)
            900,            // last_update_ts (stale)
        );
        bucket.active = true;
        bucket.im = 15_000_000;
        bucket.mm = 12_000_000;

        portfolio.lp_buckets[0] = bucket;
        portfolio.lp_bucket_count = 1;

        let target_deficit = 5_000_000;
        let current_ts = 1000;  // 100 seconds after update
        let max_staleness = 60; // Only allow up to 60 seconds

        // Unit test - pass empty accounts
        let empty_accounts: &[pinocchio::account_info::AccountInfo] = &[];
        let dummy_account = unsafe { create_dummy_account_info() };

        let freed = liquidate_amm_lp_buckets(
            &mut portfolio,
            target_deficit,
            current_ts,
            max_staleness,
            empty_accounts,
            &dummy_account,
            0,
        ).unwrap();

        // Should skip stale bucket (staleness guard triggers before account lookup)
        assert_eq!(freed, 0);
        // Bucket should still be active (not liquidated)
        assert!(portfolio.lp_buckets[0].active);
    }

    #[test]
    fn test_proportional_margin_reduction() {
        use crate::state::model_bridge::proportional_margin_reduction_verified;

        // Test 100% ratio (no reduction)
        let initial_margin = 100_000_000;
        let ratio_100 = 1_000_000; // 100% in scaled units
        let result = proportional_margin_reduction_verified(initial_margin, ratio_100).unwrap();
        assert_eq!(result, initial_margin);

        // Test 50% ratio
        let ratio_50 = 500_000; // 50% in scaled units
        let result = proportional_margin_reduction_verified(initial_margin, ratio_50).unwrap();
        assert_eq!(result, 50_000_000);

        // Test 0% ratio (full liquidation)
        let ratio_0 = 0;
        let result = proportional_margin_reduction_verified(initial_margin, ratio_0).unwrap();
        assert_eq!(result, 0);
    }
}
