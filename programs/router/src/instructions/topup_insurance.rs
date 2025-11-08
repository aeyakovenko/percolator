//! TopUpInsurance instruction - manually top up insurance fund

use crate::pda::derive_insurance_vault_pda;
use crate::state::SlabRegistry;
use percolator_common::*;
use pinocchio::{
    account_info::AccountInfo,
    msg,
    pubkey::Pubkey,
};

/// Process top_up_insurance instruction
///
/// Manually tops up the insurance fund. Only callable by insurance authority.
/// Useful for bootstrapping or emergency funding.
///
/// # Security Checks
/// - Verifies insurance_authority is signer
/// - Verifies insurance_authority matches registry.insurance_authority
/// - Verifies insurance_vault matches derived PDA
///
/// # Arguments
/// * `registry_account` - The registry account (writable)
/// * `insurance_authority` - The insurance authority (signer, writable for sending funds)
/// * `insurance_vault` - The insurance vault PDA (writable)
/// * `program_id` - The router program ID (for PDA verification)
/// * `amount` - Amount to deposit (lamports)
pub fn process_topup_insurance(
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

    // Top up the insurance vault state
    registry.insurance_state.top_up(amount);

    // Transfer lamports from insurance_authority to insurance vault PDA
    let amount_u64 = u64::try_from(amount).map_err(|_| {
        msg!("Error: Amount exceeds u64 max");
        PercolatorError::InvalidQuantity
    })?;

    unsafe {
        *insurance_authority.borrow_mut_lamports_unchecked() -= amount_u64;
        *insurance_vault.borrow_mut_lamports_unchecked() += amount_u64;
    }

    msg!("Insurance top-up successful");
    Ok(())
}

// Tests for topup_insurance are in integration tests
// Exclude test module from BPF builds
// #[cfg(all(test, not(target_os = "solana")))]
// #[path = "topup_insurance_test.rs"]
// mod topup_insurance_test;
