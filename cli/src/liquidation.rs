//! Liquidation operations

use anyhow::{Context, Result};
use colored::Colorize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use crate::{client, config::NetworkConfig};

pub async fn execute_liquidation(
    _config: &NetworkConfig,
    user: String,
    max_size: Option<u64>,
) -> Result<()> {
    println!("{}", "=== Execute Liquidation ===".bright_green().bold());
    println!("{} {}", "User:".bright_cyan(), user);
    if let Some(size) = max_size {
        println!("{} {}", "Max Size:".bright_cyan(), size);
    }

    println!("\n{}", "Liquidation execution not yet implemented".yellow());
    println!("{}", "This will:".dimmed());
    println!("  {} Check if user is below maintenance margin", "├─".dimmed());
    println!("  {} Calculate liquidation size", "├─".dimmed());
    println!("  {} Execute liquidation via router", "└─".dimmed());

    Ok(())
}

pub async fn list_liquidatable(config: &NetworkConfig, _exchange: String) -> Result<()> {
    println!("{}", "=== Liquidatable Accounts ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);

    let rpc_client = client::create_rpc_client(config);

    // Get all portfolio accounts using getProgramAccounts
    println!("\n{}", "Scanning for portfolio accounts...".dimmed());

    let accounts = rpc_client.get_program_accounts(&config.router_program_id)
        .context("Failed to fetch program accounts")?;

    if accounts.is_empty() {
        println!("{}", "No portfolio accounts found".yellow());
        return Ok(());
    }

    println!("{} {} portfolio accounts found", "Found".bright_cyan(), accounts.len());

    let mut liquidatable_count = 0;

    for (pubkey, account) in accounts {
        // Verify account size matches Portfolio
        let expected_size = percolator_router::state::Portfolio::LEN;
        if account.data.len() != expected_size {
            continue; // Skip non-portfolio accounts
        }

        // SAFETY: Portfolio has #[repr(C)] and we verified the size matches exactly
        let portfolio = unsafe {
            &*(account.data.as_ptr() as *const percolator_router::state::Portfolio)
        };

        // Check if liquidatable (health < 0)
        if portfolio.health < 0 {
            liquidatable_count += 1;

            let equity_sol = portfolio.equity as f64 / 1_000_000_000.0;
            let health_sol = portfolio.health as f64 / 1_000_000_000.0;
            let im_sol = portfolio.im as f64 / 1_000_000_000.0;
            let mm_sol = portfolio.mm as f64 / 1_000_000_000.0;
            let free_collateral_sol = portfolio.free_collateral as f64 / 1_000_000_000.0;

            // Convert pinocchio Pubkey ([u8; 32]) to Solana SDK Pubkey for display
            let user_pubkey = Pubkey::new_from_array(portfolio.user);

            println!("\n{} {}", "Liquidatable:".bright_red().bold(), pubkey);
            println!("  {} {}", "User:".bright_cyan(), user_pubkey);
            println!("  {} {:.4} SOL", "Equity:".bright_cyan(), equity_sol);
            println!("  {} {:.4} SOL {}", "Health:".bright_red(), health_sol, "(UNDERWATER)".bright_red());
            println!("  {} {:.4} SOL", "Initial Margin:".bright_cyan(), im_sol);
            println!("  {} {:.4} SOL", "Maintenance Margin:".bright_cyan(), mm_sol);
            println!("  {} {:.4} SOL", "Free Collateral:".bright_cyan(), free_collateral_sol);
            println!("  {} {}", "Exposure Count:".bright_cyan(), portfolio.exposure_count);
        }
    }

    println!();
    if liquidatable_count == 0 {
        println!("{}", "No liquidatable accounts found".green());
    } else {
        println!("{} {} {} liquidatable",
            "Found".bright_red().bold(),
            liquidatable_count,
            if liquidatable_count == 1 { "account" } else { "accounts" }
        );
    }

    Ok(())
}

pub async fn show_history(_config: &NetworkConfig, limit: usize) -> Result<()> {
    println!("{}", "=== Liquidation History ===".bright_green().bold());
    println!("{} {}", "Limit:".bright_cyan(), limit);

    println!("\n{}", "No liquidation history (not yet implemented)".dimmed());
    Ok(())
}
