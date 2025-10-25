//! Liquidate user positions via reduce-only cross-slab execution

use crate::state::{Portfolio, SlabRegistry, Vault};
use percolator_common::*;
use pinocchio::{account_info::AccountInfo, msg};

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
/// * `slab_accounts` - Array of slab accounts to execute on
/// * `receipt_accounts` - Array of receipt PDAs (one per slab)
/// * `is_preliq` - Force pre-liquidation mode (if false, auto-determine)
/// * `current_ts` - Current timestamp (for rate limiting)
///
/// # Returns
/// * Updates portfolio with reduced exposures
/// * Updates portfolio health
/// * Enforces reduce-only (no position increases)
/// * All-or-nothing atomicity
pub fn process_liquidate_user(
    portfolio: &mut Portfolio,
    registry: &mut SlabRegistry,
    vault: &mut Vault,
    router_authority: &AccountInfo,
    oracle_accounts: &[AccountInfo],
    slab_accounts: &[AccountInfo],
    receipt_accounts: &[AccountInfo],
    is_preliq: bool,
    current_ts: u64,
) -> Result<(), PercolatorError> {
    msg!("Liquidate: Starting liquidation check");

    // Step 1: Calculate health = equity - MM
    let health = portfolio.equity.saturating_sub(portfolio.mm as i128);
    msg!("Liquidate: Health calculated");

    // Store health in portfolio for tracking
    portfolio.health = health;

    // Step 1.5: Verify with formally proven liquidation check (L1-L13)
    // This uses the verified is_liquidatable function backed by 13 Kani proofs.
    // Production uses health-based check (more conservative), but we validate
    // with the verified function to ensure correctness.
    #[cfg(not(target_os = "solana"))]
    {
        use crate::state::model_bridge::is_liquidatable_verified;
        let is_liquidatable_formal = is_liquidatable_verified(portfolio, registry);

        // Production's health < 0 should imply verified liquidatable check
        // (production is more conservative - counts negative PnL, model clamps to 0)
        if health < 0 && !is_liquidatable_formal {
            // This should not happen - log for debugging but don't fail
            msg!("Warning: Health check disagrees with verified liquidatable check");
        }
    }

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
    // Clone the user pubkey before the mutable borrow to avoid borrow checker issues
    let user_pubkey = portfolio.user;
    use crate::instructions::process_execute_cross_slab;
    process_execute_cross_slab(
        portfolio,
        &user_pubkey,
        vault,
        registry,
        router_authority,
        &slab_accounts[..plan.split_count],
        &receipt_accounts[..plan.split_count],
        plan.get_splits(),
    )?;
    msg!("Liquidate: Execution complete via cross-slab logic");

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
        use crate::state::{SlabRegistry, SlabEntry};
        use pinocchio::pubkey::Pubkey;
        use percolator_common::MAX_SLABS;

        // Create registry with different bands for pre-liq vs hard liq
        let registry = SlabRegistry {
            router_id: Pubkey::default(),
            governance: Pubkey::default(),
            slab_count: 0,
            bump: 0,
            _padding: [0; 5],
            imr: 500,
            mmr: 250,
            liq_band_bps: 200,      // 2% for hard liquidation
            preliq_buffer: 10_000_000,
            preliq_band_bps: 100,  // 1% for pre-liquidation
            router_cap_per_slab: 1_000_000,
            min_equity_to_quote: 100_000_000,
            oracle_tolerance_bps: 50,
            _padding2: [0; 8],
            insurance_params: crate::state::insurance::InsuranceParams::default(),
            insurance_state: crate::state::insurance::InsuranceState::default(),
            pnl_vesting_params: crate::state::pnl_vesting::PnlVestingParams::default(),
            global_haircut: crate::state::pnl_vesting::GlobalHaircut::default(),
            warmup_config: model_safety::adaptive_warmup::AdaptiveWarmupConfig::default(),
            warmup_state: model_safety::adaptive_warmup::AdaptiveWarmupState::default(),
            total_deposits: 0,
            _padding3: [0; 8],
            slabs: [SlabEntry {
                slab_id: Pubkey::default(),
                version_hash: [0; 32],
                oracle_id: Pubkey::default(),
                imr: 0,
                mmr: 0,
                maker_fee_cap: 0,
                taker_fee_cap: 0,
                latency_sla_ms: 0,
                max_exposure: 0,
                registered_ts: 0,
                active: false,
                _padding: [0; 7],
            }; MAX_SLABS],
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
}
