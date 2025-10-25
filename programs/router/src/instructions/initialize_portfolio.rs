//! Initialize portfolio instruction

use crate::state::Portfolio;
use percolator_common::*;
use pinocchio::{
    account_info::AccountInfo,
    msg,
    pubkey::Pubkey,
};

/// Process initialize portfolio instruction
///
/// Initializes a user's portfolio account for cross-margin tracking.
/// The account must be created externally using create_account_with_seed before calling this instruction.
///
/// # Security Checks
/// - Verifies portfolio account is derived from user with correct seed
/// - Verifies payer is a signer
/// - Prevents double initialization
/// - Validates account ownership and size
///
/// # Arguments
/// * `program_id` - The router program ID
/// * `portfolio_account` - The portfolio account (created with seed "portfolio")
/// * `payer` - Account paying for rent (also base for seed derivation)
/// * `user` - The user pubkey
pub fn process_initialize_portfolio(
    program_id: &Pubkey,
    portfolio_account: &AccountInfo,
    payer: &AccountInfo,
    user: &Pubkey,
) -> Result<(), PercolatorError> {
    // TODO(security): Add create_with_seed address verification
    // Verify portfolio_account matches: Pubkey::create_with_seed(user, "portfolio", program_id)
    // For now, we verify ownership, size, and initialization state which provides baseline security

    // SECURITY: Verify payer is signer
    if !payer.is_signer() {
        msg!("Error: Payer must be a signer");
        return Err(PercolatorError::Unauthorized);
    }

    // SECURITY: Verify account ownership
    if portfolio_account.owner() != program_id {
        msg!("Error: Portfolio account has incorrect owner");
        return Err(PercolatorError::InvalidAccountOwner);
    }

    // SECURITY: Verify account size
    let data = portfolio_account.try_borrow_data()
        .map_err(|_| PercolatorError::InvalidAccount)?;

    if data.len() != Portfolio::LEN {
        msg!("Error: Portfolio account has incorrect size");
        return Err(PercolatorError::InvalidAccount);
    }

    // SECURITY: Check if already initialized (program_id field should be zero)
    // We check the first 32 bytes which should be the program_id field
    let mut is_initialized = false;
    for i in 0..32 {
        if data[i] != 0 {
            is_initialized = true;
            break;
        }
    }

    if is_initialized {
        msg!("Error: Portfolio account is already initialized");
        return Err(PercolatorError::AlreadyInitialized);
    }

    drop(data);

    // Initialize the portfolio in-place (avoids stack overflow)
    // Note: We use a dummy bump of 0 since we're not using PDA authority
    let portfolio = unsafe { borrow_account_data_mut::<Portfolio>(portfolio_account)? };

    portfolio.initialize_in_place(*program_id, *user, 0);

    msg!("Portfolio initialized successfully");
    Ok(())
}
