//! Deposit instruction - deposit SOL collateral to portfolio

use crate::state::Portfolio;
use percolator_common::*;
use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    msg,
    program::invoke,
    pubkey::Pubkey,
    ProgramResult,
};

/// Process deposit instruction (SOL only for MVP)
///
/// Deposits SOL from user's wallet to their portfolio account.
/// Updates portfolio.principal and portfolio.equity.
///
/// # Security Checks
/// - Verifies user is a signer
/// - Verifies portfolio belongs to user
/// - Validates deposit amount is non-zero
///
/// # Arguments
/// * `portfolio_account` - The user's portfolio account (receives SOL)
/// * `portfolio` - Mutable reference to portfolio state
/// * `user_account` - The user's wallet account (sends SOL)
/// * `_system_program` - The System Program account (unused for direct transfer)
/// * `amount` - Amount of lamports to deposit
pub fn process_deposit(
    portfolio_account: &AccountInfo,
    portfolio: &mut Portfolio,
    user_account: &AccountInfo,
    system_program: &AccountInfo,
    amount: u64,
) -> ProgramResult {
    // SECURITY: Validate amount
    if amount == 0 {
        msg!("Error: Deposit amount must be greater than zero");
        return Err(PercolatorError::InvalidQuantity.into());
    }

    // SECURITY: Verify user is a signer
    if !user_account.is_signer() {
        msg!("Error: User must be a signer");
        return Err(PercolatorError::Unauthorized.into());
    }

    // SECURITY: Verify portfolio belongs to this user
    if portfolio.user != *user_account.key() {
        msg!("Error: Portfolio does not belong to user");
        return Err(PercolatorError::Unauthorized.into());
    }

    // Transfer SOL from user to portfolio account using CPI to System Program
    // Build System Program transfer instruction
    // System transfer instruction: discriminator=2u32, data=amount as u64
    let mut instruction_data = [0u8; 12];
    instruction_data[0..4].copy_from_slice(&2u32.to_le_bytes()); // Transfer discriminator
    instruction_data[4..12].copy_from_slice(&amount.to_le_bytes()); // Amount

    let transfer_instruction = Instruction {
        program_id: system_program.key(),
        accounts: &[
            AccountMeta {
                pubkey: user_account.key(),
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: portfolio_account.key(),
                is_signer: false,
                is_writable: true,
            },
        ],
        data: &instruction_data,
    };

    // Invoke the System Program transfer
    invoke(
        &transfer_instruction,
        &[user_account, portfolio_account, system_program],
    )?;

    // Update portfolio state
    // Principal = deposits - withdrawals (never haircutted)
    // Equity = principal + PnL
    let amount_i128 = amount as i128;

    portfolio.principal = portfolio.principal
        .checked_add(amount_i128)
        .ok_or(PercolatorError::Overflow)?;

    portfolio.equity = portfolio.equity
        .checked_add(amount_i128)
        .ok_or(PercolatorError::Overflow)?;

    msg!("Deposit successful");

    Ok(())
}
