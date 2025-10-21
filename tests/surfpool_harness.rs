//! Surfpool Test Harness
//!
//! Integration test framework for E2E testing of Percolator v0.
//!
//! This harness uses solana-program-test to simulate Surfpool localnet.

use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_instruction,
};
use solana_program_test::{processor, tokio, ProgramTest, ProgramTestContext};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

/// Test context wrapper
pub struct SurfpoolContext {
    pub context: ProgramTestContext,
    pub router_program_id: Pubkey,
    pub slab_program_id: Pubkey,
    pub oracle_program_id: Pubkey,
}

impl SurfpoolContext {
    /// Initialize test context with all programs
    pub async fn new() -> Self {
        let router_program_id = Pubkey::new_unique();
        let slab_program_id = Pubkey::new_unique();
        let oracle_program_id = Pubkey::new_unique();

        let mut program_test = ProgramTest::default();

        // Add programs (using test processors for now)
        program_test.add_program(
            "percolator_router",
            router_program_id,
            processor!(percolator_router::entrypoint::process_instruction),
        );

        program_test.add_program(
            "percolator_slab",
            slab_program_id,
            processor!(percolator_slab::entrypoint::process_instruction),
        );

        program_test.add_program(
            "percolator_oracle",
            oracle_program_id,
            processor!(percolator_oracle::entrypoint::process_instruction),
        );

        let context = program_test.start_with_context().await;

        Self {
            context,
            router_program_id,
            slab_program_id,
            oracle_program_id,
        }
    }

    /// Create and fund a new keypair
    pub async fn create_funded_account(&mut self, lamports: u64) -> Keypair {
        let keypair = Keypair::new();
        let instruction =
            system_instruction::transfer(&self.context.payer.pubkey(), &keypair.pubkey(), lamports);

        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer],
            self.context.last_blockhash,
        );

        self.context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();

        keypair
    }

    /// Initialize an oracle account
    pub async fn initialize_oracle(
        &mut self,
        authority: &Keypair,
        instrument: Pubkey,
        initial_price: i64,
    ) -> Pubkey {
        let oracle_account = Keypair::new();
        let bump = 0; // Simplified for testing

        // Create oracle account
        let rent = self
            .context
            .banks_client
            .get_rent()
            .await
            .unwrap();
        let account_size = 128; // PRICE_ORACLE_SIZE
        let create_account_ix = system_instruction::create_account(
            &self.context.payer.pubkey(),
            &oracle_account.pubkey(),
            rent.minimum_balance(account_size),
            account_size as u64,
            &self.oracle_program_id,
        );

        // Initialize oracle instruction
        let mut instruction_data = vec![0u8]; // Initialize discriminator
        instruction_data.extend_from_slice(&initial_price.to_le_bytes());
        instruction_data.push(bump);

        let initialize_ix = Instruction {
            program_id: self.oracle_program_id,
            accounts: vec![
                AccountMeta::new(oracle_account.pubkey(), false),
                AccountMeta::new_readonly(authority.pubkey(), true),
                AccountMeta::new_readonly(instrument, false),
            ],
            data: instruction_data,
        };

        let transaction = Transaction::new_signed_with_payer(
            &[create_account_ix, initialize_ix],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer, &oracle_account, authority],
            self.context.last_blockhash,
        );

        self.context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();

        oracle_account.pubkey()
    }

    /// Update oracle price
    pub async fn update_oracle_price(
        &mut self,
        oracle: &Pubkey,
        authority: &Keypair,
        price: i64,
        confidence: i64,
    ) {
        let mut instruction_data = vec![1u8]; // UpdatePrice discriminator
        instruction_data.extend_from_slice(&price.to_le_bytes());
        instruction_data.extend_from_slice(&confidence.to_le_bytes());

        let instruction = Instruction {
            program_id: self.oracle_program_id,
            accounts: vec![
                AccountMeta::new(*oracle, false),
                AccountMeta::new_readonly(authority.pubkey(), true),
            ],
            data: instruction_data,
        };

        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.context.payer.pubkey()),
            &[&self.context.payer, authority],
            self.context.last_blockhash,
        );

        self.context
            .banks_client
            .process_transaction(transaction)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_oracle_initialize_and_update() {
        let mut ctx = SurfpoolContext::new().await;

        // Create test authority
        let authority = ctx.create_funded_account(1_000_000_000).await;
        let instrument = Pubkey::new_unique();

        // Initialize oracle with $60,000 price
        let oracle = ctx
            .initialize_oracle(&authority, instrument, 60_000_000_000)
            .await;

        // Update price to $61,000
        ctx.update_oracle_price(&oracle, &authority, 61_000_000_000, 100_000)
            .await;

        // TODO: Read oracle account and verify price
        println!("Oracle initialized and updated successfully!");
    }

    #[tokio::test]
    async fn test_setup() {
        let ctx = SurfpoolContext::new().await;
        assert!(ctx.router_program_id != Pubkey::default());
        assert!(ctx.slab_program_id != Pubkey::default());
        assert!(ctx.oracle_program_id != Pubkey::default());
    }
}
