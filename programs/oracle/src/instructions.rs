//! Oracle instruction handlers

use crate::state::{PriceOracle, PRICE_ORACLE_SIZE};
use pinocchio::{
    account_info::AccountInfo,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

/// Initialize a new price oracle
///
/// Accounts:
/// 0. `[writable]` Oracle account (PDA)
/// 1. `[signer]` Authority
/// 2. `[]` Instrument account
///
/// Instruction data:
/// - initial_price: i64 (8 bytes)
/// - bump: u8 (1 byte)
pub fn process_initialize(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if accounts.len() < 3 {
        msg!("Error: Initialize requires 3 accounts");
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    if data.len() < 9 {
        msg!("Error: Initialize requires 9 bytes of data");
        return Err(ProgramError::InvalidInstructionData);
    }

    let oracle_account = &accounts[0];
    let authority_account = &accounts[1];
    let instrument_account = &accounts[2];

    if !authority_account.is_signer() {
        msg!("Error: Authority must be signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !oracle_account.is_writable() {
        msg!("Error: Oracle account must be writable");
        return Err(ProgramError::InvalidAccountData);
    }

    // Parse instruction data
    let initial_price = i64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);
    let bump = data[8];

    // Initialize oracle
    let oracle_data = oracle_account.try_borrow_mut_data()?;
    if oracle_data.len() < PRICE_ORACLE_SIZE {
        msg!("Error: Oracle account too small");
        return Err(ProgramError::AccountDataTooSmall);
    }

    let oracle = unsafe { &mut *(oracle_data.as_ptr() as *mut PriceOracle) };
    *oracle = PriceOracle::new(
        *authority_account.key(),
        *instrument_account.key(),
        initial_price,
        bump,
    );

    msg!("Oracle initialized");
    Ok(())
}

/// Update oracle price
///
/// Accounts:
/// 0. `[writable]` Oracle account
/// 1. `[signer]` Authority
/// 2. `[]` Clock sysvar
///
/// Instruction data:
/// - price: i64 (8 bytes)
/// - confidence: i64 (8 bytes)
pub fn process_update_price(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if accounts.len() < 2 {
        msg!("Error: UpdatePrice requires 2 accounts");
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    if data.len() < 16 {
        msg!("Error: UpdatePrice requires 16 bytes of data");
        return Err(ProgramError::InvalidInstructionData);
    }

    let oracle_account = &accounts[0];
    let authority_account = &accounts[1];

    if !authority_account.is_signer() {
        msg!("Error: Authority must be signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if !oracle_account.is_writable() {
        msg!("Error: Oracle account must be writable");
        return Err(ProgramError::InvalidAccountData);
    }

    // Parse instruction data
    let price = i64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);
    let confidence = i64::from_le_bytes([
        data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
    ]);

    // Get current timestamp
    let clock = Clock::get()?;
    let timestamp = clock.unix_timestamp;

    // Update oracle
    let oracle_data = oracle_account.try_borrow_mut_data()?;
    let oracle = unsafe { &mut *(oracle_data.as_ptr() as *mut PriceOracle) };

    if !oracle.validate() {
        msg!("Error: Invalid oracle account");
        return Err(ProgramError::InvalidAccountData);
    }

    if oracle.authority != *authority_account.key() {
        msg!("Error: Invalid authority");
        return Err(ProgramError::InvalidAccountData);
    }

    oracle.update_price(price, timestamp, confidence);

    msg!("Price updated");
    Ok(())
}
