//! Slab program entrypoint (v0 minimal)

use pinocchio::{
    account_info::AccountInfo,
    entrypoint,
    msg,
    pubkey::Pubkey,
    ProgramResult,
};

use crate::instructions::{SlabInstruction, process_initialize_slab, process_commit_fill, Side};
use crate::state::SlabState;
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

    // Parse instruction discriminator
    let discriminator = instruction_data[0];
    let instruction = match discriminator {
        0 => SlabInstruction::Initialize,
        1 => SlabInstruction::CommitFill,
        _ => {
            msg!("Error: Unknown instruction");
            return Err(PercolatorError::InvalidInstruction.into());
        }
    };

    // Dispatch to instruction handler (v0 minimal)
    match instruction {
        SlabInstruction::Initialize => {
            msg!("Instruction: Initialize");
            process_initialize_inner(program_id, accounts, &instruction_data[1..])
        }
        SlabInstruction::CommitFill => {
            msg!("Instruction: CommitFill");
            process_commit_fill_inner(program_id, accounts, &instruction_data[1..])
        }
    }
}

// Instruction processors with account validation

/// Process initialize instruction (v0)
///
/// Expected accounts:
/// 0. `[writable]` Slab state account (PDA, uninitialized)
/// 1. `[signer]` Payer/authority
///
/// Expected data layout (121 bytes):
/// - lp_owner: Pubkey (32 bytes)
/// - router_id: Pubkey (32 bytes)
/// - instrument: Pubkey (32 bytes)
/// - mark_px: i64 (8 bytes)
/// - taker_fee_bps: i64 (8 bytes)
/// - contract_size: i64 (8 bytes)
/// - bump: u8 (1 byte)
fn process_initialize_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 1 {
        msg!("Error: Initialize instruction requires at least 1 account");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let slab_account = &accounts[0];
    validate_owner(slab_account, program_id)?;
    validate_writable(slab_account)?;

    // Parse instruction data
    let mut reader = InstructionReader::new(data);
    let lp_owner_bytes = reader.read_bytes::<32>()?;
    let router_id_bytes = reader.read_bytes::<32>()?;
    let instrument_bytes = reader.read_bytes::<32>()?;
    let mark_px = reader.read_i64()?;
    let taker_fee_bps = reader.read_i64()?;
    let contract_size = reader.read_i64()?;
    let bump = reader.read_u8()?;

    let lp_owner = Pubkey::from(lp_owner_bytes);
    let router_id = Pubkey::from(router_id_bytes);
    let instrument = Pubkey::from(instrument_bytes);

    // Call the initialization logic
    process_initialize_slab(
        program_id,
        slab_account,
        lp_owner,
        router_id,
        instrument,
        mark_px,
        taker_fee_bps,
        contract_size,
        bump,
    )?;

    msg!("Slab initialized successfully");
    Ok(())
}

/// Process commit_fill instruction (v0 - atomic fill)
///
/// Expected accounts:
/// 0. `[writable]` Slab state account
/// 1. `[writable]` Fill receipt account
/// 2. `[signer]` Router signer
///
/// Expected data layout (17 bytes):
/// - side: u8 (1 byte) - 0 = Buy, 1 = Sell
/// - qty: i64 (8 bytes) - quantity to fill (1e6 scale)
/// - limit_px: i64 (8 bytes) - limit price (1e6 scale)
fn process_commit_fill_inner(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if accounts.len() < 3 {
        msg!("Error: CommitFill instruction requires at least 3 accounts");
        return Err(PercolatorError::InvalidInstruction.into());
    }

    let slab_account = &accounts[0];
    let receipt_account = &accounts[1];
    let router_signer = &accounts[2];

    // Validate slab account
    validate_owner(slab_account, program_id)?;
    validate_writable(slab_account)?;
    validate_writable(receipt_account)?;

    // Borrow slab state mutably
    let slab = unsafe { borrow_account_data_mut::<SlabState>(slab_account)? };

    // Parse instruction data
    let mut reader = InstructionReader::new(data);
    let side_byte = reader.read_u8()?;
    let qty = reader.read_i64()?;
    let limit_px = reader.read_i64()?;

    // Convert side byte to Side enum
    let side = match side_byte {
        0 => Side::Buy,
        1 => Side::Sell,
        _ => {
            msg!("Error: Invalid side");
            return Err(PercolatorError::InvalidSide.into());
        }
    };

    // Call the commit_fill logic
    process_commit_fill(
        slab,
        receipt_account,
        router_signer.key(),
        side,
        qty,
        limit_px,
    )?;

    msg!("CommitFill processed successfully");
    Ok(())
}
