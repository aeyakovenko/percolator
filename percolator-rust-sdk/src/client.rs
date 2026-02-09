use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use crate::instructions::{build_deposit_ix, build_withdraw_ix, build_execute_trade_ix, build_keeper_crank_ix};
use crate::accounts::{fetch_risk_engine, RiskEngine, Account};

/// A high-level client for interacting with the Percolator risk engine via RPC.
/// 
/// `PercolatorClient` simplifies the process of sending transactions and fetching
/// protocol state by encapsulating the RPC client and instruction building.
pub struct PercolatorClient {
    /// The underlying Solana RPC client.
    pub client: RpcClient,
    /// The public key of the Percolator program (or its wrapper).
    pub program_id: Pubkey,
    /// The public key of the RiskEngine state account.
    pub engine_state: Pubkey,
}

impl PercolatorClient {
    /// Creates a new `PercolatorClient`.
    pub fn new(rpc_url: &str, program_id: Pubkey, engine_state: Pubkey) -> Self {
        Self {
            client: RpcClient::new(rpc_url.to_string()),
            program_id,
            engine_state,
        }
    }

    /// Fetches the current `RiskEngine` state from the blockchain.
    pub fn get_engine_state(&self) -> Result<RiskEngine, Box<dyn std::error::Error>> {
        fetch_risk_engine(&self.client, &self.engine_state)
    }

    /// Fetches a specific `Account` from the engine by its index.
    pub fn get_account(&self, index: usize) -> Result<Option<Account>, Box<dyn std::error::Error>> {
        let engine = self.get_engine_state()?;
        if index < crate::percolator::MAX_ACCOUNTS && engine.is_used(index) {
            Ok(Some(engine.accounts[index].clone()))
        } else {
            Ok(None)
        }
    }

    /// Sends a `Deposit` transaction.
    pub fn send_deposit(
        &self,
        payer: &Keypair,
        account_index: u64,
        amount: u128,
        now_slot: u64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let ix = build_deposit_ix(
            self.program_id,
            self.engine_state,
            payer.pubkey(),
            account_index,
            amount,
            now_slot,
        );

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let sig = self.client.send_and_confirm_transaction(&tx)?;
        Ok(sig.to_string())
    }

    /// Sends a `Withdraw` transaction.
    pub fn send_withdraw(
        &self,
        payer: &Keypair,
        account_index: u64,
        amount: u128,
        now_slot: u64,
        oracle_price: u64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let ix = build_withdraw_ix(
            self.program_id,
            self.engine_state,
            payer.pubkey(),
            account_index,
            amount,
            now_slot,
            oracle_price,
        );

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let sig = self.client.send_and_confirm_transaction(&tx)?;
        Ok(sig.to_string())
    }

    /// Sends an `ExecuteTrade` transaction.
    pub fn send_trade(
        &self,
        payer: &Keypair,
        lp_index: u64,
        user_index: u64,
        now_slot: u64,
        oracle_price: u64,
        size: i128,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let ix = build_execute_trade_ix(
            self.program_id,
            self.engine_state,
            payer.pubkey(),
            lp_index,
            user_index,
            now_slot,
            oracle_price,
            size,
        );

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let sig = self.client.send_and_confirm_transaction(&tx)?;
        Ok(sig.to_string())
    }

    /// Sends a `KeeperCrank` transaction.
    pub fn send_crank(
        &self,
        payer: &Keypair,
        caller_index: u64,
        now_slot: u64,
        oracle_price: u64,
        funding_rate: i64,
        allow_panic: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let ix = build_keeper_crank_ix(
            self.program_id,
            self.engine_state,
            payer.pubkey(),
            caller_index,
            now_slot,
            oracle_price,
            funding_rate,
            allow_panic,
        );

        let recent_blockhash = self.client.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[payer],
            recent_blockhash,
        );

        let sig = self.client.send_and_confirm_transaction(&tx)?;
        Ok(sig.to_string())
    }
}
