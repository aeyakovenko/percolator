//! WithdrawInsurance instruction - withdraw surplus from insurance fund

use crate::pda::derive_insurance_vault_pda;
use crate::state::SlabRegistry;
use percolator_common::*;
use pinocchio::{
    account_info::AccountInfo,
    msg,
    pubkey::Pubkey,
};

/// Process withdraw_insurance instruction
///
/// Withdraws surplus from the insurance fund. Only callable by insurance authority.
/// Cannot withdraw if there is uncovered bad debt.
///
/// # Security Checks
/// - Verifies insurance_authority is signer
/// - Verifies insurance_authority matches registry.insurance_authority
/// - Verifies insurance_vault matches derived PDA
/// - Ensures no uncovered bad debt exists
/// - Ensures sufficient vault balance
///
/// # Arguments
/// * `registry_account` - The registry account (writable)
/// * `insurance_authority` - The insurance authority (signer, writable for receiving funds)
/// * `insurance_vault` - The insurance vault PDA (writable)
/// * `program_id` - The router program ID (for PDA verification)
/// * `amount` - Amount to withdraw (lamports)
pub fn process_withdraw_insurance(
    registry_account: &AccountInfo,
    insurance_authority: &AccountInfo,
    insurance_vault: &AccountInfo,
    program_id: &Pubkey,
    amount: u128,
) -> Result<(), PercolatorError> {
    // SECURITY: Verify insurance_authority is signer
    if !insurance_authority.is_signer() {
        msg!("Error: Insurance authority must be a signer");
        return Err(PercolatorError::Unauthorized);
    }

    // SECURITY: Verify insurance_vault matches derived PDA
    let (expected_vault, _bump) = derive_insurance_vault_pda(program_id);
    if insurance_vault.key() != &expected_vault {
        msg!("Error: Invalid insurance vault PDA");
        return Err(PercolatorError::Unauthorized);
    }

    // Borrow registry data mutably
    let registry = unsafe { borrow_account_data_mut::<SlabRegistry>(registry_account)? };

    // SECURITY: Verify insurance_authority matches
    let insurance_authority_pubkey_array = insurance_authority.key();
    if &registry.insurance_authority != insurance_authority_pubkey_array {
        msg!("Error: Invalid insurance authority");
        return Err(PercolatorError::Unauthorized);
    }

    // Attempt withdrawal (internally checks uncovered_bad_debt and balance)
    registry.insurance_state.withdraw_surplus(amount)
        .map_err(|_| {
            msg!("Error: Cannot withdraw - either uncovered bad debt exists or insufficient balance");
            PercolatorError::InsufficientFunds
        })?;

    // Transfer lamports from insurance vault PDA to insurance_authority
    let amount_u64 = u64::try_from(amount).map_err(|_| {
        msg!("Error: Amount exceeds u64 max");
        PercolatorError::InvalidQuantity
    })?;

    unsafe {
        *insurance_vault.borrow_mut_lamports_unchecked() -= amount_u64;
        *insurance_authority.borrow_mut_lamports_unchecked() += amount_u64;
    }

    msg!("Insurance withdrawal successful");
    Ok(())
}

// Tests for withdraw_insurance are in integration tests
// Exclude test module from BPF builds
// #[cfg(all(test, not(target_os = "solana")))]
// #[path = "withdraw_insurance_test.rs"]
// mod withdraw_insurance_test;
