//! Solana RPC client utilities and helpers

use anyhow::{Context, Result};
use colored::Colorize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Signature, Signer},
    transaction::Transaction,
};

use crate::config::NetworkConfig;

/// Create an RPC client from the network configuration
pub fn create_rpc_client(config: &NetworkConfig) -> RpcClient {
    RpcClient::new_with_commitment(
        config.rpc_url.clone(),
        CommitmentConfig::confirmed(),
    )
}

/// Send and confirm a transaction
pub async fn send_and_confirm_transaction(
    config: &NetworkConfig,
    instructions: Vec<Instruction>,
) -> Result<Signature> {
    let client = create_rpc_client(config);

    let recent_blockhash = client
        .get_latest_blockhash()
        .context("Failed to get recent blockhash")?;

    let mut transaction = Transaction::new_with_payer(
        &instructions,
        Some(&config.pubkey()),
    );

    transaction.sign(&[&config.keypair], recent_blockhash);

    println!("{}", "Sending transaction...".dimmed());

    let signature = client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send and confirm transaction")?;

    println!("{} {}", "Transaction confirmed:".bright_green(), signature);

    Ok(signature)
}

/// Get account data
pub fn get_account_data(
    config: &NetworkConfig,
    address: &Pubkey,
) -> Result<Vec<u8>> {
    let client = create_rpc_client(config);

    let account = client
        .get_account(address)
        .with_context(|| format!("Failed to get account: {}", address))?;

    Ok(account.data)
}

/// Check if an account exists
pub fn account_exists(config: &NetworkConfig, address: &Pubkey) -> bool {
    let client = create_rpc_client(config);
    client.get_account(address).is_ok()
}

/// Get SOL balance
pub fn get_balance(config: &NetworkConfig, address: &Pubkey) -> Result<u64> {
    let client = create_rpc_client(config);

    client
        .get_balance(address)
        .with_context(|| format!("Failed to get balance for: {}", address))
}

/// Format lamports as SOL
pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

/// Parse SOL to lamports
pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

/// Pretty print a signature as a shortened explorer link
pub fn format_signature(signature: &Signature, network: &str) -> String {
    let sig_str = signature.to_string();
    let short = format!("{}...{}", &sig_str[0..8], &sig_str[sig_str.len() - 8..]);

    let explorer_url = match network {
        "mainnet-beta" | "mainnet" => format!("https://explorer.solana.com/tx/{}", sig_str),
        "devnet" => format!("https://explorer.solana.com/tx/{}?cluster=devnet", sig_str),
        "localnet" | "local" => format!("http://localhost:3000/tx/{}", sig_str),
        _ => sig_str.clone(),
    };

    format!("{} ({})", short.bright_blue(), explorer_url.dimmed())
}

/// Pretty print a pubkey as shortened address
pub fn format_pubkey(pubkey: &Pubkey) -> String {
    let addr = pubkey.to_string();
    format!("{}...{}", &addr[0..8], &addr[addr.len() - 8..]).bright_yellow().to_string()
}
