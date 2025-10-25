//! Margin and collateral management

use anyhow::{Context, Result};
use colored::Colorize;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_instruction,
    system_program,
    signer::Signer,
    transaction::Transaction,
};

use crate::{client, config::NetworkConfig};

/// Derive portfolio PDA for a user
/// Matches router/src/pda.rs::derive_portfolio_pda
fn derive_portfolio_pda(user: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"portfolio", user.as_ref()], program_id)
}

/// Derive registry PDA
/// Matches router/src/pda.rs::derive_registry_pda
fn derive_registry_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"registry"], program_id)
}

pub async fn initialize_portfolio(config: &NetworkConfig) -> Result<()> {
    println!("{}", "=== Initialize Portfolio ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Router Program:".bright_cyan(), config.router_program_id);

    // Get RPC client and payer keypair
    let rpc_client = client::create_rpc_client(config);
    let payer = &config.keypair;
    let user = payer.pubkey();

    println!("\n{} {}", "User:".bright_cyan(), user);

    // Derive portfolio account address using create_with_seed
    // This creates a regular account (not PDA) at a deterministic address
    // This bypasses the 10KB CPI limit since creation happens from the client
    let portfolio_seed = "portfolio";
    let portfolio_address = Pubkey::create_with_seed(
        &user,
        portfolio_seed,
        &config.router_program_id,
    )?;

    println!("{} {}", "Portfolio Address:".bright_cyan(), portfolio_address);

    // Get account size from router program
    let portfolio_size = percolator_router::state::Portfolio::LEN;
    println!("{} {} bytes", "Portfolio Size:".bright_cyan(), portfolio_size);

    // Calculate rent for portfolio account
    let rent = rpc_client.get_minimum_balance_for_rent_exemption(portfolio_size)?;
    println!("{} {} lamports ({} SOL)",
        "Rent:".bright_cyan(),
        rent,
        rent as f64 / 1e9
    );

    // Check if account already exists
    if let Ok(account) = rpc_client.get_account(&portfolio_address) {
        if account.owner == config.router_program_id && account.data.len() == portfolio_size {
            println!("\n{}", "Portfolio account already exists and is initialized".yellow());
            println!("{}", "Skipping initialization".yellow());
            return Ok(());
        }
    }

    // INSTRUCTION 1: Create the portfolio account using create_account_with_seed
    // This bypasses the 10KB CPI limit by creating from the client
    let create_account_ix = system_instruction::create_account_with_seed(
        &user,                         // Funding account (signer)
        &portfolio_address,            // Address of account to create
        &user,                         // Base for address derivation
        portfolio_seed,                // Seed string
        rent,                          // Lamports for rent exemption
        portfolio_size as u64,         // Account size (135KB - no 10KB limit!)
        &config.router_program_id,     // Owner (router program)
    );

    // Build instruction data: [discriminator (1u8), user_pubkey (32 bytes)]
    let mut instruction_data = Vec::with_capacity(33);
    instruction_data.push(1u8); // RouterInstruction::InitializePortfolio discriminator
    instruction_data.extend_from_slice(&user.to_bytes()); // user pubkey

    // INSTRUCTION 2: Initialize the created account
    let initialize_ix = Instruction {
        program_id: config.router_program_id,
        accounts: vec![
            AccountMeta::new(portfolio_address, false), // Portfolio account (writable)
            AccountMeta::new(user, true),               // Payer (signer, writable)
        ],
        data: instruction_data,
    };

    // Build atomic transaction with both instructions
    // Both must succeed or both fail (atomicity)
    let recent_blockhash = rpc_client.get_latest_blockhash()?;

    let transaction = Transaction::new_signed_with_payer(
        &[create_account_ix, initialize_ix],  // Both instructions in one transaction
        Some(&user),
        &[payer],
        recent_blockhash,
    );

    // Send transaction
    println!("{}", "Sending transaction...".bright_green());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    println!("\n{} {}", "Success!".bright_green().bold(), "✓".bright_green());
    println!("{} {}", "Signature:".bright_cyan(), signature);
    println!("{} {}", "Portfolio Address:".bright_cyan(), portfolio_address);
    println!("\n{}", "Portfolio initialized successfully".bright_green());

    Ok(())
}

pub async fn deposit_collateral(
    config: &NetworkConfig,
    amount: u64,
    token: Option<String>,
) -> Result<()> {
    println!("{}", "=== Deposit Collateral ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Router Program:".bright_cyan(), config.router_program_id);

    if token.is_some() {
        println!("\n{}", "Token deposits not yet supported (SOL only for MVP)".yellow());
        return Ok(());
    }

    println!("{} {} lamports ({} SOL)",
        "Amount:".bright_cyan(),
        amount,
        amount as f64 / 1e9
    );

    // Get RPC client and payer keypair
    let rpc_client = client::create_rpc_client(config);
    let payer = &config.keypair;
    let user = payer.pubkey();

    println!("\n{} {}", "User:".bright_cyan(), user);

    // Derive portfolio account address
    let portfolio_seed = "portfolio";
    let portfolio_address = Pubkey::create_with_seed(
        &user,
        portfolio_seed,
        &config.router_program_id,
    )?;

    println!("{} {}", "Portfolio Address:".bright_cyan(), portfolio_address);

    // Check portfolio exists
    let portfolio_account = rpc_client.get_account(&portfolio_address)
        .context("Portfolio account not found - run 'margin init' first")?;

    if portfolio_account.owner != config.router_program_id {
        anyhow::bail!("Portfolio account has incorrect owner");
    }

    println!("{} {} lamports",
        "Portfolio Balance (before):".bright_cyan(),
        portfolio_account.lamports
    );

    // Build instruction data: [discriminator (1u8), amount (8 bytes)]
    let mut instruction_data = Vec::with_capacity(9);
    instruction_data.push(2u8); // RouterInstruction::Deposit discriminator
    instruction_data.extend_from_slice(&amount.to_le_bytes());

    // Build deposit instruction
    let deposit_ix = Instruction {
        program_id: config.router_program_id,
        accounts: vec![
            AccountMeta::new(portfolio_address, false), // Portfolio account (writable)
            AccountMeta::new(user, true),                // User (signer, writable)
            AccountMeta::new_readonly(system_program::id(), false), // System program
        ],
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&user),
        &[payer],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".bright_green());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    println!("\n{} {}", "Success!".bright_green().bold(), "✓".bright_green());
    println!("{} {}", "Signature:".bright_cyan(), signature);

    // Fetch updated portfolio balance
    let updated_account = rpc_client.get_account(&portfolio_address)?;
    println!("{} {} lamports",
        "Portfolio Balance (after):".bright_cyan(),
        updated_account.lamports
    );

    println!("\n{}", "Deposit successful".bright_green());

    Ok(())
}

pub async fn withdraw_collateral(
    config: &NetworkConfig,
    amount: u64,
    token: Option<String>,
) -> Result<()> {
    println!("{}", "=== Withdraw Collateral ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Router Program:".bright_cyan(), config.router_program_id);

    if token.is_some() {
        println!("\n{}", "Token withdrawals not yet supported (SOL only for MVP)".yellow());
        return Ok(());
    }

    println!("{} {} lamports ({} SOL)",
        "Amount:".bright_cyan(),
        amount,
        amount as f64 / 1e9
    );

    // Get RPC client and payer keypair
    let rpc_client = client::create_rpc_client(config);
    let payer = &config.keypair;
    let user = payer.pubkey();

    println!("\n{} {}", "User:".bright_cyan(), user);

    // Derive portfolio account address
    let portfolio_seed = "portfolio";
    let portfolio_address = Pubkey::create_with_seed(
        &user,
        portfolio_seed,
        &config.router_program_id,
    )?;

    println!("{} {}", "Portfolio Address:".bright_cyan(), portfolio_address);

    // Derive registry account address
    let (registry_address, _bump) = derive_registry_pda(&config.router_program_id);
    println!("{} {}", "Registry Address:".bright_cyan(), registry_address);

    // Check portfolio exists
    let portfolio_account = rpc_client.get_account(&portfolio_address)
        .context("Portfolio account not found - run 'margin init' first")?;

    if portfolio_account.owner != config.router_program_id {
        anyhow::bail!("Portfolio account has incorrect owner");
    }

    println!("{} {} lamports",
        "Portfolio Balance (before):".bright_cyan(),
        portfolio_account.lamports
    );

    // Build instruction data: [discriminator (1u8), amount (8 bytes)]
    let mut instruction_data = Vec::with_capacity(9);
    instruction_data.push(3u8); // RouterInstruction::Withdraw discriminator
    instruction_data.extend_from_slice(&amount.to_le_bytes());

    // Build withdraw instruction
    let withdraw_ix = Instruction {
        program_id: config.router_program_id,
        accounts: vec![
            AccountMeta::new(portfolio_address, false),     // Portfolio account (writable)
            AccountMeta::new(user, true),                    // User (signer, writable)
            AccountMeta::new_readonly(system_program::id(), false), // System program
            AccountMeta::new_readonly(registry_address, false),     // Registry (readonly)
        ],
        data: instruction_data,
    };

    // Send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[withdraw_ix],
        Some(&user),
        &[payer],
        recent_blockhash,
    );

    println!("\n{}", "Sending transaction...".bright_green());
    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    println!("\n{} {}", "Success!".bright_green().bold(), "✓".bright_green());
    println!("{} {}", "Signature:".bright_cyan(), signature);

    // Fetch updated portfolio balance
    let updated_account = rpc_client.get_account(&portfolio_address)?;
    println!("{} {} lamports",
        "Portfolio Balance (after):".bright_cyan(),
        updated_account.lamports
    );

    println!("\n{}", "Withdrawal successful".bright_green());

    Ok(())
}

pub async fn show_margin_account(_config: &NetworkConfig, user: Option<String>) -> Result<()> {
    println!("{}", "=== Margin Account ===".bright_green().bold());
    if let Some(u) = user {
        println!("{} {}", "User:".bright_cyan(), u);
    }

    println!("\n{}", "Account info not yet implemented".yellow());
    Ok(())
}

pub async fn show_margin_requirements(_config: &NetworkConfig, user: String) -> Result<()> {
    println!("{}", "=== Margin Requirements ===".bright_green().bold());
    println!("{} {}", "User:".bright_cyan(), user);

    println!("\n{}", "Margin calculations not yet implemented".yellow());
    Ok(())
}
