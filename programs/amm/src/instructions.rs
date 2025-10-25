//! AMM instructions - initialize and commit_fill

use crate::{AmmState, math::{quote_buy, quote_sell}};
use percolator_common::{PercolatorError, Side, SlabHeader, FillReceipt, borrow_account_data_mut};
use pinocchio::{account_info::AccountInfo, msg, pubkey::Pubkey, ProgramResult};

/// Initialize a new AMM pool
pub fn process_initialize(
    accounts: &[AccountInfo],
    lp_owner: Pubkey,
    router_id: Pubkey,
    instrument: Pubkey,
    mark_px: i64,
    taker_fee_bps: i64,
    contract_size: i64,
    bump: u8,
    x_reserve: i64,
    y_reserve: i64,
) -> ProgramResult {
    let [amm_account, payer] = accounts else {
        return Err(PercolatorError::InvalidAccount.into());
    };

    // Verify payer signed
    if !payer.is_signer() {
        return Err(PercolatorError::InvalidAccount.into());
    }

    // Get account data
    let data = amm_account.try_borrow_mut_data()?;
    if data.len() != AmmState::LEN {
        msg!("Error: AMM account has incorrect size");
        return Err(PercolatorError::InvalidAccount.into());
    }

    // Create header
    let header = SlabHeader::new(
        *amm_account.key(),
        lp_owner,
        router_id,
        instrument,
        mark_px,
        taker_fee_bps,
        contract_size,
        bump,
    );

    // Create AMM state
    let mut amm = AmmState::new(header, x_reserve, y_reserve, taker_fee_bps);

    // Synthesize initial quote cache
    amm.synthesize_quote_cache();

    // Write to account (unsafe cast from bytes)
    unsafe {
        let state_ptr = data.as_ptr() as *mut AmmState;
        *state_ptr = amm;
    }

    msg!("AMM initialized successfully");
    Ok(())
}

/// Commit a fill against the AMM curve
///
/// This is the CPI endpoint for the router to execute trades against the AMM.
///
/// # Arguments
/// * `accounts` - [amm_account, receipt_account, router_signer]
/// * `side` - Buy or Sell
/// * `qty` - Desired quantity (1e6 scale, positive)
/// * `limit_px` - Worst acceptable VWAP (1e6 scale)
///
/// # Returns
/// * Writes FillReceipt to receipt_account
/// * Updates AMM reserves and QuoteCache
/// * Increments seqno
pub fn process_commit_fill(
    accounts: &[AccountInfo],
    side: Side,
    qty: i64,
    limit_px: i64,
) -> ProgramResult {
    let [amm_account, receipt_account, router_signer] = accounts else {
        return Err(PercolatorError::InvalidAccount.into());
    };

    // Verify router signer
    if !router_signer.is_signer() {
        msg!("Error: Router must be signer");
        return Err(PercolatorError::Unauthorized.into());
    }

    // Get mutable AMM state
    let data = amm_account.try_borrow_mut_data()?;
    if data.len() != AmmState::LEN {
        msg!("Error: AMM account has incorrect size");
        return Err(PercolatorError::InvalidAccount.into());
    }

    let amm = unsafe { &mut *(data.as_ptr() as *mut AmmState) };

    // Verify router authority
    if &amm.header.router_id != router_signer.key() {
        msg!("Error: Invalid router signer");
        return Err(PercolatorError::Unauthorized.into());
    }

    // Validate order parameters
    if qty <= 0 {
        msg!("Error: Quantity must be positive");
        return Err(PercolatorError::InvalidQuantity.into());
    }
    if limit_px <= 0 {
        msg!("Error: Limit price must be positive");
        return Err(PercolatorError::InvalidPrice.into());
    }

    // Capture seqno before execution
    let seqno_committed = amm.header.seqno;

    // Execute trade against AMM curve
    let result = match side {
        Side::Buy => {
            // User buys qty contracts from AMM (AMM sells)
            quote_buy(
                amm.pool.x_reserve,
                amm.pool.y_reserve,
                amm.pool.fee_bps,
                qty,
                amm.pool.min_liquidity,
            ).map_err(|e| match e {
                crate::math::amm_model::AmmError::InvalidReserves => PercolatorError::InvalidAccount,
                crate::math::amm_model::AmmError::InvalidAmount => PercolatorError::InvalidQuantity,
                crate::math::amm_model::AmmError::InsufficientLiquidity => PercolatorError::InsufficientLiquidity,
                crate::math::amm_model::AmmError::Overflow => PercolatorError::InvalidInstruction,
            })?
        }
        Side::Sell => {
            // User sells qty contracts to AMM (AMM buys)
            quote_sell(
                amm.pool.x_reserve,
                amm.pool.y_reserve,
                amm.pool.fee_bps,
                qty,
                amm.pool.min_liquidity,
            ).map_err(|e| match e {
                crate::math::amm_model::AmmError::InvalidReserves => PercolatorError::InvalidAccount,
                crate::math::amm_model::AmmError::InvalidAmount => PercolatorError::InvalidQuantity,
                crate::math::amm_model::AmmError::InsufficientLiquidity => PercolatorError::InsufficientLiquidity,
                crate::math::amm_model::AmmError::Overflow => PercolatorError::InvalidInstruction,
            })?
        }
    };

    // Check limit price
    match side {
        Side::Buy => {
            if result.vwap_px > limit_px {
                msg!("Error: VWAP exceeds buy limit");
                return Err(PercolatorError::InvalidPrice.into());
            }
        }
        Side::Sell => {
            if result.vwap_px < limit_px {
                msg!("Error: VWAP below sell limit");
                return Err(PercolatorError::InvalidPrice.into());
            }
        }
    }

    // Calculate notional and fee
    let notional = (qty as i128 * result.vwap_px as i128 / 1_000_000) as i64;
    let fee = (notional as i128 * amm.pool.fee_bps as i128 / 10_000) as i64;

    // Update AMM reserves
    amm.pool.x_reserve = result.new_x;
    amm.pool.y_reserve = result.new_y;

    // Synthesize new QuoteCache reflecting the updated curve
    amm.synthesize_quote_cache();

    // Write fill receipt
    let receipt = unsafe { borrow_account_data_mut::<FillReceipt>(receipt_account)? };
    receipt.write(seqno_committed, qty, result.vwap_px, notional, fee);

    // Increment seqno (AMM state changed)
    amm.header.increment_seqno();

    msg!("AMM CommitFill executed successfully");

    Ok(())
}
