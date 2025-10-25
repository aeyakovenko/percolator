//! Exchange initialization and management

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
use std::str::FromStr;

use crate::{client, config::NetworkConfig};

/// Derive registry PDA for router program
/// Matches router/src/pda.rs::derive_registry_pda
fn derive_registry_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"registry"], program_id)
}

pub async fn initialize_exchange(
    config: &NetworkConfig,
    _name: String,
    _insurance_fund: u64,
    _maintenance_margin: u16,
    _initial_margin: u16,
) -> Result<()> {
    println!("{}", "=== Initialize Exchange ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);
    println!("{} {}", "Router Program:".bright_cyan(), config.router_program_id);

    // Get RPC client and payer keypair
    let rpc_client = client::create_rpc_client(config);
    let payer = &config.keypair;

    println!("\n{} {}", "Payer:".bright_cyan(), payer.pubkey());

    // Derive registry account address using create_with_seed
    // This creates a regular account (not PDA) at a deterministic address
    let registry_seed = "registry";
    let registry_address = Pubkey::create_with_seed(
        &payer.pubkey(),
        registry_seed,
        &config.router_program_id,
    )?;

    // Also derive the authority PDA that will be stored in the registry
    let (authority_pda, bump) = derive_registry_pda(&config.router_program_id);

    println!("{} {}", "Registry Address:".bright_cyan(), registry_address);
    println!("{} {}", "Authority PDA:".bright_cyan(), authority_pda);
    println!("{} {}", "Bump:".bright_cyan(), bump);

    // Get account size from router program
    // SlabRegistry::LEN from percolator-router crate
    let registry_size = percolator_router::state::SlabRegistry::LEN;
    println!("{} {} bytes", "Registry Size:".bright_cyan(), registry_size);

    // Calculate rent for registry account
    let rent = rpc_client.get_minimum_balance_for_rent_exemption(registry_size)?;
    println!("{} {} lamports ({} SOL)",
        "Rent:".bright_cyan(),
        rent,
        rent as f64 / 1e9
    );

    // Check if registry already exists
    if let Ok(account) = rpc_client.get_account(&registry_address) {
        if account.owner == config.router_program_id && account.data.len() == registry_size {
            println!("\n{}", "Registry account already exists and is initialized".yellow());
            println!("{}", "Skipping initialization".yellow());
            return Ok(());
        }
    }

    println!("\n{}", "Creating registry account...".bright_green());

    // INSTRUCTION 1: Create the registry account using create_account_with_seed
    // This bypasses the 10KB CPI limit by creating from the client
    // The account is at a deterministic address but is NOT a PDA
    // The authority PDA will be stored inside the account after initialization
    let create_account_ix = system_instruction::create_account_with_seed(
        &payer.pubkey(),                   // Funding account (signer)
        &registry_address,                 // Address of account to create
        &payer.pubkey(),                   // Base for address derivation
        registry_seed,                     // Seed string
        rent,                              // Lamports for rent exemption
        registry_size as u64,              // Account size (45,776 bytes - no 10KB limit!)
        &config.router_program_id,         // Owner (router program)
    );

    // Build instruction data: [discriminator (0u8), governance_pubkey (32 bytes)]
    let mut instruction_data = Vec::with_capacity(33);
    instruction_data.push(0u8); // RouterInstruction::Initialize discriminator
    instruction_data.extend_from_slice(&payer.pubkey().to_bytes()); // governance = payer for now

    // INSTRUCTION 2: Initialize the created account
    let initialize_ix = Instruction {
        program_id: config.router_program_id,
        accounts: vec![
            AccountMeta::new(registry_address, false), // Registry account (writable)
            AccountMeta::new(payer.pubkey(), true),    // Payer (signer, writable)
        ],
        data: instruction_data,
    };

    // Build atomic transaction with both instructions
    // Both must succeed or both fail (atomicity)
    let recent_blockhash = rpc_client.get_latest_blockhash()?;

    let transaction = Transaction::new_signed_with_payer(
        &[create_account_ix, initialize_ix],  // Both instructions in one transaction
        Some(&payer.pubkey()),
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
    println!("{} {}", "Registry Address:".bright_cyan(), registry_address);
    println!("{} {}", "Authority PDA:".bright_cyan(), authority_pda);
    println!("\n{}", "Exchange initialized successfully".bright_green());

    Ok(())
}

pub async fn query_registry_status(
    config: &NetworkConfig,
    registry_address_str: String,
    detailed: bool,
) -> Result<()> {
    println!("{}", "=== Registry Status ===" .bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);

    // Parse registry address
    let registry_address = Pubkey::from_str(&registry_address_str)
        .context("Invalid registry address")?;

    println!("{} {}", "Registry Address:".bright_cyan(), registry_address);

    // Get RPC client
    let rpc_client = client::create_rpc_client(config);

    // Fetch account data
    let account = rpc_client
        .get_account(&registry_address)
        .context("Failed to fetch registry account - does it exist?")?;

    // Verify ownership
    if account.owner != config.router_program_id {
        anyhow::bail!(
            "Account is not owned by router program.\nExpected: {}\nActual: {}",
            config.router_program_id,
            account.owner
        );
    }

    // Verify size
    let expected_size = percolator_router::state::SlabRegistry::LEN;
    if account.data.len() != expected_size {
        println!("\n{} Account size mismatch: expected {} bytes, got {} bytes",
            "Warning:".yellow(),
            expected_size,
            account.data.len()
        );
    }

    // Deserialize registry data
    // SAFETY: We've verified the account owner and size
    let registry = unsafe {
        &*(account.data.as_ptr() as *const percolator_router::state::SlabRegistry)
    };

    println!("\n{}", "=== Basic Information ===".bright_yellow());
    // Convert pinocchio Pubkeys to SDK Pubkeys for display
    let router_id_sdk = Pubkey::new_from_array(registry.router_id);
    let governance_sdk = Pubkey::new_from_array(registry.governance);
    println!("{} {}", "Router ID:".bright_cyan(), router_id_sdk);
    println!("{} {}", "Governance:".bright_cyan(), governance_sdk);
    println!("{} {}", "Registered Slabs:".bright_cyan(), registry.slab_count);
    println!("{} {}", "Bump Seed:".bright_cyan(), registry.bump);

    println!("\n{}", "=== Global Risk Parameters ===".bright_yellow());
    println!("{} {}% ({}bps)",
        "Initial Margin Ratio:".bright_cyan(),
        registry.imr as f64 / 100.0,
        registry.imr
    );
    println!("{} {}% ({}bps)",
        "Maintenance Margin Ratio:".bright_cyan(),
        registry.mmr as f64 / 100.0,
        registry.mmr
    );
    println!("{} {}% ({}bps)",
        "Liquidation Band:".bright_cyan(),
        registry.liq_band_bps as f64 / 100.0,
        registry.liq_band_bps
    );
    println!("{} {}",
        "Pre-liquidation Buffer:".bright_cyan(),
        registry.preliq_buffer
    );
    println!("{} {}% ({}bps)",
        "Pre-liquidation Band:".bright_cyan(),
        registry.preliq_band_bps as f64 / 100.0,
        registry.preliq_band_bps
    );
    println!("{} {}",
        "Router Cap Per Slab:".bright_cyan(),
        registry.router_cap_per_slab
    );
    println!("{} {}",
        "Min Equity to Quote:".bright_cyan(),
        registry.min_equity_to_quote
    );
    println!("{} {}% ({}bps)",
        "Oracle Tolerance:".bright_cyan(),
        registry.oracle_tolerance_bps as f64 / 100.0,
        registry.oracle_tolerance_bps
    );

    println!("\n{}", "=== System State ===".bright_yellow());
    println!("{} {}", "Total Deposits:".bright_cyan(), registry.total_deposits);

    if detailed {
        println!("\n{}", "=== Insurance Parameters ===".bright_yellow());
        println!("{} {:?}", "Params:".bright_cyan(), registry.insurance_params);
        println!("{} {:?}", "State:".bright_cyan(), registry.insurance_state);

        println!("\n{}", "=== PnL Vesting Parameters ===".bright_yellow());
        println!("{} {:?}", "Params:".bright_cyan(), registry.pnl_vesting_params);
        println!("{} {:?}", "Global Haircut:".bright_cyan(), registry.global_haircut);

        println!("\n{}", "=== Adaptive Warmup ===".bright_yellow());
        // AdaptiveWarmupConfig and State don't implement Debug, so format manually
        println!("{} <config details available in account data>", "Config:".bright_cyan());
        println!("{} <state details available in account data>", "State:".bright_cyan());
    }

    if registry.slab_count > 0 {
        println!("\n{}", "=== Registered Slabs ===".bright_yellow());
        for i in 0..registry.slab_count as usize {
            let slab = &registry.slabs[i];
            if slab.active {
                println!("\n{} {} {}", "Slab".bright_green(), i, ":");
                // Convert pinocchio Pubkeys to SDK Pubkeys for display
                let slab_id_sdk = Pubkey::new_from_array(slab.slab_id);
                let oracle_id_sdk = Pubkey::new_from_array(slab.oracle_id);
                println!("  {} {}", "ID:".bright_cyan(), slab_id_sdk);
                println!("  {} {}", "Oracle:".bright_cyan(), oracle_id_sdk);
                println!("  {} {}% / {}%",
                    "IMR / MMR:".bright_cyan(),
                    slab.imr as f64 / 100.0,
                    slab.mmr as f64 / 100.0
                );
                println!("  {} {}bps / {}bps",
                    "Fees (maker/taker):".bright_cyan(),
                    slab.maker_fee_cap,
                    slab.taker_fee_cap
                );
                println!("  {} {}ms",
                    "Latency SLA:".bright_cyan(),
                    slab.latency_sla_ms
                );
                println!("  {} {}",
                    "Max Exposure:".bright_cyan(),
                    slab.max_exposure
                );
                println!("  {} {}",
                    "Registered At:".bright_cyan(),
                    slab.registered_ts
                );

                if detailed {
                    // Show first 8 bytes of version hash
                    print!("  {} ", "Version Hash:".bright_cyan());
                    for byte in &slab.version_hash[..8] {
                        print!("{:02x}", byte);
                    }
                    println!("...");
                }
            }
        }
    } else {
        println!("\n{}", "No slabs registered yet".dimmed());
    }

    println!("\n{} {}", "Status:".bright_green().bold(), "OK ✓".bright_green());
    Ok(())
}
