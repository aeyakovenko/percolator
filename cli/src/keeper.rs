//! Keeper bot operations and monitoring

use anyhow::{Context, Result};
use colored::Colorize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Signer,
    transaction::Transaction,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

use crate::{client, config::NetworkConfig};

pub async fn run_keeper(
    _config: &NetworkConfig,
    exchange: String,
    interval: u64,
    monitor_only: bool,
) -> Result<()> {
    println!("{}", "=== Starting Keeper Bot ===".bright_green().bold());
    println!("{} {}", "Exchange:".bright_cyan(), exchange);
    println!("{} {}s", "Interval:".bright_cyan(), interval);
    println!("{} {}", "Monitor Only:".bright_cyan(), if monitor_only { "Yes" } else { "No" });

    println!("\n{}", "Keeper is running...".bright_green());
    println!("{}", "(Press Ctrl+C to stop)".dimmed());

    let interval_duration = Duration::from_secs(interval);
    let rpc_client = client::create_rpc_client(_config);

    let mut total_liquidations = 0;
    let mut total_accounts_checked = 0;

    loop {
        println!("\n{}", format!("[{}] Checking for liquidations...", chrono::Local::now().format("%H:%M:%S")).dimmed());

        // 1. Fetch all portfolio accounts
        let accounts = match rpc_client.get_program_accounts(&_config.router_program_id) {
            Ok(accts) => accts,
            Err(e) => {
                println!("  {} Failed to fetch accounts: {}", "✗".red(), e);
                sleep(interval_duration).await;
                continue;
            }
        };

        total_accounts_checked = accounts.len();
        let mut liquidatable_this_round = 0;

        // 2. Calculate margin requirements and 3. Identify liquidatable accounts
        for (pubkey, account) in accounts {
            // Verify account size matches Portfolio
            let expected_size = percolator_common::state::Portfolio::LEN;
            if account.data.len() != expected_size {
                continue; // Skip non-portfolio accounts
            }

            // SAFETY: Portfolio has #[repr(C)] and we verified the size matches exactly
            let portfolio = unsafe {
                &*(account.data.as_ptr() as *const percolator_common::state::Portfolio)
            };

            // Check if liquidatable (health < 0)
            if portfolio.health < 0 {
                liquidatable_this_round += 1;

                let health_sol = portfolio.health as f64 / 1_000_000_000.0;
                let user_pubkey = Pubkey::new_from_array(portfolio.user);

                println!("  {} Liquidatable: {} (health: {:.4} SOL)",
                    "⚠".yellow(),
                    pubkey,
                    health_sol
                );

                // 4. Execute liquidations (if not monitor_only)
                if !monitor_only {
                    println!("    {} Executing liquidation for user {}...", "→".blue(), user_pubkey);

                    // Execute liquidation transaction
                    match execute_liquidation_tx(_config, &rpc_client, pubkey, user_pubkey).await {
                        Ok(()) => {
                            println!("    {} Liquidation executed successfully", "✓".green());
                            total_liquidations += 1;
                        }
                        Err(e) => {
                            println!("    {} Liquidation failed: {}", "✗".red(), e);
                            println!("    {} Note: Liquidations require oracle and slab infrastructure to be configured", "ℹ".dimmed());
                        }
                    }
                }
            }
        }

        if liquidatable_this_round == 0 {
            if monitor_only {
                println!("  {} No liquidatable accounts found (checked {} accounts)",
                    "ℹ".blue(),
                    total_accounts_checked
                );
            } else {
                println!("  {} No liquidatable accounts found (checked {} accounts)",
                    "✓".green(),
                    total_accounts_checked
                );
            }
        } else {
            if monitor_only {
                println!("  {} Found {} liquidatable accounts (monitor only)",
                    "⚠".yellow(),
                    liquidatable_this_round
                );
            } else {
                println!("  {} Processed {} liquidatable accounts",
                    "✓".green(),
                    liquidatable_this_round
                );
            }
        }

        sleep(interval_duration).await;
    }
}

/// Derive portfolio PDA for a user
/// Matches router/src/pda.rs::derive_portfolio_pda
fn derive_portfolio_pda(user: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"portfolio", user.as_ref()], program_id)
}

/// Derive registry PDA
fn derive_registry_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"registry"], program_id)
}

/// Derive router authority PDA
fn derive_router_authority_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"router_authority"], program_id)
}

/// Derive vault PDA
fn derive_vault_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault"], program_id)
}

/// Derive receipt PDA for a slab
fn derive_receipt_pda(slab_id: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"receipt", slab_id.as_ref()], program_id)
}

/// Query registry for active slabs and oracles
///
/// Returns (oracles, slabs, receipt_pdas)
fn query_active_slabs_and_oracles(
    rpc_client: &RpcClient,
    registry_pda: &Pubkey,
    router_program_id: &Pubkey,
) -> Result<(Vec<Pubkey>, Vec<Pubkey>, Vec<Pubkey>)> {
    // Fetch registry account data
    let registry_account = rpc_client
        .get_account(registry_pda)
        .context("Failed to fetch registry account")?;

    // SAFETY: SlabRegistry has #[repr(C)] and we verify size matches
    let expected_size = percolator_common::state::SlabRegistry::LEN;
    if registry_account.data.len() != expected_size {
        return Err(anyhow::anyhow!(
            "Registry account size mismatch: expected {}, got {}",
            expected_size,
            registry_account.data.len()
        ));
    }

    let registry = unsafe {
        &*(registry_account.data.as_ptr() as *const percolator_common::state::SlabRegistry)
    };

    // Note: Slab whitelist was removed - slabs are now permissionless
    // Return empty vectors since we can't extract slabs from registry
    // In a real keeper implementation, you would:
    // 1. Query portfolio account to get LP buckets
    // 2. Extract slab/AMM accounts from LP buckets
    // 3. Build liquidation instruction with those accounts
    let oracles = Vec::new();
    let slabs = Vec::new();
    let receipts = Vec::new();

    Ok((oracles, slabs, receipts))
}

/// Build and send liquidation transaction
///
/// This function builds a LiquidateUser instruction and sends it to the network.
/// Currently requires oracle and slab infrastructure to be set up.
async fn execute_liquidation_tx(
    config: &NetworkConfig,
    rpc_client: &RpcClient,
    portfolio_pubkey: Pubkey,
    _user_pubkey: Pubkey,
) -> Result<()> {
    // Derive required PDAs
    let (registry_pda, _) = derive_registry_pda(&config.router_program_id);
    let (router_authority_pda, _) = derive_router_authority_pda(&config.router_program_id);
    let (vault_pda, _) = derive_vault_pda(&config.router_program_id);

    // Query registry to get active slabs and oracles
    let (active_oracles, active_slabs, receipt_pdas) =
        query_active_slabs_and_oracles(rpc_client, &registry_pda, &config.router_program_id)?;

    let num_oracles = active_oracles.len() as u8;
    let num_slabs = active_slabs.len() as u8;

    // Get current timestamp
    let current_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    // Instruction data layout (from entrypoint.rs:384-390):
    // - num_oracles: u8 (1 byte)
    // - num_slabs: u8 (1 byte)
    // - is_preliq: u8 (1 byte, 0 = auto, 1 = force pre-liq)
    // - current_ts: u64 (8 bytes)
    let is_preliq: u8 = 0;   // Auto-determine mode

    let mut instruction_data = Vec::with_capacity(11);
    instruction_data.push(5u8); // RouterInstruction::LiquidateUser discriminator
    instruction_data.push(num_oracles);
    instruction_data.push(num_slabs);
    instruction_data.push(is_preliq);
    instruction_data.extend_from_slice(&current_ts.to_le_bytes());

    // Build account list (from entrypoint.rs:375-382):
    // 0. [writable] Portfolio account (to be liquidated)
    // 1. [writable] Registry account
    // 2. [writable] Vault account
    // 3. [] Router authority PDA
    // 4..4+N. [] Oracle accounts (N = num_oracles)
    // 4+N..4+N+M. [writable] Slab accounts (M = num_slabs)
    // 4+N+M..4+N+2M. [writable] Receipt PDAs (M = num_slabs)

    let mut accounts = vec![
        AccountMeta::new(portfolio_pubkey, false),    // Portfolio (writable)
        AccountMeta::new(registry_pda, false),        // Registry (writable)
        AccountMeta::new(vault_pda, false),           // Vault (writable)
        AccountMeta::new_readonly(router_authority_pda, false), // Router authority
    ];

    // Add oracle accounts (readonly)
    for oracle in &active_oracles {
        accounts.push(AccountMeta::new_readonly(*oracle, false));
    }

    // Add slab accounts (writable)
    for slab in &active_slabs {
        accounts.push(AccountMeta::new(*slab, false));
    }

    // Add receipt PDAs (writable)
    for receipt in &receipt_pdas {
        accounts.push(AccountMeta::new(*receipt, false));
    }

    let liquidate_ix = Instruction {
        program_id: config.router_program_id,
        accounts,
        data: instruction_data,
    };

    // Build and send transaction
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let transaction = Transaction::new_signed_with_payer(
        &[liquidate_ix],
        Some(&config.pubkey()),
        &[&config.keypair],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send liquidation transaction")?;

    println!("    {} Transaction confirmed: {}",
        "✓".green(),
        signature
    );

    Ok(())
}

/// Keeper statistics structure
/// In production, this would be persisted to disk or database
#[derive(Debug, Default)]
struct KeeperStats {
    total_liquidations: u64,
    successful_liquidations: u64,
    failed_liquidations: u64,
    total_accounts_checked: u64,
    total_volume_lamports: u64,
    last_check_time: Option<u64>,
    uptime_seconds: u64,
}

pub async fn show_stats(config: &NetworkConfig, _exchange: String) -> Result<()> {
    println!("{}", "=== Keeper Statistics ===".bright_green().bold());
    println!("{} {}", "Network:".bright_cyan(), config.network);

    // Get current portfolio accounts to show real-time stats
    let rpc_client = client::create_rpc_client(config);

    println!("\n{}", "Current System Status:".bright_yellow().bold());

    // Fetch all portfolio accounts
    let accounts = match rpc_client.get_program_accounts(&config.router_program_id) {
        Ok(accts) => accts,
        Err(e) => {
            println!("  {} Failed to fetch accounts: {}", "✗".red(), e);
            return Ok(());
        }
    };

    let total_accounts = accounts.len();
    let mut liquidatable_count = 0;
    let mut at_risk_count = 0;
    let mut healthy_count = 0;
    let mut total_equity: i128 = 0;
    let mut total_health: i128 = 0;

    // Analyze portfolio states
    for (_pubkey, account) in &accounts {
        // Verify account size matches Portfolio
        let expected_size = percolator_common::state::Portfolio::LEN;
        if account.data.len() != expected_size {
            continue;
        }

        // SAFETY: Portfolio has #[repr(C)] and we verified the size matches exactly
        let portfolio = unsafe {
            &*(account.data.as_ptr() as *const percolator_common::state::Portfolio)
        };

        total_equity = total_equity.saturating_add(portfolio.equity);
        total_health = total_health.saturating_add(portfolio.health);

        if portfolio.health < 0 {
            liquidatable_count += 1;
        } else if portfolio.health < (portfolio.mm as i128 / 10) {
            // Less than 10% buffer above maintenance margin
            at_risk_count += 1;
        } else {
            healthy_count += 1;
        }
    }

    // Display current system stats
    println!("  {} {}", "Total Portfolios:".bright_cyan(), total_accounts);
    println!("  {} {}", "Healthy Accounts:".bright_green(), healthy_count);
    println!("  {} {}", "At Risk (low health):".bright_yellow(), at_risk_count);
    println!("  {} {}", "Liquidatable:".bright_red(), liquidatable_count);

    println!("\n{}", "System Health:".bright_yellow().bold());
    println!("  {} {:.4} SOL",
        "Total Equity:".bright_cyan(),
        total_equity as f64 / 1e9
    );
    println!("  {} {:.4} SOL",
        "Total Health:".bright_cyan(),
        total_health as f64 / 1e9
    );

    if total_accounts > 0 {
        let avg_equity = total_equity / (total_accounts as i128);
        let avg_health = total_health / (total_accounts as i128);

        println!("  {} {:.4} SOL",
            "Avg Portfolio Equity:".bright_cyan(),
            avg_equity as f64 / 1e9
        );
        println!("  {} {:.4} SOL",
            "Avg Portfolio Health:".bright_cyan(),
            avg_health as f64 / 1e9
        );
    }

    // Risk assessment
    println!("\n{}", "Risk Assessment:".bright_yellow().bold());
    if total_accounts > 0 {
        let liquidatable_pct = (liquidatable_count as f64 / total_accounts as f64) * 100.0;
        let at_risk_pct = (at_risk_count as f64 / total_accounts as f64) * 100.0;

        println!("  {} {:.2}%",
            "Liquidatable %:".bright_cyan(),
            liquidatable_pct
        );
        println!("  {} {:.2}%",
            "At Risk %:".bright_cyan(),
            at_risk_pct
        );

        // Overall system health indicator
        let health_status = if liquidatable_pct > 10.0 {
            "CRITICAL - High liquidation risk".bright_red().bold()
        } else if liquidatable_pct > 5.0 {
            "WARNING - Elevated liquidation risk".bright_yellow()
        } else if at_risk_pct > 20.0 {
            "CAUTION - Many accounts at risk".bright_yellow()
        } else {
            "HEALTHY - System stable".bright_green()
        };

        println!("  {} {}",
            "Status:".bright_cyan(),
            health_status
        );
    }

    // Note about historical stats
    println!("\n{}", "Historical Statistics:".bright_yellow().bold());
    println!("  {}", "Historical stats require persistent storage implementation".dimmed());
    println!("  {}", "Current implementation shows real-time system status only".dimmed());
    println!("\n{}", "To track keeper performance:".dimmed());
    println!("  {} Run keeper with --monitor-only to avoid affecting stats", "-".dimmed());
    println!("  {} Check logs for liquidation events", "-".dimmed());
    println!("  {} Implement persistent stats tracking for production", "-".dimmed());

    Ok(())
}
