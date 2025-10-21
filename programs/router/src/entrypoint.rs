//! Router program entrypoint

use pinocchio::{
    account_info::AccountInfo,
    entrypoint,
    msg,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::instructions::{RouterInstruction, process_deposit, process_withdraw, process_initialize_registry, process_initialize_portfolio, process_execute_cross_slab};
use crate::state::{Vault, Portfolio};
use percolator_common::{PercolatorError, validate_owner, validate_writable, borrow_account_data_mut, InstructionReader};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    // Check minimum instruction data length
    if instruction_data.is_empty() {
        msg!("Error: Instruction data is empty");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    // Parse instruction discriminator (v0 minimal)
    let discriminator = instruction_data[0];
    let instruction = match discriminator {
        0 => RouterInstruction::Initialize,
        1 => RouterInstruction::InitializePortfolio,
        2 => RouterInstruction::Deposit,
        3 => RouterInstruction::Withdraw,
        4 => RouterInstruction::ExecuteCrossSlab,
        _ => {
            msg!("Error: Unknown instruction");
            return Err(PercolatorError::InvalidInstruction.into());
        }
    };

    // Dispatch to instruction handler (v0 minimal)
    match instruction {
        RouterInstruction::Initialize => {
            msg!("Instruction: Initialize");
            process_initialize_inner(program_id, accounts, &instruction_data[1..])
        }
        RouterInstruction::InitializePortfolio => {
            msg!("Instruction: InitializePortfolio");
            process_initialize_portfolio_inner(program_id, accounts, &instruction_data[1..])
        }
        RouterInstruction::Deposit => {
            msg!("Instruction: Deposit");
            process_deposit_inner(program_id, accounts, &instruction_data[1..])
        }
        RouterInstruction::Withdraw => {
            msg!("Instruction: Withdraw");
            process_withdraw_inner(program_id, accounts, &instruction_data[1..])
        }
        RouterInstruction::ExecuteCrossSlab => {
            msg!("Instruction: ExecuteCrossSlab");
            process_execute_cross_slab_inner(program_id, accounts, &instruction_data[1..])
        }
    }
}

// Instruction processors with account validation

/// Process initialize instruction
///
/// Expected accounts:
/// 0. `[writable]` Registry account (PDA)
/// 1. `[signer]` Governance authority
///
/// Expected data layout (32 bytes):
/// - governance: Pubkey (32 bytes)
fn process_initialize_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 2 {
        msg!("Error: Initialize instruction requires at least 2 accounts");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let registry_account = &accounts[0];
    let governance_account = &accounts[1];

    // Validate accounts
    validate_owner(registry_account, program_id)?;
    validate_writable(registry_account)?;

    // Parse instruction data - governance pubkey
    let mut reader = InstructionReader::new(data);
    let governance_bytes = reader.read_bytes::<32>()?;
    let governance = Pubkey::from(governance_bytes);

    // Verify governance signer matches instruction data
    if governance_account.key() != &governance {
        msg!("Error: Governance account does not match instruction data");
        return Err(PercolatorError::InvalidAccount.into());
    }

    // Call the initialization logic
    process_initialize_registry(program_id, registry_account, &governance)?;

    msg!("Router initialized successfully");
    Ok(())
}

/// Process deposit instruction
///
/// Expected accounts:
/// 0. `[writable]` Vault account
/// 1. `[writable]` User token account
/// 2. `[signer]` User authority
/// 3. `[]` Token program
///
/// Expected data layout (16 bytes):
/// - amount: u128 (16 bytes)
fn process_deposit_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 1 {
        msg!("Error: Deposit instruction requires at least 1 account");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let vault_account = &accounts[0];
    validate_owner(vault_account, program_id)?;
    validate_writable(vault_account)?;

    let vault = unsafe { borrow_account_data_mut::<Vault>(vault_account)? };

    // Parse instruction data
    let mut reader = InstructionReader::new(data);
    let amount = reader.read_u128()?;

    // Call the instruction handler
    process_deposit(vault, amount)?;

    msg!("Deposit processed successfully");
    Ok(())
}

/// Process withdraw instruction
///
/// Expected accounts:
/// 0. `[writable]` Vault account
/// 1. `[writable]` User token account
/// 2. `[signer]` User authority
/// 3. `[]` Token program
///
/// Expected data layout (16 bytes):
/// - amount: u128 (16 bytes)
fn process_withdraw_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 1 {
        msg!("Error: Withdraw instruction requires at least 1 account");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let vault_account = &accounts[0];
    validate_owner(vault_account, program_id)?;
    validate_writable(vault_account)?;

    let vault = unsafe { borrow_account_data_mut::<Vault>(vault_account)? };

    // Parse instruction data
    let mut reader = InstructionReader::new(data);
    let amount = reader.read_u128()?;

    // Call the instruction handler
    process_withdraw(vault, amount)?;

    msg!("Withdraw processed successfully");
    Ok(())
}

/// Process initialize portfolio instruction
///
/// Expected accounts:
/// 0. `[writable]` Portfolio account (PDA)
/// 1. `[signer]` User
///
/// Expected data layout (32 bytes):
/// - user: Pubkey (32 bytes)
fn process_initialize_portfolio_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 2 {
        msg!("Error: InitializePortfolio instruction requires at least 2 accounts");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let portfolio_account = &accounts[0];
    let user_account = &accounts[1];

    // Validate accounts
    validate_owner(portfolio_account, program_id)?;
    validate_writable(portfolio_account)?;

    // Parse instruction data - user pubkey
    let mut reader = InstructionReader::new(data);
    let user_bytes = reader.read_bytes::<32>()?;
    let user = Pubkey::from(user_bytes);

    // Verify user signer matches instruction data
    if user_account.key() != &user {
        msg!("Error: User account does not match instruction data");
        return Err(PercolatorError::InvalidAccount.into());
    }

    // Call the initialization logic
    process_initialize_portfolio(program_id, portfolio_account, &user)?;

    msg!("Portfolio initialized successfully");
    Ok(())
}

/// Process execute cross-slab instruction (v0 main instruction)
///
/// Expected accounts:
/// 0. `[writable]` Portfolio account
/// 1. `[signer]` User authority
/// 2. `[writable]` Vault account
/// 3. `[]` Router authority PDA
/// 4..N. `[writable]` Slab accounts
/// N+1..M. `[writable]` Receipt PDAs
///
/// Expected data layout: TBD
/// - num_splits: u8
/// - splits: [SlabSplit; num_splits]
fn process_execute_cross_slab_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 4 {
        msg!("Error: ExecuteCrossSlab requires at least 4 accounts");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let portfolio_account = &accounts[0];
    let user_account = &accounts[1];
    let vault_account = &accounts[2];
    let router_authority = &accounts[3];

    // Validate accounts
    validate_owner(portfolio_account, program_id)?;
    validate_writable(portfolio_account)?;
    validate_owner(vault_account, program_id)?;
    validate_writable(vault_account)?;

    // Borrow account data mutably
    let portfolio = unsafe { borrow_account_data_mut::<Portfolio>(portfolio_account)? };
    let vault = unsafe { borrow_account_data_mut::<Vault>(vault_account)? };

    // TODO: Parse instruction data to extract splits
    // For now, stub with empty slabs and receipts
    let _ = data;
    let slab_accounts = &[];
    let receipt_accounts = &[];
    let splits = &[];

    // Call the instruction handler
    process_execute_cross_slab(
        portfolio,
        user_account.key(),
        vault,
        router_authority,
        slab_accounts,
        receipt_accounts,
        splits,
    )?;

    msg!("ExecuteCrossSlab processed successfully");
    Ok(())
}
