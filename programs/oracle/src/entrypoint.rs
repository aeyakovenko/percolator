//! Oracle program entrypoint

use pinocchio::{
    account_info::AccountInfo, entrypoint, msg, program_error::ProgramError, pubkey::Pubkey,
    ProgramResult,
};

mod instructions;
mod state;

pub use state::{PriceOracle, PRICE_ORACLE_SIZE};

entrypoint!(process_instruction);

/// Oracle instruction discriminators
#[derive(Debug)]
enum OracleInstruction {
    /// Initialize a new price oracle
    Initialize,

    /// Update the oracle price
    UpdatePrice,
}

/// Process oracle instruction
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        msg!("Error: Instruction data is empty");
        return Err(ProgramError::InvalidInstructionData);
    }

    let discriminator = instruction_data[0];
    let instruction = match discriminator {
        0 => OracleInstruction::Initialize,
        1 => OracleInstruction::UpdatePrice,
        _ => {
            msg!("Error: Unknown instruction");
            return Err(ProgramError::InvalidInstructionData);
        }
    };

    match instruction {
        OracleInstruction::Initialize => {
            msg!("Instruction: Initialize");
            instructions::process_initialize(program_id, accounts, &instruction_data[1..])
        }
        OracleInstruction::UpdatePrice => {
            msg!("Instruction: UpdatePrice");
            instructions::process_update_price(program_id, accounts, &instruction_data[1..])
        }
    }
}
