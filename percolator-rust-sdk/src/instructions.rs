use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use borsh::{BorshSerialize};

/// Instruction discriminators and arguments for the Percolator engine.
/// 
/// These layouts are designed to be compatible with the TypeScript SDK
/// and are used to build CPI or RPC instructions for the protocol.
#[derive(BorshSerialize, Debug, Clone, PartialEq)]
pub enum PercolatorInstruction {
    /// Deposit funds to an account.
    /// 
    /// The wrapper program should move SPL tokens into the vault before calling this.
    Deposit {
        /// The index of the account in the engine's slab.
        account_index: u64,
        /// The amount of tokens deposited (scaled).
        amount: u128,
        /// The current slot (for time-based calculations).
        now_slot: u64,
    },
    /// Withdraw capital from an account.
    /// 
    /// Returns capital if margin requirements and system solvency checks pass.
    Withdraw {
        account_index: u64,
        amount: u128,
        now_slot: u64,
        oracle_price: u64,
    },
    /// Execute a trade between a Liquidity Provider (LP) and a User.
    /// 
    /// This triggers the LP's Matching Engine to validate and execute the trade.
    ExecuteTrade {
        lp_index: u64,
        user_index: u64,
        now_slot: u64,
        oracle_price: u64,
        /// Positive for Long, negative for Short.
        size: i128,
    },
    /// Permissionless maintenance crank.
    /// 
    /// Accrues funding, performs liquidations, and garbage collects dust accounts.
    KeeperCrank {
        caller_index: u64,
        now_slot: u64,
        oracle_price: u64,
        funding_rate_bps_per_slot: i64,
        allow_panic: bool,
    },
    /// Initialize a new user account.
    AddUser {
        fee_payment: u128,
    },
    /// Initialize a new LP account with a custom matching engine.
    AddLp {
        matching_engine_program: [u8; 32],
        matching_engine_context: [u8; 32],
        fee_payment: u128,
    },
}

/// Builds a `Deposit` instruction.
pub fn build_deposit_ix(
    program_id: Pubkey,
    engine_state: Pubkey,
    user_owner: Pubkey,
    account_index: u64,
    amount: u128,
    now_slot: u64,
) -> Instruction {
    let data = PercolatorInstruction::Deposit {
        account_index,
        amount,
        now_slot,
    }
    .try_to_vec()
    .unwrap();

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(engine_state, false),
            AccountMeta::new_readonly(user_owner, true),
        ],
        data,
    }
}

/// Builds a `Withdraw` instruction.
pub fn build_withdraw_ix(
    program_id: Pubkey,
    engine_state: Pubkey,
    user_owner: Pubkey,
    account_index: u64,
    amount: u128,
    now_slot: u64,
    oracle_price: u64,
) -> Instruction {
    let data = PercolatorInstruction::Withdraw {
        account_index,
        amount,
        now_slot,
        oracle_price,
    }
    .try_to_vec()
    .unwrap();

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(engine_state, false),
            AccountMeta::new_readonly(user_owner, true),
        ],
        data,
    }
}

/// Builds an `ExecuteTrade` instruction.
pub fn build_execute_trade_ix(
    program_id: Pubkey,
    engine_state: Pubkey,
    user_owner: Pubkey,
    lp_index: u64,
    user_index: u64,
    now_slot: u64,
    oracle_price: u64,
    size: i128,
) -> Instruction {
    let data = PercolatorInstruction::ExecuteTrade {
        lp_index,
        user_index,
        now_slot,
        oracle_price,
        size,
    }
    .try_to_vec()
    .unwrap();

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(engine_state, false),
            AccountMeta::new_readonly(user_owner, true),
        ],
        data,
    }
}

/// Builds a `KeeperCrank` instruction.
pub fn build_keeper_crank_ix(
    program_id: Pubkey,
    engine_state: Pubkey,
    caller_owner: Pubkey,
    caller_index: u64,
    now_slot: u64,
    oracle_price: u64,
    funding_rate_bps_per_slot: i64,
    allow_panic: bool,
) -> Instruction {
    let data = PercolatorInstruction::KeeperCrank {
        caller_index,
        now_slot,
        oracle_price,
        funding_rate_bps_per_slot,
        allow_panic,
    }
    .try_to_vec()
    .unwrap();

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(engine_state, false),
            AccountMeta::new_readonly(caller_owner, true),
        ],
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_serialization() {
        let program_id = Pubkey::new_unique();
        let engine_state = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let amount = 1_000_000u128;
        let now_slot = 12345u64;
        let account_index = 0u64;

        let ix = build_deposit_ix(program_id, engine_state, owner, account_index, amount, now_slot);

        // Discriminator (0) + account_index (8) + amount (16) + now_slot (8) = 33 bytes
        assert_eq!(ix.data.len(), 33);
        assert_eq!(ix.data[0], 0); // Enum variant Deposit is 0
        
        // Check amount (le)
        assert_eq!(&ix.data[9..25], &amount.to_le_bytes());
    }

    #[test]
    fn test_withdraw_serialization() {
        let program_id = Pubkey::new_unique();
        let engine_state = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let amount = 500_000u128;
        let now_slot = 12346u64;
        let oracle_price = 100_000u64;
        let account_index = 1u64;

        let ix = build_withdraw_ix(program_id, engine_state, owner, account_index, amount, now_slot, oracle_price);

        // Disc (1) + idx (8) + amt (16) + slot (8) + price (8) = 41 bytes
        assert_eq!(ix.data.len(), 41);
        assert_eq!(ix.data[0], 1); // Enum variant Withdraw is 1
    }

    #[test]
    fn test_trade_serialization() {
        let program_id = Pubkey::new_unique();
        let engine_state = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let lp_idx = 0u64;
        let user_idx = 1u64;
        let now_slot = 12347u64;
        let oracle_price = 105_000u64;
        let size = -100_000i128;

        let ix = build_execute_trade_ix(program_id, engine_state, owner, lp_idx, user_idx, now_slot, oracle_price, size);

        // Disc (2) + lp (8) + user (8) + slot (8) + price (8) + size (16) = 49 bytes
        assert_eq!(ix.data.len(), 49);
        assert_eq!(ix.data[0], 2); // Enum variant ExecuteTrade is 2
        assert_eq!(&ix.data[33..49], &size.to_le_bytes());
    }
}
